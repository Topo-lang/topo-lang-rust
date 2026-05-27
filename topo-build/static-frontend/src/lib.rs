//! topo-app Rust compile-time static `.topo` producer.
//!
//! This is the static counterpart to the runtime registration
//! bridge: it parses a Rust topo-app program with `syn` (no execution),
//! recovers the same in-memory logic graph the runtime bridge would
//! build, and emits the *same* `.topo` surface. Architecturally it is
//! just another `.topo` producer — it does not touch topo-core; it consumes the
//! existing `topo` / `topo-check` binaries for read-back and checking.
//!
//! Pipeline: `analyze` (syn AST walk) -> `Graph` -> `emit_topo`
//! (byte-identical emitter form) -> `read_topo` (real `topo`
//! parser) -> graph' for the round-trip equivalence proof; `check`
//! drives the existing `topo-check` against the emitted `.topo`.

pub mod analyze;
pub mod check;
pub mod emit;
pub mod graph;
pub mod readback;

pub use analyze::{analyze, AnalyzeError};
pub use check::{check, CheckResult};
pub use emit::emit_topo;
pub use graph::{Edge, Flow, Graph, Handler, TypeRef};
pub use readback::read_topo;
