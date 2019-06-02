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
    env,
    fs::read_to_string,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{self, Command},
    sync::{Arc, Mutex},
};

use getopts::Options;
use num_cpus;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use threadpool::ThreadPool;
use walkdir::WalkDir;

use crate::{fatal, fuzzy, parser::parse_tests};

pub struct LangTester<'a> {
    test_dir: Option<&'a str>,
    use_cmdline_args: bool,
    test_file_filter: Option<Box<Fn(&Path) -> bool>>,
    cmdline_filters: Option<Vec<String>>,
    inner: Arc<LangTesterPooler>,
}

/// This is the information shared across test threads and which needs to be hidden behind an
/// `Arc`.
struct LangTesterPooler {
    test_extract: Option<Box<Fn(&str) -> Option<String> + Send + Sync>>,
    test_cmds: Option<Box<Fn(&Path) -> Vec<(&str, Command)> + Send + Sync>>,
}

impl<'a> LangTester<'a> {
    /// Create a new `LangTester` with default options. Note that, at a minimum, you need to call
    /// [`test_dir`](#method.test_dir), [`test_extract`](#method.test_extract), and
    /// [`test_cmds`](#method.test_cmds).
    pub fn new() -> Self {
        LangTester {
            test_dir: None,
            test_file_filter: None,
            use_cmdline_args: true,
            cmdline_filters: None,
            inner: Arc::new(LangTesterPooler {
                test_extract: None,
                test_cmds: None,
            }),
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
        F: 'static + Fn(&str) -> Option<String> + Send + Sync,
    {
        Arc::get_mut(&mut self.inner).unwrap().test_extract = Some(Box::new(test_extract));
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
        F: 'static + Fn(&Path) -> Vec<(&str, Command)> + Send + Sync,
    {
        Arc::get_mut(&mut self.inner).unwrap().test_cmds = Some(Box::new(test_cmds));
        self
    }

    /// If set to `true`, this reads arguments from `std::env::args()` and interprets them in the
    /// same way as normal cargo test files. For example if you have tests "ab" and "cd" but only
    /// want to run the latter:
    ///
    /// ```sh
    /// $ <test bin> c
    /// ```
    ///
    /// As this suggests, a simple substring search is used to decide which tests to run.
    ///
    /// You can get help on `lang_tester`'s options:
    ///
    /// ```sh
    /// $ <test bin> --help
    /// ```
    ///
    /// This option defaults to `true`.
    pub fn use_cmdline_args(&'a mut self, use_cmdline_args: bool) -> &'a mut Self {
        self.use_cmdline_args = use_cmdline_args;
        self
    }

    /// Make sure the user has specified the minimum set of things we need from them.
    fn validate(&self) {
        if self.test_dir.is_none() {
            fatal("test_dir must be specified.");
        }
        if self.inner.test_extract.is_none() {
            fatal("test_extract must be specified.");
        }
        if self.inner.test_cmds.is_none() {
            fatal("test_cmds must be specified.");
        }
    }

    /// Enumerate all the test files we need to check, along with the number of files filtered out
    /// (e.g. if you have tests `a, b, c` and the user does something like `cargo test b`, 2 tests
    /// (`a` and `c`) will be filtered out.
    fn test_files(&self) -> (Vec<PathBuf>, usize) {
        let mut num_filtered = 0;
        let paths = WalkDir::new(self.test_dir.unwrap())
            .into_iter()
            .filter_map(|x| x.ok())
            .filter(|x| x.file_type().is_file())
            // Filter out non-test files
            .filter(|x| match self.test_file_filter.as_ref() {
                Some(f) => f(x.path()),
                None => true,
            })
            // If the user has named one or more tests on the command-line, run only those,
            // filtering out the rest (counting them as ignored).
            .filter(|x| {
                let x_path = x.path().to_str().unwrap();
                match self.cmdline_filters.as_ref() {
                    Some(fs) => {
                        debug_assert!(self.use_cmdline_args);
                        for f in fs {
                            if x_path.contains(f) {
                                return true;
                            }
                        }
                        num_filtered += 1;
                        false
                    }
                    None => true,
                }
            })
            .map(|x| x.into_path())
            .collect();
        (paths, num_filtered)
    }

    /// Run all the lang tests.
    pub fn run(&mut self) {
        self.validate();
        if self.use_cmdline_args {
            let args: Vec<String> = env::args().collect();
            let matches = Options::new()
                .optflag("h", "help", "")
                .parse(&args[1..])
                .unwrap_or_else(|_| usage());
            if matches.opt_present("h") {
                usage();
            }
            if !matches.free.is_empty() {
                self.cmdline_filters = Some(matches.free);
            }
        }
        let (test_files, num_filtered) = self.test_files();
        eprint!("\nrunning {} tests", test_files.len());
        let test_files_len = test_files.len();
        let (failures, num_ignored) = run_tests(test_files, Arc::clone(&self.inner));

        self.pp_failures(&failures, test_files_len, num_ignored, num_filtered);

        if !failures.is_empty() {
            process::exit(1);
        }
    }

    /// Pretty print any failures to `stderr`.
    fn pp_failures(
        &self,
        failures: &Vec<(String, TestFailure)>,
        test_files_len: usize,
        num_ignored: usize,
        num_filtered: usize,
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
            write_with_colour("ok", Color::Green);
        } else {
            write_with_colour("FAILED", Color::Red);
        }
        eprintln!(
            ". {} passed; {} failed; {} ignored; 0 measured; {} filtered out\n",
            test_files_len - failures.len(),
            failures.len(),
            num_ignored,
            num_filtered
        );
    }
}

/// The status of an executed command.
#[derive(Clone, Debug)]
pub(crate) enum Status {
    /// The command exited successfully (by whatever definition of "successful" the running
    /// platform uses).
    Success,
    /// The command did not execute successfully (by whatever definition of "not successful" the
    /// running platform uses).
    Error,
    /// The command exited with a precise exit code. This option may not be available on all
    /// platforms.
    Int(i32),
}

/// A user `Test`.
#[derive(Clone, Debug)]
pub(crate) struct Test<'a> {
    pub status: Option<Status>,
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

fn usage() -> ! {
    eprintln!("Usage: <filter1> [... <filtern>]");
    process::exit(1);
}

/// Check for the case where the user has a test called `X` but `test_cmds` doesn't have a command
/// with a matching name. This is almost certainly a bug, in the sense that the test can never,
/// ever fire.
fn check_names<'a>(cmd_pairs: &[(String, Command)], tests: &HashMap<String, Test<'a>>) {
    let cmd_names = cmd_pairs.iter().map(|x| &x.0).collect::<HashSet<_>>();
    let test_names = tests.keys().map(|x| x).collect::<HashSet<_>>();
    let diff = test_names
        .difference(&cmd_names)
        .map(|x| x.as_str())
        .collect::<Vec<_>>();
    if !diff.is_empty() {
        fatal(&format!(
            "Command name(s) '{}' in tests are not found in the actual commands.",
            diff.join(", ")
        ));
    }
}

/// Run every test in `test_files`, returning a tuple `(failures, num_ignored)`.
fn run_tests(
    test_files: Vec<PathBuf>,
    inner: Arc<LangTesterPooler>,
) -> (Vec<(String, TestFailure)>, usize) {
    let failures = Arc::new(Mutex::new(Vec::new()));
    let mut num_ignored = 0;
    let pool = ThreadPool::new(num_cpus::get());
    for p in test_files {
        let test_name = p.file_stem().unwrap().to_str().unwrap().to_owned();

        let failures = failures.clone();
        let inner = inner.clone();
        pool.execute(move || {
            let all_str = read_to_string(p.as_path())
                .unwrap_or_else(|_| fatal(&format!("Couldn't read {}", test_name)));
            let test_str = inner.test_extract.as_ref().unwrap()(&all_str).unwrap_or_else(|| {
                fatal(&format!("Couldn't extract test string from {}", test_name))
            });
            if test_str.is_empty() {
                // Grab a lock on stderr so that we can avoid the possibility of lines blurring
                // together in confusing ways.
                let stderr = StandardStream::stderr(ColorChoice::Always);
                let mut handle = stderr.lock();
                handle
                    .write_all(&format!("\ntest lang_tests::{} ... ", test_name).as_bytes())
                    .ok();
                handle
                    .set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))
                    .ok();
                handle.write_all("ignored".as_bytes()).ok();
                handle.reset().ok();
                handle.write_all(" (test string is empty)".as_bytes()).ok();
                num_ignored += 1;
                return;
            }

            let tests = parse_tests(&test_str);
            let cmd_pairs = inner.test_cmds.as_ref().unwrap()(p.as_path())
                .into_iter()
                .map(|(test_name, cmd)| (test_name.to_lowercase(), cmd))
                .collect::<Vec<_>>();
            check_names(&cmd_pairs, &tests);

            let mut failure = TestFailure {
                status: None,
                stderr: None,
                stdout: None,
            };
            for (cmd_name, mut cmd) in cmd_pairs {
                let mut pass_status = None;
                let mut pass_stderr = None;
                let mut pass_stdout = None;
                let output = cmd
                    .output()
                    .unwrap_or_else(|_| fatal(&format!("Couldn't run command {:?}.", cmd)));

                let test = match tests.get(&cmd_name) {
                    Some(t) => t,
                    None => continue,
                };
                let mut meant_to_error = false;

                // First, check whether the tests passed.
                if let Some(ref status) = test.status {
                    pass_status = Some(match status {
                        Status::Success => output.status.success(),
                        Status::Error => {
                            meant_to_error = true;
                            !output.status.success()
                        }
                        Status::Int(i) => output.status.code() == Some(*i),
                    });
                }
                let stderr_utf8 = String::from_utf8(output.stderr).unwrap();
                if let Some(ref stderr) = test.stderr {
                    pass_stderr = Some(fuzzy::match_vec(stderr, &stderr_utf8));
                }
                let stdout_utf8 = String::from_utf8(output.stdout).unwrap();
                if let Some(ref stdout) = test.stdout {
                    pass_stdout = Some(fuzzy::match_vec(stdout, &stdout_utf8));
                }

                // Second, if a test failed, we want to print out everything which didn't match
                // successfully (i.e. if the stderr test failed, print that out; but, equally, if
                // stderr wasn't specified as a test, print it out, because the user can't
                // otherwise know what it contains).
                if pass_status == Some(false)
                    || pass_stderr == Some(false)
                    || pass_stdout == Some(false)
                {
                    if pass_status.is_none() || pass_status == Some(false) {
                        match test.status {
                            None | Some(Status::Success) | Some(Status::Error) => {
                                if output.status.success() {
                                    failure.status = Some("Success".to_owned());
                                } else {
                                    failure.status = Some("Error".to_owned());
                                }
                            }
                            Some(Status::Int(_)) => {
                                failure.status = Some(
                                    output
                                        .status
                                        .code()
                                        .map(|x| x.to_string())
                                        .unwrap_or_else(|| "Exited due to signal".to_owned()),
                                )
                            }
                        }
                    }

                    if pass_stderr.is_none() || pass_stderr == Some(false) {
                        failure.stderr = Some(stderr_utf8);
                    }

                    if pass_stdout.is_none() || pass_stdout == Some(false) {
                        failure.stdout = Some(stdout_utf8);
                    }

                    // If a sub-test failed, bail out immediately, otherwise subsequent sub-tests
                    // will overwrite the failure output!
                    break;
                }

                // If a command failed, and we weren't expecting it to, bail out immediately.
                if !output.status.success() && meant_to_error {
                    break;
                }
            }

            {
                // Grab a lock on stderr so that we can avoid the possibility of lines blurring
                // together in confusing ways.
                let stderr = StandardStream::stderr(ColorChoice::Always);
                let mut handle = stderr.lock();
                handle
                    .write_all(&format!("\ntest lang_tests::{} ... ", test_name).as_bytes())
                    .ok();
                if failure
                    != (TestFailure {
                        status: None,
                        stderr: None,
                        stdout: None,
                    })
                {
                    let mut failures = failures.lock().unwrap();
                    failures.push((test_name, failure));
                    handle
                        .set_color(ColorSpec::new().set_fg(Some(Color::Red)))
                        .ok();
                    handle.write_all("FAILED".as_bytes()).ok();
                    handle.reset().ok();
                } else {
                    handle
                        .set_color(ColorSpec::new().set_fg(Some(Color::Green)))
                        .ok();
                    handle.write_all("ok".as_bytes()).ok();
                    handle.reset().ok();
                }
            }
        });
    }
    pool.join();
    let failures = Mutex::into_inner(Arc::try_unwrap(failures).unwrap()).unwrap();

    (failures, num_ignored)
}
