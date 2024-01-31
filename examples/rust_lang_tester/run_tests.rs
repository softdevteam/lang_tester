//! This is an example lang_tester for Rust: it is a simplified version of the test framework that
//! rustc uses. In essence, the first sequence of commented line(s) (note that other lines e.g.
//! `#![feature(...)]` lines and the like are skipped) in the file describe the test.
//!
//! See the test files in `lang_tests/` for example.

use std::{fs::read_to_string, path::PathBuf, process::Command};

use lang_tester::LangTester;
use tempfile::TempDir;

static COMMENT_PREFIX: &str = "//";

fn main() {
    // We use rustc to compile files into a binary: we store those binary files into `tempdir`.
    // This may not be necessary for other languages.
    let tempdir = TempDir::new().unwrap();
    LangTester::new()
        .test_dir("examples/rust_lang_tester/lang_tests")
        // Only use files named `*.rs` as test files.
        .test_path_filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
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
        //   * `Run-time`: if rustc does not error, and the `Compiler` tests succeed, then the
        //     output binary is run.
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
