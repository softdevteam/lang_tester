// Run-time:
//   status: signal

use std::process;

fn main() {
    unsafe {
        let ptr = std::ptr::null::<usize>();
        *ptr + 1;
    }
}
