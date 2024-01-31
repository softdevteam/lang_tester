# lang_tester 0.8.0 (2024-01-31)

## Breaking change

* Remove `ignored` and add `ignore-if`. The latter runs an arbitrary shell
  command which, if it returns zero, causes the test to be ignored. This allows
  much more flexibility than the overly simplistic "always ignore this test" of
  `ignored`. Tests with `ignored: <reason>` can be changed to `ignore-if: true`
  followed (or preceded) by a comment `# <reason>` (assuming `comment_prefix`
  is set: see below).

## Non-breaking change

* Allow comments in tests with a user-configurable prefix. By default no
  comment prefix is set. You can set one with `comment_prefix("...")`. For
  example `LangTester::new().comment_prefix("#")` causes lines in tests
  starting with `#` to be entirely ignored by lang_tester.


# lang_tester 0.7.6 (2024-01-22)

* `test_file_filter` is deprecated in favour of `test_path_filter`. The latter
  doesn't pre-filter non-files, making it more flexible. A simple way of moving
  from `test_file_filter` to `test_path_filter` is to change
  `test_file_filter(|p| ...)` to `test_path_fiter(|p| p.is_file() & ...)`.


# lang_tester 0.7.5 (2023-11-03)

* Make the library documentation (rather than the README) the source of
  documentation truth.


# lang_tester 0.7.4 (2023-09-21)

* Allow test filtering on the full test name (e.g. `lang_tests::a::b::c`)
  rather than just the leaf (e.g `c`).


# lang_tester 0.7.3 (2023-04-06)

* Add support for rerun-if-{status, stderr, stdout}.


# lang_tester 0.7.2 (2021-12-07)

* Fix poll() loop, so the full output of a subcommand is now read properly.


# lang_tester 0.7.1 (2021-12-07)

* Show a test as failing if `FMBuilder` throws an error.


# lang_tester 0.7.0 (2021-06-18)

## Breaking changes

* The `extra-args` key has been renamed to `exec-arg` to reflect the fact that
  each key is a single argument.

## Other changes

* The `env-var` key has been added. This allows environment variables to be set
  on a per-test basis e.g.:

```
Compiler:
  env-var: DEBUG=1
  stdout: xyz
```


# lang_tester 0.6.2 (2021-05-24)

* Fix file descriptor race for tests that contain stdin data: files were closed
  twice, which could lead to an active (reused) file descriptor being closed
  incorrectly.

* Documentation fixes.


# lang_tester 0.6.1 (2021-04-30)

* Fix test file filtering.


# lang_tester 0.6.0 (2021-04-30)

* If a function passed by the user to the user (e.g. to `test_extract`)
  `panic`s, `lang_tester` now considers that a test failure and reports it to
  the user. Because this uses `catch_unwind` underneath, the functions passed
  to `lang_tester` must now be `RefUnwindSafe`.


# lang_tester 0.5.0 (2021-01-27)

* The `test_extract` function signature has changed from:
  ```
  Fn(&str) -> Option<String> + Send + Sync,
  ```
  to:
  ```
  Fn(&Path) -> String + Send + Sync,
  ```

  In other words, users now have to both:

    1. read the contents of a path themselves (but it doesn't necessarily have
       to be the path passed to the function!),
    2. and return a `String` rather than an `Option<String>`.

  In practise, most `test_extract` functions can be changed from (roughly):
  ```
  test_extract(|s| { s.lines() ... })
  ```
  to:
  ```
  test_extract(|p| { std::fs::read_to_string(p).lines() })
  ```


# lang_tester 0.4.0 (2020-11-26)

