//! Pipeline placeholder for Rust.
//!
//! The `placeholder()` function serves as a stub that PipelineCodeGenPass
//! detects and replaces with generated pipeline logic at the LLVM IR level.

/// Pipeline placeholder — function body will be replaced by PipelineCodeGenPass.
///
/// Usage:
/// ```rust
/// pub fn process(input: i32) -> i32 {
///     topo::pipeline::placeholder::<i32>()
/// }
/// ```
///
/// Or use the `#[topo::topo_pipeline]` proc macro for a cleaner syntax.
#[inline(never)]
pub fn placeholder<T: Default>() -> T {
    T::default()
}
