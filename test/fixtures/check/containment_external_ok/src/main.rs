use std::fs;

pub fn read_file(id: i32) -> i32 {
    let _ = fs::read_to_string("data.txt");
    0
}

pub fn process(x: i32) -> i32 {
    x * 2
}
