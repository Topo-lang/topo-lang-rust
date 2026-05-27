// Adversarial: file declares `unsafe impl Send` for a raw-pointer wrapper.
// Expected: containment violation (manual Send is a soundness escape).
// Validates issue #9.
pub struct RawWrapper {
    ptr: *mut i32,
}

unsafe impl Send for RawWrapper {}

pub fn wrap(x: i32) -> i32 {
    let mut v = x;
    let _w = RawWrapper { ptr: &mut v as *mut i32 };
    x
}
