extern "C" {
    fn abs(input: i32) -> i32;
}

pub fn compute(x: i32) -> i32 {
    unsafe { abs(x) }
}
