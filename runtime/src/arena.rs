//! Safe Rust wrappers for the Topo arena allocator (libtopo-arena).
//!
//! Links to the C ABI defined in `runtime/include/topo/rt/arena_rt.h`.
//! When building without the `arena` feature, a pure-Rust bump allocator
//! stub is used instead, so tests and downstream crates can compile
//! without linking libtopo-arena.

use std::mem;

// ---------------------------------------------------------------------------
// Real FFI implementation (feature = "arena")
// ---------------------------------------------------------------------------

#[cfg(feature = "arena")]
use std::ffi::c_void;

#[cfg(feature = "arena")]
#[link(name = "topo-arena")]
extern "C" {
    fn topo_arena_create(initial_capacity: usize) -> *mut c_void;
    fn topo_arena_alloc(arena: *mut c_void, size: usize, align: usize) -> *mut c_void;
    fn topo_arena_reset(arena: *mut c_void);
    fn topo_arena_destroy(arena: *mut c_void);
    fn topo_arena_bytes_used(arena: *mut c_void) -> usize;
    fn topo_arena_capacity(arena: *mut c_void) -> usize;
}

/// Opaque arena handle wrapping a `topo_arena_t`.
///
/// Automatically destroys the underlying arena when dropped.
#[cfg(feature = "arena")]
pub struct Arena(*mut c_void);

// Safety: The topo arena implementation is thread-safe.
#[cfg(feature = "arena")]
unsafe impl Send for Arena {}

#[cfg(feature = "arena")]
impl Arena {
    /// Create a new arena with the given initial capacity in bytes.
    pub fn new(capacity: usize) -> Self {
        let handle = unsafe { topo_arena_create(capacity) };
        assert!(!handle.is_null(), "topo_arena_create returned null");
        Arena(handle)
    }

    /// Allocate space for a single `T` from the arena, returning a raw pointer.
    ///
    /// The returned memory is uninitialized. The caller must write a valid `T`
    /// before reading.
    pub fn alloc<T>(&self) -> *mut T {
        let ptr = unsafe { topo_arena_alloc(self.0, mem::size_of::<T>(), mem::align_of::<T>()) };
        ptr as *mut T
    }

    /// Reset the arena, reclaiming all allocations without releasing the
    /// backing memory.
    pub fn reset(&self) {
        unsafe { topo_arena_reset(self.0) }
    }

    /// Return the number of bytes currently allocated from this arena.
    pub fn bytes_used(&self) -> usize {
        unsafe { topo_arena_bytes_used(self.0) }
    }

    /// Return the total capacity of this arena in bytes.
    pub fn capacity(&self) -> usize {
        unsafe { topo_arena_capacity(self.0) }
    }
}

#[cfg(feature = "arena")]
impl Drop for Arena {
    fn drop(&mut self) {
        unsafe { topo_arena_destroy(self.0) }
    }
}

// ---------------------------------------------------------------------------
// Stub bump-allocator implementation (no "arena" feature)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "arena"))]
struct StubArena {
    buf: Vec<u8>,
    offset: std::cell::Cell<usize>,
}

/// Opaque arena handle backed by a pure-Rust bump allocator.
///
/// This stub is API-compatible with the real FFI-backed `Arena` so that
/// downstream code compiles and tests pass without libtopo-arena.
#[cfg(not(feature = "arena"))]
pub struct Arena(Box<StubArena>);

#[cfg(not(feature = "arena"))]
unsafe impl Send for Arena {}

#[cfg(not(feature = "arena"))]
impl Arena {
    /// Create a new arena with the given initial capacity in bytes.
    pub fn new(capacity: usize) -> Self {
        Arena(Box::new(StubArena {
            buf: vec![0u8; capacity],
            offset: std::cell::Cell::new(0),
        }))
    }

    /// Allocate space for a single `T` from the arena, returning a raw pointer.
    ///
    /// The returned memory is zero-initialized. Panics if the arena does not
    /// have enough remaining capacity.
    pub fn alloc<T>(&self) -> *mut T {
        let align = mem::align_of::<T>();
        let size = mem::size_of::<T>();
        let cur = self.0.offset.get();
        let aligned = (cur + align - 1) & !(align - 1);
        assert!(
            aligned + size <= self.0.buf.len(),
            "arena: out of memory"
        );
        self.0.offset.set(aligned + size);
        unsafe { self.0.buf.as_ptr().add(aligned) as *mut T }
    }

    /// Reset the arena, reclaiming all allocations without releasing the
    /// backing memory.
    pub fn reset(&self) {
        self.0.offset.set(0);
    }

    /// Return the number of bytes currently allocated from this arena.
    pub fn bytes_used(&self) -> usize {
        self.0.offset.get()
    }

    /// Return the total capacity of this arena in bytes.
    pub fn capacity(&self) -> usize {
        self.0.buf.len()
    }
}

#[cfg(not(feature = "arena"))]
impl Drop for Arena {
    fn drop(&mut self) {
        // StubArena is dropped automatically via Box.
    }
}

// ---------------------------------------------------------------------------
// Tests (run against the stub allocator by default)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_drop() {
        let _arena = Arena::new(1024);
        // Dropping without any allocations must not panic.
    }

    #[test]
    fn alloc_returns_aligned_pointer() {
        let arena = Arena::new(1024);
        let ptr = arena.alloc::<u64>();
        assert_eq!(
            ptr as usize % mem::align_of::<u64>(),
            0,
            "pointer must be aligned to u64"
        );
    }

    #[test]
    fn bytes_used_increases() {
        let arena = Arena::new(1024);
        arena.alloc::<u32>();
        arena.alloc::<u32>();
        assert!(arena.bytes_used() > 0, "bytes_used must increase after allocations");
    }

    #[test]
    fn reset_reclaims() {
        let arena = Arena::new(1024);
        arena.alloc::<u32>();
        assert!(arena.bytes_used() > 0);
        arena.reset();
        assert_eq!(arena.bytes_used(), 0, "bytes_used must be 0 after reset");
    }

    #[test]
    fn capacity_matches() {
        let arena = Arena::new(4096);
        assert_eq!(arena.capacity(), 4096);
    }

    #[test]
    fn arena_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Arena>();
    }
}
