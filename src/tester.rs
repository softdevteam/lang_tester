use std::{
    cmp::max,
    collections::{hash_map::HashMap, HashSet},
    convert::TryFrom,
    env,
    fs::canonicalize,
    io::{self, Read, Write},
    os::{
        raw::c_int,
        unix::{io::AsRawFd, process::ExitStatusExt},
    },
    panic::{catch_unwind, RefUnwindSafe},
    path::{Path, PathBuf, MAIN_SEPARATOR},
    process::{self, Command, ExitStatus},
    str,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::sleep,
    time::{Duration, Instant},
};

use fm::{FMBuilder, FMatchError};
use getopts::Options;
use libc::{
    close, fcntl, poll, pollfd, F_GETFL, F_SETFL, O_NONBLOCK, POLLERR, POLLHUP, POLLIN, POLLNVAL,
    POLLOUT,
};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use threadpool::ThreadPool;
use walkdir::WalkDir;

use crate::{fatal, parser::parse_tests};

/// The size of the (stack allocated) buffer use to read stderr/stdout from a child process.
const READBUF: usize = 1024 * 4; // bytes
/// Print a warning to the user every multiple of `TIMEOUT` seconds that a child process has run
/// without completing.
const TIMEOUT: u64 = 60; // seconds
/// The time that we should initially wait() for a child process to exit. This should be a very
/// small value, as most child processes will exit almost immediately.
const INITIAL_WAIT_TIMEOUT: u64 = 10000; // nanoseconds
/// The maximum time we should wait() between checking if a child process has exited.
const MAX_WAIT_TIMEOUT: u64 = 250_000_000; // nanoseconds
                                           //
/// The default maximum number of times to rerun a command if it fails and a rerun-if-* matches.
const DEFAULT_RERUN_AT_MOST: u64 = 3;

pub struct LangTester {
    use_cmdline_args: bool,
    test_path_filter: Option<Box<dyn Fn(&Path) -> bool + RefUnwindSafe>>,
    cmdline_filters: Option<Vec<String>>,
    inner: Arc<LangTesterPooler>,
}

/// This is the information shared across test threads and which needs to be hidden behind an
/// `Arc`.
struct LangTesterPooler {
    test_dir: Option<PathBuf>,
    test_threads: usize,
    ignored: bool,
    nocapture: bool,
    comment_prefix: Option<String>,
    test_extract: Option<Box<dyn Fn(&Path) -> String + RefUnwindSafe + Send + Sync>>,
    fm_options: Option<
        Box<
            dyn for<'a> Fn(&'a Path, TestStream, FMBuilder<'a>) -> FMBuilder<'a>
                + RefUnwindSafe
                + Send
                + Sync,
        >,
    >,
    test_cmds: Option<Box<dyn Fn(&Path) -> Vec<(&str, Command)> + RefUnwindSafe + Send + Sync>>,
    rerun_at_most: u64,
}

/// Specify a given test stream.
#[derive(Clone, Copy)]
pub enum TestStream {
    Stderr,
    Stdout,
}

impl LangTester {
    /// Create a new `LangTester` with default options. Note that, at a minimum, you need to call
    /// [`test_dir`](#method.test_dir), [`test_extract`](#method.test_extract), and
    /// [`test_cmds`](#method.test_cmds).
    pub fn new() -> Self {
        LangTester {
            test_path_filter: None,
            use_cmdline_args: true,
            cmdline_filters: None,
            inner: Arc::new(LangTesterPooler {
                test_dir: None,
                ignored: false,
                nocapture: false,
                comment_prefix: None,
                test_threads: num_cpus::get(),
                fm_options: None,
                test_extract: None,
                test_cmds: None,
                rerun_at_most: DEFAULT_RERUN_AT_MOST,
            }),
        }
    }

    /// Specify the directory where test files are contained. Note that this directory will be
    /// searched recursively (i.e. subdirectories and their contents will also be considered as
    /// potential test files).
    pub fn test_dir(&mut self, test_dir: &str) -> &mut Self {
        let inner = Arc::get_mut(&mut self.inner).unwrap();
        inner.test_dir = Some(canonicalize(test_dir).unwrap());
        self
    }

    /// Specify the number of simultaneous running test cases. Defaults to using
    /// all available CPUs.
    pub fn test_threads(&mut self, test_threads: usize) -> &mut Self {
        let inner = Arc::get_mut(&mut self.inner).unwrap();
        inner.test_threads = test_threads;
        self
    }

    /// Specify the maximum number of times to rerun a test if it fails and a rerun-if-* matches.
    /// Defaults to 3.
    pub fn rerun_at_most(&mut self, rerun_at_most: u64) -> &mut Self {
        let inner = Arc::get_mut(&mut self.inner).unwrap();
        inner.rerun_at_most = rerun_at_most;
        self
    }

