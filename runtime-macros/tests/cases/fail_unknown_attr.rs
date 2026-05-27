// Attr-surface contract: the attribute accepts zero tokens today;
// future additions get a real, documented surface. Any tokens in the
// attr slot today produce a compile_error pointing at them.

use topo_macros::topo_pipeline;

#[topo_pipeline(stage = "ingest")]
fn would_be_pipelined() {}

fn main() {
    would_be_pipelined();
}
