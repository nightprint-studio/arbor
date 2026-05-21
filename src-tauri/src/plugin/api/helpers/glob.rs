//! Basename glob matcher for `arbor.fs.glob`.
//!
//! Supports `*`, `?`, and `[abc]` / `[a-z]` character classes (POSIX-ish).
//! Dot-prefixed names are matched by `*` (non-POSIX but what users expect
//! when deleting `*.tmp`). Case-sensitive on every platform.

use std::path::Path;

pub(crate) fn glob_match_basename(name: &str, pattern: &str) -> bool {
    glob_inner(name.as_bytes(), pattern.as_bytes())
}

fn glob_inner(name: &[u8], pat: &[u8]) -> bool {
    // Iterative with a single backtrack slot — handles `*` the usual way.
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star_i, mut star_j) = (usize::MAX, 0usize);
    while i < name.len() {
        if j < pat.len() {
            match pat[j] {
                b'?' => { i += 1; j += 1; continue; }
                b'*' => { star_j = j; star_i = i; j += 1; continue; }
                b'[' => {
                    // Find closing ']' — malformed patterns fall through to literal match.
                    if let Some(close) = pat[j+1..].iter().position(|&b| b == b']') {
                        let class = &pat[j+1..j+1+close];
                        let mut negate = false;
                        let class = if !class.is_empty() && class[0] == b'!' { negate = true; &class[1..] } else { class };
                        let mut matched = false;
                        let mut k = 0;
                        while k < class.len() {
                            if k + 2 < class.len() && class[k+1] == b'-' {
                                if name[i] >= class[k] && name[i] <= class[k+2] { matched = true; break; }
                                k += 3;
                            } else {
                                if name[i] == class[k] { matched = true; break; }
                                k += 1;
                            }
                        }
                        if matched != negate { i += 1; j += j+2+close - j; continue; }
                    }
                    // malformed '[' — treat as literal
                    if pat[j] == name[i] { i += 1; j += 1; continue; }
                }
                c if c == name[i] => { i += 1; j += 1; continue; }
                _ => {}
            }
        }
        // No match. Backtrack to the last `*` if any.
        if star_j != usize::MAX {
            j = star_j + 1;
            star_i += 1;
            i = star_i;
            continue;
        }
        return false;
    }
    // Consume trailing `*` patterns.
    while j < pat.len() && pat[j] == b'*' { j += 1; }
    j == pat.len()
}

pub(crate) fn walk_glob(
    dir:          &Path,
    pattern:      &str,
    depth:        i64,
    max_depth:    i64,
    include_dirs: bool,
    out:          &mut Vec<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = path.is_dir();
        let name = path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let hit = glob_match_basename(name, pattern);
        if hit && (!is_dir || include_dirs) {
            if let Some(s) = path.to_str() { out.push(s.to_string()); }
        }
        if is_dir && (max_depth < 0 || depth < max_depth) {
            walk_glob(&path, pattern, depth + 1, max_depth, include_dirs, out);
        }
    }
}
