//! Safe Rust wrappers for the Topo observability runtime (libtopo-observe).
//!
//! Links to the C ABI defined in `runtime/include/topo/rt/observe_rt.h`.

#[cfg(feature = "observe")]
use std::ffi::CString;
#[cfg(feature = "observe")]
use std::os::raw::c_char;

#[cfg(feature = "observe")]
#[link(name = "topo-observe")]
extern "C" {
    fn topo_trace_init(exporter: *const c_char, sampling_rate: f64);
    fn topo_trace_shutdown();
    fn topo_trace_span_begin(name: *const c_char);
    fn topo_trace_span_end();
}

/// Initialize the tracing subsystem with the given exporter and sampling rate.
///
/// `exporter` is one of `"stdout"`, `"json"`, or `"otlp"`.
/// `sampling_rate` is a value between 0.0 and 1.0.
#[cfg(feature = "observe")]
pub fn init(exporter: &str, sampling_rate: f64) {
    let c_exporter = CString::new(exporter).expect("observe::init: exporter contains null byte");
    unsafe { topo_trace_init(c_exporter.as_ptr(), sampling_rate) }
}

#[cfg(not(feature = "observe"))]
pub fn init(exporter: &str, _sampling_rate: f64) {
    // Validate CString so null-byte panics still fire.
    let _c_exporter =
        std::ffi::CString::new(exporter).expect("observe::init: exporter contains null byte");
}

/// Shut down the tracing subsystem and flush pending spans.
#[cfg(feature = "observe")]
pub fn shutdown() {
    unsafe { topo_trace_shutdown() }
}

#[cfg(not(feature = "observe"))]
pub fn shutdown() {}

/// Begin a named tracing span.
#[cfg(feature = "observe")]
pub fn span_begin(name: &str) {
    let c_name = CString::new(name).expect("observe::span_begin: name contains null byte");
    unsafe { topo_trace_span_begin(c_name.as_ptr()) }
}

#[cfg(not(feature = "observe"))]
pub fn span_begin(name: &str) {
    // Validate CString so null-byte panics still fire.
    let _c_name =
        std::ffi::CString::new(name).expect("observe::span_begin: name contains null byte");
}

/// End the current tracing span.
#[cfg(feature = "observe")]
pub fn span_end() {
    unsafe { topo_trace_span_end() }
}

#[cfg(not(feature = "observe"))]
pub fn span_end() {}

/// RAII guard that ends a span when dropped.
///
/// Created by [`SpanGuard::new`] which calls `span_begin` immediately.
pub struct SpanGuard;

impl SpanGuard {
    /// Begin a named span and return a guard that ends it on drop.
    pub fn new(name: &str) -> Self {
        span_begin(name);
        SpanGuard
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        span_end();
    }
}

/// Execute `f` inside a named tracing span, returning its result.
pub fn span<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let _guard = SpanGuard::new(name);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_shutdown_no_panic() {
        init("stdout", 1.0);
        shutdown();
    }

    #[test]
    fn span_begin_end_no_panic() {
        span_begin("test_span");
        span_end();
    }

    #[test]
    fn span_guard_drop() {
        let guard = SpanGuard::new("guard_span");
        drop(guard);
    }

    #[test]
    fn span_closure_executes() {
        let mut executed = false;
        span("closure_span", || {
            executed = true;
        });
        assert!(executed);
    }

    #[test]
    fn span_closure_result() {
        let result = span("result_span", || 42);
        assert_eq!(result, 42);
    }

    #[test]
    #[should_panic]
    fn init_null_byte_panics() {
        init("null\0byte", 1.0);
    }

    #[test]
    #[should_panic]
    fn span_begin_null_byte_panics() {
        span_begin("null\0byte");
    }

    #[test]
    #[should_panic]
    fn span_guard_new_null_byte_panics() {
        SpanGuard::new("null\0byte");
    }
}
