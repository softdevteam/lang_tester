// Compiler:
//   status: error
//   stderr:
//     error[E0425]: cannot find value `x` in this scope
//      ...unknown_var.rs:9:20
//      ...

fn main() {
    println!("{}", x);
}
