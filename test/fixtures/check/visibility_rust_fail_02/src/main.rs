// `attacker` is NOT declared in .topo — calling an `internal` function
// from it is a visibility violation.

mod core {
    pub fn initInternal() {
        // internal implementation
    }

    pub fn bootstrap() {
        initInternal();  // declared caller — OK
    }

    pub fn run() {
        bootstrap();
    }
}

pub fn attacker() {
    crate::core::initInternal();  // violation: internal called from external
}
