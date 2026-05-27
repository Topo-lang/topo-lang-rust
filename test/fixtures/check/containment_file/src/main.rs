use std::fs;

pub fn load_data(id: i32) -> i32 {
    let _ = fs::read_to_string("data.txt");
    id * 2
}
