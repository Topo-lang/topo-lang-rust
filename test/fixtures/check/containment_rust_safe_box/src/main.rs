// Compliance: uses Box::new only; no raw round-trip, no unsafe block.
// Expected: pass.
pub fn boxed_compute(x: i32) -> i32 {
    let boxed: Box<i32> = Box::new(x.wrapping_add(1));
    let inner: i32 = i32::clone(boxed.as_ref());
    inner
}
