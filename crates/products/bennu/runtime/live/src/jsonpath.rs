//! A small JSONPath subset for assertions: `$`, `.key`, `['key']`, `[3]`, `[-1]`.
//!
//! No filters, wildcards or recursion on purpose: an assertion names one value. A path that needs
//! more is a sign the assertion should be several, and a single value is what exports cleanly to
//! other test formats later.

use serde_json::Value;

/// The value `path` names in `root`, or `None` when any step is missing.
pub fn select<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let steps = parse(path).ok()?;
    let mut cur = root;
    for step in steps {
        cur = match step {
            Step::Key(k) => cur.get(&k)?,
            Step::Index(i) => {
                let arr = cur.as_array()?;
                let idx = if i < 0 { arr.len().checked_sub(i.unsigned_abs() as usize)? } else { i as usize };
                arr.get(idx)?
            }
        };
    }
    Some(cur)
}

/// Whether `path` is well formed (for validation of a scenario file before it runs).
pub fn is_valid(path: &str) -> bool {
    parse(path).is_ok()
}

#[derive(Debug, PartialEq)]
enum Step {
    Key(String),
    Index(i64),
}

fn parse(path: &str) -> Result<Vec<Step>, String> {
    let rest = path.trim().strip_prefix('$').ok_or("a JSON path starts with `$`")?;
    let chars: Vec<char> = rest.chars().collect();
    let mut steps = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '.' => {
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end] != '.' && chars[end] != '[' {
                    end += 1;
                }
                if end == start {
                    return Err("empty key after `.`".into());
                }
                steps.push(Step::Key(chars[start..end].iter().collect()));
                i = end;
            }
            '[' => {
                let close = chars[i..].iter().position(|c| *c == ']').ok_or("unclosed `[`")? + i;
                let inner: String = chars[i + 1..close].iter().collect();
                let inner = inner.trim();
                if let Some(q) = inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).or_else(|| inner.strip_prefix('"').and_then(|s| s.strip_suffix('"'))) {
                    steps.push(Step::Key(q.to_string()));
                } else {
                    steps.push(Step::Index(inner.parse().map_err(|_| format!("`{inner}` is not an index or a quoted key"))?));
                }
                i = close + 1;
            }
            c => return Err(format!("unexpected `{c}` in the path")),
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_indexes_and_quoted_keys() {
        let v = json!({"a": {"b": [10, 20, {"c": "x"}]}, "we ird": 1});
        assert_eq!(select(&v, "$.a.b[1]"), Some(&json!(20)));
        assert_eq!(select(&v, "$.a.b[2].c"), Some(&json!("x")));
        assert_eq!(select(&v, "$.a.b[-1].c"), Some(&json!("x")));
        assert_eq!(select(&v, "$['we ird']"), Some(&json!(1)));
        assert_eq!(select(&v, "$"), Some(&v));
    }

    #[test]
    fn a_missing_step_is_none_and_a_bad_path_is_invalid() {
        let v = json!({"a": [1]});
        assert_eq!(select(&v, "$.a[5]"), None);
        assert_eq!(select(&v, "$.a[-5]"), None);
        assert_eq!(select(&v, "$.nope.deeper"), None);
        assert!(!is_valid("a.b"));
        assert!(!is_valid("$.a["));
        assert!(!is_valid("$..a"));
        assert!(is_valid("$.a[0]['b']"));
    }
}
