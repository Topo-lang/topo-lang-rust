// Adversarial: non-external function rebuilds a Box from a raw pointer.
// Expected: containment violation (Box::from_raw is unsafe escape).
pub fn round_trip(x: i32) -> i32 {
    let boxed = Box::new(x);
    let raw: *mut i32 = Box::into_raw(boxed);
    let restored: Box<i32> = unsafe { Box::from_raw(raw) };
    *restored
}
