pub fn compute(x: i32) -> i32 {
    x * 2
}

// transform is declared in .topo but missing from source — should cause completeness error

pub fn undeclared_helper(x: i32) -> i32 {
    x + 1
}
