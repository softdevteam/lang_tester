// Copyright (c) 2019 King's College London created by the Software Development Team
// <http://soft-dev.org/>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0>, or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, or the UPL-1.0 license <http://opensource.org/licenses/UPL>
// at your option. This file may not be copied, modified, or distributed except according to those
// terms.

use std::{
    collections::{hash_map::HashMap, HashSet},
    fs::read_to_string,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{self, Command},
};

use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use walkdir::WalkDir;

use crate::{fuzzy, parser::parse_tests};

pub struct LangTester<'a> {
    test_dir: Option<&'a str>,
    test_file_filter: Option<Box<Fn(&Path) -> bool>>,
    test_extract: Option<Box<Fn(&str) -> Option<String>>>,
    test_cmds: Option<Box<Fn(&Path) -> Vec<(&str, Command)>>>,
}

impl<'a> LangTester<'a> {
    /// Create a new `LangTester` with default options. Note that, at a minimum, you need to call
    /// [`test_dir`](#method.test_dir), [`test_extract`](#method.test_extract), and
    /// [`test_cmds`](#method.test_cmds).
    pub fn new() -> Self {
        LangTester {
            test_dir: None,
            test_file_filter: None,
            test_extract: None,
            test_cmds: None,
        }
    }

    /// Specify the directory where tests are contained in. Note that this directory will be
    /// searched recursively (i.e. subdirectories and their contents will also be considered as
    /// potential tests).
    pub fn test_dir(&'a mut self, test_dir: &'a str) -> &'a mut Self {
        self.test_dir = Some(test_dir);
        self
    }

    /// If `test_file_filter` is specified, only files for which it returns `true` will be
    /// considered tests. A common use of this is to filter files based on filename extensions
    /// e.g.:
    ///
    /// ```rust,ignore
    /// LangTester::new()
    ///     ...
    ///     .test_file_filter(|p| p.extension().unwrap().to_str().unwrap() == "rs")
    ///     ...
    /// ```
    pub fn test_file_filter<F>(&'a mut self, test_file_filter: F) -> &'a mut Self
    where
        F: 'static + Fn(&Path) -> bool,
    {
        self.test_file_filter = Some(Box::new(test_file_filter));
        self
    }

    /// Specify a function which can extract the test data for `lang_tester` from a test file. This
    /// function is passed a `&str` and must return a `String`.
    ///
    /// How the test is extracted from the file is entirely up to the user, though a common
    /// convention is to store the test in a comment at the beginning of the file. For example, for
    /// Rust code one could use a function along the lines of the following:
    ///
    /// ```rust,ignore
    /// LangTester::new()
    ///     ...
    ///     .test_extract(|s| {
    ///         Some(
    ///             s.lines()
    ///                 // Skip non-commented lines at the start of the file.
    ///                 .skip_while(|l| !l.starts_with("//"))
    ///                 // Extract consecutive commented lines.
    ///                 .take_while(|l| l.starts_with("//"))
    ///                 .map(|l| &l[2..])
    ///                 .collect::<Vec<_>>()
    ///                 .join("\n"),
    ///         )
    ///     })
    ///     ...
    /// ```
    pub fn test_extract<F>(&'a mut self, test_extract: F) -> &'a mut Self
    where
        F: 'static + Fn(&str) -> Option<String>,
    {
        self.test_extract = Some(Box::new(test_extract));
        self
    }

    /// Specify a function which takes a `Path` to a test file and returns a vector containing 1 or
    /// more `(<name>, <[Command](https://doc.rust-lang.org/std/process/struct.Command.html)>)
    /// pairs. The commands will be executed in order on the test file: for each executed command,
    /// tests starting with `<name>` will be checked. For example, if your pipeline requires
    /// separate compilation and linking, you might specify something along the lines of the
    /// following:
    ///
    /// ```rust,ignore
    /// let tempdir = ...; // A `Path` to a temporary directory.
    /// LangTester::new()
    ///     ...
    ///     .test_cmds(|p| {
    ///         let mut exe = PathBuf::new();
    ///         exe.push(&tempdir);
    ///         exe.push(p.file_stem().unwrap());
    ///         let mut compiler = Command::new("rustc");
    ///         compiler.args(&["-o", exe.to_str().unwrap(), p.to_str().unwrap()]);
    ///         let runtime = Command::new(exe);
    ///         vec![("Compiler", compiler), ("Run-time", runtime)]
    ///     })
    ///     ...
    /// ```
    ///
    /// and then have tests such as:
    ///
    /// ```text
    /// Compiler:
    ///   status: success
    ///   stderr:
    ///   stdout:
    ///
    /// Run-time:
    ///   status: failure
    ///   stderr:
    ///     ...
    ///     Error at line 10
    ///     ...
    /// ```
    pub fn test_cmds<F>(&'a mut self, test_cmds: F) -> &'a mut Self
    where
        F: 'static + Fn(&Path) -> Vec<(&str, Command)>,
    {
        self.test_cmds = Some(Box::new(test_cmds));
        self
    }

    /// Make sure the user has specified the minimum set of things we need from them.
    fn validate(&self) {
        if self.test_dir.is_none() {
            panic!("test_dir must be specified.");
        }
        if self.test_extract.is_none() {
            panic!("test_extract must be specified.");
        }
        if self.test_cmds.is_none() {
            panic!("test_cmds must be specified.");
        }
    }

    /// Enumerate all the test files we need to check.
    fn test_files(&self) -> Vec<PathBuf> {
        WalkDir::new(self.test_dir.unwrap())
            .into_iter()
            .filter_map(|x| x.ok())
            .filter(|x| x.file_type().is_file())
            .filter(|x| match self.test_file_filter.as_ref() {
                Some(f) => f(x.path()),
                None => true,
            })
            .map(|x| x.into_path())
            .collect()
    }

    /// Run all the lang tests.
    pub fn run(&mut self) {
        self.validate();
        let test_files = self.test_files();
        eprint!("\nrunning {} tests", test_files.len());
        let mut failures = Vec::new();
        let mut num_ignored = 0;
        'b: for p in &test_files {
            let test_name = p.file_stem().unwrap().to_str().unwrap();
            eprint!("\ntest lang_tests::{} ... ", test_name);
            let all_str =
                read_to_string(p.as_path()).expect(&format!("Couldn't read {}", test_name));
            let test_str = self.test_extract.as_ref().unwrap()(&all_str)
                .expect(&format!("Couldn't extract test string from {}", test_name));
            if test_str.trim().is_empty() {
                write_with_colour("ignored", Color::Yellow);
                eprint!(" (test string is empty)");
                num_ignored += 1;
                continue;
            }

            let tests = parse_tests(&test_str);
            let cmd_pairs = self.test_cmds.as_mut().unwrap()(p.as_path())
                .into_iter()
                .map(|(test_name, cmd)| (test_name.to_lowercase(), cmd))
                .collect::<Vec<_>>();
            self.check_names(&cmd_pairs, &tests);

            let mut failure = TestFailure {
                status: None,
                stderr: None,
                stdout: None,
            };
            for (cmd_name, mut cmd) in cmd_pairs {
                let output = cmd
                    .output()
                    .expect(&format!("Couldn't run command {:?}.", cmd));

                let test = match tests.get(&cmd_name) {
                    Some(t) => t,
                    None => continue,
                };
                let mut meant_to_error = false;
                if let Some(ref status) = test.status {
                    if status == "error" {
                        meant_to_error = true;
                    }
                    if output.status.success() && status == "error" {
                        failure.status = Some("Success".to_owned());
                    } else if !output.status.success() && status == "success" {
                        failure.status = Some("Error".to_owned());
                    }
                }
                if let Some(ref stderr) = test.stderr {
                    let stderr_utf8 = String::from_utf8(output.stderr).unwrap();
                    if !fuzzy::match_vec(stderr, &stderr_utf8) {
                        failure.stderr = Some(stderr_utf8);
                    }
                }
                if let Some(ref stdout) = test.stdout {
                    let stdout_utf8 = String::from_utf8(output.stdout).unwrap();
                    if !fuzzy::match_vec(stdout, &stdout_utf8) {
                        failure.stdout = Some(stdout_utf8);
                    }
                }
                if !output.status.success() && meant_to_error {
                    break;
                }
            }

            if failure
                != (TestFailure {
                    status: None,
                    stderr: None,
                    stdout: None,
                })
            {
                failures.push((test_name, failure));
                output_failed();
            } else {
                output_ok();
            }
        }

        self.pp_failures(&failures, test_files.len(), num_ignored);

        if !failures.is_empty() {
            process::exit(1);
        }
    }

    /// Check for the case where the user has a test called `X` but `test_cmds` doesn't have a
    /// command with a matching name. This is almost certainly a bug, in the sense that the test
    /// can never, ever fire.
    fn check_names(&self, cmd_pairs: &Vec<(String, Command)>, tests: &HashMap<String, Test>) {
        let cmd_names = cmd_pairs.iter().map(|x| &x.0).collect::<HashSet<_>>();
        let test_names = tests.keys().map(|x| x).collect::<HashSet<_>>();
        let diff = test_names
            .difference(&cmd_names)
            .map(|x| x.as_str())
            .collect::<Vec<_>>();
        if !diff.is_empty() {
            panic!(
                "Command name(s) '{}' in tests are not found in the actual commands.",
                diff.join(", ")
            );
        }
    }

    /// Pretty print any failures to `stderr`.
    fn pp_failures(
        &self,
        failures: &Vec<(&str, TestFailure)>,
        test_files_len: usize,
        num_ignored: usize,
    ) {
        if !failures.is_empty() {
            eprintln!("\n\nfailures:");
            for (test_name, test) in failures {
                if let Some(ref status) = test.status {
                    eprintln!("\n---- lang_tests::{} status ----\n{}", test_name, status);
                }
                if let Some(ref stderr) = test.stderr {
                    eprintln!("\n---- lang_tests::{} stderr ----\n{}\n", test_name, stderr);
                }
                if let Some(ref stdout) = test.stdout {
                    eprintln!("\n---- lang_tests::{} stdout ----\n{}\n", test_name, stdout);
                }
            }
            eprintln!("\nfailures:");
            for (test_name, _) in failures {
                eprint!("    lang_tests::{}", test_name);
            }
        }

        eprint!("\n\ntest result: ");
        if failures.is_empty() {
            output_ok();
        } else {
            output_failed();
        }
        eprintln!(
            ". {} passed; {} failed; {} ignored; 0 measured; 0 filtered out\n",
            test_files_len - failures.len(),
            failures.len(),
            num_ignored
        );
    }
}

/// A user `Test`.
#[derive(Debug)]
pub(crate) struct Test<'a> {
    pub status: Option<String>,
    pub stderr: Option<Vec<&'a str>>,
    pub stdout: Option<Vec<&'a str>>,
}

/// If a test fails, the parts that fail are set to `Some(...)` in an instance of this struct.
#[derive(Debug, PartialEq)]
struct TestFailure {
    status: Option<String>,
    stderr: Option<String>,
    stdout: Option<String>,
}

fn write_with_colour(s: &str, colour: Color) {
    let mut stderr = StandardStream::stderr(ColorChoice::Always);
    stderr.set_color(ColorSpec::new().set_fg(Some(colour))).ok();
    io::stderr().write_all(s.as_bytes()).ok();
    stderr.reset().ok();
}

fn output_failed() {
    write_with_colour("FAILED", Color::Red);
}

fn output_ok() {
    write_with_colour("ok", Color::Green);
}
