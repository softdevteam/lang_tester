// # Always ignore this test
// ignore-if: echo 123 | grep 2
// Compiler:
//   status: success

fn main() {
    panic!("Shouldn't happen.");
}
