// Stage isolation violation: `init` (stage<1>) calls `process` (stage<2>).

pub fn process() {
    // stage 2 work
}

pub fn init() {
    process();  // forward stage call — violation
}

pub fn run() {
    init();
    process();
}
