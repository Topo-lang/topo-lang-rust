//! Safe Rust wrappers for the Topo parallel runtime (libtopo-parallel).
//!
//! Links to the C ABI defined in `runtime/include/topo/rt/parallel_rt.h`.
//! When built without the `parallel` feature, all functions degrade to
//! synchronous stubs (no FFI linking required).

use std::ffi::{c_void, CString};

#[cfg(feature = "parallel")]
use std::os::raw::{c_char, c_int};

/// Opaque task handle returned by spawn functions.
pub struct TaskHandle(#[allow(dead_code)] *mut c_void);

// Safety: TaskHandle is a wrapper around a pointer to a thread-safe topo_task.
unsafe impl Send for TaskHandle {}

#[cfg(feature = "parallel")]
#[link(name = "topo-parallel")]
extern "C" {
    fn topo_parallel_init(num_threads: c_int);
    fn topo_parallel_shutdown();
    fn topo_parallel_ensure_init();
    fn topo_task_spawn(func: extern "C" fn(*mut c_void), arg: *mut c_void) -> *mut c_void;
    fn topo_task_spawn_ret(
        func: extern "C" fn(*mut c_void, *mut c_void),
        arg: *mut c_void,
        result_buf: *mut c_void,
        result_size: usize,
    ) -> *mut c_void;
    fn topo_task_await(task: *mut c_void);
    fn topo_task_await_all(tasks: *mut *mut c_void, count: c_int);
    fn topo_cost_begin(func_name: *const c_char);
    fn topo_cost_end(func_name: *const c_char);
    fn topo_task_spawn_ret_pri(
        func: extern "C" fn(*mut c_void, *mut c_void),
        arg: *mut c_void,
        result_buf: *mut c_void,
        result_size: usize,
        priority: c_int,
    ) -> *mut c_void;
}

/// Task priority levels for `spawn_ret_pri`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
    Background = 4,
}

// ---------------------------------------------------------------------------
// init / shutdown / ensure_init
// ---------------------------------------------------------------------------

/// Initialize the Topo parallel runtime with the given number of threads.
/// Pass 0 to use the hardware concurrency.
#[cfg(feature = "parallel")]
pub fn init(num_threads: u32) {
    unsafe { topo_parallel_init(num_threads as c_int) }
}

#[cfg(not(feature = "parallel"))]
pub fn init(_num_threads: u32) {}

/// Shut down the parallel runtime and join all worker threads.
#[cfg(feature = "parallel")]
pub fn shutdown() {
    unsafe { topo_parallel_shutdown() }
}

#[cfg(not(feature = "parallel"))]
pub fn shutdown() {}

/// Ensure the runtime is initialized (lazy init).
#[cfg(feature = "parallel")]
pub fn ensure_init() {
    unsafe { topo_parallel_ensure_init() }
}

#[cfg(not(feature = "parallel"))]
pub fn ensure_init() {}

// ---------------------------------------------------------------------------
// spawn (unsafe)
// ---------------------------------------------------------------------------

/// Spawn a task on the thread pool. Returns a handle for awaiting.
///
/// # Safety
/// The callback and argument must remain valid until the task completes.
#[cfg(feature = "parallel")]
pub unsafe fn spawn(func: extern "C" fn(*mut c_void), arg: *mut c_void) -> TaskHandle {
    TaskHandle(topo_task_spawn(func, arg))
}

/// Stub: executes the callback synchronously and returns a null handle.
///
/// # Safety
/// The callback and argument must remain valid for the duration of this call.
#[cfg(not(feature = "parallel"))]
pub unsafe fn spawn(func: extern "C" fn(*mut c_void), arg: *mut c_void) -> TaskHandle {
    func(arg);
    TaskHandle(std::ptr::null_mut())
}

// ---------------------------------------------------------------------------
// spawn_ret (unsafe)
// ---------------------------------------------------------------------------

