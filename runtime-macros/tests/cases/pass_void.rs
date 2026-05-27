use topo_macros::topo_pipeline;

#[topo_pipeline]
fn do_nothing() {
}

fn main() {
    do_nothing();
}
