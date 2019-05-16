// Copyright (c) 2019 King's College London created by the Software Development Team
// <http://soft-dev.org/>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0>, or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, or the UPL-1.0 license <http://opensource.org/licenses/UPL>
// at your option. This file may not be copied, modified, or distributed except according to those
// terms.

//! This is an example lang_tester for Rust: it is a simplified version of the test framework that
//! rustc uses. In essence, the first sequence of commented line(s) (note that other lines e.g.
//! `#![feature(...)]` lines and the like are skipped) in the file describe the test.
//!
//! See the test files in `lang_tests/` for example.

use std::{path::PathBuf, process::Command};

use lang_tester::LangTester;
use tempdir::TempDir;

fn main() {
    // We use rustc to compile files into a binary: we store those binary files into `tempdir`.
    // This may not be necessary for other languages.
    let tempdir = TempDir::new("rust_lang_tester").unwrap();
    LangTester::new()
        .test_dir("examples/rust_lang_tester/lang_tests")
        // Only use files named `*.rs` as tests.
        .test_file_filter(|p| p.extension().unwrap().to_str().unwrap() == "rs")
        // Extract the first sequence of commented line(s) as the test.
        .test_extract(|s| {
            Some(
                s.lines()
                    // Skip non-commented lines at the start of the file.
                    .skip_while(|l| !l.starts_with("//"))
                    // Extract consecutive commented lines.
                    .take_while(|l| l.starts_with("//"))
                    .map(|l| &l[2..])
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
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
