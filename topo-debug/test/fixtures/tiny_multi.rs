// Multi-variable adapter fixture (Rust).
//
// Sibling of topo-lang-cpp/topo-debug/test/fixtures/tiny_multi.cpp. Two
// integer arrays in the same frame so `sum(a) + sum(b)` exercises the
// `--var a,b` adapter path via topo-debug-rust (same liblldb adapter
// source — see topo-lang-rust/topo-debug/CMakeLists.txt for the shared
// build wiring).
//
// `std::hint::black_box` forces rustc -C opt-level=0 to materialise both
// arrays on the stack *before* the breakpoint line. Without it the array
// initialisations are emitted lazily and the variables read back as
// uninitialised stack memory when stopped on the sentinel line — same
// quirk as tiny_vector.rs (clang's straight-line codegen avoids it for
// the C++ side, so the C++ fixture doesn't need the helper).
use std::hint::black_box;

fn main() {
    let a: [i32; 4] = [1, 2, 3, 4];        // sum=10
    let b: [i32; 4] = [10, 20, 30, 40];    // sum=100
    black_box(&a);
    black_box(&b);
    let sentinel: i32 = 0;  // breakpoint here — adapter reads both vars
    black_box(&sentinel);
    println!("done");
}
