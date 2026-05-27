static mut COUNTER: i32 = 0;

pub fn parallel_work(n: i32) -> i32 {
    unsafe {
        COUNTER += n;
        COUNTER
    }
}
