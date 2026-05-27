// `graphics::render` reaches into `engine::backend::flushBuffers`,
// which is declared private in module engine::backend.

mod engine {
    pub fn orchestrate() {
        // no-op
    }

    pub fn run() {
        orchestrate();
    }

    pub mod backend {
        pub fn flushBuffers() {
            // private
        }
    }
}

mod graphics {
    pub fn render() {
        crate::engine::backend::flushBuffers();  // cross-module private — violation
    }
}
