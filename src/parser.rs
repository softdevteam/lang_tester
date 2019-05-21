// Copyright (c) 2019 King's College London created by the Software Development Team
// <http://soft-dev.org/>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0>, or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, or the UPL-1.0 license <http://opensource.org/licenses/UPL>
// at your option. This file may not be copied, modified, or distributed except according to those
// terms.

use std::collections::hash_map::{Entry, HashMap};

use crate::tester::{Status, Test};

/// Parse test input into a set of `Test`s.
pub(crate) fn parse_tests(test_str: &str) -> HashMap<String, Test> {
    let lines = test_str.lines().collect::<Vec<_>>();
    let mut tests = HashMap::new();
    let mut line_off = 0;
    while line_off < lines.len() {
        let indent = indent_level(&lines, line_off);
        if indent == lines[line_off].len() {
            line_off += 1;
            continue;
        }
        let (test_name, val) = key_val(&lines, line_off, indent);
        if !val.is_empty() {
            panic!(
                "Test name '{}' can't have a value on line {}.",
                test_name, line_off
            );
        }
        match tests.entry(test_name.to_lowercase()) {
            Entry::Occupied(_) => panic!(
                "Command name '{}' is specified more than once, line {}.",
                test_name, line_off
            ),
            Entry::Vacant(e) => {
                line_off += 1;
                let mut test = Test {
                    status: None,
                    stderr: None,
                    stdout: None,
                };
                while line_off < lines.len() {
                    let sub_indent = indent_level(&lines, line_off);
                    if sub_indent == lines[line_off].len() {
                        line_off += 1;
                        continue;
                    }
                    if sub_indent == indent {
                        break;
                    }
                    let (end_line_off, key, val) = key_multiline_val(&lines, line_off, sub_indent);
                    line_off = end_line_off;
                    match key {
                        "status" => {
                            let val_str = val.join("\n");
                            let status = match val_str.to_lowercase().as_str() {
                                "success" => Status::Success,
                                "error" => Status::Error,
                                x => {
                                    if let Ok(i) = x.parse::<i32>() {
                                        Status::Int(i)
                                    } else {
                                        panic!("Unknown status '{}' on line {}", val_str, line_off);
                                    }
                                }
                            };
                            test.status = Some(status);
                        }
                        "stderr" => {
                            test.stderr = Some(val);
                        }
                        "stdout" => {
                            test.stdout = Some(val);
                        }
                        _ => panic!("Unknown key '{}' on line {}.", key, line_off),
                    }
                }
                e.insert(test);
            }
        }
    }
    tests
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
    match line[content_start..].chars().nth(0) {
        Some(':') => content_start += ':'.len_utf8(),
        _ => panic!("Invalid key terminator at line {}.\n  {}", line_off, line),
    }
    content_start += line[content_start..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    (key, &line[content_start..].trim())
}

/// Turn one more lines of the format `key: val` (where `val` may spread over many lines) into its
/// separate components. Guarantees to trim leading and trailing newlines.
fn key_multiline_val<'a>(
    lines: &[&'a str],
    mut line_off: usize,
    indent: usize,
) -> (usize, &'a str, Vec<&'a str>) {
    let orig_line_off = line_off;
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
            val.push(&lines[line_off][sub_indent..].trim());
            line_off += 1;
        }
    }
    // Remove trailing empty strings
    val.drain(
        val.iter()
            .rposition(|x| !x.is_empty())
            .map(|x| x + 1)
            .unwrap_or_else(|| val.len())..,
    );
    // Remove leading empty strings
    val.drain(0..val.iter().position(|x| !x.is_empty()).unwrap_or(0));
    if val.is_empty() {
        panic!("Key without value at line {}", orig_line_off);
    }
    (line_off, key, val)
}
