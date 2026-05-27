use std::process::Command;

pub fn run_task(id: i32) -> i32 {
    let _ = Command::new("echo").arg(id.to_string()).spawn();
    id
}
