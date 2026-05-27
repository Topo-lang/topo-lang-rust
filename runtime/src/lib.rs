//! Topo runtime bindings for Rust.
//!
//! Provides safe Rust wrappers around the Topo C ABI runtime libraries:
//! - `parallel` — thread pool task spawn/await + cost sampling
//! - `adaptive` — runtime-driven re-specialization
//! - `jit` — JIT specialization (requires "jit" feature)
//! - `pipeline` — pipeline placeholder for PipelineCodeGenPass
//! - `arena` — bump allocator for scoped lifetime regions
//! - `observe` — tracing span instrumentation
//! - `config` — product runtime configuration (layered model + Rust
//!   TOML bridge); pure-language, no C ABI runtime, no LLVM

pub mod parallel;
pub mod adaptive;
pub mod jit;
pub mod pipeline;
pub mod arena;
pub mod observe;

// The product runtime configuration. `config_model` is the
// language-agnostic core (pure semantics, no I/O, no TOML); `config` is
// the Rust ecosystem bridge (the `toml` crate decode/encode +
// `ProductConfig`). Behaviour parity with
// `topo-lang-python/runtime/topo/_config_model.py` + `config.py`.
pub mod config_model;
pub mod config;

// topo-app: the quick-start handler/flow framework, Rust projection.
// A *runtime registration bridge* (not a static-analysis front-end):
// describe handlers/flows with idiomatic Rust, get a round-trippable
// `.topo` the existing fresh toolchain parses and checks. Behaviour
// parity with `topo-lang-python/runtime/topo/` (app/_graph/_reflect/
// _emit/_readback/check), vertical-slice T1–T5. Pure-language: no LLVM,
// no C ABI runtime; the only external processes are the fresh `topo`
// and `topo-check` binaries it consumes.
pub mod schema;
pub mod graph;
pub mod app;
pub mod emit;
pub mod readback;
pub mod check;
pub mod toolchain;

#[cfg(feature = "macros")]
pub use topo_macros::topo_pipeline;
