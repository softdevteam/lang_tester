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
