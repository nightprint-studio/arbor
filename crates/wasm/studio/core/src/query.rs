//! `core::query` — the format-agnostic JSONPath query engine lifted
//! from the per-format backends (JSON/TOML/YAML/RON share the same
//! `normalise` shorthands + `serde_json_path` run loop; see blueprint
//! §2.3).
//!
//! Two entry points:
//!
//! * [`normalise`] — smooth common user inputs into valid JSONPath
//!   (`foo` → `$..foo`, `.foo` → `$.foo`, leading-`$` passthrough,
//!   bracket form). Exposed for direct testing + reuse.
//! * [`run`] — normalise, parse, run against a `serde_json::Value`,
//!   dedup by path, cap at `max_hits` (callers pass `500`). Returns
//!   `QueryLoc { path, value }` pairs; the caller maps each to its own
//!   `QueryHit` (kind / preview / variant_tag) and, where needed,
//!   resolves the JSONPath path back to a live-AST path (RON strips
//!   synthetic `$items`; .properties flattens dotted keys — those bits
//!   stay in the format crate).

use serde_json::Value;
use serde_json_path::{JsonPath, PathElement};

/// A single located result: the path segments and the matched value
/// (owned clone, so callers can hold it across other registry calls).
#[derive(Debug, Clone)]
pub struct QueryLoc {
    pub path: Vec<String>,
    pub value: Value,
}

/// Query failure — a malformed expression that `serde_json_path` could
/// not parse. Carries the human-readable message the FE shows verbatim.
#[derive(Debug, Clone)]
pub struct QueryError(pub String);

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for QueryError {}

/// Smooth common user inputs into valid JSONPath expressions:
///
///   `foo`              → `$..foo`     (recursive descent on a name)
///   `.foo` / `[0]`     → `$.foo` / `$[0]`
///   `users[?@...]`     → `$.users[?@...]`
///   `$...` / `$`       → passthrough
///
/// Anything not recognised is passed through unchanged so the engine
/// produces an honest parse error rather than silently mangling input.
pub fn normalise(expr: &str) -> String {
    let s = expr.trim();
    if s.is_empty() || s == "$" {
        return s.to_string();
    }
    if s.starts_with('$') {
        return s.to_string();
    }
    if s.starts_with('.') || s.starts_with('[') {
        return format!("${}", s);
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return format!("$..{}", s);
    }
    if s.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') {
        return format!("$.{}", s);
    }
    s.to_string()
}

/// Run `expr` (after [`normalise`]) against `root`, dedup by path, cap at
/// `max_hits`. An empty/`$`-normalising expression yields no hits (not an
/// error). Dedup is a no-op for formats whose paths are already unique;
/// it matters for projections (e.g. RON) where synthetic-segment
/// stripping can collapse distinct located paths onto the same target.
pub fn run(root: &Value, expr: &str, max_hits: usize) -> Result<Vec<QueryLoc>, QueryError> {
    let normalised = normalise(expr);
    if normalised.is_empty() {
        return Ok(Vec::new());
    }
    let path =
        JsonPath::parse(&normalised).map_err(|e| QueryError(format!("Query parse error: {e}")))?;
    let located = path.query_located(root);
    let mut out = Vec::with_capacity(max_hits.min(located.len()));
    let mut seen = std::collections::HashSet::<String>::new();
    for ln in located.iter() {
        if out.len() >= max_hits {
            break;
        }
        let segs: Vec<String> = ln
            .location()
            .iter()
            .map(|el| match el {
                PathElement::Name(s) => s.to_string(),
                PathElement::Index(i) => i.to_string(),
            })
            .collect();
        if !seen.insert(segs.join("\u{0}")) {
            continue;
        }
        out.push(QueryLoc { path: segs, value: ln.node().clone() });
    }
    Ok(out)
}

// ── Tests (blueprint §6 query) ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalise_bare_name_is_recursive() {
        assert_eq!(normalise("foo"), "$..foo");
        assert_eq!(normalise("foo-bar_2"), "$..foo-bar_2");
    }

    #[test]
    fn normalise_leading_dot_and_bracket() {
        assert_eq!(normalise(".foo"), "$.foo");
        assert_eq!(normalise("[0]"), "$[0]");
    }

    #[test]
    fn normalise_dollar_passthrough() {
        assert_eq!(normalise("$.a.b"), "$.a.b");
        assert_eq!(normalise("$"), "$");
        assert_eq!(normalise(""), "");
    }

    #[test]
    fn normalise_complex_name_gets_dot_prefix() {
        // Contains `[` mid-string → not all-alnum, starts with a letter.
        assert_eq!(normalise("users[0]"), "$.users[0]");
    }

    #[test]
    fn run_returns_correct_path_segments() {
        let root = json!({ "a": { "name": "x" }, "b": { "name": "y" } });
        let mut hits = run(&root, "$..name", 500).unwrap();
        hits.sort_by(|l, r| l.path.cmp(&r.path));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, vec!["a".to_string(), "name".to_string()]);
        assert_eq!(hits[0].value, json!("x"));
        assert_eq!(hits[1].path, vec!["b".to_string(), "name".to_string()]);
    }

    #[test]
    fn run_empty_result() {
        let root = json!({ "a": 1 });
        let hits = run(&root, "$..nope", 500).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn run_cap_at_max_hits() {
        let root = json!({ "items": [1, 2, 3, 4, 5] });
        let hits = run(&root, "$.items[*]", 3).unwrap();
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn run_dedup_identical_paths() {
        // `$..a` and a directly-addressed `$.a` would both hit $.a; but a
        // single recursive query can also surface the same node twice via
        // overlapping selectors. Construct one: `$[*]` over an array hits
        // each element once — dedup is a no-op here, asserting it doesn't
        // drop distinct paths.
        let root = json!([10, 20, 30]);
        let hits = run(&root, "$[*]", 500).unwrap();
        assert_eq!(hits.len(), 3);
        let paths: Vec<String> = hits.iter().map(|h| h.path.join("/")).collect();
        assert_eq!(paths, vec!["0", "1", "2"]);
    }

    #[test]
    fn run_error_on_malformed_expr() {
        let root = json!({ "a": 1 });
        // A leading `$` keeps it from being shorthand-wrapped, and the
        // unbalanced bracket is a genuine JSONPath syntax error.
        let err = run(&root, "$.a[", 500).unwrap_err();
        assert!(err.0.contains("Query parse error"));
    }
}
