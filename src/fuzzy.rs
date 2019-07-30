// Copyright (c) 2019 King's College London created by the Software Development Team
// <http://soft-dev.org/>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0>, or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, or the UPL-1.0 license <http://opensource.org/licenses/UPL>
// at your option. This file may not be copied, modified, or distributed except according to those
// terms.

use crate::fatal;

const WILDCARD: &str = "...";

/// Does `s` conform to the fuzzy pattern `pattern`? Note that `plines` is expected not to start or
/// end with blank lines, and each line is expected to be `trim`ed.
pub(crate) fn match_vec(plines: &[&str], s: &str) -> bool {
    debug_assert!(plines.is_empty() || !plines[0].is_empty());
    debug_assert!(plines.is_empty() || !plines[plines.len() - 1].is_empty());
    let slines = s.trim().lines().map(|x| x.trim()).collect::<Vec<_>>();

    let mut pi = 0;
    let mut si = 0;

    while pi < plines.len() && si < slines.len() {
        if plines[pi] == WILDCARD {
            pi += 1;
            if pi == plines.len() {
                return true;
            }
            if plines[pi] == WILDCARD {
                fatal(&format!(
                    "Can't have '{}' on two consecutive lines.",
                    WILDCARD
                ));
            }
            while si < slines.len() && !match_line(&plines[pi], slines[si]) {
                si += 1;
            }
        } else if match_line(&plines[pi], slines[si]) {
            pi += 1;
            si += 1;
        } else {
            return false;
        }
    }
    (pi == plines.len() && si == slines.len())
        || (pi + 1 == plines.len() && plines[pi] == WILDCARD && si == slines.len())
}

/// Does the line `s` match the pattern `p`? Note that both strings are expected to be trimed
/// before being passed to this function.
fn match_line(p: &str, s: &str) -> bool {
    let sww = p.starts_with(WILDCARD);
    let eww = p.ends_with(WILDCARD);
    if sww && eww {
        s.find(&p[WILDCARD.len()..p.len() - WILDCARD.len()])
            .is_some()
    } else if sww {
        s.ends_with(&p[WILDCARD.len()..])
    } else if eww {
        s.starts_with(&p[..p.len() - WILDCARD.len()])
    } else {
        p == s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_vec() {
        fn match_vec_helper(p: &str, s: &str) -> bool {
            match_vec(&p.lines().collect::<Vec<_>>(), s)
        }
        assert!(match_vec_helper("", ""));
        assert!(match_vec_helper("a", "a"));
        assert!(!match_vec_helper("a", "ab"));
        assert!(match_vec_helper("...\na", "a"));
        assert!(match_vec_helper("...\na\n...", "a"));
        assert!(match_vec_helper("a\n...", "a"));
        assert!(match_vec_helper("a\n...\nd", "a\nd"));
        assert!(match_vec_helper("a\n...\nd", "a\nb\nc\nd"));
        assert!(!match_vec_helper("a\n...\nd", "a\nb\nc"));
        assert!(match_vec_helper("a\n...\nc\n...\ne", "a\nb\nc\nd\ne"));
        assert!(match_vec_helper("a\n...\n...b", "a\nb"));
        assert!(match_vec_helper("a\n...\nb...", "a\nb"));
        assert!(match_vec_helper("a\n...\nb...", "a\nbc"));
        assert!(match_vec_helper("a\nb...", "a\nbc"));
        assert!(!match_vec_helper("a\nb...", "a\nb\nc"));
        assert!(match_vec_helper("a\n...b...", "a\nb"));
        assert!(match_vec_helper("a\n...b...", "a\nxbz"));
        assert!(match_vec_helper("a\n...b...", "a\nbz"));
        assert!(match_vec_helper("a\n...b...", "a\nxb"));
        assert!(!match_vec_helper("a\n...b...", "a\nxb\nc"));
    }
}
