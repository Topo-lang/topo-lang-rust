// Three parallel-stage functions each write a different module-level
// static. Expected: 3 purity errors.

static mut BUFFER: i32 = 0;
static mut PROCESSED: i32 = 0;
static mut TOTAL_BYTES: i32 = 0;

pub fn producer() {
    unsafe {
        BUFFER = 1;
    }
}

pub fn consumer() {
    unsafe {
        PROCESSED = PROCESSED + 1;
    }
}

pub fn sideEffect() {
    unsafe {
        TOTAL_BYTES += 100;
    }
}

pub fn run() {
    producer();
    consumer();
    sideEffect();
}
