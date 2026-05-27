// Adversarial: non-external function builds a slice from a raw parts pair.
// Expected: containment violation (slice::from_raw_parts is unsafe escape).
pub fn slice_first(x: i32) -> i32 {
    let buf: [i32; 3] = [x, x + 1, x + 2];
    let ptr: *const i32 = buf.as_ptr();
    let view: &[i32] = unsafe { std::slice::from_raw_parts(ptr, 3) };
    view[0]
}