    /// If set, defines what lines will be treated as comments if, ignoring the current level of
    /// indentation, they begin with `comment_prefix`.
    ///
    /// This option defaults to `None`.
    pub fn comment_prefix<S: AsRef<str>>(&mut self, comment_prefix: S) -> &mut Self {
        let inner = Arc::get_mut(&mut self.inner).unwrap();
        inner.comment_prefix = Some(comment_prefix.as_ref().to_owned());
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
    ///
    /// Note that `lang_tester` recursively searches directories for files.
    #[deprecated(
        since = "0.7.5",
        note = "Convert `test_file_filter(|p| ...)` to `test_path_fiter(|p| p.is_file() & ...)`"
    )]
    pub fn test_file_filter<F>(&mut self, test_path_filter: F) -> &mut Self
    where
        F: 'static + Fn(&Path) -> bool + RefUnwindSafe,
    {
        self.test_path_filter = Some(Box::new(move |p| p.is_file() && test_path_filter(p)));
        self
    }

    /// If `test_path_filter` is specified, only paths for which it returns `true` will be
    /// considered tests. A common use of this is to filter tests based on filename extensions
    /// e.g.:
    ///
    /// ```rust,ignore
    /// LangTester::new()
    ///     ...
    ///     .test_path_filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
    ///     ...
    /// ```
    ///
    /// Note that `lang_tester` recursively searches directories.
    pub fn test_path_filter<F>(&mut self, test_path_filter: F) -> &mut Self
    where
        F: 'static + Fn(&Path) -> bool + RefUnwindSafe,
    {
        self.test_path_filter = Some(Box::new(test_path_filter));
        self
    }

    /// Specify a function which can extract the test data for `lang_tester` from a test file path,
    /// returning it as a `String`. Note that the test data does not have to be extracted from the
    /// `Path` passed to the function -- it can come from any source.
    ///
    /// How the test data is extracted from the test file is entirely up to the user, though a
    /// common convention is to store the test data in a comment at the beginning of the test file.
    /// For example, for Rust code one could use a function along the lines of the following:
    ///
    /// ```rust,ignore
    /// LangTester::new()
    ///     ...
    ///     .test_extract(|p| {
    ///         std::fs::read_to_string(p)
    ///             .unwrap()
    ///             .lines()
    ///             // Skip non-commented lines at the start of the file.
    ///             .skip_while(|l| !l.starts_with("//"))
    ///             // Extract consecutive commented lines.
    ///             .take_while(|l| l.starts_with("//"))
    ///             .map(|l| &l[2..])
    ///             .collect::<Vec<_>>()
    ///             .join("\n")
    ///     })
    ///     ...
    /// ```
    pub fn test_extract<F>(&mut self, test_extract: F) -> &mut Self
    where
        F: 'static + Fn(&Path) -> String + RefUnwindSafe + Send + Sync,
    {
        Arc::get_mut(&mut self.inner).unwrap().test_extract = Some(Box::new(test_extract));
        self
    }

    /// Specify a function which sets options for the [`fm`](https://crates.io/crates/fm) library.
    /// `fm` is used for the fuzzy matching in `lang_tester`. This function can be used to override
    /// `fm`'s defaults for a given test file (passed as `Path`) and a given testing stream (stderr
    /// or stdout, passed as `TestStream`) when executing a command for that file: it is passed a
    /// [`FMBuilder`](https://docs.rs/fm/*/fm/struct.FMBuilder.html) and must return a `FMBuilder`.
    /// For example, to make use of `fm`'s "name matcher" option such that all instances of `$1`
    /// must match the same value (without precisely specifying what that value is) one could use
    /// the following:
    ///
    /// ```rust,ignore
    /// LangTester::new()
    ///    ...
    ///    .fm_options(|_, _, fmb| {
    ///        let ptn_re = Regex::new(r"\$.+?\b").unwrap();
    ///        let text_re = Regex::new(r".+?\b").unwrap();
    ///        fmb.name_matcher(ptn_re, text_re)
    ///    })
    /// ```
    pub fn fm_options<F>(&mut self, fm_options: F) -> &mut Self
    where
        F: 'static
            + for<'a> Fn(&'a Path, TestStream, FMBuilder<'a>) -> FMBuilder<'a>
            + RefUnwindSafe
            + Send
            + Sync,
    {
        Arc::get_mut(&mut self.inner).unwrap().fm_options = Some(Box::new(fm_options));
        self
    }

    /// Specify a function which takes a `Path` to a test file and returns a vector containing 1 or
    /// more (`name`, <[`Command`](https://doc.rust-lang.org/std/process/struct.Command.html)>)
    /// pairs. The commands will be executed in order on the test file: for each executed command,
    /// test commands starting with `<name>` will be checked. For example, if your pipeline
    /// requires separate compilation and linking, you might specify something along the lines of
    /// the following:
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
    /// and then have test data such as:
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
    pub fn test_cmds<F>(&mut self, test_cmds: F) -> &mut Self
    where
        F: 'static + Fn(&Path) -> Vec<(&str, Command)> + RefUnwindSafe + Send + Sync,
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
    pub fn use_cmdline_args(&mut self, use_cmdline_args: bool) -> &mut Self {
        self.use_cmdline_args = use_cmdline_args;
        self
    }

    /// Make sure the user has specified the minimum set of things we need from them.
    fn validate(&self) {
        if self.inner.test_dir.is_none() {
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
    /// (`a` and `c`) will be filtered out. The `PathBuf`s returned are guaranteed to be fully
    /// canonicalised.
    fn test_files(
        &self,
        failures: Arc<Mutex<Vec<(String, TestFailure)>>>,
    ) -> (Vec<PathBuf>, usize) {
        let mut num_filtered = 0;
        let paths = WalkDir::new(self.inner.test_dir.as_ref().unwrap())
            .into_iter()
            .filter_map(|x| x.ok())
            .map(|x| canonicalize(x.into_path()).unwrap())
            // Filter out non-test files
            .filter(|x| match self.test_path_filter.as_ref() {
                Some(f) => match catch_unwind(|| f(x)) {
                    Ok(b) => b,
                    Err(_) => {
                        let failure = TestFailure {
                            status: None,
                            stdin_remaining: 0,
                            stderr: None,
                            stderr_match: None,
                            stdout: None,
                            stdout_match: None,
                        };
                        failures
                            .lock()
                            .unwrap()
                            .push((x.to_str().unwrap().to_owned(), failure));
                        false
                    }
                },
                None => true,
            })
            // If the user has named one or more tests on the command-line, run only those,
            // filtering out the rest (counting them as ignored).
            .filter(|x| {
                let test_fname = format!(
                    "lang_tests::{}",
                    test_fname(self.inner.test_dir.as_deref().unwrap(), x.as_path(),)
                );
                match self.cmdline_filters.as_ref() {
                    Some(fs) => {
                        debug_assert!(self.use_cmdline_args);
                        for f in fs {
                            if test_fname.contains(f) {
                                return true;
                            }
                        }
                        num_filtered += 1;
                        false
                    }
                    None => true,
                }
            })
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
                .optflag("", "ignored", "Run only ignored tests")
                .optflag(
                    "",
                    "nocapture",
                    "Pass command stderr/stdout through to the terminal",
                )
                .optopt(
                    "",
                    "test-threads",
                    "Number of threads used for running tests in parallel",
                    "n_threads",
                )
                .parse(&args[1..])
                .unwrap_or_else(|_| usage());
            if matches.opt_present("h") {
                usage();
            }
            if matches.opt_present("ignored") {
                Arc::get_mut(&mut self.inner).unwrap().ignored = true;
            }
            if matches.opt_present("nocapture") {
                Arc::get_mut(&mut self.inner).unwrap().nocapture = true;
            }
            if let Some(s) = matches.opt_str("test-threads") {
                let test_threads = s.parse::<usize>().unwrap_or_else(|_| usage());
                if test_threads == 0 {
                    fatal("Must specify more than 0 threads.");
                }
                Arc::get_mut(&mut self.inner).unwrap().test_threads = test_threads;
            }
            if !matches.free.is_empty() {
                self.cmdline_filters = Some(matches.free);
            }
        }
        let failures = Arc::new(Mutex::new(Vec::new()));
        let (test_files, num_filtered) = self.test_files(Arc::clone(&failures));
        let test_files_len = test_files.len();
        let num_ignored = if failures.lock().unwrap().is_empty() {
            eprint!("\nrunning {} tests", test_files.len());
            test_file(test_files, Arc::clone(&self.inner), Arc::clone(&failures))
        } else {
            0
        };

        let failures = Mutex::into_inner(Arc::try_unwrap(failures).unwrap()).unwrap();
        self.pp_failures(
            &failures,
            max(test_files_len, failures.len()),
            num_ignored,
            num_filtered,
        );

        if !failures.is_empty() {
            process::exit(1);
        }
    }

    /// Pretty print any failures to `stderr`.
    fn pp_failures(
        &self,
        failures: &[(String, TestFailure)],
        test_files_len: usize,
        num_ignored: usize,
        num_filtered: usize,
    ) {
        if !failures.is_empty() {
            eprintln!("\n\nfailures:");
            for (test_fname, test) in failures {
                if let Some(ref status) = test.status {
                    eprintln!("\n---- lang_tests::{} status ----\n{}", test_fname, status);
                }
                if test.stdin_remaining != 0 {
                    eprintln!(
                        "\n---- lang_tests::{} stdin ----\n{} bytes of stdin were not consumed",
                        test_fname, test.stdin_remaining
                    );
                }
                if let Some(ref stderr) = test.stderr {
                    eprintln!("\n---- lang_tests::{} stderr ----\n", test_fname);
                    if let Some(ref stderr_match) = test.stderr_match {
                        eprint!("{}", stderr_match);
                    } else {
                        eprintln!("{}", stderr);
                    }
                }
                if let Some(ref stdout) = test.stdout {
                    eprintln!("\n---- lang_tests::{} stdout ----\n", test_fname);
                    if let Some(ref stdout_match) = test.stdout_match {
                        eprint!("{}", stdout_match);
                    } else {
                        eprintln!("{}", stdout);
                    }
                }
            }
            eprintln!("\nfailures:");
            for (test_fname, _) in failures {
                eprint!("    lang_tests::{}", test_fname);
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// The command exited successfully (by whatever definition of "successful" the running
    /// platform uses).
    Success,
    /// The command did not execute successfully (by whatever definition of "not successful" the
    /// running platform uses).
    Error,
    /// The command terminated due to a signal. This option may not be available on all
    /// platforms.
    Signal,
    /// The command exited with a precise exit code. This option may not be available on all
    /// platforms.
    Int(i32),
}

/// A user `TestCmd`.
#[derive(Clone, Debug)]
pub(crate) struct TestCmd<'a> {
    pub status: Status,
    pub stdin: Option<String>,
    pub stderr: Vec<&'a str>,
    pub stdout: Vec<&'a str>,
    /// A list of custom command line arguments which should be passed when
    /// executing the test command.
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub rerun_if_status: Option<Status>,
    pub rerun_if_stderr: Option<Vec<&'a str>>,
    pub rerun_if_stdout: Option<Vec<&'a str>>,
}

impl<'a> TestCmd<'a> {
    pub fn default() -> Self {
        Self {
            status: Status::Success,
            stdin: None,
            stderr: vec!["..."],
            stdout: vec!["..."],
            args: Vec::new(),
            env: HashMap::new(),
            rerun_if_status: None,
            rerun_if_stderr: None,
            rerun_if_stdout: None,
        }
    }
}

/// A collection of tests.
pub(crate) struct Tests<'a> {
    pub ignore_if: Option<String>,
    pub tests: HashMap<String, TestCmd<'a>>,
}

/// If one or more parts of a `TestCmd` fail, the parts that fail are set to `Some(...)` in an
/// instance of this struct.
#[derive(Debug, PartialEq)]
struct TestFailure {
    status: Option<String>,
    stdin_remaining: usize,
    stderr: Option<String>,
    stderr_match: Option<FMatchError>,
    stdout: Option<String>,
    stdout_match: Option<FMatchError>,
}

fn write_with_colour(s: &str, colour: Color) {
    let mut stderr = StandardStream::stderr(ColorChoice::Always);
    stderr.set_color(ColorSpec::new().set_fg(Some(colour))).ok();
    io::stderr().write_all(s.as_bytes()).ok();
    stderr.reset().ok();
}

fn write_ignored(test_name: &str, message: &str, inner: Arc<LangTesterPooler>) {
    // Grab a lock on stderr so that we can avoid the possibility of lines blurring
    // together in confusing ways.
    let stderr = StandardStream::stderr(ColorChoice::Always);
    let mut handle = stderr.lock();
    if inner.test_threads > 1 {
        handle
            .write_all(format!("\ntest lang_tests::{} ... ", test_name).as_bytes())
            .ok();
    }
    handle
        .set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))
        .ok();
    handle.write_all(b"ignored").ok();
    handle.reset().ok();
    if !message.is_empty() {
        handle.write_all(format!(" ({})", message).as_bytes()).ok();
    }
}

