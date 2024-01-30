// # Never ignore this test.
// ignore-if: echo 123 | grep 4
// Run-time:
//   stdout:
//     # an ignored comment
//     check

fn main() {
    println!("check");
}
