# lang_tester

This crate provides a simple language testing framework designed to help when
you are testing things like compilers and virtual machines. It allows users to
express simple tests for process success/failure and for stderr/stdout, including
embedding those tests directly in the source file. It is loosely based on the
[`compiletest_rs`](https://crates.io/crates/compiletest_rs) crate, but is much
simpler (and hence sometimes less powerful), and designed to be used for
testing non-Rust languages too.

For example, a Rust language tester, loosely in the spirit of
[`compiletest_rs`](https://crates.io/crates/compiletest_rs), looks as follows:

```rust
use std::{fs::read_to_string, path::PathBuf, process::Command};

use lang_tester::LangTester;
use tempfile::TempDir;

static COMMENT_PREFIX: &str = "//";

fn main() {
    // We use rustc to compile files into a binary: we store those binary files
    // into `tempdir`. This may not be necessary for other languages.
    let tempdir = TempDir::new().unwrap();
    LangTester::new()
        .test_dir("examples/rust_lang_tester/lang_tests")
        // Only use files named `*.rs` as test files.
        .test_file_filter(|p| p.extension().unwrap().to_str().unwrap() == "rs")
        // Treat lines beginning with "#" as comments.
        .comment_prefix("#")
        // Extract the first sequence of commented line(s) as the tests.
        .test_extract(|p| {
            read_to_string(p)
                .unwrap()
                .lines()
                // Skip non-commented lines at the start of the file.
                .skip_while(|l| !l.starts_with(COMMENT_PREFIX))
                // Extract consecutive commented lines.
                .take_while(|l| l.starts_with(COMMENT_PREFIX))
                .map(|l| &l[COMMENT_PREFIX.len()..])
                .collect::<Vec<_>>()
                .join("\n")
        })
        // We have two test commands:
        //   * `Compiler`: runs rustc.
        //   * `Run-time`: if rustc does not error, and the `Compiler` tests
        //     succeed, then the output binary is run.
        .test_cmds(move |p| {
            // Test command 1: Compile `x.rs` into `tempdir/x`.
            let mut exe = PathBuf::new();
            exe.push(&tempdir);
            exe.push(p.file_stem().unwrap());
            let mut compiler = Command::new("rustc");
            compiler.args(&["-o", exe.to_str().unwrap(), p.to_str().unwrap()]);
            // Test command 2: run `tempdir/x`.
            let runtime = Command::new(exe);
            vec![("Compiler", compiler), ("Run-time", runtime)]
        })
        .run();
}
```

This defines a lang tester that uses all `*.rs` files in a given directory as
test files, running two test commands against them: `Compiler` (i.e. `rustc`);
and `Run-time` (the compiled binary).

Users can then write test files such as the following:

```rust
// Compiler:
//   stderr:
//     warning: unused variable: `x`
//       ...unused_var.rs:12:9
//       ...
//
// Run-time:
//   stdout: Hello world
fn main() {
    let x = 0;
    println!("Hello world");
}
```

The above file contains 4 meaningful tests, two specified by the user and
two implied by defaults: the `Compiler` should succeed (e.g. return a `0` exit
code when run on Unix), and its `stderr` output should warn about an unused
variable on line 12; and the resulting binary should succeed produce `Hello
world` on `stdout`.


## Integration with Cargo.

Tests created with lang_tester can be used as part of an existing test suite and
can be run with the `cargo test` command. For example, if the Rust source file
that runs your lang tests is `lang_tests/run.rs` then add the following to your
Cargo.toml:

```
[[test]]
name = "lang_tests"
path = "lang_tests/run.rs"
harness = false
```
