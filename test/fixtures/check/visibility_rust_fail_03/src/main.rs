// `consumer::drive` reaches into both `lib::alpha` and `lib::beta`,
// each declared private in module lib. Two distinct violations.

mod lib {
    pub fn alpha() {
        // private
    }

    pub fn beta() {
        // private
    }

    pub fn api() {
        alpha();  // same-module — OK
        beta();   // same-module — OK
    }

    pub fn run() {
        api();
    }
}

mod consumer {
    pub fn drive() {
        crate::lib::alpha();  // cross-module private — violation #1
        crate::lib::beta();   // cross-module private — violation #2
        crate::lib::api();    // public — OK
    }
}
