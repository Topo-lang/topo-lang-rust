// Both `loadA` and `loadB` (stage<1>) call `merge` (stage<2>) — two
// forward stage violations in the same fn block.

pub fn merge() {
    // stage 2
}

pub fn loadA() {
    merge();  // violation #1: stage 1 → stage 2
}

pub fn loadB() {
    merge();  // violation #2: stage 1 → stage 2
}

pub fn run() {
    loadA();
    loadB();
    merge();
}