/// Spawn a task that writes a result into `result_buf`.
///
/// # Safety
/// The callback, argument, and result buffer must remain valid until await.
#[cfg(feature = "parallel")]
pub unsafe fn spawn_ret(
    func: extern "C" fn(*mut c_void, *mut c_void),
    arg: *mut c_void,
    result_buf: *mut c_void,
    result_size: usize,
) -> TaskHandle {
    TaskHandle(topo_task_spawn_ret(func, arg, result_buf, result_size))
}

/// Stub: executes the callback synchronously and returns a null handle.
///
/// # Safety
/// The callback, argument, and result buffer must remain valid for the
/// duration of this call.
#[cfg(not(feature = "parallel"))]
pub unsafe fn spawn_ret(
    func: extern "C" fn(*mut c_void, *mut c_void),
    arg: *mut c_void,
    result_buf: *mut c_void,
    _result_size: usize,
) -> TaskHandle {
    func(arg, result_buf);
    TaskHandle(std::ptr::null_mut())
}

// ---------------------------------------------------------------------------
// spawn_ret_pri (unsafe)
// ---------------------------------------------------------------------------

/// Spawn a prioritized task that writes a result into `result_buf`.
///
/// # Safety
/// The callback, argument, and result buffer must remain valid until await.
#[cfg(feature = "parallel")]
pub unsafe fn spawn_ret_pri(
    func: extern "C" fn(*mut c_void, *mut c_void),
    arg: *mut c_void,
    result_buf: *mut c_void,
    result_size: usize,
    priority: Priority,
) -> TaskHandle {
    TaskHandle(topo_task_spawn_ret_pri(
        func,
        arg,
        result_buf,
        result_size,
        priority as c_int,
    ))
}

/// Stub: executes the callback synchronously, ignoring priority.
///
/// # Safety
/// The callback, argument, and result buffer must remain valid for the
/// duration of this call.
#[cfg(not(feature = "parallel"))]
pub unsafe fn spawn_ret_pri(
    func: extern "C" fn(*mut c_void, *mut c_void),
    arg: *mut c_void,
    result_buf: *mut c_void,
    _result_size: usize,
    _priority: Priority,
) -> TaskHandle {
    func(arg, result_buf);
    TaskHandle(std::ptr::null_mut())
}

// ---------------------------------------------------------------------------
// await_task / await_all
// ---------------------------------------------------------------------------

/// Wait for a specific task to complete. Consumes the handle.
#[cfg(feature = "parallel")]
pub fn await_task(handle: TaskHandle) {
    unsafe { topo_task_await(handle.0) }
}

#[cfg(not(feature = "parallel"))]
pub fn await_task(_handle: TaskHandle) {}

/// Wait for all tasks to complete. Consumes all handles.
#[cfg(feature = "parallel")]
pub fn await_all(handles: &mut [TaskHandle]) {
    let mut ptrs: Vec<*mut c_void> = handles.iter().map(|h| h.0).collect();
    unsafe { topo_task_await_all(ptrs.as_mut_ptr(), ptrs.len() as c_int) }
}

#[cfg(not(feature = "parallel"))]
pub fn await_all(_handles: &mut [TaskHandle]) {}

// ---------------------------------------------------------------------------
// cost_begin / cost_end
// ---------------------------------------------------------------------------

/// Begin cost sampling for a named function.
#[cfg(feature = "parallel")]
pub fn cost_begin(name: &str) {
    let c_name = CString::new(name).expect("cost_begin: name contains null byte");
    unsafe { topo_cost_begin(c_name.as_ptr()) }
}

#[cfg(not(feature = "parallel"))]
pub fn cost_begin(name: &str) {
    let _c_name = CString::new(name).expect("cost_begin: name contains null byte");
}

/// End cost sampling for a named function.
#[cfg(feature = "parallel")]
pub fn cost_end(name: &str) {
    let c_name = CString::new(name).expect("cost_end: name contains null byte");
    unsafe { topo_cost_end(c_name.as_ptr()) }
}

