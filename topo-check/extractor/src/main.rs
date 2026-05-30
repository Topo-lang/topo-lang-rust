// topo-extract-rust: Source code extractor for Rust files.
//
// Subprocess protocol:
//   stdin  -> JSON { "files": [...], "functions": [...], "symbolTable": {...} }
//   stdout <- JSON TranspileModule
//
// Parses .rs files with `syn`, matches requested qualified names,
// and converts syn AST nodes to the TranspileModel JSON format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Read};

mod safety;

// ---------------------------------------------------------------------------
// TranspileModel wire types (mirrors topo-core/include/topo/Transpile/TranspileModel.h)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct TypeNode {
    name_parts: Vec<String>,
    // Smart-pointer ownership recovered from Box/Rc/Arc/Weak wrappers. When
    // set, name_parts holds the *inner* type and the cross-language emitter
    // re-targets the wrapper per host (e.g. owned -> C++ unique_ptr). Absent
    // for every other path so non-ownership generics keep prior wire shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    ownership: Option<String>,
    // Associated-type bindings on a parameterised trait bound (Rust
    // `Iterator<Item = u8>`). Only populated when this TypeNode is used as a
    // trait bound and the bound path's last segment carries one or more
    // `GenericArgument::Constraint` entries. Empty for every other TypeNode
    // — so plain types (return types, parameter types, field types,
    // single-bound trait names without assoc clauses) keep byte-identical
    // wire output.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    assoc_bindings: Vec<AssocBindingWire>,
    // Positional template arguments. Currently only populated for
    // parenthesised Fn-trait bounds (`Fn(A, B) -> C` desugars to
    // `Fn<(A, B), Output = C>` and the inputs flow into `templateArgs`).
    // Empty for every other TypeNode so non-Fn-trait wire output stays
    // byte-identical.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    template_args: Vec<TypeNode>,
    // Higher-Ranked Trait Bound (HRTB) lifetimes (Rust
    // `for<'a, 'b> Fn(...)`). Names stored sans-apostrophe (`["a", "b"]`);
    // the `'` is added at emit time. Only populated when this TypeNode is
    // used as a trait bound and the bound carries `for<...>` lifetimes —
    // empty for every other TypeNode so non-HRTB payloads stay byte-
    // identical.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    hrtb_lifetimes: Vec<String>,
}

// Wire shape for `{name, type: TypeNode}` entries inside `assocBindings`.
// Mirrors C++ `TypeNode::RecordField` (name + recursive TypeNode in a
// vector box) but rendered flat as `{name, type}` on the wire to match
// `TranspileModelJson.cpp`'s emitted shape.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct AssocBindingWire {
    name: String,
    #[serde(rename = "type")]
    binding_type: TypeNode,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct ParamDecl {
    name: String,
    #[serde(rename = "type")]
    param_type: TypeNode,
}

// Mirrors the foundation wire shape `{"kind":"type"|"nontype"|"template",
// "name":"...", "bound"?: TypeNode, "bounds"?: [TypeNode], "default"?:
// TypeNode}`. The Rust extractor only ever recovers type params, so kind
// is always "type" here; the field is kept generic to match the C++-side
// model contract. `bound` (single) carries one trait bound; `bounds`
// (list) carries multi-bound `<T: A + B>`. Exactly one of the two is set
// when bounds exist — single-bound payloads stay on the legacy `bound`
// key for byte-identical wire output, multi-bound graduates to `bounds`.
// `default` is omitted when absent so a plain `<T>` stays byte-identical
// to pre-feature output.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct TemplateParam {
    kind: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    bound: Option<TypeNode>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    bounds: Option<Vec<TypeNode>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    default: Option<TypeNode>,
    // NonTypeParam (kind="nontype") default literal expression for
    // `<const N: usize = 16>`. Carried as a source-literal string —
    // integer / bool / scoped-enum literal-spelling. omit-when-empty so
    // a const-generic without a default stays byte-identical to the
    // pre-feature wire payload.
    #[serde(rename = "defaultValue", skip_serializing_if = "Option::is_none", default)]
    default_value: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct TranspileFunction {
    qualified_name: String,
    return_type: TypeNode,
    params: Vec<ParamDecl>,
    body: Vec<serde_json::Value>,
    unsupported: Vec<String>,
    fidelity: String,
    // Omitted from the wire when empty so a non-generic fn stays
    // byte-identical to pre-generics output (matches the foundation contract).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    template_params: Vec<TemplateParam>,
}

