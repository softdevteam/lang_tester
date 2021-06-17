// Run-time:
//   extra-arg: 1
//   extra-arg: 2 3

use std::env;

fn main() {
    println!("{:?}", env::args());
    let arg1 = env::args()
        .nth(1)
        .expect("no arg 1 passed")
        .parse::<i32>()
        .expect("arg 1 should be numeric");

    let arg2 = env::args()
        .nth(2)
        .unwrap();
    assert_eq!(arg2, "2 3");
}
