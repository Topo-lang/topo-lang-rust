#include "RustEmitter.h"
#include "topo/Stdlib/Types.h"
#include <functional>
#include <map>
#include <set>
#include <unordered_set>

namespace topo::transpile {

static std::string ind(int level) {
    return std::string(level * 4, ' ');
}

static std::string fidelityComment(Fidelity f, int level) {
    if (f == Fidelity::Recovered) return ind(level) + "// [recovered]\n";
    if (f == Fidelity::Inferred) return ind(level) + "// [inferred]\n";
    return "";
}

static std::string binaryOpStr(BinaryOp op) {
    switch (op) {
    case BinaryOp::Add: return "+";
    case BinaryOp::Sub: return "-";
    case BinaryOp::Mul: return "*";
    case BinaryOp::Div: return "/";
    case BinaryOp::Mod: return "%";
    case BinaryOp::Eq: return "==";
    case BinaryOp::NotEq: return "!=";
    case BinaryOp::Less: return "<";
    case BinaryOp::Greater: return ">";
    case BinaryOp::LessEq: return "<=";
    case BinaryOp::GreaterEq: return ">=";
    case BinaryOp::And: return "&&";
    case BinaryOp::Or: return "||";
    case BinaryOp::BitAnd: return "&";
    case BinaryOp::BitOr: return "|";
    case BinaryOp::BitXor: return "^";
    case BinaryOp::Shl: return "<<";
    case BinaryOp::Shr: return ">>";
    }
    return "??";
}

RustEmitter::RustEmitter(TypeBinder binder) : binder_(std::move(binder)) {}

static std::pair<std::string, std::string> splitQualifiedName(const std::string& qname) {
    auto pos = qname.rfind("::");
    if (pos == std::string::npos)
        return {"", qname};
    return {qname.substr(0, pos), qname.substr(pos + 2)};
}

static std::string mapConcreteType(const std::string& name) {
    // C++ / Java / Python integer types -> Rust
    if (name == "int" || name == "int32_t") return "i32";
    if (name == "long" || name == "int64_t" || name == "long long") return "i64";
    if (name == "short" || name == "int16_t") return "i16";
    if (name == "char" || name == "int8_t") return "i8";
    if (name == "unsigned" || name == "uint32_t" || name == "unsigned int") return "u32";
    if (name == "uint64_t" || name == "unsigned long" || name == "unsigned long long") return "u64";
    if (name == "uint16_t" || name == "unsigned short") return "u16";
    if (name == "uint8_t" || name == "unsigned char") return "u8";
    if (name == "size_t") return "usize";
    // Float types. Includes identity entries for the already-lowered Rust
    // scalar names (`f64`/`f32`): the TypeScript extractor lowers `number`
    // to a non-stdlib TypeNode with nameParts={"f64"}, which emitType()
    // echoes verbatim. Without these, isFloatType() (which routes through
    // mapConcreteType) disagreed with emitType() on that exact TypeNode, so
    // the f64-literal coercion guard never fired and bare integer literals
    // were emitted into f64 slots (non-compilable Rust: E0308/E0277).
    if (name == "double" || name == "float64" || name == "f64") return "f64";
    if (name == "float" || name == "float32" || name == "f32") return "f32";
    // Boolean
    if (name == "bool" || name == "boolean") return "bool";
    // String
    if (name == "string" || name == "String" || name == "str") return "String";
    // Void
    if (name == "void" || name == "Void") return "()";
    return "";
}

// Does this static type denote a Rust floating-point scalar (f64/f32)?
// Recognises both the stdlib bridging form (TypeId::F64 / F32) and the
// concrete source-language names that mapConcreteType folds to "f64"/"f32"
// (double / float / float64 / float32). Pointers/refs/containers are not
// floats themselves, so a modified or templated type is excluded — only a
// bare scalar position can take a coerced float literal.
static bool isFloatType(const TypeNode& type) {
    if (type.ownership != OwnershipKind::None) return false;
    if (type.modifier != TypeNode::None) return false;
    if (!type.templateArgs.empty() || !type.recordFields.empty()) return false;
    if (type.isStdlib())
        return type.stdlibId == stdlib::TypeId::F64 || type.stdlibId == stdlib::TypeId::F32;
    if (type.nameParts.size() == 1) {
        auto mapped = mapConcreteType(type.nameParts[0]);
        return mapped == "f64" || mapped == "f32";
    }
    if (type.nameParts.size() > 1) {
        auto mapped = mapConcreteType(type.nameParts.back());
        return mapped == "f64" || mapped == "f32";
    }
    return false;
}

static std::string mapContainerType(const std::string& name) {
    if (name == "vector" || name == "ArrayList" || name == "List" || name == "list") return "Vec";
    if (name == "optional" || name == "Optional") return "Option";
    if (name == "unordered_map" || name == "map" || name == "HashMap" || name == "Map" || name == "dict") return "HashMap";
    if (name == "unordered_set" || name == "set" || name == "HashSet" || name == "Set") return "HashSet";
    if (name == "tuple" || name == "Tuple") return "tuple";
    return "";
}

EmitResult RustEmitter::emit(const TranspileModule& module) {
    EmitResult result;

    // Group types and functions by namespace prefix
    struct NsGroup {
        std::vector<const TranspileType*> types;
        std::vector<const TranspileFunction*> functions;
    };
    std::map<std::string, NsGroup> groups;

    for (const auto& t : module.types) {
        auto [ns, _] = splitQualifiedName(t.qualifiedName);
        groups[ns].types.push_back(&t);
    }
    for (const auto& f : module.functions) {
        auto [ns, _] = splitQualifiedName(f.qualifiedName);
        groups[ns].functions.push_back(&f);
    }

    // A base trait shared by several structs must be declared once, not once
    // per `impl`, or Rust rejects the duplicate definition. Collect distinct
    // trait names in first-seen source order (insertion-ordered set) and emit
    // every marker trait ahead of the structs. When no type derives from a
    // base this loop produces nothing, keeping pre-inheritance output
    // byte-identical.
    std::vector<std::string> markerTraits;
    {
        std::set<std::string> seen;
        for (const auto& t : module.types)
            for (const auto& base : t.baseClasses) {
                std::string name = baseTraitName(base);
                if (seen.insert(name).second)
                    markerTraits.push_back(name);
            }
    }
    for (const auto& name : markerTraits)
        result.code += "trait " + name + " {}\n";

    for (const auto& [ns, group] : groups) {
        // Parse namespace parts for nested mod blocks
        std::vector<std::string> nsParts;
        if (!ns.empty()) {
            size_t start = 0;
            while (start < ns.size()) {
                auto next = ns.find("::", start);
                if (next == std::string::npos) {
                    nsParts.push_back(ns.substr(start));
                    break;
                }
                nsParts.push_back(ns.substr(start, next - start));
                start = next + 2;
            }
            for (const auto& part : nsParts)
                result.code += "mod " + part + " {\n";
        }

        for (const auto* t : group.types)
            result.code += emitStruct(*t) + "\n";
        for (const auto* f : group.functions)
            result.code += emitFunction(*f) + "\n";

        if (!ns.empty()) {
            for (size_t i = nsParts.size(); i > 0; --i)
                result.code += "} // mod " + nsParts[i - 1] + "\n";
        }
    }

    return result;
}

std::string RustEmitter::emitOwnership(const TypeNode& type) {
    // Copy-and-mutate, not positional reconstruction: a positional
    // TypeNode{...} silently drops any field not listed (stdlibId,
    // recordFields), so `owned slice<T>` / `owned record<...>` would lose
    // their stdlib identity through the ownership path.
    TypeNode bare = type;
    bare.ownership = OwnershipKind::None;
    bare.modifier = TypeNode::None;
    std::string inner = emitType(bare);

    switch (type.ownership) {
    case OwnershipKind::Owned: return "Box<" + inner + ">";
    case OwnershipKind::Shared: return "Arc<" + inner + ">";
    case OwnershipKind::Weak: return "Weak<" + inner + ">";
    case OwnershipKind::None: break;
    }
    return inner;
}

// True iff `type` is emitted as a Rust borrow (`&...`). Used to decide
// whether lifetime elision covers a function/struct: see emitFunction /
// emitStruct. Mirrors exactly which emitTypeCore paths prepend a `&`.
bool RustEmitter::isBorrowType(const TypeNode& type) {
    // Owned wrappers (Box/Arc/Weak) own their contents — not a borrow.
    if (type.ownership != OwnershipKind::None) return false;
    if (type.modifier == TypeNode::Ref) return true;
    if (type.isStdlib()) {
        switch (type.stdlibId) {
        case stdlib::TypeId::String: // -> &str
        case stdlib::TypeId::Slice:  // -> &[T]
        case stdlib::TypeId::Bytes:  // -> &[u8]
            return true;
        default:
            return false;
        }
    }
    return false;
}

std::string RustEmitter::emitType(const TypeNode& type) {
    std::string s = emitTypeCore(type);
    if (!annotateLifetime_) return s;
    // Inject the single shared lifetime into every leading borrow.
    // Each `&` that is not `&&` and not already followed by a lifetime
    // becomes `&<lifetime> `. A single shared lifetime across the whole
    // signature / struct is the conservative correct choice (correctness
    // over minimality): it never changes which references may alias, it
    // only makes the elision-failed case nameable so the Rust compiles.
    // The lifetime name is `lifetimeAnnotName_` (defaults to "a" for the
    // elision-injected path; emit-sites switch it to the wire lifetime's
    // name when one is declared via `kind=Lifetime` in templateParams so
    // round-tripped Rust source stays byte-faithful).
    const std::string annot = "'" + lifetimeAnnotName_ + " ";
    std::string out;
    out.reserve(s.size() + annot.size() + 1);
    for (size_t i = 0; i < s.size(); ++i) {
        out += s[i];
        if (s[i] == '&' && i + 1 < s.size() && s[i + 1] != '&' &&
            s[i + 1] != '\'' && (i == 0 || s[i - 1] != '&')) {
            out += annot;
        }
    }
    return out;
}

std::string RustEmitter::emitTypeCore(const TypeNode& type) {
    if (type.ownership != OwnershipKind::None) return emitOwnership(type);

    // Stdlib bridging types take priority over the legacy
    // primitive / container name lookups. Parser sets both `stdlibId` and
    // `nameParts={"i64"}`/`{"string"}`/etc. on stdlib uses; without this branch
    // running first, `string` would fall into mapConcreteType -> "String"
    // (the legacy owned mapping) instead of the contract `&str` view.
    //
    // Lifetime annotations: borrow types (`&str`, `&[T]`, explicit `&T`)
    // are emitted WITHOUT an explicit `'a` whenever Rust's lifetime
    // elision already produces a compilable signature — the common cases
    // (single input ref, method-receiver `&self`, a single param with a
    // returned reference). emitTypeCore stays elision-shaped; the named
    // lifetime is layered on by emitType only when emitFunction /
    // emitStruct have determined elision FAILS (a free function returning
    // a reference with >=2 reference params -> E0106, or a struct holding
    // a reference field). In that case a single shared `'a` is introduced
    // on the construct and threaded through every borrow here. This keeps
    // elided output everywhere elision works (no fixture regression) while
    // making the previously-uncompilable cases compile.
    if (type.isStdlib()) {
        switch (type.stdlibId) {
        case stdlib::TypeId::Bool:   return "bool";
        case stdlib::TypeId::I64:    return "i64";
        case stdlib::TypeId::TimeNs: return "i64"; // ns since epoch, i64-isomorphic
        case stdlib::TypeId::Uuid:   return "[u8; 16]"; // 16-byte RFC 4122 buffer (no native Rust UUID)
        case stdlib::TypeId::Decimal128: return "[u8; 16]"; // 16-byte IEEE 754-2008 buffer (no native Rust decimal)
        case stdlib::TypeId::F64:    return "f64";
        case stdlib::TypeId::String: return "&str";
        case stdlib::TypeId::Optional: {
            if (type.templateArgs.empty()) return "Option<()>"; // defensive; Sema rejects optional<> upstream
            return "Option<" + emitType(type.templateArgs[0]) + ">";
        }
        case stdlib::TypeId::Slice: {
            if (type.templateArgs.empty()) return "&[()]"; // defensive; Sema rejects slice<> upstream
            return "&[" + emitType(type.templateArgs[0]) + "]";
        }
        case stdlib::TypeId::Bytes:
            // `bytes` is slice<u8>-isomorphic: the element type is implicitly
            // u8, so it maps to exactly what the Slice case emits for a u8
            // element (`&[u8]`). No new Rust type is invented.
            return "&[u8]";
        case stdlib::TypeId::Array: {
            // array<T, N> is a fixed-length inline buffer of N contiguous T.
            // templateArgs[0] is the element type T (recurse, mirroring the
            // Slice element recursion); templateArgs[1].nonTypeValue carries
            // the integer N. Maps to idiomatic Rust `[T; N]`; the
            // N * align_up(sizeof(T), align(T)) byte layout is the natural
            // layout of a Rust fixed-size array.
            if (type.templateArgs.size() < 2) return "[(); 0]"; // defensive; Sema rejects malformed array upstream
            const auto& elem = type.templateArgs[0];
            const auto& count = type.templateArgs[1];
            int n = count.nonTypeValue.value_or(0);
            return "[" + emitType(elem) + "; " + std::to_string(n) + "]";
        }
        // Rust native scalar names match exactly.
        case stdlib::TypeId::U8:     return "u8";
        case stdlib::TypeId::I32:    return "i32";
        case stdlib::TypeId::U32:    return "u32";
        case stdlib::TypeId::U64:    return "u64";
        case stdlib::TypeId::F32:    return "f32";
        case stdlib::TypeId::I8:     return "i8";
        case stdlib::TypeId::I16:    return "i16";
        case stdlib::TypeId::U16:    return "u16";
        case stdlib::TypeId::Record: {
            // record<f1: T1, ...> -> Rust tuple (T1, T2, ...). Field order
            // is the load-bearing cross-language byte contract; field names
            // live in the .topo declaration, not the host type (same
            // positional idiom PythonEmitter uses). A 1-field record needs
            // the trailing-comma form `(T,)` to be a tuple, not a group.
            const auto& fields = type.recordFields;
            if (fields.empty()) return "()"; // defensive; Sema rejects record<> upstream
            std::string out = "(";
            for (size_t i = 0; i < fields.size(); ++i) {
                if (i > 0) out += ", ";
                out += emitType(fields[i].type());
            }
            if (fields.size() == 1) out += ",";
            out += ")";
            return out;
        }
        case stdlib::TypeId::Union: {
            // union<tag: TagT, v1: T1, ...> -> Rust tuple (TagT, T1, ...).
            // The variants overlap in the .topo byte contract (only the
            // tag-selected variant occupies the shared storage); Rust has no
            // anonymous tagged-union literal, so the order-preserving tuple
            // is the faithful surface — the same idiom record uses. Field
            // names and overlap stay in the declaration.
            const auto& fields = type.recordFields;
            if (fields.empty()) return "()"; // defensive; Sema rejects upstream
            std::string out = "(";
            for (size_t i = 0; i < fields.size(); ++i) {
                if (i > 0) out += ", ";
                out += emitType(fields[i].type());
            }
            if (fields.size() == 1) out += ","; // 1-elem tuple; Sema enforces >=2
            out += ")";
            return out;
        }
        case stdlib::TypeId::None:
            break; // fall through to legacy paths
        }
    }

    // Try TypeBinder resolution for single-part abstract names
    if (type.nameParts.size() == 1) {
        auto resolved = binder_.resolve(type.nameParts[0]);
        if (resolved) {
            std::string result;
            if (type.modifier == TypeNode::Ref)
                result += "&";
            else if (type.modifier == TypeNode::Ptr)
                result += "*mut ";
            result += *resolved;
            return result;
        }
    }

    // Try concrete source-language type mapping
    if (type.nameParts.size() == 1) {
        auto mapped = mapConcreteType(type.nameParts[0]);
        if (!mapped.empty()) {
            std::string result;
            if (type.modifier == TypeNode::Ref)
                result += "&";
            else if (type.modifier == TypeNode::Ptr)
                result += "*mut ";
            result += mapped;
            return result;
        }
        auto container = mapContainerType(type.nameParts[0]);
        if (!container.empty()) {
            std::string result;
            if (type.modifier == TypeNode::Ref) result += "&";
            else if (type.modifier == TypeNode::Ptr) result += "*mut ";
            result += container;
            if (!type.templateArgs.empty()) {
                result += "<";
                for (size_t i = 0; i < type.templateArgs.size(); ++i) {
                    if (i > 0) result += ", ";
                    result += emitType(type.templateArgs[i]);
                }
                result += ">";
            }
            return result;
        }
    }
    // Multi-part qualified types: check last part for container/primitive
    if (type.nameParts.size() > 1) {
        const auto& lastName = type.nameParts.back();
        auto container = mapContainerType(lastName);
        if (!container.empty()) {
            std::string result;
            if (type.modifier == TypeNode::Ref) result += "&";
            else if (type.modifier == TypeNode::Ptr) result += "*mut ";
            result += container;
            if (!type.templateArgs.empty()) {
                result += "<";
                for (size_t i = 0; i < type.templateArgs.size(); ++i) {
                    if (i > 0) result += ", ";
                    result += emitType(type.templateArgs[i]);
                }
                result += ">";
            }
            return result;
        }
        auto mapped = mapConcreteType(lastName);
        if (!mapped.empty()) {
            std::string result;
            if (type.modifier == TypeNode::Ref) result += "&";
            else if (type.modifier == TypeNode::Ptr) result += "*mut ";
            result += mapped;
            return result;
        }
    }

    std::string result;
    if (type.modifier == TypeNode::Ref)
        result += "&";
    else if (type.modifier == TypeNode::Ptr)
        result += "*mut ";

    for (size_t i = 0; i < type.nameParts.size(); ++i) {
        if (i > 0) result += "::";
        result += type.nameParts[i];
    }

    if (!type.templateArgs.empty()) {
        result += "<";
        for (size_t i = 0; i < type.templateArgs.size(); ++i) {
            if (i > 0) result += ", ";
            result += emitType(type.templateArgs[i]);
        }
        result += ">";
    }

    return result;
}

// Static type of a VarRef-or-literal operand, if knowable from the current
// function scope. Returns nullptr when the operand is anything else, or its
// type is not tracked. Only used to discover an f64/f32 anchor in a binary
// expression so the *other* (literal) side can be coerced.
const TypeNode* RustEmitter::operandType(const Expr& expr) const {
    if (expr.kind() == Expr::Kind::VarRef) {
        auto it = varTypes_.find(static_cast<const VarRefExpr&>(expr).name);
        if (it != varTypes_.end()) return it->second;
    }
    return nullptr;
}

std::string RustEmitter::emitExpr(const Expr& expr, const TypeNode* expected) {
    switch (expr.kind()) {
    case Expr::Kind::BinaryOp: {
        const auto& e = static_cast<const BinaryOpExpr&>(expr);
        // Pick a float anchor for the operands: an explicit float `expected`
        // hint (arithmetic feeding an f64 slot), or a float-typed operand on
        // either side (so `x <= n` / `s + 1` with f64 `x`/`n`/`s` coerces the
        // integer-literal side). The anchor is forwarded only to the literal
        // side; never invents a float context for genuine integers.
        const TypeNode* anchor = nullptr;
        if (expected && isFloatType(*expected)) anchor = expected;
        if (!anchor) {
            const TypeNode* lt = operandType(*e.lhs);
            if (lt && isFloatType(*lt)) anchor = lt;
        }
        if (!anchor) {
            const TypeNode* rt = operandType(*e.rhs);
            if (rt && isFloatType(*rt)) anchor = rt;
        }
        return "(" + emitExpr(*e.lhs, anchor) + " " + binaryOpStr(e.op) + " " + emitExpr(*e.rhs, anchor) + ")";
    }
    case Expr::Kind::UnaryOp: {
        const auto& e = static_cast<const UnaryOpExpr&>(expr);
        std::string op;
        switch (e.op) {
        case UnaryOp::Negate: op = "-"; break;
        case UnaryOp::Not: op = "!"; break;
        case UnaryOp::BitNot: op = "!"; break; // Rust uses ! for both logical and bitwise NOT
        case UnaryOp::PreIncrement:
            return "{ " + emitExpr(*e.operand) + " += 1; " + emitExpr(*e.operand) + " }";
        case UnaryOp::PostIncrement:
            return "{ let _prev = " + emitExpr(*e.operand) + "; " + emitExpr(*e.operand) + " += 1; _prev }";
        case UnaryOp::PreDecrement:
            return "{ " + emitExpr(*e.operand) + " -= 1; " + emitExpr(*e.operand) + " }";
        case UnaryOp::PostDecrement:
            return "{ let _prev = " + emitExpr(*e.operand) + "; " + emitExpr(*e.operand) + " -= 1; _prev }";
        }
        // Negation preserves the slot type: `-1` into an f64 slot is `-1.0`.
        if (e.op == UnaryOp::Negate) return op + emitExpr(*e.operand, expected);
        return op + emitExpr(*e.operand);
    }
    case Expr::Kind::Call: {
        const auto& e = static_cast<const CallExpr&>(expr);
        std::string result = e.callee + "(";
        for (size_t i = 0; i < e.args.size(); ++i) {
            if (i > 0) result += ", ";
            result += emitExpr(*e.args[i]);
        }
        result += ")";
        return result;
    }
    case Expr::Kind::MemberAccess: {
        const auto& e = static_cast<const MemberAccessExpr&>(expr);
        return emitExpr(*e.object) + "." + e.member;
    }
    case Expr::Kind::Index: {
        const auto& e = static_cast<const IndexExpr&>(expr);
        return emitExpr(*e.object) + "[" + emitExpr(*e.index) + "]";
    }
    case Expr::Kind::Literal: {
        const auto& e = static_cast<const LiteralExpr&>(expr);
        if (e.litKind == LiteralKind::String) return "\"" + e.value + "\"";
        if (e.litKind == LiteralKind::Boolean) return (e.value == "true") ? "true" : "false";
        // Rust float literals need explicit suffix or dot
        if (e.litKind == LiteralKind::Float) {
            if (e.value.find('.') == std::string::npos) return e.value + ".0";
        }
        // Coerce a bare integer literal sitting in a statically-known f64/f32
        // position (declared var / return / float-typed binary operand) into
        // a float literal so rustc does not reject `let s: f64 = 0;` (E0308)
        // or `f64 <= {integer}` (E0277). Only fires when `expected` is a
        // float type; genuine integer contexts pass expected=nullptr and stay
        // bare. Skip values already carrying a `.`, exponent, or type suffix.
        if (e.litKind == LiteralKind::Integer && expected && isFloatType(*expected)) {
            if (e.value.find_first_of(".eExXfiu") == std::string::npos)
                return e.value + ".0";
        }
        return e.value;
    }
    case Expr::Kind::VarRef: {
        const auto& e = static_cast<const VarRefExpr&>(expr);
        return e.name;
    }
    case Expr::Kind::Construct: {
        const auto& e = static_cast<const ConstructExpr&>(expr);
        std::string result = emitType(e.type) + "::new(";
        for (size_t i = 0; i < e.args.size(); ++i) {
            if (i > 0) result += ", ";
            result += emitExpr(*e.args[i]);
        }
        result += ")";
        return result;
    }
    case Expr::Kind::Lambda: {
        const auto& e = static_cast<const LambdaExpr&>(expr);
        // Rust closures: |params| { body }
        // Captures are implicit in Rust; emit move keyword if any by-value
        bool hasMove = false;
        for (const auto& c : e.captures) {
            if (c.mode == CaptureMode::ByValue) {
                hasMove = true;
                break;
            }
        }
        std::string result;
        if (hasMove) result += "move ";
        result += "|";
        for (size_t i = 0; i < e.params.size(); ++i) {
            if (i > 0) result += ", ";
            result += e.params[i].name + ": " + emitType(e.params[i].type);
        }
        result += "|";
        if (!e.returnType.nameParts.empty()) result += " -> " + emitType(e.returnType);
        result += " {\n";
        for (const auto& st : e.body)
            result += emitStmt(*st, 1);
        result += "}";
        return result;
    }
    case Expr::Kind::Throw: {
        const auto& e = static_cast<const ThrowExpr&>(expr);
        // Rust uses Result/panic; emit panic! as closest equivalent
        return "panic!(\"exception: {}\", " + emitExpr(*e.operand) + ")";
    }
    case Expr::Kind::Unsupported: {
        const auto& e = static_cast<const UnsupportedExpr&>(expr);
        return "/* TOPO-TRANSPILE: unsupported — " + e.description + " */";
    }
    case Expr::Kind::Ternary: {
        // Rust has no ternary operator; use if/else expression
        const auto& e = static_cast<const TernaryExpr&>(expr);
        // Both arms occupy the same slot as the ternary itself.
        return "(if " + emitExpr(*e.condition) + " { " + emitExpr(*e.trueExpr, expected) + " } else { " +
               emitExpr(*e.falseExpr, expected) + " })";
    }
    case Expr::Kind::CompoundAssign: {
        const auto& e = static_cast<const CompoundAssignExpr&>(expr);
        // `s += 1` with f64 `s` must emit `s += 1.0`. Coerce the value
        // against the target's statically-known type when available.
        const TypeNode* targetTy = operandType(*e.target);
        return emitExpr(*e.target) + " " + binaryOpStr(e.op) + "= " + emitExpr(*e.value, targetTy);
    }
    }
    return "/* TOPO-TRANSPILE: unsupported — unknown expression */";
}

std::string RustEmitter::emitStmt(const Stmt& stmt, int level) {
    std::string prefix = fidelityComment(stmt.fidelity, level);

    switch (stmt.kind()) {
    case Stmt::Kind::VarDecl: {
        const auto& s = static_cast<const VarDeclStmt&>(stmt);
        std::string result = prefix + ind(level) + "let mut " + s.name + ": " + emitType(s.type);
        if (s.init) result += " = " + emitExpr(*s.init, &s.type);
        result += ";\n";
        // Track the declared type so later expressions (binary ops,
        // compound-assign) can coerce integer literals against this var.
        varTypes_[s.name] = &s.type;
        return result;
    }
    case Stmt::Kind::Assign: {
        const auto& s = static_cast<const AssignStmt&>(stmt);
        // Coerce RHS integer literals against the target's known type
        // (`s = 0` with f64 `s` → `s = 0.0`).
        const TypeNode* targetTy = operandType(*s.target);
        return prefix + ind(level) + emitExpr(*s.target) + " = " + emitExpr(*s.value, targetTy) + ";\n";
    }
    case Stmt::Kind::Return: {
        const auto& s = static_cast<const ReturnStmt&>(stmt);
        if (s.value)
            return prefix + ind(level) + "return " + emitExpr(*s.value, currentReturnType_) + ";\n";
        return prefix + ind(level) + "return;\n";
    }
    case Stmt::Kind::If: {
        const auto& s = static_cast<const IfStmt&>(stmt);

        // Try to emit as match expression for idiomatic Rust
        if (auto candidate = isMatchCandidate(s)) {
            return prefix + emitMatchExpr(*candidate, level);
        }

        std::string result = prefix + ind(level) + "if " + emitExpr(*s.condition) + " {\n";
        for (const auto& st : s.thenBody)
            result += emitStmt(*st, level + 1);
        result += ind(level) + "}";
        if (!s.elseBody.empty()) {
            result += " else {\n";
            for (const auto& st : s.elseBody)
                result += emitStmt(*st, level + 1);
            result += ind(level) + "}";
        }
        result += "\n";
        return result;
    }
    case Stmt::Kind::For: {
        const auto& s = static_cast<const ForStmt&>(stmt);

        // Detect counted-loop pattern: for (let i = START; i < END; i += 1)
        // and emit as Rust `for i in START..END`
        bool isCountedLoop = false;
        std::string loopVar, startVal, endVal;

        if (s.init && s.init->kind() == Stmt::Kind::VarDecl && s.condition && s.increment) {
            const auto& initDecl = static_cast<const VarDeclStmt&>(*s.init);
            loopVar = initDecl.name;
            startVal = initDecl.init ? emitExpr(*initDecl.init) : "0";

            // Check condition: i < END
            if (s.condition->kind() == Expr::Kind::BinaryOp) {
                const auto& cond = static_cast<const BinaryOpExpr&>(*s.condition);
                if (cond.op == BinaryOp::Less && cond.lhs->kind() == Expr::Kind::VarRef) {
                    const auto& lhs = static_cast<const VarRefExpr&>(*cond.lhs);
                    if (lhs.name == loopVar) {
                        endVal = emitExpr(*cond.rhs);
                        isCountedLoop = true;
                    }
                }
            }
        }

        if (isCountedLoop) {
            std::string range = (startVal == "0") ? ("0.." + endVal) : (startVal + ".." + endVal);
            std::string result = prefix + ind(level) + "for " + loopVar + " in " + range + " {\n";
            for (const auto& st : s.body)
                result += emitStmt(*st, level + 1);
            result += ind(level) + "}\n";
            return result;
        }

        // Fallback: init + while (existing behavior)
        std::string result = prefix;
        if (s.init) result += emitStmt(*s.init, level);
        std::string cond = s.condition ? emitExpr(*s.condition) : "true";
        result += ind(level) + "while " + cond + " {\n";
        for (const auto& st : s.body)
            result += emitStmt(*st, level + 1);
        if (s.increment) result += ind(level + 1) + emitExpr(*s.increment) + ";\n";
        result += ind(level) + "}\n";
        return result;
    }
    case Stmt::Kind::While: {
        const auto& s = static_cast<const WhileStmt&>(stmt);
        std::string result = prefix + ind(level) + "while " + emitExpr(*s.condition) + " {\n";
        for (const auto& st : s.body)
            result += emitStmt(*st, level + 1);
        result += ind(level) + "}\n";
        return result;
    }
    case Stmt::Kind::ExprStmt: {
        const auto& s = static_cast<const ExprStmt&>(stmt);
        return prefix + ind(level) + emitExpr(*s.expr) + ";\n";
    }
    case Stmt::Kind::TryCatch: {
        const auto& s = static_cast<const TryCatchStmt&>(stmt);
        // Rust has no try/catch; emit as comment-annotated block with match on Result
        std::string result = prefix + ind(level) + "// NOTE: Rust uses Result<T, E> instead of try/catch\n";
        result += ind(level) + "{\n";
        for (const auto& st : s.tryBody)
            result += emitStmt(*st, level + 1);
        result += ind(level) + "}\n";
        for (const auto& c : s.catchClauses) {
            result += ind(level) + "// catch " + emitType(c.exceptionType);
            if (!c.varName.empty()) result += " " + c.varName;
            result += " {\n";
            for (const auto& st : c.body)
                result += emitStmt(*st, level + 1);
            result += ind(level) + "// }\n";
        }
        if (!s.finallyBody.empty()) {
            result += ind(level) + "// finally {\n";
            for (const auto& st : s.finallyBody)
                result += emitStmt(*st, level + 1);
            result += ind(level) + "// }\n";
        }
        return result;
    }
    case Stmt::Kind::Break: return prefix + ind(level) + "break;\n";
    case Stmt::Kind::Continue: return prefix + ind(level) + "continue;\n";
    case Stmt::Kind::Switch: {
        const auto& s = static_cast<const SwitchStmt&>(stmt);
        std::string result = prefix + ind(level) + "match " + emitExpr(*s.subject) + " {\n";
        for (const auto& c : s.cases) {
            if (c.value)
                result += ind(level + 1) + emitExpr(*c.value) + " => {\n";
            else
                result += ind(level + 1) + "_ => {\n";
            for (const auto& st : c.body)
                result += emitStmt(*st, level + 2);
            result += ind(level + 1) + "}\n";
        }
        result += ind(level) + "}\n";
        return result;
    }
    }
    return prefix + ind(level) + "// TOPO-TRANSPILE: unsupported — unknown statement\n";
}

// Collect names that appear as the LHS of an AssignStmt or CompoundAssignExpr
// anywhere within the given statement list (recursively). Used to decide which
// Rust parameters must be emitted with `mut` — Rust parameters are immutable
// by default, so reassigning them produces E0384.
//
// Conservative rule: if any assignment target with the given name appears in
// the body, mark the parameter `mut`. This may produce spurious `unused_mut`
// warnings in the presence of shadowing, but avoids compile errors and keeps
// the analysis local to RustEmitter.
static void collectAssignTargetsExpr(const Expr& expr, std::unordered_set<std::string>& out);
static void collectAssignTargetsStmt(const Stmt& stmt, std::unordered_set<std::string>& out);

static void collectAssignTargetsExpr(const Expr& expr, std::unordered_set<std::string>& out) {
    switch (expr.kind()) {
    case Expr::Kind::CompoundAssign: {
        const auto& e = static_cast<const CompoundAssignExpr&>(expr);
        if (e.target && e.target->kind() == Expr::Kind::VarRef) {
            out.insert(static_cast<const VarRefExpr&>(*e.target).name);
        }
        if (e.target) collectAssignTargetsExpr(*e.target, out);
        if (e.value) collectAssignTargetsExpr(*e.value, out);
        return;
    }
    case Expr::Kind::UnaryOp: {
        const auto& e = static_cast<const UnaryOpExpr&>(expr);
        // Pre/post ++ and -- mutate their operand
        if ((e.op == UnaryOp::PreIncrement || e.op == UnaryOp::PostIncrement ||
             e.op == UnaryOp::PreDecrement || e.op == UnaryOp::PostDecrement) &&
            e.operand && e.operand->kind() == Expr::Kind::VarRef) {
            out.insert(static_cast<const VarRefExpr&>(*e.operand).name);
        }
        if (e.operand) collectAssignTargetsExpr(*e.operand, out);
        return;
    }
    case Expr::Kind::BinaryOp: {
        const auto& e = static_cast<const BinaryOpExpr&>(expr);
        if (e.lhs) collectAssignTargetsExpr(*e.lhs, out);
        if (e.rhs) collectAssignTargetsExpr(*e.rhs, out);
        return;
    }
    case Expr::Kind::Call: {
        const auto& e = static_cast<const CallExpr&>(expr);
        for (const auto& a : e.args)
            if (a) collectAssignTargetsExpr(*a, out);
        return;
    }
    case Expr::Kind::MemberAccess: {
        const auto& e = static_cast<const MemberAccessExpr&>(expr);
        if (e.object) collectAssignTargetsExpr(*e.object, out);
        return;
    }
    case Expr::Kind::Index: {
        const auto& e = static_cast<const IndexExpr&>(expr);
        if (e.object) collectAssignTargetsExpr(*e.object, out);
        if (e.index) collectAssignTargetsExpr(*e.index, out);
        return;
    }
    case Expr::Kind::Construct: {
        const auto& e = static_cast<const ConstructExpr&>(expr);
        for (const auto& a : e.args)
            if (a) collectAssignTargetsExpr(*a, out);
        return;
    }
    case Expr::Kind::Lambda: {
        const auto& e = static_cast<const LambdaExpr&>(expr);
        for (const auto& st : e.body)
            if (st) collectAssignTargetsStmt(*st, out);
        return;
    }
    case Expr::Kind::Throw: {
        const auto& e = static_cast<const ThrowExpr&>(expr);
        if (e.operand) collectAssignTargetsExpr(*e.operand, out);
        return;
    }
    case Expr::Kind::Ternary: {
        const auto& e = static_cast<const TernaryExpr&>(expr);
        if (e.condition) collectAssignTargetsExpr(*e.condition, out);
        if (e.trueExpr) collectAssignTargetsExpr(*e.trueExpr, out);
        if (e.falseExpr) collectAssignTargetsExpr(*e.falseExpr, out);
        return;
    }
    case Expr::Kind::Literal:
    case Expr::Kind::VarRef:
    case Expr::Kind::Unsupported:
        return;
    }
}

static void collectAssignTargetsStmt(const Stmt& stmt, std::unordered_set<std::string>& out) {
    switch (stmt.kind()) {
    case Stmt::Kind::Assign: {
        const auto& s = static_cast<const AssignStmt&>(stmt);
        if (s.target && s.target->kind() == Expr::Kind::VarRef) {
            out.insert(static_cast<const VarRefExpr&>(*s.target).name);
        }
        if (s.target) collectAssignTargetsExpr(*s.target, out);
        if (s.value) collectAssignTargetsExpr(*s.value, out);
        return;
    }
    case Stmt::Kind::VarDecl: {
        const auto& s = static_cast<const VarDeclStmt&>(stmt);
        if (s.init) collectAssignTargetsExpr(*s.init, out);
        return;
    }
    case Stmt::Kind::Return: {
        const auto& s = static_cast<const ReturnStmt&>(stmt);
        if (s.value) collectAssignTargetsExpr(*s.value, out);
        return;
    }
    case Stmt::Kind::If: {
        const auto& s = static_cast<const IfStmt&>(stmt);
        if (s.condition) collectAssignTargetsExpr(*s.condition, out);
        for (const auto& st : s.thenBody)
            if (st) collectAssignTargetsStmt(*st, out);
        for (const auto& st : s.elseBody)
            if (st) collectAssignTargetsStmt(*st, out);
        return;
    }
    case Stmt::Kind::For: {
        const auto& s = static_cast<const ForStmt&>(stmt);
        if (s.init) collectAssignTargetsStmt(*s.init, out);
        if (s.condition) collectAssignTargetsExpr(*s.condition, out);
        if (s.increment) collectAssignTargetsExpr(*s.increment, out);
        for (const auto& st : s.body)
            if (st) collectAssignTargetsStmt(*st, out);
        return;
    }
    case Stmt::Kind::While: {
        const auto& s = static_cast<const WhileStmt&>(stmt);
        if (s.condition) collectAssignTargetsExpr(*s.condition, out);
        for (const auto& st : s.body)
            if (st) collectAssignTargetsStmt(*st, out);
        return;
    }
    case Stmt::Kind::ExprStmt: {
        const auto& s = static_cast<const ExprStmt&>(stmt);
        if (s.expr) collectAssignTargetsExpr(*s.expr, out);
        return;
    }
    case Stmt::Kind::TryCatch: {
        const auto& s = static_cast<const TryCatchStmt&>(stmt);
        for (const auto& st : s.tryBody)
            if (st) collectAssignTargetsStmt(*st, out);
        for (const auto& c : s.catchClauses)
            for (const auto& st : c.body)
                if (st) collectAssignTargetsStmt(*st, out);
        for (const auto& st : s.finallyBody)
            if (st) collectAssignTargetsStmt(*st, out);
        return;
    }
    case Stmt::Kind::Switch: {
        const auto& s = static_cast<const SwitchStmt&>(stmt);
        if (s.subject) collectAssignTargetsExpr(*s.subject, out);
        for (const auto& c : s.cases) {
            if (c.value) collectAssignTargetsExpr(*c.value, out);
            for (const auto& st : c.body)
                if (st) collectAssignTargetsStmt(*st, out);
        }
        return;
    }
    case Stmt::Kind::Break:
    case Stmt::Kind::Continue:
        return;
    }
}

static std::unordered_set<std::string> collectReassignedParams(const TranspileFunction& func) {
    std::unordered_set<std::string> assignTargets;
    for (const auto& s : func.body)
        if (s) collectAssignTargetsStmt(*s, assignTargets);

    std::unordered_set<std::string> reassignedParams;
    for (const auto& p : func.params) {
        if (assignTargets.find(p.name) != assignTargets.end())
            reassignedParams.insert(p.name);
    }
    return reassignedParams;
}

// True iff the wire `TemplateParamDecl` carries at least one `kind=Lifetime`
// entry (i.e. the source already declared an explicit `'a`-style param). In
// that case `genericsClause` MUST NOT inject the elision-default `'a` again
// — the wire lifetime is rendered in its declared position instead.
static bool hasWireLifetimeParam(const std::vector<TemplateParamDecl>& tps) {
    for (const auto& tp : tps) {
        if (tp.kind == TemplateParamDecl::LifetimeParam) return true;
    }
    return false;
}

// Builds the single ordered generics clause `<...>` for a decl. Rust requires
// the lifetime (whether wire-declared OR emitter-injected for borrow
// correctness) to precede type parameters in one combined list — two
// separate `<...>` groups are a syntax error. Returns "" when there is
// neither a lifetime nor a type param so non-generic, non-borrowing decls
// stay byte-identical.
//
// `injectLifetime` is set by struct/fn emit-sites when Rust lifetime elision
// would fail AND the wire carries no explicit lifetime param of its own —
// in that case the emitter prepends a single shared `'a` to the clause.
// When the wire ALREADY carries a `kind=Lifetime` entry the injection is
// suppressed (the wire entry occupies the slot) regardless of the flag.
//
// `allowDefaults` is set by struct/enum/trait emit-sites; defaults are valid
// there in Rust. Free-function call-sites must pass false — Rust forbids
// `<T = Default>` on `fn` and rejects the resulting code (`error[E0091]:
// type parameter \`T\` has a default`). When defaults are disallowed and
// the wire carries one, it is silently dropped here; the call-site
// responsibility is to emit the TOPO-TRANSPILE downgrade comment.
// True iff this bound TypeNode is a positional `union<...>` — the wire
// shape a Python TypeVar constraint-tuple lowers to. Rust has no trait or
// type usable as such a generic bound, so it is dropped with a downgrade
// note.
static bool rustIsPositionalUnionBound(const TypeNode& t) {
    return t.nameParts.size() == 1 && t.nameParts[0] == "union";
}

static std::string genericsClause(bool injectLifetime,
                                  const std::vector<TemplateParamDecl>& tps,
                                  bool allowDefaults = false) {
    const bool wireHasLifetime = hasWireLifetimeParam(tps);
    const bool emitInjected = injectLifetime && !wireHasLifetime;
    if (!emitInjected && tps.empty()) return "";
    std::string clause = "<";
    // Active HRTB lifetime scope — set by `renderPath` when descending into
    // a Fn-trait parenthesised body and consumed by inner reference
    // rendering so `&'a u8` reuses the HRTB-introduced lifetime label.
    // Empty outside a Fn-trait body (references in regular bound paths fall
    // back to the elision-time annotation handled by emitType, not by
    // renderPath).
    std::string hrtbLifetimeName_;
    // Render a trait-bound path with optional associated-type bindings
    // (Rust `Iterator<Item = u8>`). Positional templateArgs (recovered from
    // demangled type-name decoration in LLVM lifting) and assocBindings
    // (recovered by the source-mode Rust extractor) can co-exist; when both
    // are present positional args come first, then the named bindings —
    // matching Rust's lexical order `Container<T, Item = U>`. Renders
    // recursively via emitType so nested bindings round-trip.
    // Detects the parenthesised Fn-trait shape stored by the Rust extractor:
    // a `Fn` / `FnMut` / `FnOnce` path whose `assocBindings` carries a single
    // `Output` entry (the rest of the args are positional `templateArgs`).
    // The Rust emitter re-renders the parenthesised form (`Fn(A) -> B`)
    // when emitting to Rust; cross-host emitters fall back to angle-
    // bracketed and silently drop the HRTB prefix since neither concept
    // exists for them.
    auto isFnTraitParenShape = [](const TypeNode& n) {
        if (n.nameParts.empty()) return false;
        const std::string& last = n.nameParts.back();
        if (last != "Fn" && last != "FnMut" && last != "FnOnce") return false;
        if (n.assocBindings.size() != 1) return false;
        return n.assocBindings.front().name == "Output";
    };
    std::function<void(const TypeNode&)> renderPath = [&](const TypeNode& n) {
        // HRTB `for<'a, 'b>` prefix on a trait bound. Stored sans-
        // apostrophe in `hrtbLifetimes`; the `'` is added here at emit time.
        // Cross-host emitters silently drop this field (no analogue in
        // C++/Java/Python/TS).
        if (!n.hrtbLifetimes.empty()) {
            clause += "for<";
            for (size_t i = 0; i < n.hrtbLifetimes.size(); ++i) {
                if (i > 0) clause += ", ";
                clause += "'";
                clause += n.hrtbLifetimes[i];
            }
            clause += "> ";
        }
        // Detect the Rust extractor's reference-as-prefix idiom
        // (`&u8` → nameParts = ["&", "u8"] / `&mut T` → ["&mut", "T"]).
        // For HRTB Fn-trait inputs/output the path renderer must produce
        // valid `&'<hrtb> Ty` rather than `&::Ty`. The shared HRTB lifetime
        // name (the first hrtbLifetime captured on the enclosing trait
        // bound — defaulting to "a") is threaded through `hrtbLifetimeName_`
        // so the borrow carries an explicit lifetime label. Spacing matches
        // idiomatic Rust: `&'a u8` (no space between `&` and `'a`; one
        // space between `'a` and the type).
        size_t firstNamePart = 0;
        if (!n.nameParts.empty() &&
            (n.nameParts.front() == "&" || n.nameParts.front() == "&mut")) {
            clause += n.nameParts.front();
            if (n.nameParts.front() == "&mut") clause += " ";
            if (!hrtbLifetimeName_.empty()) {
                clause += "'";
                clause += hrtbLifetimeName_;
                clause += " ";
            }
            firstNamePart = 1;
        }
        for (size_t i = firstNamePart; i < n.nameParts.size(); ++i) {
            if (i > firstNamePart) clause += "::";
            clause += n.nameParts[i];
        }
        if (isFnTraitParenShape(n)) {
            // Re-render parenthesised: `Fn(A, B) -> C`. Inputs come from
            // `templateArgs`; the single Output binding becomes the return.
            // The HRTB lifetime name (first hrtbLifetime captured on this
            // trait bound) becomes the scope for inner reference labels —
            // saved across the recursive `renderPath` calls for inputs and
            // the Output type.
            std::string prevHrtbName = hrtbLifetimeName_;
            if (!n.hrtbLifetimes.empty()) hrtbLifetimeName_ = n.hrtbLifetimes.front();
            clause += "(";
            for (size_t i = 0; i < n.templateArgs.size(); ++i) {
                if (i > 0) clause += ", ";
                renderPath(n.templateArgs[i]);
            }
            clause += ")";
            const TypeNode& out = n.assocBindings.front().type();
            if (!out.nameParts.empty()) {
                clause += " -> ";
                renderPath(out);
            }
            hrtbLifetimeName_ = prevHrtbName;
            return;
        }
        const bool hasPositional = !n.templateArgs.empty();
        const bool hasBindings = !n.assocBindings.empty();
        if (!hasPositional && !hasBindings) return;
        clause += "<";
        bool firstArg = true;
        for (const auto& arg : n.templateArgs) {
            if (!firstArg) clause += ", ";
            renderPath(arg);
            firstArg = false;
        }
        for (const auto& b : n.assocBindings) {
            if (!firstArg) clause += ", ";
            clause += b.name + " = ";
            renderPath(b.type());
            firstArg = false;
        }
        clause += ">";
    };
    bool first = true;
    if (emitInjected) {
        clause += "'a";
        first = false;
    }
    for (const auto& tp : tps) {
        if (!first) clause += ", ";
        // Lifetime param (`kind=Lifetime`): wire `name` carries the lifetime
        // label without the leading apostrophe ("a"); the emitter adds it.
        // Optional outlives target rides `constraintType` with the
        // apostrophe kept (`["'b"]`) so a lifetime-on-lifetime outlives is
        // rendered as `'a: 'b`.
        if (tp.kind == TemplateParamDecl::LifetimeParam) {
            clause += "'" + tp.name;
            if (!tp.constraintType.nameParts.empty()) {
                clause += ": ";
                // Render the outlives target verbatim — nameParts[0]
                // already carries the apostrophe by wire contract.
                renderPath(tp.constraintType);
            }
            first = false;
            continue;
        }
        // Non-type generic (`<const N: usize>`, optionally `<const N: usize = 16>`):
        // prepend `const` and emit the value type after the name.
        // constraintType carries the value type; extraBounds stays empty
        // for nontype params (no multi-bound concept). A literal default
        // is appended verbatim from the wire `defaultValue` string —
        // Rust allows const-generic defaults on type-level decls
        // (struct/enum/trait) just like type-param defaults; the
        // function-level allowDefaults=false suppresses both axes (E0091
        // forbids defaults on free functions).
        if (tp.kind == TemplateParamDecl::NonTypeParam &&
            !tp.constraintType.nameParts.empty()) {
            clause += "const " + tp.name + ": ";
            renderPath(tp.constraintType);
            if (allowDefaults && tp.defaultValue.has_value() &&
                !tp.defaultValue->empty()) {
                clause += " = " + *tp.defaultValue;
            }
            first = false;
            continue;
        }
        clause += tp.name;
        // Bound rendering: single bound emits `<T: Trait>`; multi-bound
        // emits `<T: A + B>` joining each bound with ` + ` (Rust syntax).
        // Multi-segment paths use `::` (`std::fmt::Debug`). The full bound
        // list is [constraintType, ...extraBounds]; extraBounds stays
        // empty for single-bound payloads (legacy wire shape). A bound
        // entry whose nameParts[0] starts with `'` is a lifetime bound
        // (`T: 'a`); it's rendered verbatim (apostrophe kept) and joined
        // with trait bounds by ` + ` (`T: Trait + 'a`).
        if (tp.kind == TemplateParamDecl::TypeParam &&
            !tp.constraintType.nameParts.empty()) {
            // Drop positional union<...> bounds (Python TypeVar
            // constraint-tuple) — no Rust trait/bound equivalent — and
            // render the rest. A param whose only bound was a union
            // renders bare with a downgrade note.
            std::vector<const TypeNode*> bounds;
            bool unionDropped = false;
            if (rustIsPositionalUnionBound(tp.constraintType)) unionDropped = true;
            else bounds.push_back(&tp.constraintType);
            for (const auto& eb : tp.extraBounds) {
                if (rustIsPositionalUnionBound(eb)) unionDropped = true;
                else bounds.push_back(&eb);
            }
            for (size_t b = 0; b < bounds.size(); ++b) {
                clause += (b == 0 ? ": " : " + ");
                renderPath(*bounds[b]);
            }
            if (unionDropped) {
                clause += " /* TOPO-TRANSPILE: union<...> bound on " +
                          tp.name + " dropped (no Rust generic-bound "
                          "equivalent) */";
            }
        }
        // Single default render: `<T = Default>`. Multi-segment defaults
        // join with `::` for consistency with bound rendering. Only allowed
        // by Rust on type-level decls (struct/enum/trait); the function
        // call-site passes allowDefaults=false to suppress.
        if (allowDefaults && tp.kind == TemplateParamDecl::TypeParam &&
            tp.defaultType.has_value() && !tp.defaultType->nameParts.empty()) {
            clause += " = ";
            renderPath(*tp.defaultType);
        }
        first = false;
    }
    clause += ">";
    return clause;
}

// First wire lifetime param's name (without leading `'`) in `tps`, or empty
// string if none. Used by emit-sites to thread the wire lifetime label
// through `emitType`'s borrow-annotation pass so round-tripped Rust source
// reuses the original `'a` instead of an emitter-injected one.
static std::string firstWireLifetimeName(
    const std::vector<TemplateParamDecl>& tps) {
    for (const auto& tp : tps) {
        if (tp.kind == TemplateParamDecl::LifetimeParam) return tp.name;
    }
    return std::string{};
}

std::string RustEmitter::emitFunction(const TranspileFunction& func) {
    std::string result;
    result += fidelityComment(func.fidelity, 0);

    // Establish a fresh per-function type scope: parameter types and the
    // return type drive integer-literal -> float-literal coercion. Cleared
    // first so no state leaks between functions.
    varTypes_.clear();
    for (const auto& p : func.params)
        varTypes_[p.name] = &p.type;
    bool returnIsVoid = func.returnType.nameParts.empty() ||
                        (func.returnType.nameParts.size() == 1 && func.returnType.nameParts[0] == "void");
    currentReturnType_ = returnIsVoid ? nullptr : &func.returnType;

    for (const auto& u : func.unsupported)
        result += "// TOPO-TRANSPILE: unsupported — " + u + "\n";

    // Rust forbids defaults on free-function type parameters (E0091). If the
    // wire carries one (cross-language source allows it, e.g. TS / C++),
    // surface the downgrade and let genericsClause drop it. The same rule
    // applies to const-generic defaults — Rust rejects `fn f<const N: usize = 16>`
    // for the same reason.
    for (const auto& tp : func.templateParams) {
        if (tp.kind == TemplateParamDecl::TypeParam &&
            tp.defaultType.has_value() &&
            !tp.defaultType->nameParts.empty()) {
            result += "// TOPO-TRANSPILE: default on type parameter `" +
                      tp.name +
                      "` dropped (Rust forbids defaults on free functions)\n";
        }
        if (tp.kind == TemplateParamDecl::NonTypeParam &&
            tp.defaultValue.has_value() &&
            !tp.defaultValue->empty()) {
            result += "// TOPO-TRANSPILE: default on const parameter `" +
                      tp.name +
                      "` dropped (Rust forbids defaults on free functions)\n";
        }
    }

    // Return type: check for void (empty nameParts or "void")
    bool isVoid = func.returnType.nameParts.empty() ||
                  (func.returnType.nameParts.size() == 1 && func.returnType.nameParts[0] == "void");

    // Rust visibility: public -> pub, private -> (default), protected/internal -> pub(crate)
    // Functions from namespaces default to pub (accessible from outside the mod)
    if (!func.accessModifier.empty()) {
        if (func.accessModifier == "public") result += "pub ";
        else if (func.accessModifier == "protected") result += "pub(crate) ";
        // "private" -> default (no prefix) in Rust
    } else {
        auto [ns, _sn] = splitQualifiedName(func.qualifiedName);
        if (!ns.empty()) result += "pub ";
    }

    // Lifetime elision fails (E0106) for a free function that returns a
    // reference and has >=2 reference inputs: the output lifetime is
    // ambiguous. In exactly that case introduce one shared named lifetime
    // `<'a>` and annotate every borrow (params + return). The single-input
    // -ref case and the method-receiver (`&self`) case stay elided —
    // elision already produces a compilable signature there, so emitting
    // `'a` would be needless noise (and risks regressing fixtures).
    size_t refParamCount = 0;
    for (const auto& p : func.params)
        if (isBorrowType(p.type)) ++refParamCount;
    bool returnIsRef = !isVoid && isBorrowType(func.returnType);
    // Wire-declared explicit lifetime (Rust source has `<'a, ...>`): reuse
    // its name for the borrow-annotation pass so the emitted signature
    // round-trips to the same `'a` the source used. When the wire has a
    // lifetime AND any borrow is present we still annotate; otherwise we
    // fall back to the elision-rule heuristic (only annotate when elision
    // would actually fail).
    std::string wireLt = firstWireLifetimeName(func.templateParams);
    const bool elisionWouldFail = returnIsRef && refParamCount >= 2;
    const bool anyBorrow = returnIsRef || refParamCount > 0;
    annotateLifetime_ = elisionWouldFail || (!wireLt.empty() && anyBorrow);
    lifetimeAnnotName_ = wireLt.empty() ? std::string("a") : wireLt;

    auto [_, simpleName] = splitQualifiedName(func.qualifiedName);
    result += "fn " + simpleName;
    result += genericsClause(annotateLifetime_, func.templateParams);
    result += "(";
    auto reassignedParams = collectReassignedParams(func);
    for (size_t i = 0; i < func.params.size(); ++i) {
        if (i > 0) result += ", ";
        if (reassignedParams.count(func.params[i].name))
            result += "mut ";
        result += func.params[i].name + ": " + emitType(func.params[i].type);
    }
    result += ")";

    if (!isVoid) result += " -> " + emitType(func.returnType);

    // The named lifetime belongs to the signature only. Clear before
    // emitting the body so a function-local borrow does not get tagged
    // with `'a` (which would be a meaning change / borrow-check error).
    annotateLifetime_ = false;
    lifetimeAnnotName_ = "a"; // reset shared state for next decl

    result += " {\n";

    for (const auto& s : func.body)
        result += emitStmt(*s, 1);

    result += "}\n";
    return result;
}

std::string RustEmitter::emitStruct(const TranspileType& type) {
    std::string result;
    result += fidelityComment(type.fidelity, 0);
    auto [_, simpleName] = splitQualifiedName(type.qualifiedName);

    // A struct that holds a reference-typed field requires a lifetime
    // parameter (a `&T` field with no `'a` does not compile). Introduce a
    // single shared `<'a>` on the struct and thread it through every
    // borrow field. Conservative single shared lifetime: correctness over
    // minimality. Structs with no reference field stay unparameterised.
    //
    // When the wire already declares an explicit lifetime param via
    // `kind=Lifetime`, the emitter reuses its name (instead of the default
    // "a") so a Rust source `<'b, T> { v: &'b T }` round-trips without
    // renaming. `genericsClause` will detect the wire entry and skip the
    // elision-default injection automatically.
    bool hasRefField = false;
    for (const auto& f : type.fields)
        if (isBorrowType(f.type)) { hasRefField = true; break; }
    std::string wireLt = firstWireLifetimeName(type.templateParams);
    // refs are what we annotate; a bare lifetime param with no ref field
    // needs no per-borrow annotation (the wire entry still surfaces in the
    // generics clause via genericsClause).
    annotateLifetime_ = hasRefField;
    lifetimeAnnotName_ = wireLt.empty() ? std::string("a") : wireLt;

    result += "struct " + simpleName;
    // Struct/enum/trait sites allow type-param defaults in Rust (E0091 only
    // fires on free functions); pass allowDefaults=true.
    result += genericsClause(annotateLifetime_, type.templateParams, /*allowDefaults=*/true);
    result += " {\n";

    for (const auto& f : type.fields) {
        result += fidelityComment(f.fidelity, 1);
        result += ind(1) + f.name + ": " + emitType(f.type) + ",\n";
    }

    result += "}\n";
    annotateLifetime_ = false;

    // Each base class / interface becomes a marker-trait `impl`. The trait
    // itself is declared once module-wide by emit(); here we only bind this
    // struct to it. A struct carrying a lifetime parameter must repeat it on
    // the impl (`impl<'a> Trait for S<'a>`) or the impl does not name `'a`.
    // The repeated lifetime name matches whatever the struct emitted
    // (wire-declared name when present, otherwise the injected `'a`).
    const std::string ltLabel = "'" + lifetimeAnnotName_;
    for (const auto& base : type.baseClasses) {
        std::string traitName = baseTraitName(base);
        if (hasRefField)
            result += "impl<" + ltLabel + "> " + traitName + " for " +
                      simpleName + "<" + ltLabel + "> {}\n";
        else
            result += "impl " + traitName + " for " + simpleName + " {}\n";
    }
    lifetimeAnnotName_ = "a"; // reset shared state for next decl
    return result;
}

std::string RustEmitter::baseTraitName(const TypeNode& base) {
    std::string simple =
        base.nameParts.empty() ? std::string() : base.nameParts.back();
    return simple + "Trait";
}

// --- Match expression optimization ---

static std::string extractVarName(const Expr& expr) {
    if (expr.kind() == Expr::Kind::VarRef)
        return static_cast<const VarRefExpr&>(expr).name;
    return "";
}

static bool isConstantExpr(const Expr& expr) {
    return expr.kind() == Expr::Kind::Literal;
}

std::optional<RustEmitter::MatchCandidate> RustEmitter::isMatchCandidate(const IfStmt& stmt) {
    MatchCandidate result;

    // First condition must be BinaryOp with Eq
    if (!stmt.condition || stmt.condition->kind() != Expr::Kind::BinaryOp)
        return std::nullopt;

    const auto& firstCond = static_cast<const BinaryOpExpr&>(*stmt.condition);
    if (firstCond.op != BinaryOp::Eq)
        return std::nullopt;

    std::string varName = extractVarName(*firstCond.lhs);
    if (varName.empty() || !isConstantExpr(*firstCond.rhs))
        return std::nullopt;

    result.variable = varName;
    result.arms.push_back({emitExpr(*firstCond.rhs), &stmt.thenBody});

    // Walk the else-if chain
    const IfStmt* current = &stmt;
    while (true) {
        // Check if elseBody is a single IfStmt (else-if chain)
        if (current->elseBody.size() == 1 &&
            current->elseBody[0]->kind() == Stmt::Kind::If) {

            const auto& next = static_cast<const IfStmt&>(*current->elseBody[0]);

            if (!next.condition || next.condition->kind() != Expr::Kind::BinaryOp)
                return std::nullopt;

            const auto& cond = static_cast<const BinaryOpExpr&>(*next.condition);
            if (cond.op != BinaryOp::Eq)
                return std::nullopt;

            std::string nextVar = extractVarName(*cond.lhs);
            if (nextVar != varName || !isConstantExpr(*cond.rhs))
                return std::nullopt;

            result.arms.push_back({emitExpr(*cond.rhs), &next.thenBody});
            current = &next;
        } else {
            // End of chain: set elseBody if present
            result.elseBody = current->elseBody.empty() ? nullptr : &current->elseBody;
            break;
        }
    }

    // Need at least 2 arms to justify a match expression
    if (result.arms.size() < 2)
        return std::nullopt;

    return result;
}

std::string RustEmitter::emitMatchExpr(const MatchCandidate& candidate, int level) {
    std::string result = ind(level) + "match " + candidate.variable + " {\n";

    for (const auto& arm : candidate.arms) {
        result += ind(level + 1) + arm.value + " => {\n";
        for (const auto& st : *arm.body)
            result += emitStmt(*st, level + 2);
        result += ind(level + 1) + "}\n";
    }

    if (candidate.elseBody) {
        result += ind(level + 1) + "_ => {\n";
        for (const auto& st : *candidate.elseBody)
            result += emitStmt(*st, level + 2);
        result += ind(level + 1) + "}\n";
    }

    result += ind(level) + "}\n";
    return result;
}

} // namespace topo::transpile
