pub fn compute(x: i32) -> i32 {
    let val: i32;
    unsafe {
        val = x + 1;
    }
    val
}
