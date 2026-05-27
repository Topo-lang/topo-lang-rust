// Adversarial: non-external function uses Arc::get_mut_unchecked.
// Expected: containment violation (unchecked aliased mutation is unsafe escape).
use std::sync::Arc;

pub fn mutate_arc(x: i32) -> i32 {
    let mut arc = Arc::new(x);
    unsafe {
        *Arc::get_mut_unchecked(&mut arc) += 1;
    }
    *arc
}
