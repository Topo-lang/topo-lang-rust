//! Compile-time static recognition of the topo-app Rust registration
//! surface.
//!
//! The static path never executes the user's program. It parses the
//! `.rs` source with `syn` (the same static-AST parser the existing
//! `topo-extract-rust` subprocess uses) and reconstructs the same
//! logic graph the runtime registration bridge would build at run time.
//!
//! ## The registration surface (Rust idiom projection)
//!
//! The topo-app design fixes the philosophy and lets each host project it onto
//! its own idioms (C++ templates, Python decorators, "Rust macro").
//! The Rust projection keeps registration as ordinary, statically
//! visible calls so a compile-time scan — not execution — recovers the
//! graph:
//!
//! ```ignore
//! topo::record! { OrderRec { id: i64, amount: f64 } }
//!
//! fn parse(raw: String) -> OrderRec { /* ... */ }
//! fn validate(order: OrderRec) -> OrderRec { /* ... */ }
//! fn persist(order: OrderRec) -> bool { /* ... */ }
//!
//! fn build_app() -> topo::App {
//!     let mut app = topo::App::new("orders");
//!     app.handler!(parse);
//!     app.handler!(validate);
//!     app.handler!(persist);
//!     app.flow!("order_pipeline", parse, validate, persist);
//!     app
//! }
//! ```
//!
//! In/Out types are read from the `fn` signatures (never re-declared),
//! exactly as the C++ style derives them from `function_traits` and the
//! Python style from `__annotations__`. `parallel(a, b)` inside a
//! `flow!` lists same-stage parallel candidates, matching the runtime
//! bridge's `topo.parallel(...)` mapping.

use std::collections::HashMap;

use syn::visit::Visit;

use crate::graph::{Edge, Flow, Graph, Handler, TypeRef};

/// A recognition failure. The static path refuses to emit a partial
/// graph: a correctness producer that silently drops handlers would
/// defeat the point, so every unrepresentable construct is a hard error
/// here, with the same intent as the core Parser / Python `_reflect`
/// guards.
#[derive(Debug, PartialEq, Eq)]
pub struct AnalyzeError(pub String);

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AnalyzeError {}

type Result<T> = std::result::Result<T, AnalyzeError>;

/// Map a Rust scalar type identifier to its stdlib `.topo` scalar alias.
///
/// The Rust host binds the same stdlib scalar vocabulary the rest of
/// the toolchain expects (the Rust handler/flow spec fixture spells
/// `i64` / `f64` / `bool` / `string`). Anything else cannot be a
/// handler In/Out (a handler's In/Out must be a stdlib schema type) and
/// is rejected rather than guessed.
fn scalar_alias(ident: &str) -> Option<&'static str> {
    match ident {
        "i64" | "i32" | "isize" | "u64" | "u32" | "usize" => Some("i64"),
        "f64" | "f32" => Some("f64"),
        "bool" => Some("bool"),
        "String" | "str" => Some("string"),
        _ => None,
    }
}

/// A `record! { Name { f: T, ... } }` declaration discovered statically.
#[derive(Debug, Clone)]
struct RecordDef {
    fields: Vec<(String, TypeRef)>,
}

/// Resolve a `syn::Type` to a `TypeRef`, using record definitions found
/// in the same source. Reference / generic / unsupported shapes are
/// rejected — a handler In/Out must be a stdlib scalar or a declared
/// record, mirroring the Python `_to_typeref` guard.
fn type_to_ref(ty: &syn::Type, records: &HashMap<String, RecordDef>) -> Result<TypeRef> {
    match ty {
        syn::Type::Path(tp) => {
            let ident = tp
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            if let Some(rec) = records.get(&ident) {
                return Ok(TypeRef::Record(rec.fields.clone()));
            }
            if let Some(alias) = scalar_alias(&ident) {
                return Ok(TypeRef::Scalar(alias.to_string()));
            }
            Err(AnalyzeError(format!(
                "unsupported handler type `{ident}`; use a stdlib scalar \
                 (i64/f64/bool/String) or a `record!`-declared type"
            )))
        }
        // A handler is a value-semantics Functor: references hide the
        // target data in a side effect (topo-app handler/flow model), so a
        // `&T` / `&mut T` In/Out is rejected, not silently unwrapped.
        syn::Type::Reference(_) => Err(AnalyzeError(
            "handler In/Out must be by value (value semantics, no \
             references) — aggregate into a `record!` type"
                .to_string(),
        )),
        other => Err(AnalyzeError(format!(
            "unsupported handler type `{}`",
            quote::quote!(#other)
        ))),
    }
}

/// Visitor over the parsed file: collects `record!` defs, free `fn`
/// signatures, `App::new(...)`, `app.handler!(...)`, `app.flow!(...)`.
#[derive(Default)]
struct SurfaceCollector {
    records: HashMap<String, RecordDef>,
    fn_sigs: HashMap<String, (Option<syn::Type>, syn::Type)>,
    namespace: Option<String>,
    /// Registered handler names, in source order.
    registered: Vec<String>,
    /// Flow name + ordered stages; each stage is one-or-more parallel
    /// member names.
    flow: Option<(String, Vec<Vec<String>>)>,
    error: Option<AnalyzeError>,
}

/// Parse a `record! { Name { field: Type, ... } }` token stream.
fn parse_record_macro(
    mac: &syn::Macro,
) -> Option<(String, RecordDef)> {
    // `record! { Ident { ident: Type, ... } }` — parse with syn so the
    // field types reuse the same type machinery as everything else.
    struct RecordMac {
        name: syn::Ident,
        fields: Vec<(syn::Ident, syn::Type)>,
    }
    impl syn::parse::Parse for RecordMac {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let name: syn::Ident = input.parse()?;
            let body;
            syn::braced!(body in input);
            let mut fields = Vec::new();
            while !body.is_empty() {
                let fname: syn::Ident = body.parse()?;
                body.parse::<syn::Token![:]>()?;
                let fty: syn::Type = body.parse()?;
                fields.push((fname, fty));
                if body.peek(syn::Token![,]) {
                    body.parse::<syn::Token![,]>()?;
                }
            }
            Ok(RecordMac { name, fields })
        }
    }
    let parsed: RecordMac = mac.parse_body().ok()?;
    let mut fields = Vec::new();
    for (fname, fty) in parsed.fields {
        // Record fields are scalar in the vertical slice (one nesting
        // level, matching the topo-app design's order example); resolve with an
        // empty record map so a non-scalar field surfaces as an error
        // rather than a silent miss.
        let tref = type_to_ref(&fty, &HashMap::new()).ok()?;
        fields.push((fname.to_string(), tref));
    }
    Some((parsed.name.to_string(), RecordDef { fields }))
}

