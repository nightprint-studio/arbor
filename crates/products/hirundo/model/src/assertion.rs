//! Declarative assertions: the 90% of tests that are one line each.
//!
//! ```text
//! -- comments and blank lines are ignored
//! status == 200
//! time < 500ms
//! header.content-type contains json
//! body.content[*].stato == "APERTO"      every element must satisfy it
//! len(body.content) > 0
//! body.id exists
//! ```
//!
//! Subjects: `status`, `time` (ms), `size` (bytes), `header.<name>`, `body` (the text), `body.<path>` (JSON path:
//! `a.b[0].c`, `[*]` for "every element"), `len(<subject>)`. Operators: `==` `!=` `<` `<=` `>` `>=` `contains`
//! `startswith` `endswith` and the postfix `exists` / `not exists` / `empty`. Values: a number (`500`, `500ms`),
//! a JSON string or literal, or a bare word (taken as text).
//!
//! Anything past a line that is exactly `--- lua` belongs to the Lua runner and is ignored here
//! ([`split_lua`]).

use serde_json::Value;

/// What the assertions look at: a response, flattened.
pub struct ResponseView<'a> {
    pub status: u16,
    pub time_ms: u64,
    pub size_bytes: usize,
    pub headers: &'a [(String, String)],
    pub body: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The line as written.
    pub line: String,
    pub passed: bool,
    /// Why it failed (or an error in the line itself); empty when it passed.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    StartsWith,
    EndsWith,
    Exists,
    NotExists,
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
enum Subject {
    Status,
    Time,
    Size,
    Header(String),
    Body,
    Path(Vec<Seg>),
    Len(Box<Subject>),
}

#[derive(Debug, Clone, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
    All,
}

#[derive(Debug, Clone, PartialEq)]
struct Assertion {
    subject: Subject,
    op: Op,
    expected: Value,
}

/// The assertion part of a test script and the Lua part, split at the first `--- lua` line.
pub fn split_lua(script: &str) -> (String, Option<String>) {
    let mut declarative = Vec::new();
    let mut lines = script.lines();
    for line in lines.by_ref() {
        if line.trim() == "--- lua" {
            let lua: Vec<&str> = lines.collect();
            return (declarative.join("\n"), Some(lua.join("\n")));
        }
        declarative.push(line);
    }
    (declarative.join("\n"), None)
}

/// Run every assertion line of `script` against the response.
pub fn run(script: &str, response: &ResponseView) -> Vec<Outcome> {
    let (declarative, _) = split_lua(script);
    declarative
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("--"))
        .map(|line| match parse_line(line) {
            Err(e) => Outcome { line: line.to_string(), passed: false, detail: e },
            Ok(a) => match evaluate(&a, response) {
                Ok(()) => Outcome { line: line.to_string(), passed: true, detail: String::new() },
                Err(why) => Outcome { line: line.to_string(), passed: false, detail: why },
            },
        })
        .collect()
}

// ── parsing ───────────────────────────────────────────────────────────────────────────────────────

fn parse_line(line: &str) -> Result<Assertion, String> {
    for (word, op) in [("not exists", Op::NotExists), ("exists", Op::Exists), ("empty", Op::Empty)] {
        if let Some(subject) = line.strip_suffix(word) {
            if subject.ends_with(' ') {
                return Ok(Assertion { subject: parse_subject(subject.trim())?, op, expected: Value::Null });
            }
        }
    }
    // Longest operators first so `<=` is not read as `<`.
    const OPS: [(&str, Op); 9] = [
        ("==", Op::Eq),
        ("!=", Op::Ne),
        ("<=", Op::Le),
        (">=", Op::Ge),
        ("<", Op::Lt),
        (">", Op::Gt),
        (" contains ", Op::Contains),
        (" startswith ", Op::StartsWith),
        (" endswith ", Op::EndsWith),
    ];
    let hit = OPS.iter().filter_map(|(sym, op)| line.find(sym).map(|at| (at, *sym, *op))).min_by_key(|(at, sym, _)| (*at, usize::MAX - sym.len()));
    let Some((at, sym, op)) = hit else {
        return Err("not an assertion (expected `subject operator value`)".into());
    };
    let subject = parse_subject(line[..at].trim())?;
    let expected = parse_value(line[at + sym.len()..].trim());
    Ok(Assertion { subject, op, expected })
}

