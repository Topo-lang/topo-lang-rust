// Parallel stage function `compute` writes to a module-level `static mut`.
// Expected: purity violation for `compute` in parallel stage<1>.

static mut COUNTER: i32 = 0;

pub fn compute() {
    unsafe {
        COUNTER = COUNTER + 1;
    }
}

pub fn render() {
    // pure: no global access
}

pub fn run() {
    compute();
    render();
}
