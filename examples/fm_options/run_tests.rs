//! This is an example lang_tester that shows how to set options using the `fm` fuzzy matcher
//! library, using Python files as an example.

use std::process::Command;

use lang_tester::LangTester;
use regex::Regex;

fn main() {
    LangTester::new()
        .test_dir("examples/fm_options/lang_tests")
        .test_file_filter(|p| p.extension().unwrap().to_str().unwrap() == "py")
        .test_extract(|s| {
            Some(
                s.lines()
                    // Skip non-commented lines at the start of the file.
                    .skip_while(|l| !l.starts_with("#"))
                    // Extract consecutive commented lines.
                    .take_while(|l| l.starts_with("#"))
                    .map(|l| &l[2..])
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })
        .fm_options(|_, _, fmb| {
            let ptn_re = Regex::new(r"\$.+?\b").unwrap();
            let text_re = Regex::new(r".+?\b").unwrap();
            fmb.name_matcher(Some((ptn_re, text_re)))
                .ignore_leading_whitespace(false)
        })
        .test_cmds(move |p| {
            let mut vm = Command::new("python3");
            vm.args(&[p.to_str().unwrap()]);
            vec![("VM", vm)]
        })
        .run();
}
