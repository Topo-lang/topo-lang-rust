// Project-simple Rust fixture — library half (placeholder).
//
// The interesting code is in src/main.rs. `topo-build-llvm-rust` compiles
// the lib via `cargo rustc --lib --emit=llvm-bc` (no symbols to pass-optimise
// here, but the driver requires a lib target), then re-links src/main.rs
// against the optimised rlib to produce the final executable. The
// breakpoint and the `data: [i32; 8]` local live in src/main.rs.
