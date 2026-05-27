// Both `taskA` and `taskB` are in stage<1>. Same-stage calls are allowed.

pub fn taskB() {
    // stage 1 work
}

pub fn taskA() {
    taskB();  // same-stage call — OK
}

pub fn run() {
    taskA();
    taskB();
}