* Update to fm 0.2.0. This changes the interface exposed by the `fm_options`
  function. See the [`fm`
  changes](https://github.com/softdevteam/fm/blob/master/CHANGES.md) for more
  information.


# lang_tester 0.3.13 (2020-11-09)

* Silence some Clippy warnings and fix documentation inconsistencies.


# lang_tester 0.3.12 (2020-07-13)

* Failed stderr/stdout tests now use fm to show the offending line and up to 3
  lines of surrounding context. This makes it much easier to understand why a
  stderr/test failed.


# lang_tester 0.3.11 (2020-07-09)

* Remove the built-in fuzzy matcher and use the [`fm`
  library](https://crates.io/crates/fm) instead. This should be entirely
  backwards compatible in its default state. Users who want non-default `fm`
  options can use the new `fm_options` function in `LangTester`.

* Add a `stdin` key to allow users to specify stdin input which should be
  passed to a sub-command.

* Lines are no longer stripped of their leading or trailing whitespace allowing
  tests to be whitespace sensitive if required. Since matching in `fm` defaults
  to ignoring leading and trailing whitespace, the previous behaviour is
  preserved unless users explicitly tell `fm` to match whitespace.


# lang_tester 0.3.10 (2020-06-04)

* Print out the name of tests inside nested directories rather than flattening
  them all such that they appear to be the top-level directory. If you have
  tests `a/x` and `b/x` these are pretty printed as `a::x` and `b::x`
  respectively (whereas before they were pretty printed as simply `x`, meaning
  that you could not tell which had succeeded / failed).


# lang_tester 0.3.9 (2020-05-18)

* Add `test_threads` function which allows you to specify the number of test
  threads programatically.

* Move from the deprecated `tempdir` to the maintained `tempfile` crate.


# lang_tester 0.3.8 (2019-12-24)

* Fix bug on OS X where input from sub-processes blocked forever.


# lang_tester 0.3.7 (2019-11-26)

* Add support for ignorable tests. A test command `ignore:`  is interpreted as
  causing that entire test file to be ignored. As with `cargo test`, such tests
  can be run with the `--ignored` switch.

* Fix a bug whereby the number of ignored tests was incorrectly reported.


# lang_tester 0.3.6 (2019-11-21)

* License as dual Apache-2.0/MIT (instead of a more complex, and little
  understood, triple license of Apache-2.0/MIT/UPL-1.0).


# lang_tester 0.3.5 (2019-11-15)

* Add support for programs which terminated due to a signal. Users can now
  specify `status: signal` to indicate that a test should exit due to a signal:
  on platforms which do not support this (e.g. Windows), such tests are
  ignored. Similarly, if a program was terminated due to a signal then, on
  Unix, the user is informed of that after test failure.


# lang_tester 0.3.4 (2019-10-30)

* Add support for `--nocapture` to better emulate `cargo test`. As with `cargo
  test`, if you're running more than one test then `--nocapture` is generally
  best paired with `--test-threads=1` to avoid confusing, multiplexed output to
  the terminal.

* Be clearer that tests can have defaults: notably commands default to `status:
  success` unless overridden.


# lang_tester 0.3.3 (2019-10-24)

* Individual tests can now add extra arguments to an invoked command with the
  `extra-args` field.

* Ensure that, if a command in a chain fails, the whole chain of commands
  fails. This means that if, for example, compilation of command C fails, we do
  not try and run C anyway (which can end up doing confusing things like
  running an old version of C).


# lang_tester 0.3.2 (2019-07-31)

* Fixed bug where potentially multi-line keys with empty values were not always
  parsed correctly.


# lang_tester 0.3.1 (2019-06-04)

* Add support for running a defined number of parallel processes, using the
  `cargo test`-ish option `--test-threads=n`. For example, to run tests
  sequentially, specify `--test-threads=1`.

* Warn users if a given test has run unexpectedly long (currently every
  multiple of 60 seconds). This is often a sign that a test has entered an
  infinite loop.

* Use better terminology in the documentation. Previously "test" was used to
  mean a number of subtly different things which was rather confusing. Now
  test files contain test data. Test data contains test commands. Test commands
  contain sub-tests.

* Stop testing a given test file on the first failed sub-test. Previously only
  a test command which exited unsuccessfully caused a test file to be
  considered as failed, causing the source of errors to sometimes be missed.


# lang_tester 0.3.0 (2019-05-29)

## Breaking changes

* The `test_extract` and `test_cmds` functions must now satisfy the `Sync`
  trait. This is a breaking change, albeit one that nearly all such functions
  already satisfied.

## Major changes

* When a test fails, report to the user both the parts of the test that failed
  and the parts that weren't specified. For example, if a test merely checks
  that a command runs successfully, we now report stdout and stderr output to
  the user, so that they can better understand what happened.

## Minor changes

* Fatal errors (e.g. an inability to run a command, or an error in the way a
  user has specified a test, such as a syntax error) now cause the process to
  exit (whereas before they merely caused the thread erroring to panic, leading
  to errors being lost in the noise).


# lang_tester 0.2.0 (2019-05-21)

* Accept cargo-ish command-line parameters. In particular, this lets users run
  a subset of tests e.g. "<run tests> ab cd" only runs tests with "ab" or "cd"
  in their name. If you don't want `lang_tester` to look at your command-line
  arguments, set `use_cmdline_args(false)` (the default is `true`).

* Run tests in parallel (one per CPU core). Depending on the size of your
  machine and the size of your test suite, this can be a significant
  performance improvement.

* The `status` field can now take integer exit codes. i.e. if you specify
  `status: 7` then the exit code of the binary being run will be checked to see
  if it is 7.


# lang_tester 0.1.0 (2019-05-16)

First stable release.