fn parse_subject(text: &str) -> Result<Subject, String> {
    if let Some(inner) = text.strip_prefix("len(").and_then(|t| t.strip_suffix(')')) {
        return Ok(Subject::Len(Box::new(parse_subject(inner.trim())?)));
    }
    match text {
        "status" => return Ok(Subject::Status),
        "time" => return Ok(Subject::Time),
        "size" => return Ok(Subject::Size),
        "body" => return Ok(Subject::Body),
        _ => {}
    }
    if let Some(name) = text.strip_prefix("header.") {
        return Ok(Subject::Header(name.to_lowercase()));
    }
    if let Some(path) = text.strip_prefix("body.").or_else(|| text.strip_prefix("body[")) {
        let path = if text.starts_with("body[") { format!("[{path}") } else { path.to_string() };
        return Ok(Subject::Path(parse_path(&path)?));
    }
    Err(format!("unknown subject '{text}' (status, time, size, header.<name>, body, body.<path>, len(...))"))
}

fn parse_path(path: &str) -> Result<Vec<Seg>, String> {
    let mut segs = Vec::new();
    for part in path.split('.') {
        let (key, mut rest) = match part.find('[') {
            Some(i) => (&part[..i], &part[i..]),
            None => (part, ""),
        };
        if !key.is_empty() {
            segs.push(Seg::Key(key.to_string()));
        }
        while let Some(open) = rest.strip_prefix('[') {
            let Some(close) = open.find(']') else { return Err(format!("unclosed [ in '{part}'")) };
            let inside = &open[..close];
            segs.push(if inside == "*" { Seg::All } else { Seg::Index(inside.parse().map_err(|_| format!("'{inside}' is not an index"))?) });
            rest = &open[close + 1..];
        }
    }
    Ok(segs)
}