// Mirrors `topo::transpile::TranspileType` in
// `topo-core/include/topo/Transpile/TranspileModel.h`. The C++ JSON
// deserializer (TranspileModelJson.cpp) accepts:
//   { qualifiedName, fields, [baseClasses], [baseClassKinds],
//     [templateParams], fidelity }
// `baseClasses` and `baseClassKinds` are always empty for Rust (no class
// inheritance in the language) but are part of the contract; we omit them
// when empty so the wire output stays byte-identical to the cross-language
// "no inheritance" idiom every other extractor follows. `templateParams`
// uses the same skip-when-empty rule for the same reason.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct TranspileType {
    qualified_name: String,
    fields: Vec<FieldDecl>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    base_classes: Vec<TypeNode>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    base_class_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    template_params: Vec<TemplateParam>,
    fidelity: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct FieldDecl {
    name: String,
    #[serde(rename = "type")]
    field_type: TypeNode,
    fidelity: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct TranspileModule {
    types: Vec<TranspileType>,
    functions: Vec<TranspileFunction>,
}

// ---------------------------------------------------------------------------
// Input request
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ExtractRequest {
    files: Vec<String>,
    functions: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    symbol_table: serde_json::Value,
}

// ---------------------------------------------------------------------------
// syn -> TypeNode conversion
// ---------------------------------------------------------------------------

// Convert a syn::Path into a TypeNode by concatenating segment idents into
// nameParts. Used for trait-bound recovery (`<T: Clone>` → bound TypeNode of
// `Clone`).
//
// Associated-type bindings (`Iterator<Item = u8>`) on the LAST segment are
// recovered into `assoc_bindings` so the Rust emitter can re-render them.
// Other forms of last-segment generic argument (positional `Type`, lifetime,
// const, or the `Item: Display` assoc-type-bound `Constraint`) are NOT
// recovered here — the caller is responsible for detecting them via
// `path_last_segment_unsupported_kind` and dropping the whole bound
// (downgrading fidelity) rather than emitting a partial / malformed clause.
fn path_to_type_node(p: &syn::Path) -> TypeNode {
    let mut node = TypeNode {
        name_parts: p.segments.iter().map(|s| s.ident.to_string()).collect(),
        ..Default::default()
    };
    if let Some(last) = p.segments.last() {
        match &last.arguments {
            syn::PathArguments::AngleBracketed(ab) => {
                for arg in &ab.args {
                    if let syn::GenericArgument::AssocType(at) = arg {
                        // `Item = u8` → push as {name: "Item", type: TypeNode(u8)}.
                        // The right-hand side recurses through `type_to_node` so
                        // nested forms (`Item = Vec<T>`, references, etc.) round-
                        // trip via the same wire shape as parameter / field types.
                        if at.generics.is_some() {
                            // `Item<'a> = ...` is outside MVP — caller treats the
                            // path as unsupported via the kind check below.
                            continue;
                        }
                        let bound_ty = type_to_node(&at.ty);
                        node.assoc_bindings.push(AssocBindingWire {
                            name: at.ident.to_string(),
                            binding_type: bound_ty,
                        });
                    }
                }
            }
            syn::PathArguments::Parenthesized(pa) => {
                // Parenthesised Fn-trait form `Fn(A, B) -> C` desugars to
                // `Fn<(A, B), Output = C>`. The extractor stores the
                // inputs flat (without the tuple wrapper) in
                // `templateArgs` and the output as a synthetic
                // `assocBindings[{name:"Output"}]`.
                // RustEmitter detects this Fn/FnMut/FnOnce + Output pattern
                // and re-renders parenthesised. Cross-host emitters render
                // `Fn<A, Output = C>` (angle-bracketed) — they don't know
                // the parenthesised form, but the data round-trips.
                for input in &pa.inputs {
                    node.template_args.push(type_to_node(input));
                }
                if let syn::ReturnType::Type(_, out_ty) = &pa.output {
                    node.assoc_bindings.push(AssocBindingWire {
                        name: "Output".to_string(),
                        binding_type: type_to_node(out_ty),
                    });
                }
            }
            syn::PathArguments::None => {}
        }
    }
    node
}

// Inspects the last segment of `p` and reports whether its angle-bracketed
// arguments contain any kind that this MVP can NOT recover. Returns Some(reason)
// to drop+downgrade the entire bound; None means the segment is either bare
// (no `<...>`), parenthesised Fn-trait (`Fn(A) -> B` — recovered via the
// inputs/output mapping in `path_to_type_node`), or contains only `AssocType`
// entries we already capture.
//
// In syn 2: `Iterator<Item = u8>` is `GenericArgument::AssocType` (recovered).
// `Iterator<Item: Display>` is `GenericArgument::Constraint` (assoc-type-bound,
// out of MVP). Positional `Type` / `Lifetime` / `Const` / `AssocConst` are
// all out of MVP for trait-bound paths and trigger the conservative drop.
fn path_last_segment_unsupported_kind(p: &syn::Path) -> Option<&'static str> {
    let last = match p.segments.last() {
        Some(s) => s,
        None => return None,
    };
    let ab = match &last.arguments {
        syn::PathArguments::AngleBracketed(ab) => ab,
        syn::PathArguments::None => return None,
        // `Fn(A, B) -> C` parenthesised Fn-trait form — the extractor
        // recovers inputs into `templateArgs` and the output into a
        // synthetic `assocBindings[{name:"Output"}]`; `path_to_type_node`
        // handles it alongside the HRTB lifetime capture. The emitter
        // detects the Fn/FnMut/FnOnce + Output pattern and re-renders
        // parenthesised.
        syn::PathArguments::Parenthesized(_) => return None,
    };
    for arg in &ab.args {
        match arg {
            syn::GenericArgument::AssocType(at) => {
                if at.generics.is_some() {
                    return Some("assoc-type-with-its-own-generics");
                }
                // Plain `Item = T` — recovered.
            }
            syn::GenericArgument::Type(_) => return Some("positional-type-arg"),
            syn::GenericArgument::Lifetime(_) => return Some("lifetime-arg"),
            syn::GenericArgument::Const(_) => return Some("const-arg"),
            syn::GenericArgument::AssocConst(_) => return Some("assoc-const"),
            syn::GenericArgument::Constraint(_) => return Some("assoc-type-bound"),
            _ => return Some("unrecognised-generic-arg"),
        }
    }
    None
}

