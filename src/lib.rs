//! This crate provides a simple language testing framework designed to help when you are testing
//! things like compilers and virtual machines. It allows users to express simple tests for process
//! success/failure and for stderr/stdout, including embedding those tests directly in the source
//! file. It is loosely based on the [`compiletest_rs`](https://crates.io/crates/compiletest_rs)
//! crate, but is much simpler (and hence sometimes less powerful), and designed to be used for
//! testing non-Rust languages too.
//!
//! For example, a Rust language tester, loosely in the spirit of
//! [`compiletest_rs`](https://crates.io/crates/compiletest_rs), looks as follows:
//!
//! ```rust,ignore
//! use std::{fs::read_to_string, path::PathBuf, process::Command};
//!
//! use lang_tester::LangTester;
//! use tempfile::TempDir;
//!
//! static COMMENT_PREFIX: &str = "//";
//!
//! fn main() {
//!     // We use rustc to compile files into a binary: we store those binary files
//!     // into `tempdir`. This may not be necessary for other languages.
//!     let tempdir = TempDir::new().unwrap();
//!     LangTester::new()
//!         .test_dir("examples/rust_lang_tester/lang_tests")
//!         // Only use files named `*.rs` as test files.
//!         .test_path_filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
//!         // Treat lines beginning with "#" as comments.
//!         .comment_prefix("#")
//!         // Extract the first sequence of commented line(s) as the tests.
//!         .test_extract(|p| {
//!             read_to_string(p)
//!                 .unwrap()
//!                 .lines()
//!                 // Skip non-commented lines at the start of the file.
//!                 .skip_while(|l| !l.starts_with(COMMENT_PREFIX))
//!                 // Extract consecutive commented lines.
//!                 .take_while(|l| l.starts_with(COMMENT_PREFIX))
//!                 .map(|l| &l[COMMENT_PREFIX.len()..])
//!                 .collect::<Vec<_>>()
//!                 .join("\n")
//!         })
//!         // We have two test commands:
//!         //   * `Compiler`: runs rustc.
//!         //   * `Run-time`: if rustc does not error, and the `Compiler` tests
//!         //     succeed, then the output binary is run.
//!         .test_cmds(move |p| {
//!             // Test command 1: Compile `x.rs` into `tempdir/x`.
//!             let mut exe = PathBuf::new();
//!             exe.push(&tempdir);
//!             exe.push(p.file_stem().unwrap());
//!             let mut compiler = Command::new("rustc");
//!             compiler.args(&["-o", exe.to_str().unwrap(), p.to_str().unwrap()]);
//!             // Test command 2: run `tempdir/x`.
//!             let runtime = Command::new(exe);
//!             vec![("Compiler", compiler), ("Run-time", runtime)]
//!         })
//!         .run();
//! }
//! ```
//!
//! This defines a lang tester that uses all `*.rs` files in a given directory as test files,
//! running two test commands against them: `Compiler` (i.e. `rustc`); and `Run-time` (the compiled
//! binary).
//!
//! Users can then write test files such as the following:
//!
//! ```rust,ignore
//! // Compiler:
//! //   stderr:
//! //     warning: unused variable: `x`
//! //       ...unused_var.rs:12:9
//! //       ...
//! //
//! // Run-time:
//! //   stdout: Hello world
//! fn main() {
//!     let x = 0;
//!     println!("Hello world");
//! }
//! ```
//!
//! `lang_tester` is entirely ignorant of the language being tested, leaving it entirely to the
//! user to determine what the test data in/for a file is. In this case, since we are embedding the
//! test data as a Rust comment at the start of the file, the `test_extract` function we specified
//! returns the following string:
//!
//! ```text
//! Compiler:
//!   stderr:
//!     warning: unused variable: `x`
//!       ...unused_var.rs:12:9
//!       ...
//!
//! Run-time:
//!   stdout: Hello world
//! ```
//!
//! Test data is specified with a two-level indentation syntax: the outer most level of indentation
//! defines a test command (multiple command names can be specified, as in the above); the inner
//! most level of indentation defines alterations to the general command or sub-tests. Multi-line
//! values are stripped of their common indentation, such that:
//!
//! ```text
//! x:
//!   a
//!     b
//!   c
//! ```
//!
//! defines a test command `x` with a value `a\n  b\nc`. Trailing whitespace is preserved.
//!
//! String matching is performed by the [fm crate](https://crates.io/crates/fm), which provides
//! support for `...` operators and so on. Unless `lang_tester` is explicitly instructed otherwise,
//! it uses `fm`'s defaults. In particular, even though `lang_tester` preserves (some) leading and
//! (all) trailing whitespace, `fm` ignores leading and trailing whitespace by default (though this
//! can be changed).
//!
//! Each test command must define at least one sub-test:
//!
//!   * `status: <success|error|signal|<int>>`, where `success` and `error` map to platform
//!     specific notions of a command completing successfully or unsuccessfully respectively.
//!     `signal` checks for termination due to a signal on Unix platforms; on non-Unix platforms,
//!     the test will be ignored. `<int>` is a signed integer checking for a specific exit code on
//!     platforms that support it. If not specified, defaults to `success`.
//!   * `stderr: [<string>]`, `stdout: [<string>]` match `<string>` against a command's `stderr` or
//!     `stdout`. The special string `...` can be used as a simple wildcard: if a line consists
//!     solely of `...`, it means "match zero or more lines"; if a line begins with `...`, it means
//!     "match the remainder of the line only"; if a line ends with `...`, it means "match the
//!     start of the line only". A line may start and end with `...`. Note that `stderr`/`stdout`
//!     matches ignore leading/trailing whitespace and newlines, but are case sensitive. If not
//!     specified, defaults to `...` (i.e. match anything). Note that the empty string matches only
//!     the empty string so e.g. `stderr:` on its own means that a command's `stderr` muct not
//!     contain any output.
//!
//! Test commands can alter the general command by specifying zero or more of the following:
//!
//!   * `env-var: <key>=<string>` will set (or override if it is already present) the environment
//!     variable `<key>` to the value `<string>`. `env-var` can be specified multiple times, each
//!     setting an additional (or overriding an existing) environment variable.
//!   * `exec-arg: <string>` specifies a string which will be passed as an additional command-line
//!     argument to the command (in addition to those specified by the `test_cmds` function).
//!     Multiple `exec-arg`s can be specified, each adding an additional command-line argument.
//!   * `stdin: <string>` specifies text to be passed to the command's `stdin`. If the command
//!     exits without consuming all of `<string>`, an error will be raised. Note, though, that
//!     operating system file buffers can mean that the command *appears* to have consumed all of
//!     `<string>` without it actually having done so.
//!
//! Test commands can specify that a test should be rerun if one of the following (optional) is
//! specified and it matches the test's output:
//!
//!   * `rerun-if-status` follows the same format as the `status`.
//!   * `rerun-if-stderr` and `rerun-if-stdout` follow the same format as `stderr` and `stdout`.
//!
//! These can be useful if tests are subject to intermittent errors (e.g. network failure) that
//! should not be considered as a failure of the test itself. Test commands are rerun at most *n*
//! times, which by default is specified as 3. If no `rerun-if-` is specified, then the first time
//! a test fails, it will be reported to the user.
//!
//! The above file thus contains 4 meaningful tests, two specified by the user and two implied by
//! defaults: the `Compiler` should succeed (e.g. return a `0` exit code when run on Unix), and
//! its `stderr` output should warn about an unused variable on line 12; and the resulting binary
//! should succeed produce `Hello world` on `stdout`.
//!
//! A file's tests can be ignored entirely with:
//!
//!   * `ignore-if: <cmd>` defines a shell command that will be run to determine whether to ignore
//!     this test or not. If `<cmd>` returns 0 the test will be ignored, otherwise it will be run.
//!
//! `lang_tester`'s output is deliberately similar to Rust's normal testing output. Running the
//! example `rust_lang_tester` in this crate produces the following output:
//!
//! ```text
//! $ cargo run --example=rust_lang_tester
//!    Compiling lang_tester v0.1.0 (/home/ltratt/scratch/softdev/lang_tester)
//!     Finished dev [unoptimized + debuginfo] target(s) in 3.49s
//!      Running `target/debug/examples/rust_lang_tester`
//!
//! running 4 tests
//! test lang_tests::no_main ... ok
//! test lang_tests::unknown_var ... ok
//! test lang_tests::unused_var ... ok
//! test lang_tests::exit_code ... ok
//!
//! test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
//! ```
//!
//! If you want to run a subset of tests, you can specify simple filters which use substring match
//! to run a subset of tests:
//!
//! ```text
//! $ cargo run --example=rust_lang_tester var
//!    Compiling lang_tester v0.1.0 (/home/ltratt/scratch/softdev/lang_tester)
//!     Finished dev [unoptimized + debuginfo] target(s) in 3.37s
//!      Running `target/debug/examples/rust_lang_tester var`
//!
//! running 2 tests
//! test lang_tests::unknown_var ... ok
//! test lang_tests::unused_var ... ok
//!
//! test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out
//! ```
//!
//! ## Integration with Cargo.
//!
//! Tests created with lang_tester can be used as part of an existing test suite and can be run
//! with the `cargo test` command. For example, if the Rust source file that runs your lang tests
//! is `lang_tests/run.rs` then add the following to your Cargo.toml:
//!
//! ```text
//! [[test]]
//! name = "lang_tests"
//! path = "lang_tests/run_tests.rs"
//! harness = false
//! ```

#![allow(clippy::needless_doctest_main)]
#![allow(clippy::new_without_default)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::type_complexity)]

mod parser;
mod tester;

pub use tester::LangTester;

pub(crate) fn fatal(msg: &str) -> ! {
    eprintln!("\nFatal exception:\n  {}", msg);
    std::process::exit(1);
}
