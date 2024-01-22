//! This is an example lang_tester that shows how to use the "rerun_if" commands.

use std::{
    env,
    fs::{read_to_string, remove_file},
    path::PathBuf,
    process::Command,
};

/// In order that we can meaningfully have the same test behave differently from run to run, we
/// have to leave cookies behind in target/tmp, which we must clear before starting a test run.
/// This is horrible, but without doing something stateful, we can't test anything.
const COOKIES: &[&str] = &[
    "rerun_status_cookie",
    "rerun_stderr_cookie",
    "rerun_stdout_cookie",
];

use lang_tester::LangTester;
use regex::Regex;

fn main() {
    env::set_var("CARGO_TARGET_TMPDIR", env!("CARGO_TARGET_TMPDIR"));

    for cookie in COOKIES {
        let mut p = PathBuf::new();
        p.push(env::var("CARGO_TARGET_TMPDIR").unwrap());
        p.push(cookie);
        if p.exists() {
            remove_file(p).unwrap();
        }
    }

    LangTester::new()
        .rerun_at_most(5)
        .test_dir("lang_tests/rerun/")
        .test_path_filter(|p| p.extension().and_then(|x| x.to_str()) == Some("py"))
        .test_extract(|p| {
            read_to_string(p)
                .unwrap()
                .lines()
                // Skip non-commented lines at the start of the file.
                .skip_while(|l| !l.starts_with("#"))
                // Extract consecutive commented lines.
                .take_while(|l| l.starts_with("#"))
                .map(|l| &l[2..])
                .collect::<Vec<_>>()
                .join("\n")
        })
        .fm_options(|_, _, fmb| {
            let ptn_re = Regex::new(r"\$.+?\b").unwrap();
            let text_re = Regex::new(r".+?\b").unwrap();
            fmb.name_matcher(ptn_re, text_re)
                .ignore_leading_whitespace(false)
        })
        .test_cmds(move |p| {
            let mut vm = Command::new("python3");
            vm.args(&[p.to_str().unwrap()]);
            vec![("VM", vm)]
        })
        .run();
}
