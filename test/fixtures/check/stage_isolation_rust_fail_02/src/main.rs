// `prepare` (stage<1>) directly calls `cleanup` (stage<3>), which skips
// stage<2>. Expected: one stage-isolation violation.

pub fn cleanup() {
    // stage 3 work
}

pub fn execute() {
    // stage 2 work
}

pub fn prepare() {
    cleanup();  // forward call: stage 1 → stage 3 — violation
}

pub fn run() {
    prepare();
    execute();
    cleanup();
}
