// All parallel stage-1 functions are pure: they only operate on locals.
// No module-level statics are declared, so no writes can be attributed.

fn helper_double(x: i32) -> i32 {
    let result = x * 2;
    result
}

pub fn transform() {
    let mut tmp = 5;
    tmp = helper_double(tmp);
    let _ = tmp;
}

pub fn validate() {
    let a = 1;
    let b = 2;
    let sum = a + b;
    let _ = sum;
}

pub fn run() {
    transform();
    validate();
}
