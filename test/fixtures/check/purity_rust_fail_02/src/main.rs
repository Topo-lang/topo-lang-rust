// Parallel stage function `stepA` writes to a `thread_local!` static —
// thread_local statics still violate parallel-purity because each call
// site mutates the shared logical state.

use std::cell::Cell;

thread_local! {
    static SHARED_STATE: Cell<i32> = Cell::new(0);
}

// A non-thread_local mutable static for the second pass exposes a second
// write that the extractor must catch.
static mut COUNTER: i32 = 0;

pub fn stepA() {
    // Direct module-level static mut write.
    unsafe {
        COUNTER = 42;
    }
}

pub fn stepB() {
    let local = 1;
    let _ = local;
}

pub fn run() {
    stepA();
    stepB();
}
