//! Shell-style wildcard path matching shared by `ls` (filter/exclude
//! patterns) and `diff` (filter patterns).

/// Returns true if `full_path` equals `pattern`, or is nested underneath it.
/// `pattern` is matched segment-by-segment against `full_path`'s leading
/// segments; each pattern segment may contain `*` as a wildcard matching any
/// sequence of characters *within that segment* (unlike the `-x` exclude
/// patterns, a segment wildcard here does not span `/` — e.g. `Root/*`
/// matches any direct child of `Root`, not arbitrarily deep descendants).
pub(crate) fn path_under_pattern(full_path: &str, pattern: &str) -> bool {
    let path_segments: Vec<&str> = full_path.trim_start_matches('/').split('/').collect();
    let pattern_segments: Vec<&str> = pattern.trim_start_matches('/').split('/').collect();
    if path_segments.len() < pattern_segments.len() {
        return false;
    }
    pattern_segments
        .iter()
        .zip(path_segments.iter())
        .all(|(pat, seg)| wildcard_match(pat, seg))
}

/// Simple shell-style wildcard match: `*` in `pattern` matches any sequence
/// of characters (including none, and including `/`). All other characters
/// must match literally. No other wildcard syntax (e.g. `?`, `[abc]`) is
/// supported.
pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut match_start = 0usize;

    while ti < t.len() {
        if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            match_start = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            match_start += 1;
            ti = match_start;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}
