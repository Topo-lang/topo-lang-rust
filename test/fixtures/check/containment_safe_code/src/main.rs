pub fn compute(x: i32, y: i32) -> i32 {
    x + y
}

pub fn sum_up(n: i32) -> i32 {
    let v: Vec<i32> = (1..=n).collect();
    v.iter().fold(0, |acc, &val| acc + val)
}
