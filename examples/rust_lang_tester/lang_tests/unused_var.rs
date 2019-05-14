// Compiler:
//   status: success
//   stderr:
//     warning: unused variable: `x`
//       ...unused_var.rs:12:9
//       ...
//
// Run-time:
//   status: success
//   stdout: Hello world
fn main() {
    let x = 0;
    println!("Hello world");
}
