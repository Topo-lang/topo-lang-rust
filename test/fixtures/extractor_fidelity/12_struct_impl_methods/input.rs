pub struct Counter {
    value: i32,
}

impl Counter {
    pub fn new() -> Counter {
        Counter { value: 0 }
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }
}