/// Pull bare identifier arguments out of a macro/call token stream,
/// recognising `parallel(a, b, ...)` as a grouped (same-stage) entry.
/// The static path only needs the call *shape*, so a light token walk
/// over idents + the `parallel` group is sufficient and avoids coupling
/// to a not-yet-shipped runtime crate's exact types.
fn collect_stage_args(tokens: proc_macro2::TokenStream) -> Vec<Vec<String>> {
    use proc_macro2::TokenTree;

    let mut stages: Vec<Vec<String>> = Vec::new();
    let mut iter = tokens.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Literal(_) => {
                // The flow name literal; not a stage.
            }
            TokenTree::Ident(id) => {
                let name = id.to_string();
                if name == "parallel" {
                    // Next token is the `( ... )` group of members.
                    if let Some(TokenTree::Group(g)) = iter.peek() {
                        let members = g
                            .stream()
                            .into_iter()
                            .filter_map(|t| match t {
                                TokenTree::Ident(i) => Some(i.to_string()),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        iter.next(); // consume the group
                        if !members.is_empty() {
                            stages.push(members);
                        }
                    }
                } else {
                    stages.push(vec![name]);
                }
            }
            _ => {}
        }
    }
    stages
}

impl<'ast> Visit<'ast> for SurfaceCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if self.error.is_some() {
            return;
        }
        let name = node.sig.ident.to_string();
        // A handler is a pure Functor: at most one input. More inputs
        // are rejected with the same intent as the core Parser and the
        // Python `_reflect` guard.
        let typed_inputs: Vec<&syn::PatType> = node
            .sig
            .inputs
            .iter()
            .filter_map(|a| match a {
                syn::FnArg::Typed(pt) => Some(pt),
                syn::FnArg::Receiver(_) => None,
            })
            .collect();
        if typed_inputs.len() > 1 {
            self.error = Some(AnalyzeError(format!(
                "handler `{name}` has {} inputs; a handler is a pure \
                 Functor with at most one input — aggregate into a \
                 `record!` type",
                typed_inputs.len()
            )));
            return;
        }
        let in_ty = typed_inputs.first().map(|pt| (*pt.ty).clone());
        let out_ty = match &node.sig.output {
            syn::ReturnType::Default => {
                // `-> ()` style: a void return is not a Functor Out
                // (the target data must be the return value). Only
                // flagged if this fn is later registered, so just record
                // it and let registration decide.
                syn::parse_quote!(())
            }
            syn::ReturnType::Type(_, t) => (**t).clone(),
        };
        self.fn_sigs.insert(name, (in_ty, out_ty));
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.error.is_some() {
            return;
        }
        let last = node
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if last == "record" {
            if let Some((nm, def)) = parse_record_macro(node) {
                self.records.insert(nm, def);
            }
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.error.is_some() {
            return;
        }
        let method = node.method.to_string();
        if method == "handler" {
            // `app.handler!(name)` parses as a method call whose single
            // argument is the macro-style ident; in stable Rust the
            // surface is `app.handler(name)` — accept the call form and
            // take the first path-ident argument as the handler name.
            if let Some(syn::Expr::Path(p)) = node.args.first() {
                if let Some(id) = p.path.get_ident() {
                    self.registered.push(id.to_string());
                }
            }
        } else if method == "flow" {
            let mut args = node.args.iter();
            if let Some(syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            })) = args.next()
            {
                let fname = s.value();
                let mut stages: Vec<Vec<String>> = Vec::new();
                for a in args {
                    match a {
                        syn::Expr::Path(p) => {
                            if let Some(id) = p.path.get_ident() {
                                stages.push(vec![id.to_string()]);
                            }
                        }
                        // `parallel(a, b)` call inside the flow arg list.
                        syn::Expr::Call(c) => {
                            if let syn::Expr::Path(cp) = &*c.func {
                                let fn_name = cp
                                    .path
                                    .segments
                                    .last()
                                    .map(|s| s.ident.to_string())
                                    .unwrap_or_default();
                                if fn_name == "parallel" {
                                    let members: Vec<String> = c
                                        .args
                                        .iter()
                                        .filter_map(|m| match m {
                                            syn::Expr::Path(mp) => mp
                                                .path
                                                .get_ident()
                                                .map(|i| i.to_string()),
                                            _ => None,
                                        })
                                        .collect();
                                    if !members.is_empty() {
                                        stages.push(members);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                self.flow = Some((fname, stages));
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.error.is_some() {
            return;
        }
        if let syn::Expr::Path(p) = &*node.func {
            let tail = p
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            // `topo::App::new("orders")` — the namespace.
            if tail == "new"
                && p.path
                    .segments
                    .iter()
                    .any(|s| s.ident == "App")
            {
                if let Some(syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                })) = node.args.first()
                {
                    self.namespace = Some(s.value());
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_stmt_macro(&mut self, _node: &'ast syn::StmtMacro) {
        // `app.handler!(name);` / `app.flow!(...)` as statement macros:
        // syn surfaces these as `StmtMacro` whose path tail is the
        // method ident with a `!`. Recover the same data the call-form
        // visitors do so both spellings are accepted.
        if self.error.is_some() {
            return;
        }
        let mac = &_node.mac;
        let tail = mac
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if tail == "handler" {
            if let Ok(id) = mac.parse_body::<syn::Ident>() {
                self.registered.push(id.to_string());
            }
        } else if tail == "flow" {
            for st in collect_stage_args(mac.tokens.clone()) {
                let _ = st; // handled below via structured parse
            }
            // Structured parse: first a string literal, then idents /
            // `parallel(...)` groups.
            let stages = collect_stage_args(mac.tokens.clone());
            // The flow name is the leading string literal.
            let name = mac
                .tokens
                .clone()
                .into_iter()
                .find_map(|t| match t {
                    proc_macro2::TokenTree::Literal(l) => {
                        let s = l.to_string();
                        Some(s.trim_matches('"').to_string())
                    }
                    _ => None,
                })
                .unwrap_or_default();
            if !name.is_empty() {
                self.flow = Some((name, stages));
            }
        }
        syn::visit::visit_stmt_macro(self, _node);
    }
}

/// Statically analyze Rust source text and build the logic graph.
/// Never executes the program.
pub fn analyze(src: &str) -> Result<Graph> {
    let file = syn::parse_file(src)
        .map_err(|e| AnalyzeError(format!("rust parse error: {e}")))?;

    let mut c = SurfaceCollector::default();
    c.visit_file(&file);
    if let Some(e) = c.error {
        return Err(e);
    }

    let namespace = c.namespace.ok_or_else(|| {
        AnalyzeError(
            "no `topo::App::new(\"<namespace>\")` found — the static \
             path needs a statically visible namespace"
                .to_string(),
        )
    })?;

    let mut graph = Graph::new(namespace);

    for hname in &c.registered {
        let (in_ty, out_ty) = c.fn_sigs.get(hname).ok_or_else(|| {
            AnalyzeError(format!(
                "registered handler `{hname}` has no `fn {hname}(...)` \
                 definition in the analyzed source"
            ))
        })?;
        let in_ref = match in_ty {
            None => None,
            Some(t) => Some(type_to_ref(t, &c.records)?),
        };
        // A `-> ()` return is not a valid Functor Out.
        if matches!(out_ty, syn::Type::Tuple(t) if t.elems.is_empty()) {
            return Err(AnalyzeError(format!(
                "handler `{hname}` returns `()`; a handler's target data \
                 must be its return value — return a stdlib scalar or \
                 `record!` type"
            )));
        }
        let out_ref = type_to_ref(out_ty, &c.records)?;
        graph.handlers.push(Handler {
            name: hname.clone(),
            in_type: in_ref,
            out_type: out_ref,
        });
    }

    if let Some((fname, stages)) = c.flow {
        let mut edges: Vec<Edge> = Vec::new();
        for i in 0..stages.len().saturating_sub(1) {
            for src in &stages[i] {
                for tgt in &stages[i + 1] {
                    edges.push(Edge {
                        source: src.clone(),
                        target: Some(tgt.clone()),
                    });
                }
            }
        }
        if let Some(last) = stages.last() {
            for src in last {
                edges.push(Edge {
                    source: src.clone(),
                    target: None, // terminal -> void
                });
            }
        }
        graph.flow = Some(Flow { name: fname, edges });
    }

    Ok(graph)
}