#[cfg(not(feature = "parallel"))]
pub fn cost_end(name: &str) {
    let _c_name = CString::new(name).expect("cost_end: name contains null byte");
}

// ---------------------------------------------------------------------------
// High-level safe API (unconditional — delegates to gated primitives)
// ---------------------------------------------------------------------------

/// Execute a closure on the thread pool and return its result.
///
/// This is a safe, high-level wrapper that boxes the closure and result,
/// handling all FFI details internally.
pub fn run<T: Send + Default>(f: impl FnOnce() -> T + Send + 'static) -> T {
    extern "C" fn trampoline<T: Send>(arg: *mut c_void, out: *mut c_void) {
        let f = unsafe { Box::from_raw(arg as *mut Box<dyn FnOnce() -> T + Send>) };
        let result = f();
        unsafe { std::ptr::write(out as *mut T, result) };
    }

    ensure_init();
    let mut result = T::default();
    let erased: Box<dyn FnOnce() -> T + Send> = Box::new(f);
    let boxed = Box::into_raw(Box::new(erased));
    let handle = unsafe {
        spawn_ret(
            trampoline::<T>,
            boxed as *mut c_void,
            &mut result as *mut T as *mut c_void,
            std::mem::size_of::<T>(),
        )
    };
    await_task(handle);
    result
}

/// Execute multiple closures in parallel and collect their results.
///
/// Returns a `Vec<T>` with one result per closure, in order.
pub fn run_all<T: Send + Default>(tasks: Vec<Box<dyn FnOnce() -> T + Send>>) -> Vec<T> {
    extern "C" fn trampoline<T: Send>(arg: *mut c_void, out: *mut c_void) {
        let f = unsafe { Box::from_raw(arg as *mut Box<dyn FnOnce() -> T + Send>) };
        let result = f();
        unsafe { std::ptr::write(out as *mut T, result) };
    }

    ensure_init();
    let n = tasks.len();
    let mut results: Vec<T> = (0..n).map(|_| T::default()).collect();
    let mut handles = Vec::with_capacity(n);

    for (i, f) in tasks.into_iter().enumerate() {
        let boxed = Box::into_raw(Box::new(f));
        let handle = unsafe {
            spawn_ret(
                trampoline::<T>,
                boxed as *mut c_void,
                &mut results[i] as *mut T as *mut c_void,
                std::mem::size_of::<T>(),
            )
        };
        handles.push(handle);
    }

    await_all(&mut handles);
    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_values() {
        assert_eq!(Priority::Critical as i32, 0);
        assert_eq!(Priority::High as i32, 1);
        assert_eq!(Priority::Normal as i32, 2);
        assert_eq!(Priority::Low as i32, 3);
        assert_eq!(Priority::Background as i32, 4);
    }

    #[test]
    fn run_returns_result() {
        assert_eq!(run(|| 42), 42);
    }

    #[test]
    fn run_all_ordered_results() {
        let tasks: Vec<Box<dyn FnOnce() -> i32 + Send>> = vec![
            Box::new(|| 1),
            Box::new(|| 2),
            Box::new(|| 3),
        ];
        assert_eq!(run_all(tasks), vec![1, 2, 3]);
    }

    #[test]
    fn run_all_empty() {
        let tasks: Vec<Box<dyn FnOnce() -> i32 + Send>> = vec![];
        let results = run_all::<i32>(tasks);
        assert!(results.is_empty());
    }

    #[test]
    fn cost_begin_end_no_panic() {
        cost_begin("f");
        cost_end("f");
    }

    #[test]
    #[should_panic(expected = "cost_begin: name contains null byte")]
    fn cost_begin_null_byte_panics() {
        cost_begin("bad\0name");
    }

    #[test]
    #[should_panic(expected = "cost_end: name contains null byte")]
    fn cost_end_null_byte_panics() {
        cost_end("bad\0name");
    }

    #[test]
    fn task_handle_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<TaskHandle>();
    }
}
