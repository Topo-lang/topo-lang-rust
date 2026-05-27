// Adversarial: file declares an `unsafe trait`. Since the trait header
// itself is the safety contract escape, a non-external file that contains it
// must produce a containment violation. Validates issue #9.
pub unsafe trait MyTrait {
    fn invariant(&self) -> i32;
}

pub fn compute(x: i32) -> i32 {
    x + 1
}
