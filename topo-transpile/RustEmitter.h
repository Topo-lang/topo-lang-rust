#ifndef TOPO_TRANSPILE_RUSTEMITTER_H
#define TOPO_TRANSPILE_RUSTEMITTER_H

#include "topo/Transpile/Emitter.h"
#include "topo/Sema/TypeBinder.h"
#include <optional>
#include <string>
#include <unordered_map>

namespace topo::transpile {

class RustEmitter : public Emitter {
public:
    explicit RustEmitter(TypeBinder binder = TypeBinder::createDefault(HostLanguage::Rust));
    EmitResult emit(const TranspileModule& module) override;

private:
    TypeBinder binder_;

    // Statically-known type of in-scope vars/params within the function
    // currently being emitted. Lets emitExpr coerce an integer LiteralExpr
    // appearing against an f64/f32-typed operand into a float literal.
    std::unordered_map<std::string, const TypeNode*> varTypes_;
    // Return type of the function currently being emitted (nullptr at module
    // scope / void). Used to coerce integer literals in `return` position.
    const TypeNode* currentReturnType_ = nullptr;

    // When true, every borrow `&` produced by emitType for the construct
    // currently being emitted is annotated with a single shared named
    // lifetime (`lifetimeAnnotName_`). Set by emitFunction / emitStruct in
    // two cases:
    //  (1) Rust's lifetime elision does NOT cover the construct (a free
    //      function that returns a reference with >=2 reference parameters,
    //      or a struct that holds a reference-typed field) — the emitter
    //      injects a single shared `'a`.
    //  (2) The construct already carries an explicit wire lifetime
    //      parameter (`kind=Lifetime` entry in templateParams) AND has a
    //      borrow field/parameter — the emitter reuses the wire lifetime's
    //      name for the annotation so round-tripped Rust source stays
    //      byte-faithful (`<'a, T: 'a> { value: &'a T }`).
    // Cleared again afterwards so elided output is produced everywhere
    // elision already works.
    bool annotateLifetime_ = false;
    // Name (without leading `'`) of the lifetime used by `emitType` when
    // `annotateLifetime_` is true. Defaults to "a" for the elision-injected
    // path; emit-sites override to the wire lifetime entry's name when one
    // is present.
    std::string lifetimeAnnotName_ = "a";

    // True iff this type is emitted as a Rust borrow (`&...`): an explicit
    // `&`-modifier reference, or a stdlib type whose Rust mapping is a
    // borrow (`string` -> `&str`, `slice<T>` -> `&[T]`, `bytes` -> `&[u8]`).
    // Owned wrappers (Box/Arc/Weak) and value types are NOT borrows.
    static bool isBorrowType(const TypeNode& type);

    std::string emitType(const TypeNode& type);
    // emitType core; emitType wraps this and injects the shared `'a`
    // lifetime into every leading `&` when annotateLifetime_ is set.
    std::string emitTypeCore(const TypeNode& type);
    // `expected` is an optional static type hint for the position this
    // expression occupies (declared var type, return type, float-typed
    // binary operand). When non-null and float, a bare integer LiteralExpr
    // is emitted as a float literal so the Rust compiles. nullptr = no hint.
    std::string emitExpr(const Expr& expr, const TypeNode* expected = nullptr);
    // Static type of a VarRef operand within the current function, or nullptr.
    const TypeNode* operandType(const Expr& expr) const;
    std::string emitStmt(const Stmt& stmt, int indent);
    std::string emitFunction(const TranspileFunction& func);
    std::string emitStruct(const TranspileType& type);
    std::string emitOwnership(const TypeNode& type);

    // Rust expresses a base class / interface as a marker trait `impl`ed by
    // the derived struct (Rust has no inheritance and no class/interface
    // distinction, so `baseClassKinds` is uniformly ignored — same stance as
    // the C++/Python emitters). Given a base TypeNode this returns the
    // simple (unqualified) trait name `<Base>Trait`; the `Trait` suffix keeps
    // the marker trait from colliding with a same-named base struct.
    static std::string baseTraitName(const TypeNode& base);

    // Match expression optimization for if/else-if chains
    struct MatchArm {
        std::string value;                    // constant value string
        const std::vector<StmtPtr>* body;     // pointer to body statements
    };
    struct MatchCandidate {
        std::string variable;                 // common variable being compared
        std::vector<MatchArm> arms;           // (value, body) pairs
        const std::vector<StmtPtr>* elseBody; // final else body (may be nullptr)
    };
    std::optional<MatchCandidate> isMatchCandidate(const IfStmt& stmt);
    std::string emitMatchExpr(const MatchCandidate& candidate, int level);
};

} // namespace topo::transpile

#endif // TOPO_TRANSPILE_RUSTEMITTER_H
