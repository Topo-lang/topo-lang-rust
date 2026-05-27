// Even though `compute` would be impure under purity checks, the
// Topo.toml has `[purity].mode = "off"` so the check must emit a Note
// diagnostic and return 0 regardless of the host code.

static mut IMPURITY: i32 = 0;

pub fn compute() {
    unsafe {
        IMPURITY = IMPURITY + 1;
    }
}

pub fn render() {
    unsafe {
        IMPURITY = IMPURITY * 2;
    }
}

pub fn run() {
    compute();
    render();
}
