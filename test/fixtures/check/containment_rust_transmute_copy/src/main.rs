// Adversarial: non-external function calls std::mem::transmute_copy.
// Expected: containment violation (escape via mem::transmute_copy).
pub fn reinterpret(x: i32) -> i32 {
    let y: u32 = unsafe { std::mem::transmute_copy::<i32, u32>(&x) };
    y as i32
}
