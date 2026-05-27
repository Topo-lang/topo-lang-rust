use std::net::TcpStream;

pub fn connect(port: i32) -> i32 {
    let addr = format!("127.0.0.1:{}", port);
    let _ = TcpStream::connect(&addr);
    0
}
