// Project-multi Rust fixture — host binary.
//
// Companion to main.topo. Two arrays in the same frame so the summary
// template references both via `{sum(data_a)}` / `{sum(data_b)}` /
// `{sum(data_a)+sum(data_b)}` / `{max(data_b)-max(data_a)}` — all resolved
// by a single adapter spawn via the multi-var protocol + summary batching.
//
// As in project_simple/main.rs, the breakpoint sits on the loop-body
// line that *reads* both arrays — at -Copt-level=2 rustc's DWARF scope
// ranges report `data_a` / `data_b` "not in scope" between their
// declarations and the next observable use even with intervening
// `black_box(&...)` calls. The loop body is a guaranteed-live read of
// both names so lldb resolves them at the breakpoint PC.

use std::hint::black_box;

fn main() {
    let data_a: [i32; 4] = [1, 2, 3, 4];        // sum=10,  max=4
    let data_b: [i32; 4] = [10, 20, 30, 40];    // sum=100, max=40
    black_box(&data_a);
    black_box(&data_b);
    // Trailing summation keeps both arrays live past the breakpoint and
    // gives the breakpoint line below a guaranteed in-scope read of each.
    let mut t: i32 = 0;
    for i in 0..4 {
        let pair = (data_a[i], data_b[i]);  // breakpoint here — reads both
        t = t.wrapping_add(pair.0).wrapping_add(pair.1);
    }
    black_box(t);
    println!("a+b sum={}", t);
}
