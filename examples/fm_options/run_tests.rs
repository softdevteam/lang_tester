//! This is an example lang_tester that shows how to set options using the `fm` fuzzy matcher
//! library, using Python files as an example.

use std::{fs::read_to_string, process::Command};

use lang_tester::LangTester;
use regex::Regex;

fn main() {
    LangTester::new()
        .test_dir("examples/fm_options/lang_tests")
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
