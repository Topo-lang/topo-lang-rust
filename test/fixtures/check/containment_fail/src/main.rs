use std::fs;

pub fn compute(x: i32) -> i32 {
    let _ = fs::read_to_string("data.txt");
    x * 2
}
