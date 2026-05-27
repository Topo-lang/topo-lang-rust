//! Handler In/Out schema capture for the topo-app Rust projection.
//!
//! Rust has no runtime reflection: a function cannot be asked for its
//! parameter/return types, and a struct cannot be asked for its field
//! names. The Python projection reads `__annotations__`; the Rust
//! projection substitutes the type system. Two devices carry the same
//! information `_reflect.py` derives from annotations:
//!
//!   * a [`TopoType`] trait — every type usable as a handler In/Out
//!     reports its `.topo` spelling through `type_ref()`. Implemented for
//!     the stdlib scalar bridges; a `record` is produced by the
//!     [`record!`] macro.
//!   * the [`record!`] macro — the only place a record's *field names*
//!     can be captured, because Rust cannot recover them from a struct at
//!     runtime. Names are written once, at the call site, exactly as
//!     Python's `Record[("id", int)]` writes them once. This is the
//!     deliberate parity boundary, not a silent degrade: both hosts
//!     require the field names to be stated where the record is declared.
//!
//! The scalar vocabulary mirrors the config port's discipline: the
//! config bridge's [`crate::config_model::ConfigValue`] is a closed enum
//! over `bool/int/float/str` + an ordered-`BTreeMap` `record`, and
//! [`crate::config_model::stdlib_type_of`] maps a value to its
//! schema-contract spelling. This module reuses that *structure* (a
//! closed scalar set + an ordered named-field record) while emitting the
//! **Rust host** `.topo` spelling (`i64`/`f64`/`bool`/`string`), which is
//! what `topo-init`'s Rust template and the `handler_flow` Rust fixture
//! use — distinct from the config bridge's `int`/`float`/`str` contract
//! names, by design (the two layers spell stdlib types for different
//! consumers).

use crate::config_model::ConfigValue;

/// A topo type as it will be spelled in the emitted `.topo`.
///
/// Mirrors `_graph.py::TypeRef`: exactly one of `scalar` / `record` is
/// populated. `void` (no input / terminal) is the *absence* of a
/// `TypeRef` at the use site, never a `TypeRef` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// A stdlib scalar alias as the Rust host spells it in `.topo`
    /// (`i64` / `f64` / `bool` / `string`).
    Scalar(&'static str),
    /// An order-preserving named-field record. Field order is
    /// significant and stable: it is the declared order, never sorted,
    /// matching `record<a: T, b: U>` positional spelling.
    Record(Vec<(String, TypeRef)>),
}

impl TypeRef {
    /// The `.topo` spelling of this type.
    ///
    /// A record nests as `record<name: inner, ...>`; the Rust host
    /// fixture (`topo-lang-rust/test/fixtures/spec/handler_flow`) and
    /// `topo-core`'s `handler_chain.topo` both use exactly this form.
    pub fn topo(&self) -> String {
        match self {
            TypeRef::Scalar(s) => (*s).to_string(),
            TypeRef::Record(fields) => {
                let inner = fields
                    .iter()
                    .map(|(n, t)| format!("{n}: {}", t.topo()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("record<{inner}>")
            }
        }
    }
}

/// A type that can appear as a handler In or Out.
///
/// The Rust analogue of "this annotation maps to a stdlib bridge type".
/// Implemented for the scalar bridges below; a record implements it via
/// the [`record!`] macro, which is the only construct that can pin the
/// field *names* at declaration sites (no reflection exists to recover
/// them later).
pub trait TopoType {
    fn type_ref() -> TypeRef;
}

// The scalar bridges. Spelling matches `RustInitTemplateProvider`'s
// `using <name> = std::rust::<name>;` table and the handler_flow Rust
// fixture: `i64`, `f64`, `bool`, and the built-in `string`. `String` is
// the idiomatic Rust owned-string handler parameter; `&str` borrows are
// out of scope for the In/Out value contract (a Functor owns its I/O).
impl TopoType for i64 {
    fn type_ref() -> TypeRef {
        TypeRef::Scalar("i64")
    }
}

impl TopoType for f64 {
    fn type_ref() -> TypeRef {
        TypeRef::Scalar("f64")
    }
}

impl TopoType for bool {
    fn type_ref() -> TypeRef {
        TypeRef::Scalar("bool")
    }
}

impl TopoType for String {
    fn type_ref() -> TypeRef {
        TypeRef::Scalar("string")
    }
}

/// Bridge a config-port [`ConfigValue`] scalar to its Rust-host `.topo`
/// spelling.
///
/// The config bridge already classifies stdlib values; reusing its
/// closed enum here keeps a single source of truth for "what is a
/// bridgeable scalar" while re-spelling for the handler/flow `.topo`
/// consumer (the config contract says `int`/`float`/`str`; the Rust
/// host `.topo` says `i64`/`f64`/`string`). A non-scalar
/// (`Array`/`Record`/`Datetime`) has no single scalar spelling and
/// returns `None` — records are built structurally via [`record!`],
/// arrays/datetime are not handler In/Out types.
pub fn scalar_topo_of(value: &ConfigValue) -> Option<&'static str> {
    match value {
        ConfigValue::Bool(_) => Some("bool"),
        ConfigValue::Int(_) => Some("i64"),
        ConfigValue::Float(_) => Some("f64"),
        ConfigValue::Str(_) => Some("string"),
        ConfigValue::Array(_) | ConfigValue::Record(_) | ConfigValue::Datetime(_) => None,
    }
}

/// Declare a record In/Out type with explicit, ordered field names.
///
/// `record!(id: i64, amount: f64)` evaluates to a [`TypeRef::Record`]
/// with fields in written order. Field names are captured here because
/// this is the *only* point they exist as source tokens — Rust offers no
/// runtime field-name reflection, so the names must be written at the
/// declaration site exactly as Python writes `Record[("id", int)]`.
/// Nested records compose: a field type may itself be `record!(...)`.
#[macro_export]
macro_rules! record {
    ( $( $name:ident : $ty:expr ),+ $(,)? ) => {{
        let fields: ::std::vec::Vec<(::std::string::String, $crate::schema::TypeRef)> =
            ::std::vec![ $( (::std::stringify!($name).to_string(), $ty) ),+ ];
        $crate::schema::TypeRef::Record(fields)
    }};
}

/// The `.topo` `TypeRef` of a scalar handler type `T: TopoType`.
///
/// Sugar so registration sites read `scalar::<i64>()` next to
/// `record!(...)`, keeping In/Out declaration uniform.
pub fn scalar<T: TopoType>() -> TypeRef {
    T::type_ref()
}