fn parse_value(text: &str) -> Value {
    let text = text.trim();
    if let Some(ms) = text.strip_suffix("ms") {
        if let Ok(n) = ms.trim().parse::<f64>() {
            return serde_json::json!(n);
        }
    }
    serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

// ── evaluation ────────────────────────────────────────────────────────────────────────────────────

/// What a subject points at: one value, or one per element for `[*]`. `None` is "not there".
fn extract(subject: &Subject, r: &ResponseView) -> Vec<Option<Value>> {
    match subject {
        Subject::Status => vec![Some(Value::from(r.status))],
        Subject::Time => vec![Some(Value::from(r.time_ms))],
        Subject::Size => vec![Some(Value::from(r.size_bytes as u64))],
        Subject::Body => vec![Some(Value::String(r.body.to_string()))],
        Subject::Header(name) => vec![r.headers.iter().find(|(k, _)| k.to_lowercase() == *name).map(|(_, v)| Value::String(v.clone()))],
        Subject::Path(segs) => match serde_json::from_str::<Value>(r.body) {
            Ok(root) => walk(&root, segs),
            Err(_) => vec![None],
        },
        Subject::Len(inner) => extract(inner, r)
            .into_iter()
            .map(|v| {
                v.and_then(|v| match v {
                    Value::Array(a) => Some(a.len()),
                    Value::Object(o) => Some(o.len()),
                    Value::String(s) => Some(s.chars().count()),
                    _ => None,
                })
                .map(|n| Value::from(n as u64))
            })
            .collect(),
    }
}

fn walk(value: &Value, segs: &[Seg]) -> Vec<Option<Value>> {
    let Some((first, rest)) = segs.split_first() else { return vec![Some(value.clone())] };
    match (first, value) {
        (Seg::Key(k), Value::Object(o)) => o.get(k).map_or(vec![None], |v| walk(v, rest)),
        (Seg::Index(i), Value::Array(a)) => a.get(*i).map_or(vec![None], |v| walk(v, rest)),
        (Seg::All, Value::Array(a)) if a.is_empty() => Vec::new(),
        (Seg::All, Value::Array(a)) => a.iter().flat_map(|v| walk(v, rest)).collect(),
        _ => vec![None],
    }
}

fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn loosely_equal(actual: &Value, expected: &Value) -> bool {
    match (as_number(actual), as_number(expected)) {
        (Some(a), Some(b)) => a == b,
        _ => as_text(actual) == as_text(expected),
    }
}

fn holds(op: Op, actual: Option<&Value>, expected: &Value) -> bool {
    match op {
        Op::Exists => actual.is_some_and(|v| !v.is_null()),
        Op::NotExists => actual.map_or(true, |v| v.is_null()),
        Op::Empty => match actual {
            None | Some(Value::Null) => true,
            Some(Value::String(s)) => s.is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            Some(Value::Object(o)) => o.is_empty(),
            Some(_) => false,
        },
        _ => {
            let Some(actual) = actual else { return false };
            match op {
                Op::Eq => loosely_equal(actual, expected),
                Op::Ne => !loosely_equal(actual, expected),
                Op::Lt | Op::Le | Op::Gt | Op::Ge => match (as_number(actual), as_number(expected)) {
                    (Some(a), Some(b)) => match op {
                        Op::Lt => a < b,
                        Op::Le => a <= b,
                        Op::Gt => a > b,
                        _ => a >= b,
                    },
                    _ => false,
                },
                Op::Contains => match actual {
                    Value::Array(items) => items.iter().any(|i| loosely_equal(i, expected)),
                    Value::Object(o) => o.contains_key(&as_text(expected)),
                    other => as_text(other).contains(&as_text(expected)),
                },
                Op::StartsWith => as_text(actual).starts_with(&as_text(expected)),
                Op::EndsWith => as_text(actual).ends_with(&as_text(expected)),
                _ => false,
            }
        }
    }
}

fn evaluate(a: &Assertion, r: &ResponseView) -> Result<(), String> {
    let found = extract(&a.subject, r);
    for actual in &found {
        if !holds(a.op, actual.as_ref(), &a.expected) {
            return Err(match actual {
                Some(v) => format!("got {}", short(v)),
                None => "the value is not in the response".to_string(),
            });
        }
    }
    Ok(())
}

fn short(v: &Value) -> String {
    let text = v.to_string();
    if text.chars().count() > 80 {
        format!("{}…", text.chars().take(80).collect::<String>())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"content":[{"id":42,"stato":"APERTO"},{"id":43,"stato":"APERTO"}],"totalElements":2}"#;

    fn view<'a>(headers: &'a [(String, String)]) -> ResponseView<'a> {
        ResponseView { status: 200, time_ms: 184, size_bytes: 2300, headers, body: BODY }
    }

    fn passes(script: &str) -> Vec<bool> {
        let headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        run(script, &view(&headers)).into_iter().map(|o| o.passed).collect()
    }

    #[test]
    fn the_mock_examples_pass() {
        assert_eq!(passes("status == 200\nbody.content[*].stato == \"APERTO\"\ntime < 500ms"), [true, true, true]);
    }

    #[test]
    fn comparisons_and_headers() {
        assert_eq!(passes("status != 404\nstatus >= 200\nstatus < 300\nsize > 1000\nheader.content-type contains json"), [true; 5]);
        assert_eq!(passes("status == 201\ntime > 1000"), [false, false]);
    }

    #[test]
    fn paths_lengths_and_existence() {
        assert_eq!(passes("len(body.content) == 2\nbody.totalElements == 2\nbody.content[0].id == 42\nbody.content[1].id exists"), [true; 4]);
        assert_eq!(passes("body.missing exists\nbody.missing not exists\nbody.content empty"), [false, true, false]);
    }

    #[test]
    fn every_element_must_satisfy_a_wildcard() {
        assert_eq!(passes("body.content[*].id > 42"), [false]);
        assert_eq!(passes("body.content[*].id >= 42"), [true]);
    }

    #[test]
    fn a_failure_says_what_was_there() {
        let headers = vec![];
        let out = run("status == 500", &view(&headers));
        assert!(!out[0].passed);
        assert_eq!(out[0].detail, "got 200");
    }

    #[test]
    fn a_line_that_is_not_an_assertion_fails_with_a_reason() {
        let headers = vec![];
        let out = run("hello world\nnonsense == 1", &view(&headers));
        assert!(out.iter().all(|o| !o.passed));
        assert!(out[0].detail.contains("not an assertion"));
        assert!(out[1].detail.contains("unknown subject"));
    }

    #[test]
    fn comments_blank_lines_and_lua_are_not_assertions() {
        let script = "-- smoke\n\nstatus == 200\n--- lua\nhirundo.expect(true)";
        assert_eq!(passes(script), [true]);
        let (declarative, lua) = split_lua(script);
        assert!(declarative.contains("status == 200"));
        assert_eq!(lua.as_deref(), Some("hirundo.expect(true)"));
        assert_eq!(split_lua("status == 200").1, None);
    }

    #[test]
    fn strings_compare_as_text_and_numbers_as_numbers() {
        assert_eq!(passes("body.content[0].stato startswith APE\nbody.content[0].stato endswith TO\nbody.content[0].id == \"42\""), [true; 3]);
    }

    #[test]
    fn a_body_that_is_not_json_has_no_paths() {
        let headers = vec![];
        let v = ResponseView { status: 200, time_ms: 1, size_bytes: 5, headers: &headers, body: "plain" };
        let out = run("body.a exists\nbody contains plain", &v);
        assert_eq!(out.iter().map(|o| o.passed).collect::<Vec<_>>(), [false, true]);
    }
}