fn usage() -> ! {
    eprintln!("Usage: [--ignored] [--nocapture] [--test-threads=<n>] [<filter1>] [... <filtern>]");
    process::exit(1);
}

/// Check for the case where the user has a test called `X` but `test_cmds` doesn't have a command
/// with a matching name. This is almost certainly a bug, in the sense that the test can never,
/// ever fire.
fn check_names(cmd_pairs: &[(String, Command)], tests: &HashMap<String, TestCmd>) {
    let cmd_names = cmd_pairs.iter().map(|x| &x.0).collect::<HashSet<_>>();
    let test_names = tests.keys().collect::<HashSet<_>>();
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
fn test_file(
    test_files: Vec<PathBuf>,
    inner: Arc<LangTesterPooler>,
    failures: Arc<Mutex<Vec<(String, TestFailure)>>>,
) -> usize {
    let num_ignored = Arc::new(AtomicUsize::new(0));
    let pool = ThreadPool::new(inner.test_threads);
    for p in test_files {
        let test_fname = test_fname(inner.test_dir.as_ref().unwrap(), &p);

        let num_ignored = num_ignored.clone();
        let failures = failures.clone();
        let inner = inner.clone();
        pool.execute(move || {
            if inner.test_threads == 1 {
                eprint!("\ntest lang_test::{} ... ", test_fname);
            }
            let test_extract = inner.test_extract.as_ref().unwrap();
            match catch_unwind(|| test_extract(p.as_path())) {
                Ok(test_str) => {
                    if test_str.is_empty() {
                        write_ignored(test_fname.as_str(), "test string is empty", inner);
                        num_ignored.fetch_add(1, Ordering::Relaxed);
                        return;
                    }

                    let tests = parse_tests(inner.comment_prefix.as_deref(), &test_str);
                    let ignore = if let Some(ignore_if) = tests.ignore_if {
                        Command::new(env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned()))
                            .args(["-c", &ignore_if])
                            .stdin(process::Stdio::piped())
                            .stderr(process::Stdio::piped())
                            .stdout(process::Stdio::piped())
                            .status()
                            .unwrap_or_else(|_| {
                                fatal(&format!("Couldn't run ignore-if '{ignore_if}'"))
                            })
                            .success()
                    } else {
                        false
                    };
                    if (inner.ignored && !ignore) || (!inner.ignored && ignore) {
                        write_ignored(test_fname.as_str(), "", inner);
                        num_ignored.fetch_add(1, Ordering::Relaxed);
                        return;
                    }

                    if run_tests(Arc::clone(&inner), tests.tests, p, test_fname, failures) {
                        num_ignored.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    let failure = TestFailure {
                        status: None,
                        stdin_remaining: 0,
                        stderr: None,
                        stderr_match: None,
                        stdout: None,
                        stdout_match: None,
                    };
                    failures.lock().unwrap().push((test_fname, failure));
                }
            };
        });
    }
    pool.join();

    Arc::try_unwrap(num_ignored).unwrap().into_inner()
}

/// Convert a test file name to a user-friendly test name (e.g. "lang_tests/a/b.x" might become
/// "a::b.x").
fn test_fname(test_dir_path: &Path, test_fpath: &Path) -> String {
    if let Some(test_fpath) = test_fpath.as_os_str().to_str() {
        if let Some(testdir_path) = test_dir_path.as_os_str().to_str() {
            if test_fpath.starts_with(testdir_path) {
                return test_fpath[testdir_path.len() + MAIN_SEPARATOR.len_utf8()..]
                    .to_owned()
                    .replace(MAIN_SEPARATOR, "::");
            }
        }
    }

    test_fpath.file_stem().unwrap().to_str().unwrap().to_owned()
}

/// Run the tests for `path`.
fn run_tests(
    inner: Arc<LangTesterPooler>,
    tests: HashMap<String, TestCmd>,
    path: PathBuf,
    test_fname: String,
    failures: Arc<Mutex<Vec<(String, TestFailure)>>>,
) -> bool {
    if !cfg!(unix) && tests.values().any(|t| t.status == Status::Signal) {
        write_ignored(
            test_fname.as_str(),
            "signal termination not supported on this platform",
            inner,
        );
        return true;
    }

    let mut failure = TestFailure {
        status: None,
        stdin_remaining: 0,
        stderr: None,
        stderr_match: None,
        stdout: None,
        stdout_match: None,
    };

    let test_cmds = inner.test_cmds.as_ref().unwrap();
    let cmd_pairs = match catch_unwind(|| test_cmds(path.as_path())) {
        Ok(x) => x
            .into_iter()
            .map(|(test_name, cmd)| (test_name.to_lowercase(), cmd))
            .collect::<Vec<_>>(),
        Err(_) => {
            failures.lock().unwrap().push((test_fname, failure));
            return false;
        }
    };
    check_names(&cmd_pairs, &tests);

    'a: for (cmd_name, mut cmd) in cmd_pairs {
        let default_test = TestCmd::default();
        let test = tests.get(&cmd_name).unwrap_or(&default_test);
        cmd.args(&test.args);
        cmd.envs(&test.env);
        let mut rerun = 0;
        loop {
            rerun += 1;
            let (status, stdin_remaining, stderr, stdout) =
                run_cmd(inner.clone(), &test_fname, &mut cmd, test);

            let mut meant_to_error = false;

            // Give the user the option of setting options for the fuzzy matchers.
            let stderr_str = test.stderr.join("\n");
            let mut stderr_fmb = FMBuilder::new(&stderr_str).unwrap();
            let stdout_str = test.stdout.join("\n");
            let mut stdout_fmb = FMBuilder::new(&stdout_str).unwrap();

            let rerun_if_stderr_str = test.rerun_if_stderr.as_ref().unwrap_or(&vec![]).join("\n");
            let mut rerun_if_stderr_fmb = FMBuilder::new(&rerun_if_stderr_str).unwrap();
            let rerun_if_stdout_str = test.rerun_if_stdout.as_ref().unwrap_or(&vec![]).join("\n");
            let mut rerun_if_stdout_fmb = FMBuilder::new(&rerun_if_stdout_str).unwrap();
            if let Some(ref fm_options) = inner.fm_options {
                match catch_unwind(|| {
                    (
                        fm_options(path.as_path(), TestStream::Stderr, stderr_fmb),
                        fm_options(path.as_path(), TestStream::Stdout, stdout_fmb),
                        fm_options(path.as_path(), TestStream::Stderr, rerun_if_stderr_fmb),
                        fm_options(path.as_path(), TestStream::Stdout, rerun_if_stdout_fmb),
                    )
                }) {
                    Ok((a, b, c, d)) => {
                        stderr_fmb = a;
                        stdout_fmb = b;
                        rerun_if_stderr_fmb = c;
                        rerun_if_stdout_fmb = d;
                    }
                    Err(_) => {
                        failures.lock().unwrap().push((test_fname, failure));
                        return false;
                    }
                }
            }

            let match_stderr = match stderr_fmb.build() {
                Ok(x) => x.matches(&stderr),
                Err(e) => {
                    failure.stderr = Some(format!("FM error: {}", e));
                    break 'a;
                }
            };
            let match_stdout = match stdout_fmb.build() {
                Ok(x) => x.matches(&stdout),
                Err(e) => {
                    failure.stdout = Some(format!("FM error: {}", e));
                    break 'a;
                }
            };

            // First, check whether the tests passed.
            let pass_status = match test.status {
                Status::Success => status.success(),
                Status::Error => {
                    meant_to_error = true;
                    !status.success()
                }
                Status::Signal => status.signal().is_some(),
                Status::Int(i) => status.code() == Some(i),
            };

            // Second, if a test failed, we want to print out everything which didn't match
            // successfully (i.e. if the stderr test failed, print that out; but, equally, if
            // stderr wasn't specified as a test, print it out, because the user can't
            // otherwise know what it contains).
            if !(pass_status
                && stdin_remaining == 0
                && match_stderr.is_ok()
                && match_stdout.is_ok())
            {
                if rerun <= inner.rerun_at_most {
                    if let Some(rerun_if_status) = &test.rerun_if_status {
                        let rerun = match rerun_if_status {
                            Status::Success => status.success(),
                            Status::Error => !status.success(),
                            Status::Signal => status.signal().is_some(),
                            Status::Int(i) => status.code() == Some(*i),
                        };
                        if rerun {
                            continue;
                        }
                    }
                    if test.rerun_if_stderr.is_some() {
                        match rerun_if_stderr_fmb.build() {
                            Ok(x) if x.matches(&stderr).is_ok() => continue,
                            Ok(_) => {}
                            Err(e) => {
                                failure.stderr = Some(format!("FM error: {}", e));
                                break 'a;
                            }
                        }
                    }
                    if test.rerun_if_stdout.is_some() {
                        match rerun_if_stdout_fmb.build() {
                            Ok(x) if x.matches(&stdout).is_ok() => continue,
                            Ok(_) => {}
                            Err(e) => {
                                failure.stdout = Some(format!("FM error: {}", e));
                                break 'a;
                            }
                        }
                    }
                }

                match test.status {
                    Status::Success | Status::Error => {
                        if status.success() {
                            failure.status = Some("Success".to_owned());
                        } else if status.code().is_none() {
                            failure.status = Some(format!(
                                "Exited due to signal: {}",
                                status.signal().unwrap()
                            ));
                        } else {
                            failure.status = Some("Error".to_owned());
                        }
                    }
                    Status::Signal => {
                        failure.status = Some("Exit was not due to signal".to_owned());
                    }
                    Status::Int(_) => {
                        failure.status =
                            Some(status.code().map(|x| x.to_string()).unwrap_or_else(|| {
                                format!("Exited due to signal: {}", status.signal().unwrap())
                            }))
                    }
                }

                if match_stderr.is_err() || failure.stderr.is_none() {
                    failure.stderr = Some(stderr);
                }
                if let Err(e) = match_stderr {
                    failure.stderr_match = Some(e);
                }

                if match_stdout.is_err() || failure.stdout.is_none() {
                    failure.stdout = Some(stdout);
                }
                if let Err(e) = match_stdout {
                    failure.stdout_match = Some(e);
                }

                failure.stdin_remaining = stdin_remaining;

                // If a sub-test failed, bail out immediately, otherwise subsequent sub-tests
                // will overwrite the failure output!
                break 'a;
            }

            // If a command failed, and we weren't expecting it to, bail out immediately.
            if !status.success() && meant_to_error {
                break 'a;
            }
            break;
        }
    }

    {
        // Grab a lock on stderr so that we can avoid the possibility of lines blurring
        // together in confusing ways.
        let stderr = StandardStream::stderr(ColorChoice::Always);
        let mut handle = stderr.lock();
        if inner.test_threads > 1 {
            handle
                .write_all(format!("\ntest lang_tests::{} ... ", test_fname).as_bytes())
                .ok();
        }
        if failure
            != (TestFailure {
                status: None,
                stdin_remaining: 0,
                stderr: None,
                stderr_match: None,
                stdout: None,
                stdout_match: None,
            })
        {
            let mut failures = failures.lock().unwrap();
            failures.push((test_fname, failure));
            handle
                .set_color(ColorSpec::new().set_fg(Some(Color::Red)))
                .ok();
            handle.write_all(b"FAILED").ok();
        } else {
            handle
                .set_color(ColorSpec::new().set_fg(Some(Color::Green)))
                .ok();
            handle.write_all(b"ok").ok();
        }
        handle.reset().ok();
    }

    false
}

