pub fn compute(x: i32) -> i32 {
    let ptr: *const i32 = &x;
    let val = unsafe { *ptr };
    val + 1
}
