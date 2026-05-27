// Project-simple Rust fixture — host binary.
//
// Companion to main.topo. The breakpoint fires inside the read-loop on
// the `let v = data[i];` line where `data` is unambiguously live and in
// DWARF scope under the -Copt-level=2 build that `topo-build-llvm-rust`
// produces.
//
// Why not breakpoint on a sentinel-style "after black_box" line as the
// cpp fixture does? rustc at -O2 emits aggressive DWARF scope ranges:
// `data` is reported "not in scope" between the declaration and the
// next read inside the same function, even when `black_box(&data)` was
// called between them. Putting the breakpoint on a line that *reads*
// `data` (the loop body) guarantees the variable's location is live and
// readable at that PC. clang doesn't have this scope-narrowing quirk so
// project_simple/main.cpp can use a separate `int sentinel` line.
//
// Data values are chosen so that the halves give visibly distinct sums:
//   first_half  (data[0..4]) = 1+2+3+4         = 10
//   second_half (data[4..8]) = 10+20+30+40     = 100
//   total       (sum(data))                    = 110
//   shape(data)                                 = [8]
//   dtype(data)                                 = i32

use std::hint::black_box;

fn main() {
    let data: [i32; 8] = [1, 2, 3, 4, 10, 20, 30, 40];
    black_box(&data);
    // Trailing summation forces the array onto the stack at -O2 and gives
    // the breakpoint line below a guaranteed in-scope read of `data`.
    let mut s: i32 = 0;
    for i in 0..8 {
        let v = data[i];  // breakpoint here — adapter reads `data`
        s = s.wrapping_add(v);
    }
    black_box(s);
    println!("data sum={}", s);
}
