//! Built-in pipeline operations — small, cross-platform, side-effect-free
//! steps the runtime executes directly without spawning a shell or
//! dispatching to Lua. Their main role is producing a `ReturnValue` that a
//! `CaptureSpec` then stores into the run's variable bag, so later steps
//! and `if/elif/else` conditions can branch on file presence, env vars,
//! parsed JSON fields, etc.
//!
//! Anything more than basic file/env/JSON inspection should still go
//! through `lua_op` — this set is intentionally small and stable.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use super::vars::{RunContext, VarValue, resolve_vars};

/// Each variant is its own kind/struct so plugins serialize a tagged JSON
/// value (`{ "kind": "file_exists", "path": "..." }`). String fields are
/// resolved against the run context (`${var}`) before the op runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuiltinSpec {
    /// True when `path` exists (file OR directory). Path is resolved
    /// relative to the step's `cwd`.
    FileExists  { path: String },
    /// Read a UTF-8 file's contents as a string.
    FileRead    { path: String, #[serde(default)] max_bytes: Option<u64> },
    /// Read an env var. Empty string when unset.
    Env         { name: String, #[serde(default)] default: Option<String> },
    /// Walk a JSON value (provided inline as a string) with a dotted path.
    JsonGet     { source: String, path: String },
    /// Join path segments using the host's separator.
    PathJoin    { parts: Vec<String> },
    /// Write a literal value (any JSON type) to the captured `var`. Useful
    /// to seed defaults or to force a known value at the start of a branch.
    SetVar      { value: serde_json::Value },
    /// Echo a message to the run log (returns the message as ReturnValue
    /// for capture). Resolves `${var}` so plugins can debug-print state.
    Echo        { message: String },
    /// True when `target` matches `pattern` (literal substring) or
    /// `regex` (when set, takes precedence).
    Match       { target: String, #[serde(default)] pattern: Option<String>,
                  #[serde(default)] regex: Option<String> },
}

/// Outcome of a built-in op execution. `value` becomes the step's
/// `ReturnValue` for capture; `lines` are appended to the step output for
/// the run log (one chunk, no live streaming).
pub struct BuiltinOutcome {
    pub value: VarValue,
    pub lines: Vec<String>,
    /// Mirrors a shell process exit: 0 = ok, non-zero = failed.
    pub exit_code: i32,
}

pub fn run_builtin(spec: &BuiltinSpec, cwd: &str, ctx: &RunContext) -> BuiltinOutcome {
    match spec {
        BuiltinSpec::FileExists { path } => {
            let p = resolve_path(path, cwd, ctx);
            let exists = p.exists();
            BuiltinOutcome {
                value: VarValue::Bool(exists),
                lines: vec![format!("file_exists({}) = {}", p.display(), exists)],
                exit_code: 0,
            }
        }
        BuiltinSpec::FileRead { path, max_bytes } => {
            let p = resolve_path(path, cwd, ctx);
            match std::fs::read_to_string(&p) {
                Ok(mut s) => {
                    if let Some(cap) = max_bytes {
                        if s.len() as u64 > *cap { s.truncate(*cap as usize); }
                    }
                    let preview_len = s.len().min(80);
                    let preview = &s[..preview_len];
                    BuiltinOutcome {
                        value: VarValue::String(s.clone()),
                        lines: vec![format!("file_read({}) → {} bytes ({}…)",
                            p.display(), s.len(), preview)],
                        exit_code: 0,
                    }
                }
                Err(e) => BuiltinOutcome {
                    value: VarValue::Null,
                    lines: vec![format!("⚠ file_read({}) failed: {e}", p.display())],
                    exit_code: 1,
                },
            }
        }
        BuiltinSpec::Env { name, default } => {
            let resolved = resolve_vars(name, ctx);
            match std::env::var(&resolved) {
                Ok(v) => BuiltinOutcome {
                    value: VarValue::String(v.clone()),
                    lines: vec![format!("env({resolved}) = {:?}", v)],
                    exit_code: 0,
                },
                Err(_) => {
                    let fb = default.as_deref().map(|d| resolve_vars(d, ctx))
                        .unwrap_or_default();
                    BuiltinOutcome {
                        value: if fb.is_empty() { VarValue::Null } else { VarValue::String(fb.clone()) },
                        lines: vec![format!("env({resolved}) unset → {:?}", fb)],
                        exit_code: 0,
                    }
                }
            }
        }
        BuiltinSpec::JsonGet { source, path } => {
            let s = resolve_vars(source, ctx);
            let p = resolve_vars(path, ctx);
            match serde_json::from_str::<serde_json::Value>(&s) {
                Ok(v) => {
                    let mut cur = &v;
                    for seg in p.split('.') {
                        if seg.is_empty() { continue; }
                        cur = if let Ok(idx) = seg.parse::<usize>() {
                            cur.get(idx).unwrap_or(&serde_json::Value::Null)
                        } else {
                            cur.get(seg).unwrap_or(&serde_json::Value::Null)
                        };
                    }
                    BuiltinOutcome {
                        value: VarValue::from_json(cur),
                        lines: vec![format!("json_get({p}) → {}", cur)],
                        exit_code: 0,
                    }
                }
                Err(e) => BuiltinOutcome {
                    value: VarValue::Null,
                    lines: vec![format!("⚠ json_get: invalid JSON ({e})")],
                    exit_code: 1,
                },
            }
        }
        BuiltinSpec::PathJoin { parts } => {
            let mut p = PathBuf::new();
            for part in parts { p.push(resolve_vars(part, ctx)); }
            let s = p.to_string_lossy().to_string();
            BuiltinOutcome {
                value: VarValue::String(s.clone()),
                lines: vec![format!("path_join → {}", s)],
                exit_code: 0,
            }
        }
        BuiltinSpec::SetVar { value } => {
            let resolved = super::vars::resolve_vars_in_json(value, ctx);
            BuiltinOutcome {
                value: VarValue::from_json(&resolved),
                lines: vec![format!("set_var = {}", resolved)],
                exit_code: 0,
            }
        }
        BuiltinSpec::Echo { message } => {
            let m = resolve_vars(message, ctx);
            BuiltinOutcome {
                value: VarValue::String(m.clone()),
                lines: vec![m],
                exit_code: 0,
            }
        }
        BuiltinSpec::Match { target, pattern, regex } => {
            let t = resolve_vars(target, ctx);
            let m = if let Some(rx) = regex {
                match regex::Regex::new(&resolve_vars(rx, ctx)) {
                    Ok(re) => re.is_match(&t),
                    Err(e) => return BuiltinOutcome {
                        value: VarValue::Bool(false),
                        lines: vec![format!("⚠ match: invalid regex ({e})")],
                        exit_code: 1,
                    },
                }
            } else if let Some(p) = pattern {
                let needle = resolve_vars(p, ctx);
                t.contains(needle.as_str())
            } else {
                false
            };
            BuiltinOutcome {
                value: VarValue::Bool(m),
                lines: vec![format!("match({t:?}) = {m}")],
                exit_code: 0,
            }
        }
    }
}

fn resolve_path(input: &str, cwd: &str, ctx: &RunContext) -> PathBuf {
    let resolved = resolve_vars(input, ctx);
    let p = PathBuf::from(&resolved);
    if p.is_absolute() { p } else { PathBuf::from(cwd).join(p) }
}

/// Short human-readable label for the run log (what the orchestrator prints
/// when entering the step). Matches the `lua_op {op}` style.
pub fn describe(spec: &BuiltinSpec) -> String {
    match spec {
        BuiltinSpec::FileExists { path }   => format!("builtin file_exists({path})"),
        BuiltinSpec::FileRead   { path, .. } => format!("builtin file_read({path})"),
        BuiltinSpec::Env        { name, .. } => format!("builtin env({name})"),
        BuiltinSpec::JsonGet    { path, .. } => format!("builtin json_get({path})"),
        BuiltinSpec::PathJoin   { .. }     => "builtin path_join".into(),
        BuiltinSpec::SetVar     { .. }     => "builtin set_var".into(),
        BuiltinSpec::Echo       { .. }     => "builtin echo".into(),
        BuiltinSpec::Match      { .. }     => "builtin match".into(),
    }
}
