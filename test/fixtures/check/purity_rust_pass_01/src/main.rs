// Pure functions: no global writes. Parallel stage-1 functions operate
// only on locals and parameters.

fn compute_helper(a: i32, b: i32) -> i32 {
    let result = a + b;
    result
}

pub fn compute() {
    let mut local = 42;
    local = local + 1;
    let _ = compute_helper(local, 10);
}

pub fn render() {
    let x = 5;
    let y = 10;
    let _ = x + y;
}

pub fn run() {
    compute();
    render();
}
