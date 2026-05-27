// Compliance: borrow-based references (&[i32], &mut [i32]) only, no raw
// pointers, no unsafe blocks, no escape calls. Expected: pass.
fn first(slice: &[i32]) -> i32 {
    slice[0]
}

fn bump_first(slice: &mut [i32]) {
    slice[0] = slice[0].wrapping_add(1);
}

pub fn pass_by_ref(x: i32) -> i32 {
    let buf: [i32; 1] = [x];
    first(&buf)
}

pub fn mutate_by_ref(x: i32) -> i32 {
    let mut buf: [i32; 1] = [x];
    bump_first(&mut buf);
    buf[0]
}