fn run_cmd(
    inner: Arc<LangTesterPooler>,
    test_fname: &str,
    cmd: &mut Command,
    test: &TestCmd,
) -> (ExitStatus, usize, String, String) {
    // The basic sequence here is:
    //   1) Spawn the command
    //   2) Read everything from stderr & stdout until they are both disconnected
    //   3) wait() for the command to finish

    let mut child = cmd
        .stdin(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| fatal(&format!("Couldn't run command {:?}.", cmd)));

    let mut stdin = child.stdin.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let stdin_fd = stdin.as_raw_fd();
    let stderr_fd = stderr.as_raw_fd();
    let stdout_fd = stdout.as_raw_fd();
    if let Err(e) = set_nonblock(stdin_fd)
        .and_then(|_| set_nonblock(stderr_fd))
        .and_then(|_| set_nonblock(stdout_fd))
    {
        fatal(&format!(
            "Couldn't set stdin and/or stderr and/or stdout to be non-blocking: {e:}"
        ));
    }

    const POLL_STDIN: usize = 0;
    const POLL_STDERR: usize = 1;
    const POLL_STDOUT: usize = 2;

    let mut cap_stderr = String::new();
    let mut cap_stdout = String::new();
    let mut stdin_off = 0;
    let mut buf = [0; READBUF];
    let start = Instant::now();
    let mut last_warning = Instant::now();
    let mut next_warning = last_warning
        .checked_add(Duration::from_secs(TIMEOUT))
        .unwrap();

    // Has this file reached EOF and thus been closed?
    const STATUS_EOF: u8 = 1;
    // Has this file hit an error and thus been closed? Note that EOF and ERR
    // are mutually exclusive.
    const STATUS_ERR: u8 = 2;

    let mut statuses: [u8; 3] = [0, 0, 0];
    if test.stdin.is_none() {
        unsafe {
            close(stdin_fd);
        }
        statuses[POLL_STDIN] = STATUS_EOF;
    }
    loop {
        // Are all files successfully closed?
        if statuses[POLL_STDIN] == STATUS_EOF
            && statuses[POLL_STDERR] == STATUS_EOF
            && statuses[POLL_STDOUT] == STATUS_EOF
        {
            // If there's still stuff in the buffer to write out, we've failed.
            if let Some(stdin_str) = &test.stdin {
                if stdin_off < stdin_str.len() {
                    fatal(&format!("{} failed to consume all of stdin", test_fname));
                }
            }
            break;
        }

        // Is at least one file in an error state and the other files are
        // closed?
        if statuses[POLL_STDIN] & (STATUS_EOF | STATUS_ERR) != 0
            && statuses[POLL_STDERR] & (STATUS_EOF | STATUS_ERR) != 0
            && statuses[POLL_STDOUT] & (STATUS_EOF | STATUS_ERR) != 0
        {
            fatal(&format!(
                "{} has left one of stdin/stderr/stdout in an error condition",
                test_fname
            ));
        }

        let mut pollfds = [
            pollfd {
                fd: stdin_fd,
                events: POLLOUT,
                revents: 0,
            },
            pollfd {
                fd: stderr_fd,
                events: POLLIN,
                revents: 0,
            },
            pollfd {
                fd: stdout_fd,
                events: POLLIN,
                revents: 0,
            },
        ];

        if statuses[POLL_STDIN] & (STATUS_EOF | STATUS_ERR) != 0 {
            // If the child process won't accept further input, there's no
            // point polling it.
            pollfds[POLL_STDIN].fd = -1;
        } else if let Some(stdin_str) = &test.stdin {
            if stdin_off == stdin_str.len() {
                // There's nothing to write to the child's stdin, but we'd still
                // like to check whether it is closed or has suffered an error,
                // so we don't want to set the fd to -1.
                pollfds[POLL_STDIN].events = 0;
            }
        }

        if statuses[POLL_STDERR] & (STATUS_EOF | STATUS_ERR) != 0 {
            // If the child's stderr cannot produce further output, there's no
            // point polling it.
            pollfds[POLL_STDERR].fd = -1;
        }

        if statuses[POLL_STDOUT] & (STATUS_EOF | STATUS_ERR) != 0 {
            // If the child's stdout cannot produce further output, there's no
            // point polling it.
            pollfds[POLL_STDOUT].fd = -1;
        }

        let timeout = i32::try_from(
            next_warning
                .checked_duration_since(Instant::now())
                .map(|d| d.as_millis())
                .unwrap_or(1000),
        )
        .unwrap_or(1000);
        if unsafe { poll((&mut pollfds) as *mut _ as *mut pollfd, 3, timeout) } != -1 {
            assert_eq!(pollfds[POLL_STDIN].revents & POLLNVAL, 0);
            if pollfds[POLL_STDIN].revents & POLLERR != 0 {
                assert!(test.stdin.is_some());
                statuses[POLL_STDIN] = STATUS_ERR;
                unsafe {
                    close(stdin_fd);
                }
            } else if pollfds[POLL_STDIN].revents & POLLOUT != 0 {
                let stdin_str = test.stdin.as_ref().unwrap();
                match stdin.write(&stdin_str.as_bytes()[stdin_off..]) {
                    Ok(i) => stdin_off += i,
                    Err(e) => {
                        if e.kind() != io::ErrorKind::Interrupted {
                            unsafe {
                                close(stdin_fd);
                            }
                            statuses[POLL_STDIN] = STATUS_ERR;
                        }
                    }
                }
                debug_assert!(stdin_off <= stdin_str.len());
                if stdin_off == stdin_str.len() {
                    // We've fully written to the child's stdin. We close the child's stdin
                    // explicitly otherwise some child processes will hang, waiting for more input
                    // to be received.
                    unsafe {
                        close(stdin_fd);
                    }
                    statuses[POLL_STDIN] = STATUS_EOF;
                }
            } else if pollfds[POLL_STDIN].revents & POLLHUP != 0 {
                // POSiX specifies that POLLOUT and POLLHUP are mutually exclusive.
                unsafe {
                    close(stdin_fd);
                }
                statuses[POLL_STDIN] = STATUS_EOF;
            }

            assert_eq!(pollfds[POLL_STDERR].revents & POLLNVAL, 0);
            if pollfds[POLL_STDERR].revents & POLLERR != 0 {
                unsafe {
                    close(stderr_fd);
                }
                statuses[POLL_STDERR] = STATUS_ERR;
            } else {
                if pollfds[POLL_STDERR].revents & POLLIN != 0 {
                    loop {
                        match stderr.read(&mut buf) {
                            Ok(i) => {
                                if i == 0 {
                                    // We'll pick up POLLHUP on the next poll()
                                    break;
                                }
                                let utf8 = str::from_utf8(&buf[..i]).unwrap_or_else(|_| {
                                    fatal(&format!(
                                        "Can't convert stderr from '{:?}' into UTF-8",
                                        cmd
                                    ))
                                });
                                cap_stderr.push_str(utf8);
                                if inner.nocapture {
                                    eprint!("{}", utf8);
                                }
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                            Err(e) => {
                                fatal(&format!("{}: failed to read stderr: {e:}", test_fname))
                            }
                        }
                    }
                }
                if pollfds[POLL_STDERR].revents & POLLHUP != 0 {
                    // Note that POLLIN and POLLHUP are not mutually exclusive.
                    unsafe {
                        close(stderr_fd);
                    }
                    statuses[POLL_STDERR] = STATUS_EOF;
                }
            }

            assert_eq!(pollfds[POLL_STDOUT].revents & POLLNVAL, 0);
            if pollfds[POLL_STDOUT].revents & POLLERR != 0 {
                unsafe {
                    close(stdout_fd);
                }
                statuses[POLL_STDOUT] = STATUS_ERR;
            } else {
                if pollfds[POLL_STDOUT].revents & POLLIN != 0 {
                    loop {
                        match stdout.read(&mut buf) {
                            Ok(i) => {
                                if i == 0 {
                                    // We'll pick up POLLHUP on the next poll()
                                    break;
                                }
                                let utf8 = str::from_utf8(&buf[..i]).unwrap_or_else(|_| {
                                    fatal(&format!(
                                        "Can't convert stdout from '{:?}' into UTF-8",
                                        cmd
                                    ))
                                });
                                cap_stdout.push_str(utf8);
                                if inner.nocapture {
                                    eprint!("{}", utf8);
                                }
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                            Err(e) => {
                                fatal(&format!("{}: failed to read stdout: {e:}", test_fname))
                            }
                        }
                    }
                }
                if pollfds[POLL_STDOUT].revents & POLLHUP != 0 {
                    // Note that POLLIN and POLLHUP are not mutually exclusive.
                    if statuses[POLL_STDOUT] & STATUS_EOF == 0 {
                        unsafe {
                            close(stdout_fd);
                        }
                        statuses[POLL_STDOUT] = STATUS_EOF;
                    }
                }
            }
        }

        if Instant::now() >= next_warning {
            let running_for = ((Instant::now() - start).as_secs() / TIMEOUT) * TIMEOUT;
            if inner.test_threads == 1 {
                eprint!("running for over {} seconds... ", running_for);
            } else {
                eprintln!(
                    "\nlang_tests::{} ... has been running for over {} seconds",
                    test_fname, running_for
                );
            }
            last_warning = next_warning;
            next_warning = last_warning
                .checked_add(Duration::from_secs(TIMEOUT))
                .unwrap();
        }
    }
    if statuses[POLL_STDIN] != 0 {
        std::mem::forget(stdin);
    }
    if statuses[POLL_STDERR] != 0 {
        std::mem::forget(stderr);
    }
    if statuses[POLL_STDOUT] != 0 {
        std::mem::forget(stdout);
    }

    let status = {
        // We have no idea how long it will take the child process to exit. In practise, the mere
        // act of yielding (via sleep) for a ridiculously short period of time will often be enough
        // for the child process to exit. So we use an exponentially increasing timeout with a very
        // short initial period so that, in the common case, we don't waste time waiting for
        // something that's almost certainly already occurred.
        let mut wait_timeout = INITIAL_WAIT_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => (),
                Err(e) => fatal(&format!("{:?} did not exit correctly: {:?}", cmd, e)),
            }

            if Instant::now() >= next_warning {
                let running_for = ((Instant::now() - start).as_secs() / TIMEOUT) * TIMEOUT;
                if inner.test_threads == 1 {
                    eprint!("running for over {} seconds... ", running_for);
                } else {
                    eprintln!(
                        "\nlang_tests::{} ... has been running for over {} seconds",
                        test_fname, running_for
                    );
                }
                last_warning = next_warning;
                next_warning = last_warning
                    .checked_add(Duration::from_secs(TIMEOUT))
                    .unwrap();
            }
            sleep(Duration::from_nanos(wait_timeout));
            wait_timeout *= 2;
            if wait_timeout > MAX_WAIT_TIMEOUT {
                wait_timeout = MAX_WAIT_TIMEOUT;
            }
        }
    };

    let stdin_remaining = if let Some(stdin_str) = &test.stdin {
        stdin_str.len() - stdin_off
    } else {
        0
    };
    (status, stdin_remaining, cap_stderr, cap_stdout)
}

fn set_nonblock(fd: c_int) -> Result<(), io::Error> {
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags == -1 || unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}
