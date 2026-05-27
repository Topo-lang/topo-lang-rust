// Rust counterpart of topo-lang-cpp/topo-debug/
// test/fixtures/tiny_vector.cpp. Declares `vec: [f64; 8]` at a known line
// and pauses on the `sentinel` line so the adapter reads the array after
// initialisation. Compiled by the e2e CMake glue with rustc -g -C
// opt-level=0 -C debuginfo=2; do not run cargo, the executable goes
// straight into the build tree alongside the other adapter fixtures.
//
// `std::hint::black_box` forces rustc to materialise `vec` onto the stack
// *before* the breakpoint line — at opt-level=0 the array initialisation
// is otherwise emitted lazily and the variable reads back as uninitialised
// stack memory when stopped immediately after the declaration. The C++
// equivalent does not have this quirk because clang's straight-line code
// gen writes the array eagerly.
use std::hint::black_box;

fn main() {
    let vec: [f64; 8] = [0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5];
    black_box(&vec);
    let sentinel: i32 = 0; // breakpoint here — adapter reads `vec` at this line
    black_box(&sentinel);
    println!("done");
}
