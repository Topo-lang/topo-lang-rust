// `engine::coordinate` (public) calls `engine::detail` (private) from
// the same module — visibility check must allow this.

mod engine {
    pub fn detail() {
        // private implementation
    }

    pub fn coordinate() {
        detail();  // same-module private call — OK
    }

    pub fn run() {
        coordinate();
    }
}
