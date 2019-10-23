// Run-time:
//   extra-args: 1
//   extra-args: 2
//   status: success

use std::env;

fn main() {
    let arg1 = env::args()
        .nth(1)
        .expect("no arg 1 passed")
        .parse::<i32>()
        .expect("arg 1 should be numeric");

    let arg2 = env::args()
        .nth(2)
        .expect("no arg 2 passed")
        .parse::<i32>()
        .expect("arg 2 should be numeric");
    assert!( arg1 < arg2)
}
