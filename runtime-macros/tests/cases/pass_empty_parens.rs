// `#[topo_pipeline()]` with empty parens MUST be accepted as equivalent
// to the bareword `#[topo_pipeline]`. Pins the contract that the attr
// surface is "no tokens", not "no parens".

use topo_macros::topo_pipeline;

#[topo_pipeline()]
fn noop() {}

fn main() {
    noop();
}
