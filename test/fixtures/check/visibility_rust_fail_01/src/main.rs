// `other::invoke` calls the private `app::helper`, which crosses a
// module boundary.

mod app {
    pub fn helper() {
        // private implementation detail
    }

    pub fn compute() {
        helper();  // same-module private call — OK
    }

    pub fn run() {
        compute();
    }
}

mod other {
    pub fn invoke() {
        crate::app::helper();  // violation: cross-module private call
    }
}
