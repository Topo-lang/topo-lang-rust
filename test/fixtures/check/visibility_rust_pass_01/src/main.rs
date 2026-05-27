// All declared functions are in mod `app` and are public — any call is
// legal. Mod name `app` matches the .topo namespace.

mod app {
    pub fn stepA() {
        // no-op
    }

    pub fn stepB() {
        stepA();  // public → public, same module: allowed
    }

    pub fn run() {
        stepA();
        stepB();
    }
}
