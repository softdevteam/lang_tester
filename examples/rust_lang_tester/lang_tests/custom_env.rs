// Run-time:
//   env-var: XYZ=123
//   env-var: XYZ=456
//   env-var: ABC=789 012

use std::env;

fn main() {
    assert_eq!(env::var("XYZ").unwrap(), "456".to_owned());
    assert_eq!(env::var("ABC").unwrap(), "789 012");
}
