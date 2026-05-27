// Body-shape contract: the placeholder design replaces the entire
// body at IR level, so a user-written body would be silently dropped
// at run-time. The macro now surfaces that mismatch at compile time.

use topo_macros::topo_pipeline;

#[topo_pipeline]
fn surprise() -> i32 {
    let v = 42;
    v + 1
}

fn main() {
    let _ = surprise();
}
