// Would be a violation under `mode = "force"` — an undeclared module
// calls `app::secret` from outside. With visibility off, no errors are produced.

mod app {
    pub fn secret() {
        // private
    }

    pub fn api() {
        // no-op
    }

    pub fn run() {
        api();
    }
}

mod outside {
    pub fn trespass() {
        crate::app::secret();  // would violate — but mode=off
    }
}
