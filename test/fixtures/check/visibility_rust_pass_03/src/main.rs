// Private helpers called only from within the same module.

mod lib {
    pub fn normalize() {
        // private
    }

    pub fn helper() {
        normalize();  // private → private, same module — OK
    }

    pub fn compute() {
        helper();      // public → private, same module — OK
        normalize();   // public → private, same module — OK
    }

    pub fn finalize() {
        helper();      // public → private, same module — OK
    }

    pub fn run() {
        compute();
        finalize();
    }
}
