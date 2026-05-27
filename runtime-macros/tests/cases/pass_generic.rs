use topo_macros::topo_pipeline;

#[topo_pipeline]
fn double(_x: f64) -> Vec<f64> {}

fn main() {
    // The macro replaces the body with topo::pipeline::placeholder::<Vec<f64>>(),
    // which returns Vec::default() == [].  We only verify compilation succeeds.
    let _result = double(3.14);
}
