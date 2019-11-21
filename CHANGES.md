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
