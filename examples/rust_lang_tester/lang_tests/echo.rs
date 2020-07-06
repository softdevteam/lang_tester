// Run-time:
//   status: success
//   stdin: abc
//   stdout: Hello abc

use std::io::{Read, stdin};

fn main() {
    let mut buf = String::new();
    stdin().read_to_string(&mut buf).unwrap();
    println!("Hello {}", buf);
}
