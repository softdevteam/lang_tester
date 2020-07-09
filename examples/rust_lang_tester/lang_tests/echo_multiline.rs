// Run-time:
//   status: success
//   stdin:
//     a
//     b
//     c
//   stdout:
//     Hello a
//     b
//     c

use std::io::{Read, stdin};

fn main() {
    let mut buf = String::new();
    stdin().read_to_string(&mut buf).unwrap();
    println!("Hello {}", buf);
}
