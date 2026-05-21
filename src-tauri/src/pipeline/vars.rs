//! Pipeline run variables — per-run typed key/value store with `${var}`
//! string interpolation and a small declarative transform chain language.
//!
//! Lifecycle: a single `RunContext` is created at run start (empty), shared
//! across every step of the run via `Arc<Mutex<…>>`, and mutated by the
//! orchestrator after each step to honor that step's `CaptureSpec`. Variables
//! are visible to every later step (and inside if/elif/else conditions) of
//! the same run; they do NOT leak across runs.
//!
//! Concurrency: parallel stages serialize var writes through the Mutex. Two
//! parallel steps both writing the same `var` produce a non-deterministic
//! winner — by design we don't fight that, plugins should keep captures in
//! sequential stages.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Typed values
// ---------------------------------------------------------------------------

/// A pipeline variable's value. Untagged JSON so persisted runs round-trip
/// cleanly: `null`, booleans, numbers, strings, and lists of values are all
/// representable as plain JSON without a wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum VarValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<VarValue>),
}

impl Default for VarValue {
    fn default() -> Self { Self::Null }
}

impl VarValue {
    /// Render as a flat string suitable for substitution into a shell command
    /// or a condition operand. Lists join with newlines (matches the inverse
    /// of `Transform::Lines`); null yields the empty string so missing vars
    /// don't produce literal "null".
    pub fn as_string(&self) -> String {
        match self {
            VarValue::Null      => String::new(),
            VarValue::Bool(b)   => if *b { "true".into() } else { "false".into() },
            VarValue::Number(n) => {
                // Integers serialize without a trailing ".0" — most users will
                // do `${exit_code}` and expect "0" not "0.0".
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            VarValue::String(s) => s.clone(),
            VarValue::List(xs)  => xs.iter().map(|x| x.as_string())
                .collect::<Vec<_>>().join("\n"),
        }
    }

    /// Standard truthiness for conditions:
    /// - Null, Bool(false), Number(0), empty String, empty List → false
    /// - everything else → true
    /// Plus the string literals "false" / "0" / "" / "no" / "off" → false
    /// so a captured `success` flag from a shell `echo` works intuitively.
    pub fn truthy(&self) -> bool {
        match self {
            VarValue::Null      => false,
            VarValue::Bool(b)   => *b,
            VarValue::Number(n) => *n != 0.0 && !n.is_nan(),
            VarValue::String(s) => {
                let t = s.trim();
                !t.is_empty()
                    && !t.eq_ignore_ascii_case("false")
                    && !t.eq_ignore_ascii_case("no")
                    && !t.eq_ignore_ascii_case("off")
                    && t != "0"
            }
            VarValue::List(xs)  => !xs.is_empty(),
        }
    }

    /// Try to coerce to a number (for `gt`/`lt` comparisons).
    pub fn as_number(&self) -> Option<f64> {
        match self {
            VarValue::Number(n) => Some(*n),
            VarValue::Bool(b)   => Some(if *b { 1.0 } else { 0.0 }),
            VarValue::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn from_json(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null       => VarValue::Null,
            serde_json::Value::Bool(b)    => VarValue::Bool(*b),
            serde_json::Value::Number(n)  => VarValue::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s)  => VarValue::String(s.clone()),
            serde_json::Value::Array(xs)  => VarValue::List(xs.iter().map(Self::from_json).collect()),
            serde_json::Value::Object(_)  => VarValue::String(v.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Run context — shared mutable var store
// ---------------------------------------------------------------------------

/// Per-run variable store. Wrapped in an `Arc<Mutex<…>>` by the orchestrator
/// so every step of a run gets a coherent view. Reads inside an `${…}` resolve
/// hold the mutex briefly; writes happen once per step at capture time.
#[derive(Default, Debug)]
pub struct RunContext {
    pub vars: HashMap<String, VarValue>,
}

impl RunContext {
    pub fn new() -> Self { Self::default() }
    pub fn get(&self, name: &str) -> Option<&VarValue> { self.vars.get(name) }
    pub fn set(&mut self, name: impl Into<String>, value: VarValue) {
        self.vars.insert(name.into(), value);
    }
}

// ---------------------------------------------------------------------------
// String interpolation: ${var} and ${var:-default}
// ---------------------------------------------------------------------------

/// Replace every `${name}` (or `${name:-fallback}`) occurrence in `input`
/// with the value of `name` from `ctx`, rendered via `VarValue::as_string`.
/// Unknown variables resolve to the fallback when supplied or to "" otherwise.
/// `$$` is the escape for a literal `$` so commands like `echo $$PID` survive
/// substitution unchanged.
pub fn resolve_vars(input: &str, ctx: &RunContext) -> String {
    if !input.contains('$') { return input.to_string(); }
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'$' && i + 1 < bytes.len() {
            // $$ → literal $
            if bytes[i + 1] == b'$' {
                out.push('$');
                i += 2;
                continue;
            }
            // ${name} or ${name:-fallback}
            if bytes[i + 1] == b'{' {
                if let Some(end) = bytes[i + 2..].iter().position(|&c| c == b'}') {
                    let inside = &input[i + 2 .. i + 2 + end];
                    let (name, fallback) = match inside.find(":-") {
                        Some(p) => (&inside[..p], Some(&inside[p + 2 ..])),
                        None    => (inside, None),
                    };
                    let value = ctx.get(name)
                        .map(|v| v.as_string())
                        .unwrap_or_else(|| fallback.unwrap_or("").to_string());
                    out.push_str(&value);
                    i += 2 + end + 1;
                    continue;
                }
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

/// Recursively resolve `${var}` substitutions inside every string value of a
/// JSON tree. Used to expand variables in `lua_op.params` / built-in op specs
/// before dispatch. Non-string scalars and structure are preserved as-is.
pub fn resolve_vars_in_json(v: &serde_json::Value, ctx: &RunContext) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => serde_json::Value::String(resolve_vars(s, ctx)),
        serde_json::Value::Array(xs) => serde_json::Value::Array(
            xs.iter().map(|x| resolve_vars_in_json(x, ctx)).collect()
        ),
        serde_json::Value::Object(m) => {
            let mut out = serde_json::Map::with_capacity(m.len());
            for (k, x) in m { out.insert(k.clone(), resolve_vars_in_json(x, ctx)); }
            serde_json::Value::Object(out)
        }
        _ => v.clone(),
    }
}

// ---------------------------------------------------------------------------
// Capture spec — what to extract from a step's outcome and where to put it
// ---------------------------------------------------------------------------

/// What part of a step's result is captured into a variable.
///
/// `Stdout` joins all stdout lines back together (including stderr-prefixed
/// lines for shell steps). `ReturnValue` is meaningful for lua_op and built-in
/// steps; for shell steps it falls back to stdout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSource {
    #[default]
    Stdout,
    Stderr,
    ExitCode,
    Success,
    ReturnValue,
}

/// Capture specification on a `StepDef`. After the step runs, the
/// orchestrator extracts the chosen `source`, applies every `transforms`
/// entry in order, and stores the final value under `var` in the run's
/// `RunContext`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSpec {
    pub var: String,
    #[serde(default)]
    pub source: CaptureSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transforms: Vec<Transform>,
}

// ---------------------------------------------------------------------------
// Transform chain
// ---------------------------------------------------------------------------

/// A single declarative transformation applied to a captured value. Chains
/// run left-to-right; each entry receives the previous entry's output as its
/// input. Errors raised by a transform abort the chain (the variable is set
/// to `Null` and a warning is logged).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transform {
    /// Strip leading/trailing whitespace (string only; no-op otherwise).
    Trim,
    /// ASCII lowercase.
    Lower,
    /// ASCII uppercase.
    Upper,
    /// Split on `\n` → list of lines (drops trailing empty lines).
    Lines,
    /// Split on a literal separator → list of strings.
    Split { sep: String },
    /// Join a list with `sep` → string. No-op for non-lists.
    Join { sep: String },
    /// First element of a list (string for non-list input).
    First,
    /// Last element.
    Last,
    /// Element at `n` (negative = from end). Out-of-range → null.
    Nth { n: i64 },
    /// Regex match. With `group` the captured group is returned, otherwise
    /// the whole match. No match → empty string.
    Regex { pattern: String, #[serde(default)] group: Option<usize> },
    /// Walk a JSON value with a dotted path (`foo.bar.0.baz`). Input must be
    /// a JSON-valued string OR a string; the transform parses the string
    /// when needed. Missing keys → null.
    JsonGet { path: String },
    /// Parse a JSON string into a structured value (used so a later
    /// `json_get` doesn't have to re-parse).
    JsonParse,
    /// Coerce to bool via `VarValue::truthy`.
    ToBool,
    /// Coerce to number via `VarValue::as_number`. Failure → null.
    ToNumber,
    /// Replace null/empty results with a fallback string.
    Default { value: String },
    /// Match against a regex and return the boolean outcome.
    MatchesBool { pattern: String },
}

/// Apply the chain to a starting value. Returns the transformed value and a
/// list of human-readable trace lines (one per applied step) used by the
/// orchestrator's debug log so users can diagnose chains that surprise them.
pub fn apply_transforms(start: VarValue, ts: &[Transform]) -> (VarValue, Vec<String>) {
    let mut value = start;
    let mut trace = Vec::with_capacity(ts.len());
    for t in ts {
        let before = value.clone();
        match apply_one(&mut value, t) {
            Ok(_)  => trace.push(format!("  · {} → {}",
                describe(t), preview(&value))),
            Err(e) => {
                trace.push(format!("  · {} FAILED: {e}", describe(t)));
                value = before; // keep prior value on failure (best-effort)
            }
        }
    }
    (value, trace)
}

fn describe(t: &Transform) -> String {
    match t {
        Transform::Trim                => "trim".into(),
        Transform::Lower               => "lower".into(),
        Transform::Upper               => "upper".into(),
        Transform::Lines               => "lines".into(),
        Transform::Split { sep }       => format!("split({sep:?})"),
        Transform::Join  { sep }       => format!("join({sep:?})"),
        Transform::First               => "first".into(),
        Transform::Last                => "last".into(),
        Transform::Nth   { n }         => format!("nth({n})"),
        Transform::Regex { pattern, group } => format!("regex({pattern:?},g={group:?})"),
        Transform::JsonGet { path }    => format!("json_get({path})"),
        Transform::JsonParse           => "json_parse".into(),
        Transform::ToBool              => "to_bool".into(),
        Transform::ToNumber            => "to_number".into(),
        Transform::Default { value }   => format!("default({value:?})"),
        Transform::MatchesBool { pattern } => format!("matches_bool({pattern:?})"),
    }
}

fn preview(v: &VarValue) -> String {
    let s = v.as_string();
    if s.len() > 60 { format!("{:?}…", &s[..60]) } else { format!("{s:?}") }
}

fn apply_one(v: &mut VarValue, t: &Transform) -> std::result::Result<(), String> {
    match t {
        Transform::Trim => {
            if let VarValue::String(s) = v { *s = s.trim().to_string(); }
        }
        Transform::Lower => {
            if let VarValue::String(s) = v { *s = s.to_lowercase(); }
        }
        Transform::Upper => {
            if let VarValue::String(s) = v { *s = s.to_uppercase(); }
        }
        Transform::Lines => {
            let s = match v {
                VarValue::String(s) => s.clone(),
                _ => v.as_string(),
            };
            let mut xs: Vec<VarValue> = s.split('\n').map(|l|
                VarValue::String(l.trim_end_matches('\r').to_string())
            ).collect();
            // Drop trailing empty lines (common when stdout ends in \n).
            while xs.last().map(|x| matches!(x, VarValue::String(s) if s.is_empty())).unwrap_or(false) {
                xs.pop();
            }
            *v = VarValue::List(xs);
        }
        Transform::Split { sep } => {
            let s = v.as_string();
            *v = VarValue::List(s.split(sep.as_str())
                .map(|x| VarValue::String(x.to_string())).collect());
        }
        Transform::Join { sep } => {
            if let VarValue::List(xs) = v {
                let s = xs.iter().map(|x| x.as_string())
                    .collect::<Vec<_>>().join(sep);
                *v = VarValue::String(s);
            }
        }
        Transform::First => {
            *v = match v {
                VarValue::List(xs) => xs.first().cloned().unwrap_or(VarValue::Null),
                VarValue::String(s) => s.chars().next()
                    .map(|c| VarValue::String(c.to_string()))
                    .unwrap_or(VarValue::Null),
                _ => v.clone(),
            };
        }
        Transform::Last => {
            *v = match v {
                VarValue::List(xs) => xs.last().cloned().unwrap_or(VarValue::Null),
                VarValue::String(s) => s.chars().last()
                    .map(|c| VarValue::String(c.to_string()))
                    .unwrap_or(VarValue::Null),
                _ => v.clone(),
            };
        }
        Transform::Nth { n } => {
            let xs = match v {
                VarValue::List(xs) => xs.clone(),
                _ => return Err("nth requires a list (use lines/split first)".into()),
            };
            let idx: usize = if *n < 0 {
                let abs = (-*n) as usize;
                if abs > xs.len() { return Err("nth out of range".into()); }
                xs.len() - abs
            } else {
                *n as usize
            };
            *v = xs.get(idx).cloned().unwrap_or(VarValue::Null);
        }
        Transform::Regex { pattern, group } => {
            let re = regex::Regex::new(pattern)
                .map_err(|e| format!("invalid regex: {e}"))?;
            let s = v.as_string();
            *v = match re.captures(&s) {
                Some(caps) => {
                    let g = group.unwrap_or(0);
                    match caps.get(g) {
                        Some(m) => VarValue::String(m.as_str().to_string()),
                        None    => VarValue::Null,
                    }
                }
                None => VarValue::Null,
            };
        }
        Transform::JsonParse => {
            let s = v.as_string();
            let parsed: serde_json::Value = serde_json::from_str(&s)
                .map_err(|e| format!("json_parse: {e}"))?;
            *v = VarValue::from_json(&parsed);
        }
        Transform::JsonGet { path } => {
            let json = match v {
                VarValue::String(s) => serde_json::from_str::<serde_json::Value>(s)
                    .map_err(|e| format!("json_get: not valid JSON ({e})"))?,
                _ => serde_json::to_value(&*v).unwrap_or(serde_json::Value::Null),
            };
            let mut cur = &json;
            for seg in path.split('.') {
                if seg.is_empty() { continue; }
                cur = if let Ok(idx) = seg.parse::<usize>() {
                    cur.get(idx).unwrap_or(&serde_json::Value::Null)
                } else {
                    cur.get(seg).unwrap_or(&serde_json::Value::Null)
                };
            }
            *v = VarValue::from_json(cur);
        }
        Transform::ToBool   => *v = VarValue::Bool(v.truthy()),
        Transform::ToNumber => *v = match v.as_number() {
            Some(n) => VarValue::Number(n),
            None    => VarValue::Null,
        },
        Transform::Default { value } => {
            let empty = match v {
                VarValue::Null      => true,
                VarValue::String(s) => s.is_empty(),
                VarValue::List(xs)  => xs.is_empty(),
                _ => false,
            };
            if empty { *v = VarValue::String(value.clone()); }
        }
        Transform::MatchesBool { pattern } => {
            let re = regex::Regex::new(pattern)
                .map_err(|e| format!("invalid regex: {e}"))?;
            let s = v.as_string();
            *v = VarValue::Bool(re.is_match(&s));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(pairs: &[(&str, VarValue)]) -> RunContext {
        let mut c = RunContext::new();
        for (k, v) in pairs { c.set(*k, v.clone()); }
        c
    }

    #[test]
    fn resolve_simple() {
        let c = ctx_with(&[("name", VarValue::String("arbor".into()))]);
        assert_eq!(resolve_vars("hello ${name}!", &c), "hello arbor!");
    }

    #[test]
    fn resolve_fallback() {
        let c = ctx_with(&[]);
        assert_eq!(resolve_vars("${missing:-default}", &c), "default");
    }

    #[test]
    fn resolve_dollar_escape() {
        let c = ctx_with(&[]);
        assert_eq!(resolve_vars("price=$$5", &c), "price=$5");
    }

    #[test]
    fn truthiness() {
        assert!(!VarValue::Null.truthy());
        assert!(!VarValue::Bool(false).truthy());
        assert!(!VarValue::String("".into()).truthy());
        assert!(!VarValue::String("false".into()).truthy());
        assert!(!VarValue::Number(0.0).truthy());
        assert!(VarValue::String("yes".into()).truthy());
        assert!(VarValue::Number(1.0).truthy());
    }

    #[test]
    fn chain_lines_first() {
        let (v, _) = apply_transforms(
            VarValue::String("first\nsecond\nthird\n".into()),
            &[Transform::Lines, Transform::First],
        );
        assert_eq!(v, VarValue::String("first".into()));
    }

    #[test]
    fn chain_regex_group() {
        let (v, _) = apply_transforms(
            VarValue::String("Version: v1.2.3-beta".into()),
            &[Transform::Regex { pattern: r"v(\d+\.\d+\.\d+)".into(), group: Some(1) }],
        );
        assert_eq!(v, VarValue::String("1.2.3".into()));
    }

    #[test]
    fn chain_json_get() {
        let (v, _) = apply_transforms(
            VarValue::String(r#"{"a":{"b":42}}"#.into()),
            &[Transform::JsonGet { path: "a.b".into() }],
        );
        assert_eq!(v, VarValue::Number(42.0));
    }
}
