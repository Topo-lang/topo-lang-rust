//! JIT specialization bindings for Rust.
//!
//! The Topo JIT engine (libtopo-jit) provides runtime specialization of
//! pipeline functions using LLVM OrcJIT. The C++ API (`topo::jit::Context`,
//! `topo::jit::specialize`) operates through C++ types (std::string, etc.)
//! that cannot be called directly from Rust via FFI.
//!
//! This module exposes a C ABI subset defined in the companion C wrapper.
//! The full JIT API (context building, constraint narrowing) requires the
//! C++ side; Rust users can check availability and trigger specialization
//! through the simplified interface below.

#[cfg(feature = "jit")]
use std::ffi::CString;
#[cfg(feature = "jit")]
use std::os::raw::c_char;

// C ABI functions provided by libtopo-jit.
// These are thin C wrappers around the C++ topo::jit namespace.
// If building without JIT support, these symbols will be absent and
// linking will fail — use the "jit" feature to opt in.
#[cfg(feature = "jit")]
#[link(name = "topo-jit")]
extern "C" {
    fn topo_jit_available() -> i32;
    fn topo_jit_specialize_simple(pipeline_name: *const c_char) -> i32;
}

/// Check if the JIT engine is available on this platform.
///
/// Returns `true` if the binary was built with embedded IR and the
/// LLVM OrcJIT backend is functional.
#[cfg(feature = "jit")]
pub fn available() -> bool {
    unsafe { topo_jit_available() != 0 }
}

#[cfg(not(feature = "jit"))]
pub fn available() -> bool {
    false
}

/// Trigger JIT specialization for a named pipeline using default constraints.
///
/// Returns `true` if specialization was initiated successfully.
/// The actual compilation happens asynchronously; the specialized version
/// will replace the AOT version via atomic function pointer swap.
#[cfg(feature = "jit")]
pub fn specialize_simple(pipeline_name: &str) -> bool {
    let c_name = CString::new(pipeline_name)
        .expect("specialize_simple: pipeline_name contains null byte");
    unsafe { topo_jit_specialize_simple(c_name.as_ptr()) != 0 }
}

#[cfg(not(feature = "jit"))]
pub fn specialize_simple(_pipeline_name: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test the non-jit fallback paths (no FFI linking needed)

    #[test]
    fn available_returns_false_without_jit_feature() {
        // When compiled without "jit" feature (default), available() returns false
        // This test runs in default feature configuration
        assert!(!available());
    }

    #[test]
    fn specialize_simple_returns_false_without_jit_feature() {
        // Without JIT feature, specialization always returns false
        assert!(!specialize_simple("test_pipeline"));
    }

    #[test]
    fn specialize_simple_with_empty_name() {
        assert!(!specialize_simple(""));
    }

    #[test]
    fn specialize_simple_with_special_chars() {
        // Pipeline names with special characters should not panic
        assert!(!specialize_simple("pipeline::with::colons"));
        assert!(!specialize_simple("pipeline-with-dashes"));
        assert!(!specialize_simple("pipeline_with_underscores"));
    }
}
