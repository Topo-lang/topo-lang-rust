// `process` (stage<2>) calls `init` (stage<1>). Backward stage calls
// are allowed — the later stage reads earlier-stage outputs.

pub fn init() {
    // stage 1 setup
}

pub fn process() {
    init();  // backward call (stage 2 → stage 1) — OK
}

pub fn run() {
    init();
    process();
}
