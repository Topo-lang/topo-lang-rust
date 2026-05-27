use topo_macros::topo_pipeline;

#[topo_pipeline]
fn add_one(_x: i32) -> i32 {}

fn main() {
    // The macro replaces the body with topo::pipeline::placeholder::<i32>(),
    // which returns i32::default() == 0.  We only verify compilation succeeds
    // and the function is callable.
    let _result = add_one(5);
}
