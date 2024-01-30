use std::collections::hash_map::{Entry, HashMap};

use crate::{
    fatal,
    tester::{Status, TestCmd, Tests},
};

/// Parse test data into a set of `Test`s.
pub(crate) fn parse_tests<'a>(comment_prefix: Option<&str>, test_str: &'a str) -> Tests<'a> {
    let lines = test_str.lines().collect::<Vec<_>>();
    let mut tests = HashMap::new();
    let mut line_off = 0;
    let mut ignore_if = None;
    while line_off < lines.len() {
        let indent = indent_level(&lines, line_off);
        if indent == lines[line_off].len() {
            line_off += 1;
            continue;
        }
        if let Some(cp) = comment_prefix {
            if lines[line_off][indent..].starts_with(cp) {
                line_off += 1;
                continue;
            }
        }
        let (test_name, val) = key_val(&lines, line_off, indent);
        if test_name == "ignore-if" {
            ignore_if = Some(val.into());
            line_off += 1;
            continue;
        }
        if !val.is_empty() {
            fatal(&format!(
                "Test name '{}' can't have a value on line {}.",
                test_name, line_off
            ));
        }
        match tests.entry(test_name.to_lowercase()) {
            Entry::Occupied(_) => fatal(&format!(
                "Command name '{}' is specified more than once, line {}.",
                test_name, line_off
            )),
            Entry::Vacant(e) => {
                line_off += 1;
                let mut testcmd = TestCmd::default();
                while line_off < lines.len() {
                    let sub_indent = indent_level(&lines, line_off);
                    if sub_indent == lines[line_off].len() {
                        line_off += 1;
                        continue;
                    }
                    if sub_indent == indent {
                        break;
                    }
                    let (end_line_off, key, val) =
                        key_multiline_val(comment_prefix, &lines, line_off, sub_indent);
                    line_off = end_line_off;
                    match key {
                        "env-var" => {
                            let val_str = val.join("\n");
                            match val_str.find('=') {
                                Some(i) => {
                                    let key = val_str[..i].trim().to_owned();
                                    let var = val_str[i + 1..].trim().to_owned();
                                    testcmd.env.insert(key, var);
                                }
                                None => {
                                    fatal(&format!(
                                        "'{}' is not in the format '<key>=<string>' on line {}",
                                        val_str, line_off
                                    ));
                                }
                            }
                        }
                        "exec-arg" => {
                            let val_str = val.join("\n");
                            testcmd.args.push(val_str);
                        }
                        "status" | "rerun-if-status" => {
                            let val_str = val.join("\n");
                            let status = match val_str.to_lowercase().as_str() {
                                "success" => Status::Success,
                                "error" => Status::Error,
                                "signal" => Status::Signal,
                                x => {
                                    if let Ok(i) = x.parse::<i32>() {
                                        Status::Int(i)
                                    } else {
                                        fatal(&format!(
                                            "Unknown status '{}' on line {}",
                                            val_str, line_off
                                        ));
                                    }
                                }
                            };
                            match key {
                                "status" => {
                                    testcmd.status = status;
                                }
                                "rerun-if-status" => {
                                    testcmd.rerun_if_status = Some(status);
                                }
                                _ => {
                                    unreachable!();
                                }
                            }
                        }
                        "stdin" => {
                            testcmd.stdin = Some(val.join("\n"));
                        }
                        "stderr" => {
                            testcmd.stderr = val;
                        }
                        "stdout" => {
                            testcmd.stdout = val;
                        }
                        "rerun-if-stderr" => {
                            testcmd.rerun_if_stderr = Some(val);
                        }
                        "rerun-if-stdout" => {
                            testcmd.rerun_if_stdout = Some(val);
                        }
                        _ => fatal(&format!("Unknown key '{}' on line {}.", key, line_off)),
                    }
                }
                e.insert(testcmd);
            }
        }
    }
    Tests { ignore_if, tests }
}

fn indent_level(lines: &[&str], line_off: usize) -> usize {
    lines[line_off]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count()
}

/// Turn a line such as `key: val` into its separate components.
fn key_val<'a>(lines: &[&'a str], line_off: usize, indent: usize) -> (&'a str, &'a str) {
    let line = lines[line_off];
    let key_len = line[indent..]
        .chars()
        .take_while(|c| !(c.is_whitespace() || c == &':'))
        .count();
    let key = &line[indent..indent + key_len];
    let mut content_start = indent + key_len;
    content_start += line[content_start..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    match line[content_start..].chars().next() {
        Some(':') => content_start += ':'.len_utf8(),
        _ => fatal(&format!(
            "Invalid key terminator at line {}.\n  {}",
            line_off, line
        )),
    }
    content_start += line[content_start..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    (key, &line[content_start..])
}

/// Turn one more lines of the format `key: val` (where `val` may spread over many lines) into its
/// separate components.
fn key_multiline_val<'a>(
    comment_prefix: Option<&str>,
    lines: &[&'a str],
    mut line_off: usize,
    indent: usize,
) -> (usize, &'a str, Vec<&'a str>) {
    let (key, first_line_val) = key_val(lines, line_off, indent);
    line_off += 1;
    let mut val = vec![first_line_val];
    if line_off < lines.len() {
        let sub_indent = indent_level(lines, line_off);
        while line_off < lines.len() {
            let cur_indent = indent_level(lines, line_off);
            if cur_indent == lines[line_off].len() {
                val.push("");
                line_off += 1;
                continue;
            }
            if cur_indent <= indent {
                break;
            }
            if let Some(cp) = comment_prefix {
                if lines[line_off][sub_indent..].starts_with(cp) {
                    line_off += 1;
                    continue;
                }
            }
            val.push(&lines[line_off][sub_indent..]);
            line_off += 1;
        }
    }
    // Remove trailing empty strings
    while !val.is_empty() && val[val.len() - 1].is_empty() {
        val.pop();
    }
    // Remove leading empty strings
    while !val.is_empty() && val[0].is_empty() {
        val.remove(0);
    }

    (line_off, key, val)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_key_multiline() {
        assert_eq!(key_multiline_val(None, &["x:", ""], 0, 0), (2, "x", vec![]));
        assert_eq!(
            key_multiline_val(None, &["x: y", "  z", "a"], 0, 0),
            (2, "x", vec!["y", "z"])
        );
        assert_eq!(
            key_multiline_val(None, &["x:", "  z", "a"], 0, 0),
            (2, "x", vec!["z"])
        );
        assert_eq!(
            key_multiline_val(None, &["x:", "  z  ", "  a  ", "  ", "b"], 0, 0),
            (4, "x", vec!["z  ", "a  "])
        );
        assert_eq!(
            key_multiline_val(None, &["x:", "  z  ", "    a  ", "  ", "  b"], 0, 0),
            (5, "x", vec!["z  ", "  a  ", "", "b"])
        );
        assert_eq!(
            key_multiline_val(
                Some("#"),
                &["x:", "  z  ", "    a  ", "  # c2", "  ", "  b"],
                0,
                0
            ),
            (6, "x", vec!["z  ", "  a  ", "", "b"])
        );
    }
}
