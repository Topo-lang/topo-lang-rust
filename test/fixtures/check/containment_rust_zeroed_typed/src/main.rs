// Adversarial: non-external function calls std::mem::zeroed::<MyType>().
// Expected: containment violation (escape via mem::zeroed).
struct MyType {
    a: i32,
    b: i32,
}

pub fn make_zero(x: i32) -> i32 {
    let z: MyType = unsafe { std::mem::zeroed::<MyType>() };
    z.a + z.b + x
}
