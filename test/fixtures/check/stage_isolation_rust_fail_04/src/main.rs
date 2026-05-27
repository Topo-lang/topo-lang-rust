// `acquire` (stage<1>) and `transform` (stage<2>) both call `store`
// (stage<3>) directly — two forward-stage violations.

pub fn store() {
    // stage 3
}

pub fn transform() {
    store();  // violation #1: stage 2 → stage 3
}

pub fn acquire() {
    store();  // violation #2: stage 1 → stage 3
}

pub fn run() {
    acquire();
    transform();
    store();
}
