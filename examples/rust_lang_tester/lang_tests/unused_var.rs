// Compiler:
//   stderr:
//     warning: unused variable: `x`
//       ...unused_var.rs:11:9
//       ...
//
// Run-time:
//   stdout: Hello world

fn main() {
    let x = 0;
    println!("Hello world");
}
