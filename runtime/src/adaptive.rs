//! Safe Rust wrappers for the Topo adaptive runtime (libtopo-adaptive).
//!
//! Links to the C ABI defined in `runtime/include/topo/rt/adaptive_rt.h`.

use std::ffi::c_void;
use std::ffi::CString;
#[cfg(feature = "adaptive")]
use std::os::raw::c_char;

#[cfg(feature = "adaptive")]
#[link(name = "topo-adaptive")]
extern "C" {
    fn topo_adaptive_register(
        mangled_name: *const c_char,
        pipeline_name: *const c_char,
        jit_ptr: *mut *mut c_void,
        aot_tti_cost: u64,
    );
    fn topo_adaptive_init();
    fn topo_adaptive_shutdown();
}

/// Initialize the adaptive monitoring thread.
#[cfg(feature = "adaptive")]
pub fn init() {
    unsafe { topo_adaptive_init() }
}

#[cfg(not(feature = "adaptive"))]
pub fn init() {}

/// Shut down the adaptive monitoring thread.
#[cfg(feature = "adaptive")]
pub fn shutdown() {
    unsafe { topo_adaptive_shutdown() }
}

#[cfg(not(feature = "adaptive"))]
pub fn shutdown() {}

/// Register a pipeline for adaptive dispatch.
///
/// # Safety
/// - `jit_ptr` must point to a valid atomic function pointer global.
#[cfg(feature = "adaptive")]
pub unsafe fn register(
    mangled_name: &str,
    pipeline_name: &str,
    jit_ptr: *mut *mut c_void,
    aot_tti_cost: u64,
) {
    let c_mangled = CString::new(mangled_name)
        .expect("register: mangled_name contains null byte");
    let c_pipeline = CString::new(pipeline_name)
        .expect("register: pipeline_name contains null byte");
    topo_adaptive_register(c_mangled.as_ptr(), c_pipeline.as_ptr(), jit_ptr, aot_tti_cost);
}

#[cfg(not(feature = "adaptive"))]
pub unsafe fn register(
    mangled_name: &str,
    pipeline_name: &str,
    _jit_ptr: *mut *mut c_void,
    _aot_tti_cost: u64,
) {
    let _c_mangled = CString::new(mangled_name)
        .expect("register: mangled_name contains null byte");
    let _c_pipeline = CString::new(pipeline_name)
        .expect("register: pipeline_name contains null byte");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_shutdown_no_panic() {
        init();
        shutdown();
    }

    #[test]
    #[should_panic]
    fn register_null_byte_mangled_panics() {
        unsafe {
            register("bad\0name", "pipeline", std::ptr::null_mut(), 0);
        }
    }

    #[test]
    #[should_panic]
    fn register_null_byte_pipeline_panics() {
        unsafe {
            register("mangled", "bad\0pipe", std::ptr::null_mut(), 0);
        }
    }
}
