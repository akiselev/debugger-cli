use std::io::{self, Write};

fn main() {
    let x = 10;
    let y = 20;
    let sum = x + y;
    print!("Hello from Rust! Sum is {}\n", sum);
    io::stdout().flush().unwrap();
}
