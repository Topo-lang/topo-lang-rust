// Sequential stages: `init` (stage<1>) and `finalize` (stage<2>) are
// NOT in a parallel stage so writes to module-level statics are allowed.

static mut G_STATE: i32 = 0;
static mut G_COUNT: i32 = 0;

pub fn init() {
    unsafe {
        G_STATE = 1;
        G_COUNT = G_COUNT + 1;
    }
}

pub fn finalize() {
    unsafe {
        G_STATE = 2;
        G_COUNT = G_COUNT + 10;
    }
}

pub fn run() {
    init();
    finalize();
}
