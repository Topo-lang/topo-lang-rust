// Parallel `tick()` uses compound assignment `TICKS += 1` on a static
// mut global. `monitor()` uses simple assignment. Both are writes — both
// are violations.

static mut TICKS: i32 = 0;

pub fn tick() {
    unsafe {
        TICKS += 1;
    }
}

pub fn monitor() {
    unsafe {
        TICKS = TICKS - 1;
    }
}

pub fn run() {
    tick();
    monitor();
}
