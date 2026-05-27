pub trait Doubler {
    fn double(&self, x: i32) -> i32;
}

pub struct Foo;

impl Doubler for Foo {
    fn double(&self, x: i32) -> i32 {
        x * 2
    }
}
