// `init` and `process` never call each other — the host call graph
// respects the stage ordering declared in .topo.

pub fn init() {
    let mut local = 0;
    local = local + 1;
    let _ = local;
}

pub fn process() {
    let tmp = 42;
    let _ = tmp;
}

pub fn run() {
    init();
    process();
}