fn type_to_node(ty: &syn::Type) -> TypeNode {
    match ty {
        syn::Type::Path(tp) => {
            // Owner-wrapper ownership recovery: Box -> owned, Rc/Arc ->
            // shared, Weak -> weak. Only when the LAST segment is exactly one
            // of these AND it carries exactly one angle-bracketed *type* arg
            // (e.g. `Box<Foo>`). Anything else — including Vec/Option and any
            // multi-arg or non-type-arg form — keeps the prior segment-ident
            // behavior so the wire shape of unrelated generics is unchanged.
            if let Some(last) = tp.path.segments.last() {
                let owner = match last.ident.to_string().as_str() {
                    "Box" => Some("owned"),
                    "Rc" | "Arc" => Some("shared"),
                    "Weak" => Some("weak"),
                    _ => None,
                };
                if let Some(kind) = owner {
                    if let syn::PathArguments::AngleBracketed(ab) = &last.arguments {
                        let type_args: Vec<&syn::Type> = ab
                            .args
                            .iter()
                            .filter_map(|a| match a {
                                syn::GenericArgument::Type(t) => Some(t),
                                _ => None,
                            })
                            .collect();
                        if ab.args.len() == 1 && type_args.len() == 1 {
                            let mut inner = type_to_node(type_args[0]);
                            inner.ownership = Some(kind.to_string());
                            return inner;
                        }
                    }
                }
            }
            let parts: Vec<String> = tp
                .path
                .segments
                .iter()
                .map(|seg| seg.ident.to_string())
                .collect();
            TypeNode {
                name_parts: parts,
                ..Default::default()
            }
        }
        syn::Type::Reference(r) => {
            let mut node = type_to_node(&r.elem);
            if r.mutability.is_some() {
                node.name_parts.insert(0, "&mut".to_string());
            } else {
                node.name_parts.insert(0, "&".to_string());
            }
            node
        }
        syn::Type::Tuple(t) if t.elems.is_empty() => TypeNode {
            name_parts: vec!["()".to_string()],
            ..Default::default()
        },
        syn::Type::Slice(s) => {
            let inner = type_to_node(&s.elem);
            TypeNode {
                name_parts: vec![format!("[{}]", inner.name_parts.join("::"))],
                ..Default::default()
            }
        }
        syn::Type::Array(a) => {
            let inner = type_to_node(&a.elem);
            TypeNode {
                name_parts: vec![format!("[{}; N]", inner.name_parts.join("::"))],
                ..Default::default()
            }
        }
        _ => TypeNode {
            name_parts: vec![quote::quote!(#ty).to_string()],
            ..Default::default()
        },
    }
}

fn return_type_node(ret: &syn::ReturnType) -> TypeNode {
    match ret {
        syn::ReturnType::Default => TypeNode {
            name_parts: vec!["void".to_string()],
            ..Default::default()
        },
        syn::ReturnType::Type(_, ty) => type_to_node(ty),
    }
}

// ---------------------------------------------------------------------------
// syn::Expr -> JSON statement/expression
// ---------------------------------------------------------------------------

fn convert_expr(expr: &syn::Expr) -> serde_json::Value {
    match expr {
        syn::Expr::Lit(lit) => convert_lit(&lit.lit),

        syn::Expr::Path(ep) => {
            let name: Vec<String> = ep
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            serde_json::json!({
                "kind": "varref",
                "name": name.join("::")
            })
        }

        syn::Expr::Binary(bin) => {
            // Check for compound assignment operators
            if let Some(base_op) = compound_assign_op(&bin.op) {
                serde_json::json!({
                    "kind": "compoundassign",
                    "op": base_op,
                    "target": convert_expr(&bin.left),
                    "value": convert_expr(&bin.right)
                })
            } else {
                let op = binop_str(&bin.op);
                serde_json::json!({
                    "kind": "binaryop",
                    "op": op,
                    "lhs": convert_expr(&bin.left),
                    "rhs": convert_expr(&bin.right)
                })
            }
        }

        syn::Expr::Unary(un) => {
            let op = unaryop_str(&un.op);
            serde_json::json!({
                "kind": "unaryop",
                "op": op,
                "operand": convert_expr(&un.expr)
            })
        }

        syn::Expr::Call(call) => {
            // CallExpr.callee is a `std::string` in the C++ model
            // (qualified name), NOT a nested Expr. Render the callee
            // textually via syn's quote pretty-printer for unambiguous
            // function-path shape (`a::b::c`, `obj.method`, etc.); host
            // emitters parse the dots/colons themselves. `quote!`
            // interpolates a single named variable via `#name`, so
            // dereference call.func into a local first.
            let func = &*call.func;
            let callee = quote::quote!(#func).to_string();
            let args: Vec<serde_json::Value> = call.args.iter().map(convert_expr).collect();
            serde_json::json!({
                "kind": "call",
                "callee": callee,
                "args": args
            })
        }

        syn::Expr::MethodCall(mc) => {
            // Method call collapses to a "receiver.method" textual
            // callee (same convention as the TS extractor's
            // PropertyAccessExpression-based call). The receiver text
            // is the source-spelled expression; args lift normally.
            let receiver_expr = &*mc.receiver;
            let receiver = quote::quote!(#receiver_expr).to_string();
            let callee = format!("{}.{}", receiver, mc.method);
            let args: Vec<serde_json::Value> = mc.args.iter().map(convert_expr).collect();
            serde_json::json!({
                "kind": "call",
                "callee": callee,
                "args": args
            })
        }

        syn::Expr::Field(f) => {
            let obj = convert_expr(&f.base);
            let member = match &f.member {
                syn::Member::Named(ident) => ident.to_string(),
                syn::Member::Unnamed(idx) => idx.index.to_string(),
            };
            serde_json::json!({
                "kind": "memberaccess",
                "object": obj,
                "member": member
            })
        }

        syn::Expr::Index(idx) => {
            let obj = convert_expr(&idx.expr);
            let index = convert_expr(&idx.index);
            serde_json::json!({
                "kind": "index",
                "object": obj,
                "index": index
            })
        }

        syn::Expr::Struct(es) => {
            let type_name: Vec<String> = es
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let fields: Vec<serde_json::Value> = es
                .fields
                .iter()
                .map(|f| {
                    let field_name = match &f.member {
                        syn::Member::Named(ident) => ident.to_string(),
                        syn::Member::Unnamed(idx) => idx.index.to_string(),
                    };
                    serde_json::json!({
                        "name": field_name,
                        "value": convert_expr(&f.expr)
                    })
                })
                .collect();
            serde_json::json!({
                "kind": "construct",
                "typeName": type_name.join("::"),
                "fields": fields
            })
        }

        syn::Expr::Assign(a) => {
            serde_json::json!({
                "kind": "assign",
                "target": convert_expr(&a.left),
                "value": convert_expr(&a.right)
            })
        }

        syn::Expr::Return(r) => {
            let value = r.expr.as_ref().map(|e| convert_expr(e));
            serde_json::json!({
                "kind": "return",
                "value": value
            })
        }

        syn::Expr::If(ei) => convert_if_expr(ei),

        syn::Expr::While(w) => {
            let cond = convert_expr(&w.cond);
            let body = convert_block(&w.body);
            serde_json::json!({
                "kind": "while",
                "condition": cond,
                "body": body
            })
        }

        syn::Expr::ForLoop(fl) => {
            let iter_expr = convert_expr(&fl.expr);
            let body = convert_block(&fl.body);
            let var = pat_name(&fl.pat);
            serde_json::json!({
                "kind": "for",
                "variable": var,
                "iterable": iter_expr,
                "body": body
            })
        }

        syn::Expr::Block(_eb) => {
            // Rust expression-blocks (`{ stmt1; stmt2; tail }`) have no
            // direct TranspileModel equivalent — the model's expressions
            // don't carry a statement list. Surface as Unsupported so
            // downstream emitters preserve a stable error shape (and the
            // C++ deserializer recognises the kind). The full sequence is
            // available to a future lowering pass.
            serde_json::json!({
                "kind": "unsupported",
                "description": "Rust expression-block"
            })
        }

        syn::Expr::Paren(p) => convert_expr(&p.expr),

        syn::Expr::Reference(r) => {
            let inner = convert_expr(&r.expr);
            let op = if r.mutability.is_some() {
                "&mut"
            } else {
                "&"
            };
            serde_json::json!({
                "kind": "unaryop",
                "op": op,
                "operand": inner
            })
        }

        syn::Expr::Cast(c) => {
            // Rust `expr as T` has no TranspileModel equivalent; mark as
            // Unsupported with the textual target type for downstream
            // diagnosis. (A future lowering could synthesise a Call to a
            // host-specific cast helper, but the contract today is "lift
            // to a kind the C++ deserializer recognises or surface as
            // unsupported".)
            let ty = type_to_node(&c.ty);
            let desc = format!("Rust `as` cast to {}", ty.name_parts.join("::"));
            serde_json::json!({
                "kind": "unsupported",
                "description": desc
            })
        }

        syn::Expr::Break(_) => serde_json::json!({
            "kind": "break"
        }),

        syn::Expr::Continue(_) => serde_json::json!({
            "kind": "continue"
        }),

        syn::Expr::Range(r) => {
            let start = r.start.as_ref().map(|e| convert_expr(e));
            let end = r.end.as_ref().map(|e| convert_expr(e));
            serde_json::json!({
                "kind": "unsupported",
                "description": "range expression",
                "start": start,
                "end": end
            })
        }

        syn::Expr::Closure(_) => serde_json::json!({
            "kind": "unsupported",
            "description": "closure expression"
        }),

        syn::Expr::Match(m) => {
            let subject = convert_expr(&m.expr);
            let cases: Vec<serde_json::Value> = m
                .arms
                .iter()
                .map(|arm| {
                    let value = convert_pat(&arm.pat);
                    let body_expr = convert_expr(&arm.body);
                    // Wrap the body expression as a single-element body array
                    let body = vec![body_expr];
                    serde_json::json!({
                        "value": value,
                        "body": body
                    })
                })
                .collect();
            serde_json::json!({
                "kind": "switch",
                "subject": subject,
                "cases": cases
            })
        }

        syn::Expr::Await(_) => serde_json::json!({
            "kind": "unsupported",
            "description": "await expression"
        }),

        syn::Expr::Try(_) => serde_json::json!({
            "kind": "unsupported",
            "description": "try (?) expression"
        }),

        syn::Expr::Macro(m) => {
            let macro_name = m
                .mac
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            serde_json::json!({
                "kind": "unsupported",
                "description": format!("macro invocation: {}!", macro_name)
            })
        }

        _ => serde_json::json!({
            "kind": "unsupported",
            "description": format!("unhandled expression: {}", quote::quote!(#expr))
        }),
    }
}

fn convert_if_expr(ei: &syn::ExprIf) -> serde_json::Value {
    let cond = convert_expr(&ei.cond);
    let then_body = convert_block(&ei.then_branch);
    let else_body = ei.else_branch.as_ref().map(|(_, e)| {
        if let syn::Expr::If(nested) = e.as_ref() {
            vec![convert_if_expr(nested)]
        } else if let syn::Expr::Block(eb) = e.as_ref() {
            convert_block(&eb.block)
        } else {
            vec![convert_expr(e)]
        }
    });
    serde_json::json!({
        "kind": "if",
        "condition": cond,
        "thenBody": then_body,
        "elseBody": else_body
    })
}

fn convert_lit(lit: &syn::Lit) -> serde_json::Value {
    match lit {
        syn::Lit::Int(i) => serde_json::json!({
            "kind": "literal",
            "litKind": "int",
            "value": i.base10_digits()
        }),
        syn::Lit::Float(f) => serde_json::json!({
            "kind": "literal",
            "litKind": "float",
            "value": f.base10_digits()
        }),
        syn::Lit::Str(s) => serde_json::json!({
            "kind": "literal",
            "litKind": "string",
            "value": s.value()
        }),
        syn::Lit::Bool(b) => serde_json::json!({
            "kind": "literal",
            "litKind": "bool",
            "value": if b.value { "true" } else { "false" }
        }),
        syn::Lit::Char(c) => serde_json::json!({
            "kind": "literal",
            "litKind": "char",
            "value": c.value().to_string()
        }),
        _ => serde_json::json!({
            "kind": "literal",
            "litKind": "unknown",
            "value": quote::quote!(#lit).to_string()
        }),
    }
}

fn convert_stmt(stmt: &syn::Stmt) -> serde_json::Value {
    match stmt {
        syn::Stmt::Local(local) => {
            let name = pat_name(&local.pat);
            let ty = pat_type(&local.pat);
            let init = local.init.as_ref().map(|li| convert_expr(&li.expr));
            serde_json::json!({
                "kind": "vardecl",
                "name": name,
                "type": ty,
                "init": init
            })
        }
        syn::Stmt::Expr(expr, semi) => {
            // Distinguish statement-like exprs (which the converter
            // returns already in Stmt shape — if / while / for / match
            // / assign / break / continue / return) from value-like
            // exprs (binaryop, call, varref, …). Statement-likes are
            // passed through unchanged; value-likes need to be wrapped
            // in an ExprStmt (when followed by a semicolon) or a
            // Return (when the final trailing expression supplies the
            // function's implicit return).
            let stmt_like = matches!(
                expr,
                syn::Expr::Assign(_)
                    | syn::Expr::Return(_)
                    | syn::Expr::If(_)
                    | syn::Expr::While(_)
                    | syn::Expr::ForLoop(_)
                    | syn::Expr::Break(_)
                    | syn::Expr::Continue(_)
                    | syn::Expr::Match(_)
                    | syn::Expr::Block(_)
            );
            let is_compound_assign = matches!(
                expr,
                syn::Expr::Binary(bin) if compound_assign_op(&bin.op).is_some());
            let converted = convert_expr(expr);
            if stmt_like {
                // Block-like / control-flow expressions are statements
                // in this position, regardless of trailing semicolon.
                // Rust's `if cond { return n; }` (no trailing semi) is
                // NOT an implicit return — it's a unit-valued statement.
                converted
            } else if semi.is_some() {
                if is_compound_assign {
                    // Compound-assign was already produced in stmt shape
                    // by convert_expr; pass through.
                    converted
                } else {
                    serde_json::json!({
                        "kind": "exprstmt",
                        "expr": converted
                    })
                }
            } else {
                // Trailing value-like expression → implicit return.
                serde_json::json!({
                    "kind": "return",
                    "value": converted
                })
            }
        }
        // Stmt-context Unsupported must be wrapped in `exprstmt`:
        // `unsupported` is an Expr kind in the C++ model, not a Stmt kind,
        // and the C++ deserializer would treat a bare `unsupported`-kinded
        // statement as ExprStmt then crash on the missing `expr` key.
        // Same wrap applies to macro statements below.
        syn::Stmt::Item(_) => serde_json::json!({
            "kind": "exprstmt",
            "expr": {
                "kind": "unsupported",
                "description": "item inside function body"
            }
        }),
        syn::Stmt::Macro(m) => {
            let macro_name = m
                .mac
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            serde_json::json!({
                "kind": "exprstmt",
                "expr": {
                    "kind": "unsupported",
                    "description": format!("macro statement: {}!", macro_name)
                }
            })
        }
    }
}

fn convert_block(block: &syn::Block) -> Vec<serde_json::Value> {
    block.stmts.iter().map(convert_stmt).collect()
}

// ---------------------------------------------------------------------------
// Pattern helpers
// ---------------------------------------------------------------------------

fn pat_name(pat: &syn::Pat) -> String {
    match pat {
        syn::Pat::Ident(pi) => pi.ident.to_string(),
        syn::Pat::Type(pt) => pat_name(&pt.pat),
        syn::Pat::Wild(_) => "_".to_string(),
        _ => "_".to_string(),
    }
}

fn pat_type(pat: &syn::Pat) -> Option<TypeNode> {
    match pat {
        syn::Pat::Type(pt) => Some(type_to_node(&pt.ty)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Operator string mappings
// ---------------------------------------------------------------------------

/// Returns the base operator string for compound assignment operators, or None
/// if the operator is not a compound assignment.
fn compound_assign_op(op: &syn::BinOp) -> Option<&'static str> {
    match op {
        syn::BinOp::AddAssign(_) => Some("+"),
        syn::BinOp::SubAssign(_) => Some("-"),
        syn::BinOp::MulAssign(_) => Some("*"),
        syn::BinOp::DivAssign(_) => Some("/"),
        syn::BinOp::RemAssign(_) => Some("%"),
        syn::BinOp::BitAndAssign(_) => Some("&"),
        syn::BinOp::BitOrAssign(_) => Some("|"),
        syn::BinOp::BitXorAssign(_) => Some("^"),
        syn::BinOp::ShlAssign(_) => Some("<<"),
        syn::BinOp::ShrAssign(_) => Some(">>"),
        _ => None,
    }
}

/// Convert a match arm pattern to a JSON value expression for SwitchCase.
fn convert_pat(pat: &syn::Pat) -> serde_json::Value {
    match pat {
        syn::Pat::Ident(pi) => {
            // A catch-all binding (like `x`) or a named pattern
            serde_json::json!({
                "kind": "varref",
                "name": pi.ident.to_string()
            })
        }
        syn::Pat::Wild(_) => serde_json::Value::Null, // default case (underscore _)
        syn::Pat::Or(po) => {
            // Multiple patterns (a | b): use the first one for the case value
            if let Some(first) = po.cases.first() {
                convert_pat(first)
            } else {
                serde_json::Value::Null
            }
        }
        syn::Pat::Path(pp) => {
            let name: Vec<String> = pp
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            serde_json::json!({
                "kind": "varref",
                "name": name.join("::")
            })
        }
        syn::Pat::Lit(pl) => convert_lit(&pl.lit),
        syn::Pat::Const(pc) => {
            // In syn 2, literal patterns in match arms may appear as Pat::Const
            let stmts = convert_block(&pc.block);
            if stmts.len() == 1 {
                stmts.into_iter().next().unwrap()
            } else {
                serde_json::json!({
                    "kind": "unsupported",
                    "description": "complex const pattern"
                })
            }
        }
        _ => serde_json::json!({
            "kind": "unsupported",
            "description": format!("match pattern: {}", quote::quote!(#pat))
        }),
    }
}

// The C++ deserializer encodes BinaryOp / UnaryOp as named words, not
// symbolic operators (`add`, `sub`, `mul`, `eq`, `less`, … — see
// `from_json(BinaryOp&)` in topo-core/lib/Transpile/TranspileModelJson.cpp).
// The Rust extractor historically emitted symbolic strings (`+`, `<`,
// `>>`); that worked against its own frozen fidelity goldens but the
// C++ side silently mis-mapped `<` → unknown, and `>>` → Shr (visible
// as `i >> n` showing up where `i < n` was expected). Word-form
// matches the C++ deserialiser end-to-end.
fn binop_str(op: &syn::BinOp) -> &'static str {
    match op {
        syn::BinOp::Add(_) => "add",
        syn::BinOp::Sub(_) => "sub",
        syn::BinOp::Mul(_) => "mul",
        syn::BinOp::Div(_) => "div",
        syn::BinOp::Rem(_) => "mod",
        syn::BinOp::And(_) => "and",
        syn::BinOp::Or(_) => "or",
        syn::BinOp::BitXor(_) => "bitxor",
        syn::BinOp::BitAnd(_) => "bitand",
        syn::BinOp::BitOr(_) => "bitor",
        syn::BinOp::Shl(_) => "shl",
        syn::BinOp::Shr(_) => "shr",
        syn::BinOp::Eq(_) => "eq",
        syn::BinOp::Lt(_) => "less",
        syn::BinOp::Le(_) => "lesseq",
        syn::BinOp::Ne(_) => "noteq",
        syn::BinOp::Ge(_) => "greatereq",
        syn::BinOp::Gt(_) => "greater",
        // Compound-assign forms are recognised by compound_assign_op
        // and split off before binop_str is called, but kept here for
        // completeness so unexpected shapes surface as Unsupported
        // rather than mis-tag as a regular op.
        syn::BinOp::AddAssign(_) => "add",
        syn::BinOp::SubAssign(_) => "sub",
        syn::BinOp::MulAssign(_) => "mul",
        syn::BinOp::DivAssign(_) => "div",
        syn::BinOp::RemAssign(_) => "mod",
        syn::BinOp::BitXorAssign(_) => "bitxor",
        syn::BinOp::BitAndAssign(_) => "bitand",
        syn::BinOp::BitOrAssign(_) => "bitor",
        syn::BinOp::ShlAssign(_) => "shl",
        syn::BinOp::ShrAssign(_) => "shr",
        _ => "add",
    }
}

// UnaryOp string vocabulary — same word-form convention. The C++ side
// recognises `negate`, `not`, `bitnot`, `preincrement`, `postincrement`,
// `predecrement`, `postdecrement`. Rust source has no increment/decrement
// operators (`++`/`--` aren't in the language), so the extractor only
// produces the negate / not / bitnot triple. `&` and `&mut` are
// borrow-of-place expressions in Rust, modelled here as a unary op for
// the wire shape's sake; downstream emitters can render them as `&`.
fn unaryop_str(op: &syn::UnOp) -> &'static str {
    match op {
        // Deref `*x` has no Topo UnaryOp; surface as Not to avoid a
        // crash but expect downstream to flag (rare in the bodies the
        // equivalence suite exercises).
        syn::UnOp::Deref(_) => "not",
        syn::UnOp::Not(_) => "not",
        syn::UnOp::Neg(_) => "negate",
        _ => "not",
    }
}

// ---------------------------------------------------------------------------
// Module traversal: collect functions with qualified names
// ---------------------------------------------------------------------------

struct FunctionCollector {
    /// Maps qualified name -> (function signature info, block body)
    functions: HashMap<String, CollectedFn>,
    /// Struct/enum declarations lifted to TranspileType entries, in source
    /// order. Kept as a Vec (not HashMap) so the test goldens see a stable
    /// declaration order; the normaliser sorts by qualifiedName before
    /// comparison anyway, but stable in-order output keeps the wire payload
    /// readable.
    types: Vec<TranspileType>,
}

struct CollectedFn {
    params: Vec<ParamDecl>,
    return_type: TypeNode,
    body: Vec<serde_json::Value>,
    // Recovered bare type-parameter names (`<T, U>`); kind is always "type".
    template_params: Vec<TemplateParam>,
    // Human-readable notes for generics features outside the MVP (bounds,
    // defaults, lifetimes, const generics). Non-empty ⇒ fidelity downgraded.
    unsupported: Vec<String>,
}

// Convert a single `TypeParamBound` into the wire TypeNode (or an
// unsupported reason). Handles:
// - Trait bound (`Trait`) — `path_to_type_node` does the heavy lifting,
//   including parenthesised Fn-trait inputs / output recovery.
// - HRTB on a Trait bound (`for<'a> Trait`) — `tb.lifetimes` flows into the
//   resulting TypeNode's `hrtb_lifetimes` (sans-apostrophe); combined
//   with parenthesised Fn-trait it round-trips both the HRTB prefix and
//   the input/output types via the emitter.
// - Lifetime bound (`'a`) — surfaced as a TypeNode whose nameParts[0] keeps
//   the apostrophe, matching the lifetime-param wire shape.
fn trait_bound_to_node(b: &syn::TypeParamBound) -> Result<TypeNode, &'static str> {
    match b {
        syn::TypeParamBound::Trait(tb) => {
            if let Some(reason) = path_last_segment_unsupported_kind(&tb.path) {
                return Err(reason);
            }
            let has_hrtb = tb
                .lifetimes
                .as_ref()
                .map_or(false, |bl| !bl.lifetimes.is_empty());
            // Out of scope: HRTB combined with *explicit* (angle-
            // bracketed) associated-type bindings is dropped to a
            // conservative downgrade. The implicit Output binding
            // synthesised from parenthesised Fn-trait `Fn(A) -> B` is
            // in-scope (re-rendered parenthesised by RustEmitter) and
            // does NOT trigger this guard — the check looks for
            // angle-bracketed AssocType args specifically.
            if has_hrtb {
                if let Some(last) = tb.path.segments.last() {
                    if let syn::PathArguments::AngleBracketed(ab) = &last.arguments {
                        let has_explicit_assoc = ab.args.iter().any(|a| {
                            matches!(a, syn::GenericArgument::AssocType(_))
                        });
                        if has_explicit_assoc {
                            return Err("hrtb-with-assoc-binding");
                        }
                    }
                }
            }
            let mut node = path_to_type_node(&tb.path);
            if has_hrtb {
                if let Some(bl) = &tb.lifetimes {
                    for gp in &bl.lifetimes {
                        if let syn::GenericParam::Lifetime(lp) = gp {
                            node.hrtb_lifetimes.push(lp.lifetime.ident.to_string());
                        }
                    }
                }
            }
            Ok(node)
        }
        syn::TypeParamBound::Lifetime(lt) => Ok(TypeNode {
            name_parts: vec![format!("'{}", lt.ident)],
            ..Default::default()
        }),
        _ => Err("unrecognised-type-param-bound"),
    }
}

// Walks `generics.where_clause.predicates` and pulls out every
// `WherePredicate::Type` whose `bounded_ty` is a bare single-ident path —
// that ident names a type param declared in `generics.params`, so its bounds
// can be merged onto that param. Returns a map ident -> bounds. Predicates
// the current MVP cannot recover (HRTB lifetimes on the predicate itself,
// non-ident bounded types, etc.) are returned as `unsupported` reasons keyed
// by the bounded type's textual form.
//
// The fixture target `pub fn map<F>(f: F) where F: for<'a> Fn(&'a u8) -> &'a u8`
// goes through this path: the predicate `F: for<'a> Fn(...)` is collected
// against ident "F" and merged in by `collect_generics` below.
fn collect_where_clause_bounds(
    generics: &syn::Generics,
) -> (
    std::collections::HashMap<String, Vec<syn::TypeParamBound>>,
    Vec<String>,
) {
    let mut by_ident: std::collections::HashMap<String, Vec<syn::TypeParamBound>> =
        std::collections::HashMap::new();
    let mut unsupported = Vec::new();
    let Some(wc) = &generics.where_clause else {
        return (by_ident, unsupported);
    };
    for pred in &wc.predicates {
        match pred {
            syn::WherePredicate::Type(pt) => {
                // HRTB-on-predicate (`for<'a> T: Trait`) is rare in modern
                // Rust style — `for<'a>` sits on the trait bound directly
                // (`T: for<'a> Trait`). The MVP captures only the
                // trait-bound-level HRTB and surfaces a note when the
                // predicate-level HRTB form appears.
                if pt.lifetimes.is_some() {
                    unsupported.push(
                        "where-clause predicate carries HRTB lifetimes at the predicate level (only trait-bound-level `for<...>` recovered; predicate-level dropped)".to_string(),
                    );
                    continue;
                }
                // Recover only `Ident: ...` predicates — those map back to
                // a declared type param. Anything else (`<T as Trait>::X:
                // ...`, `&T: ...`, etc.) stays unrecovered with a note.
                let syn::Type::Path(tp) = &pt.bounded_ty else {
                    unsupported.push(
                        "where-clause predicate has a non-path bounded type (dropped)".to_string(),
                    );
                    continue;
                };
                if tp.qself.is_some() || tp.path.segments.len() != 1 {
                    unsupported.push(
                        "where-clause predicate's bounded type is not a single ident (dropped)"
                            .to_string(),
                    );
                    continue;
                }
                let ident_seg = &tp.path.segments[0];
                if !matches!(ident_seg.arguments, syn::PathArguments::None) {
                    unsupported.push(
                        "where-clause predicate's bounded ident carries generic args (dropped)"
                            .to_string(),
                    );
                    continue;
                }
                let entry = by_ident
                    .entry(ident_seg.ident.to_string())
                    .or_insert_with(Vec::new);
                for b in &pt.bounds {
                    entry.push(b.clone());
                }
            }
            syn::WherePredicate::Lifetime(_) => {
                unsupported.push(
                    "where-clause lifetime predicate dropped (not recovered)".to_string(),
                );
            }
            _ => {
                unsupported
                    .push("where-clause predicate of unrecognised kind dropped".to_string());
            }
        }
    }
    (by_ident, unsupported)
}

// Extract the source-literal spelling of a `<const N: usize = <expr>>`
// default expression. Returns `Some(string)` for the literal forms the
// MVP accepts; returns `None` for anything else so the caller can record
// a conservative drop. Accepted forms:
// - Integer / float / char / byte / string literals (`16`, `0x1F`, `1u32`,
//   `'a'`): rendered as the source token spelling via `quote::ToTokens`.
// - Bool literals (`true`, `false`): same path.
// - A leading unary `-` or `+` followed by a literal (`-1`, `+0`).
// - Scoped enum literal path (`Color::Red`, `ns::E::Val`) — surfaced as
//   the path joined by `::`.
//
// Returning the source spelling rather than the parsed value keeps the
// wire shape language-agnostic; emitters consume the string verbatim.
fn extract_const_default_literal(expr: &syn::Expr) -> Option<String> {
    use quote::ToTokens;
    match expr {
        // Pure literal (`16`, `0x1F`, `true`, `'a'`, …).
        syn::Expr::Lit(el) => Some(el.lit.to_token_stream().to_string()),
        // Leading unary `-` / `+` on a literal (e.g. `-1`).
        syn::Expr::Unary(syn::ExprUnary { op, expr: inner, .. }) => {
            let prefix = match op {
                syn::UnOp::Neg(_) => "-",
                _ => return None,
            };
            if let syn::Expr::Lit(el) = inner.as_ref() {
                let lit_str = el.lit.to_token_stream().to_string();
                return Some(format!("{}{}", prefix, lit_str));
            }
            None
        }
        // Path expression — accept only identifier-only path segments
        // joined by `::` (scoped enum literal: `Color::Red`,
        // `ns::Enum::Val`). Any path with generic args or leading colons
        // beyond the MVP is rejected to keep the contract narrow.
        syn::Expr::Path(ep) if ep.qself.is_none() => {
            let mut parts: Vec<String> = Vec::with_capacity(ep.path.segments.len());
            for seg in &ep.path.segments {
                if !matches!(seg.arguments, syn::PathArguments::None) {
                    return None;
                }
                parts.push(seg.ident.to_string());
            }
            if parts.is_empty() {
                return None;
            }
            Some(parts.join("::"))
        }
        _ => None,
    }
}

// Recovers declaration-level generics from a `syn::Generics`. Scope:
// - Type params with bare-Trait bounds (single / multi), defaults, and Rust
//   lifetime bounds (`T: 'a` and `T: Trait + 'a`).
// - Const generics (`<const N: usize>`) as kind="nontype", optionally with
//   literal default `<const N: usize = 16>` (integer / bool / enum literal).
// - Lifetime params (`'a`), with optional outlives clauses (`'a: 'b`),
//   emitted as kind="lifetime" entries. The wire `name` carries the
//   lifetime label WITHOUT the leading apostrophe (e.g. `"a"`); the
//   outlives target rides the `bound` TypeNode with the apostrophe kept
//   (`["'b"]`) so a type-param lifetime-bound entry and a lifetime
//   outlives entry share the same `'<name>` spelling at the wire level.
// - HRTB on a trait bound (`for<'a, 'b> Trait`) — recovered as
//   `TypeNode.hrtb_lifetimes` on the bound TypeNode. Supports both
//   inline (`<F: for<'a> Trait>`) and where-clause
//   (`where F: for<'a> Trait`) forms.
fn collect_generics(generics: &syn::Generics) -> (Vec<TemplateParam>, Vec<String>) {
    let mut params = Vec::new();
    let mut unsupported = Vec::new();
    let (where_by_ident, mut wc_unsupported) = collect_where_clause_bounds(generics);
    unsupported.append(&mut wc_unsupported);
    for gp in &generics.params {
        match gp {
            syn::GenericParam::Type(tp) => {
                let name = tp.ident.to_string();
                // Trait-bound MVP + HRTB: `<T: Trait>` captures into
                // the wire `bound` (single) and `<T: A + B>` captures into
                // the wire `bounds` (multi). HRTB (`for<'a>` prefix on a
                // trait bound) populates the bound TypeNode's
                // `hrtb_lifetimes`. Where-clause predicates on the same
                // ident are merged in after the inline bounds.
                let mut bound: Option<TypeNode> = None;
                let mut bounds: Option<Vec<TypeNode>> = None;
                // Collect every Trait bound's TypeNode + lifetime bound's
                // `'<name>` TypeNode. Associated-type bindings
                // (`Iterator<Item = u8>`) are recovered into the bound
                // TypeNode's `assoc_bindings`. Truly unsupported forms
                // (assoc-type-bounds, assoc consts, etc.) drop the WHOLE
                // bound group + flag unsupported so we never emit a partial
                // / widened contract.
                let mut all_nodes: Vec<TypeNode> = Vec::new();
                let mut unsupported_reason: Option<&'static str> = None;
                let inline_bounds_iter = tp.bounds.iter().cloned();
                let merged_bounds: Vec<syn::TypeParamBound> = inline_bounds_iter
                    .chain(
                        where_by_ident
                            .get(&name)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter(),
                    )
                    .collect();
                for b in &merged_bounds {
                    match trait_bound_to_node(b) {
                        Ok(node) => all_nodes.push(node),
                        Err(reason) => {
                            unsupported_reason = Some(reason);
                            break;
                        }
                    }
                }
                if let Some(reason) = unsupported_reason {
                    unsupported.push(format!(
                        "generic type parameter `{}` has unsupported trait-bound argument ({}); bound dropped",
                        name, reason
                    ));
                } else if all_nodes.len() == 1 {
                    bound = Some(all_nodes.pop().unwrap());
                } else if all_nodes.len() >= 2 {
                    bounds = Some(all_nodes);
                }
                // Single trait-default MVP: capture `<T = Default>` into
                // the wire `default` field via the same `type_to_node` path
                // the rest of the model uses, so the default carries through
                // semantically. Rust syntax forbids defaults on free
                // functions; the emitter is responsible for honoring that
                // restriction (and downgrading on functions if a default is
                // present).
                let default = tp.default.as_ref().map(type_to_node);
                params.push(TemplateParam {
                    kind: "type".to_string(),
                    name,
                    bound,
                    bounds,
                    default,
                    default_value: None,
                });
            }
            syn::GenericParam::Lifetime(lp) => {
                // Emit kind="lifetime" entry positionally alongside type
                // params so the original `<'a, T>` ordering round-trips.
                // Outlives target `'a: 'b` rides the wire `bound` TypeNode
                // (`["'b"]` — apostrophe kept on outlives target). Multiple
                // outlives bounds `'a: 'b + 'c` are out of MVP scope; if
                // present, the FIRST is captured and the rest are dropped
                // with an unsupported note (no fixture in MVP exercises
                // multi-outlives but the contract stays narrow).
                let name = lp.lifetime.ident.to_string();
                let mut bound: Option<TypeNode> = None;
                let mut iter = lp.bounds.iter();
                if let Some(first) = iter.next() {
                    bound = Some(TypeNode {
                        name_parts: vec![format!("'{}", first.ident)],
                        ..Default::default()
                    });
                }
                if iter.next().is_some() {
                    unsupported.push(format!(
                        "lifetime parameter `'{}` has multiple outlives bounds (only the first captured, rest dropped)",
                        name
                    ));
                }
                params.push(TemplateParam {
                    kind: "lifetime".to_string(),
                    name,
                    bound,
                    bounds: None,
                    default: None,
                    default_value: None,
                });
            }
            syn::GenericParam::Const(cp) => {
                // Const generic `<const N: usize>` and `<const N: usize = 16>`:
                // surface as kind="nontype". constraintType carries the
                // value type (e.g. `usize`). For the default expression,
                // only accept literal forms — integer literal
                // (`Expr::Lit(LitInt|LitFloat|LitChar|LitByte)`), bool
                // literal (`LitBool`), and scoped-path enum literals
                // (`Expr::Path` resolving to a path). Anything else
                // (`= N+1`, `= sizeof::<T>()`, block expressions) drops
                // the default with a downgrade note while still emitting
                // the bare parameter.
                let mut default_value: Option<String> = None;
                if let Some(default_expr) = cp.default.as_ref() {
                    default_value = extract_const_default_literal(default_expr);
                    if default_value.is_none() {
                        unsupported.push(format!(
                            "const generic `{}` has non-literal default expression; default dropped (bare param kept)",
                            cp.ident
                        ));
                    }
                }
                params.push(TemplateParam {
                    kind: "nontype".to_string(),
                    name: cp.ident.to_string(),
                    bound: Some(type_to_node(&cp.ty)),
                    bounds: None,
                    default: None,
                    default_value,
                });
            }
        }
    }
    (params, unsupported)
}

impl FunctionCollector {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            types: Vec::new(),
        }
    }

    fn visit_file(&mut self, file: &syn::File) {
        let prefix = Vec::new();
        for item in &file.items {
            self.visit_item(item, &prefix);
        }
    }

    fn visit_item(&mut self, item: &syn::Item, ns: &[String]) {
        match item {
            syn::Item::Fn(f) => {
                let name = f.sig.ident.to_string();
                let mut qname_parts = ns.to_vec();
                qname_parts.push(name);
                let qualified = qname_parts.join("::");

                let params = f
                    .sig
                    .inputs
                    .iter()
                    .filter_map(|arg| match arg {
                        syn::FnArg::Typed(pat_type) => {
                            let name = pat_name(&pat_type.pat);
                            let ty = type_to_node(&pat_type.ty);
                            Some(ParamDecl {
                                name,
                                param_type: ty,
                            })
                        }
                        syn::FnArg::Receiver(_) => None,
                    })
                    .collect();

                let ret = return_type_node(&f.sig.output);
                let body = convert_block(&f.block);
                let (template_params, unsupported) = collect_generics(&f.sig.generics);

                self.functions.insert(
                    qualified,
                    CollectedFn {
                        params,
                        return_type: ret,
                        body,
                        template_params,
                        unsupported,
                    },
                );
            }
            syn::Item::Mod(m) => {
                let mod_name = m.ident.to_string();
                let mut child_ns = ns.to_vec();
                child_ns.push(mod_name);
                if let Some((_, items)) = &m.content {
                    for item in items {
                        self.visit_item(item, &child_ns);
                    }
                }
            }
            syn::Item::Struct(s) => {
                // Lift `pub struct Foo<T> { f: T }` into a TranspileType.
                // Rust has no class inheritance, so baseClasses stays empty
                // (omitted from JSON via skip_serializing_if). Generics flow
                // through the existing collect_generics helper — bounds /
                // defaults / lifetimes / const generics downgrade fidelity to
                // "inferred" while bare type-param names still surface. Both
                // named fields and tuple-struct positional fields are
                // captured; a unit struct (`pub struct Foo;`) yields an empty
                // fields vector, matching the C++ TranspileType contract.
                let name = s.ident.to_string();
                let mut qname_parts = ns.to_vec();
                qname_parts.push(name);
                let qualified = qname_parts.join("::");

                let (template_params, type_unsupported) = collect_generics(&s.generics);
                let fidelity = if type_unsupported.is_empty() {
                    "source"
                } else {
                    "inferred"
                };

                let mut fields: Vec<FieldDecl> = Vec::new();
                match &s.fields {
                    syn::Fields::Named(named) => {
                        for f in &named.named {
                            let fname = f
                                .ident
                                .as_ref()
                                .map(|i| i.to_string())
                                .unwrap_or_default();
                            fields.push(FieldDecl {
                                name: fname,
                                field_type: type_to_node(&f.ty),
                                fidelity: "source".to_string(),
                            });
                        }
                    }
                    syn::Fields::Unnamed(unnamed) => {
                        // Tuple-struct positional fields surface as `0`, `1`,
                        // …, mirroring how the converter renders unnamed
                        // member access (`syn::Member::Unnamed`).
                        for (idx, f) in unnamed.unnamed.iter().enumerate() {
                            fields.push(FieldDecl {
                                name: idx.to_string(),
                                field_type: type_to_node(&f.ty),
                                fidelity: "source".to_string(),
                            });
                        }
                    }
                    syn::Fields::Unit => {}
                }

                self.types.push(TranspileType {
                    qualified_name: qualified,
                    fields,
                    base_classes: Vec::new(),
                    base_class_kinds: Vec::new(),
                    template_params,
                    fidelity: fidelity.to_string(),
                });
            }
            syn::Item::Impl(imp) => {
                // For impl blocks, use the type name as namespace
                if let syn::Type::Path(tp) = imp.self_ty.as_ref() {
                    let type_name: Vec<String> = tp
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect();
                    let mut child_ns = ns.to_vec();
                    child_ns.extend(type_name);
                    for item in &imp.items {
                        if let syn::ImplItem::Fn(method) = item {
                            let name = method.sig.ident.to_string();
                            let mut qname_parts = child_ns.clone();
                            qname_parts.push(name);
                            let qualified = qname_parts.join("::");

                            let params = method
                                .sig
                                .inputs
                                .iter()
                                .filter_map(|arg| match arg {
                                    syn::FnArg::Typed(pat_type) => {
                                        let pname = pat_name(&pat_type.pat);
                                        let ty = type_to_node(&pat_type.ty);
                                        Some(ParamDecl {
                                            name: pname,
                                            param_type: ty,
                                        })
                                    }
                                    syn::FnArg::Receiver(_) => None,
                                })
                                .collect();

                            let ret = return_type_node(&method.sig.output);
                            let body = convert_block(&method.block);
                            let (template_params, unsupported) =
                                collect_generics(&method.sig.generics);

                            self.functions.insert(
                                qualified,
                                CollectedFn {
                                    params,
                                    return_type: ret,
                                    body,
                                    template_params,
                                    unsupported,
                                },
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Exit codes used by the extractor. Stable enough for the parent
/// (``topo-check``) to special-case the bad-input arms separately from a
/// genuine internal panic.
const EXIT_OK: i32 = 0;
const EXIT_REQUEST_READ: i32 = 2;
const EXIT_REQUEST_PARSE: i32 = 3;
const EXIT_SERIALIZE: i32 = 4;

/// Per-file read cap. Caps the memory the extractor allocates per source
/// file at 8 MiB; an oversize file becomes a per-file ``unsupported``
/// entry instead of an OOM. The cap is overridable via the
/// ``TOPO_EXTRACT_RUST_MAX_FILE_BYTES`` env var to accommodate
/// legitimately giant generated sources (which a power user would already
/// be tuning anyway). The path-traversal hardening covered the
/// canonicalisation half; this cap closes the bound-memory half.
const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

fn max_file_bytes() -> u64 {
    std::env::var("TOPO_EXTRACT_RUST_MAX_FILE_BYTES")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_FILE_BYTES)
}

/// Emit a structured ``{"error": ..., "kind": ...}`` envelope on stdout
/// so the parent caller (``topo-check``) sees a parseable JSON object
/// rather than a Rust panic backtrace. Mirrors the per-file error shape
/// used by ``topo_extract_python.py``'s ``fileErrors`` array (so a
/// malformed stdin request never escapes as a panic backtrace).
fn emit_error_envelope(kind: &str, message: &str) {
    // serde_json::to_string is fallible only on non-Send/non-Sync types,
    // which str-only payloads cannot hit; ``unwrap_or_else`` keeps the
    // function infallible regardless.
    let envelope = serde_json::json!({
        "error": message,
        "kind": kind,
    });
    let s = serde_json::to_string(&envelope)
        .unwrap_or_else(|_| String::from(
            "{\"error\":\"failed to serialize error envelope\",\"kind\":\"serialize\"}"));
    println!("{}", s);
    eprintln!("topo-extract-rust: {}: {}", kind, message);
}

/// Pure entry that returns a structured exit code; ``main`` is a thin
/// wrapper so the body is testable and free of ``.expect()``.
fn run() -> i32 {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        emit_error_envelope("request-read",
            &format!("failed to read stdin: {}", e));
        return EXIT_REQUEST_READ;
    }

    let request: ExtractRequest = match serde_json::from_str(&input) {
        Ok(r) => r,
        Err(e) => {
            emit_error_envelope("request-parse",
                &format!("failed to parse JSON request from stdin: {}", e));
            return EXIT_REQUEST_PARSE;
        }
    };

    let requested: std::collections::HashSet<String> =
        request.functions.iter().cloned().collect();

    let mut all_functions: Vec<TranspileFunction> = Vec::new();
    let mut all_types: Vec<TranspileType> = Vec::new();
    let mut all_unsupported: Vec<String> = Vec::new();

    // Validate input at the system boundary: every path
    // read from the JSON stdin request is untrusted host-side input. The
    // ``TOPO_EXTRACT_ROOT`` env var lets the parent process (topo-check)
    // pin a workspace root the extractor must stay under. Absent the
    // env var we fall back to the current working directory — preserves
    // pre-hardening behaviour but every concrete path still goes through
    // ``safety::sanitize_path`` so traversal payloads (``../../etc/passwd``)
    // are rejected before the open syscall.
    let extract_root: std::path::PathBuf = std::env::var_os("TOPO_EXTRACT_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));

    let cap = max_file_bytes();

    for file_path in &request.files {
        let safe_path = match safety::sanitize_path(file_path, &extract_root) {
            Ok(p) => p,
            Err(e) => {
                all_unsupported.push(format!("rejected file path '{}': {}", file_path, e));
                continue;
            }
        };

        // File-size cap before the read so an oversized source becomes a
        // structured per-file error instead of an OOM. Best-effort: a
        // metadata error simply falls through to the read, which has its
        // own error arm.
        if let Ok(md) = std::fs::metadata(&safe_path) {
            if md.len() > cap {
                all_unsupported.push(format!(
                    "rejected file path '{}': size {} bytes exceeds cap {} (set TOPO_EXTRACT_RUST_MAX_FILE_BYTES to raise)",
                    file_path, md.len(), cap));
                continue;
            }
        }

        let source = match std::fs::read_to_string(&safe_path) {
            Ok(s) => s,
            Err(e) => {
                all_unsupported.push(format!("failed to read {}: {}", file_path, e));
                continue;
            }
        };

        let parsed = match syn::parse_file(&source) {
            Ok(f) => f,
            Err(e) => {
                all_unsupported.push(format!("failed to parse {}: {}", file_path, e));
                continue;
            }
        };

        let mut collector = FunctionCollector::new();
        collector.visit_file(&parsed);

        // Type extraction is unconditional — the `request.functions` filter
        // targets function symbols only. A cross-language transpile may need
        // referenced struct types resolved even when the caller asks for a
        // specific function subset (same convention the Java extractor uses).
        for ty in collector.types {
            all_types.push(ty);
        }

        for (qname, collected) in collector.functions {
            // If specific functions requested, filter; otherwise include all
            if !requested.is_empty() && !requested.contains(&qname) {
                continue;
            }

            // Generics features outside the MVP downgrade fidelity to
            // "inferred" while still emitting the bare recovered type params.
            let fidelity = if collected.unsupported.is_empty() {
                "source"
            } else {
                "inferred"
            };
            all_functions.push(TranspileFunction {
                qualified_name: qname,
                return_type: collected.return_type,
                params: collected.params,
                body: collected.body,
                unsupported: collected.unsupported,
                fidelity: fidelity.to_string(),
                template_params: collected.template_params,
            });
        }
    }

    let module = TranspileModule {
        types: all_types,
        functions: all_functions,
    };

    let output = match serde_json::to_string(&module) {
        Ok(s) => s,
        Err(e) => {
            emit_error_envelope("serialize",
                &format!("failed to serialize TranspileModule: {}", e));
            return EXIT_SERIALIZE;
        }
    };
    println!("{}", output);

    // Report file-level errors to stderr
    for msg in &all_unsupported {
        eprintln!("topo-extract-rust: {}", msg);
    }
    EXIT_OK
}

fn main() {
    std::process::exit(run());
}
