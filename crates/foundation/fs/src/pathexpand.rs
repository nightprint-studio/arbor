//! Address-bar path expansion: a leading `~` and `%VAR%` / `$VAR` / `${VAR}`
//! references, so the explorer accepts shell-style shortcuts on every platform.

use std::path::PathBuf;

/// Expand environment-variable references and a leading `~` in a user-typed
/// path. Both `%VAR%` and `$VAR` / `${VAR}` syntaxes are honoured on every
/// platform; the virtual names `appdata` / `localappdata` / `home` resolve to
/// the right OS folder everywhere, so `%appdata%` works on macOS and Linux too.
/// Unknown variables are left intact.
pub fn expand_path(path: &str) -> String {
    expand_path_str(path)
}

/// Resolve one variable name to a path value. Recognises a few cross-platform
/// virtual names before falling back to a real environment variable (exact,
/// then upper-cased so `%appdata%` matches `APPDATA`).
fn resolve_path_var(name: &str) -> Option<String> {
    let to_s = |p: PathBuf| p.to_string_lossy().into_owned();
    match name.to_ascii_lowercase().as_str() {
        "appdata"              => std::env::var("APPDATA").ok().or_else(|| dirs::config_dir().map(to_s)),
        "localappdata"         => std::env::var("LOCALAPPDATA").ok().or_else(|| dirs::data_local_dir().map(to_s)),
        "home" | "userprofile" => dirs::home_dir().map(to_s),
        _ => std::env::var(name).ok().or_else(|| std::env::var(name.to_ascii_uppercase()).ok()),
    }
}

fn expand_path_str(input: &str) -> String {
    let mut s = input.trim().to_string();
    if s.is_empty() {
        return s;
    }
    // Leading ~ → home directory (bare `~`, or `~/…` / `~\…`).
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            s = format!("{}{}", home.to_string_lossy(), &s[1..]);
        }
    }
    s = expand_percent_vars(&s);
    s = expand_dollar_vars(&s);
    s
}

/// Expand `%VAR%` tokens. A `%` with no closing `%`, or an unknown variable, is
/// left verbatim (so a stray `%` in a real filename survives).
fn expand_percent_vars(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == '%') {
                let end = i + 1 + rel;
                let name: String = chars[i + 1..end].iter().collect();
                if let Some(val) = resolve_path_var(&name) {
                    out.push_str(&val);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Expand `$VAR` and `${VAR}` tokens. Variable names are `[A-Za-z0-9_]`.
fn expand_dollar_vars(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            let braced = chars[i + 1] == '{';
            let start = if braced { i + 2 } else { i + 1 };
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || chars[end] == '_')
                && !(braced && chars[end] == '}')
            {
                end += 1;
            }
            let name: String = chars[start..end].iter().collect();
            let close_ok = !braced || (end < chars.len() && chars[end] == '}');
            if !name.is_empty() && close_ok {
                if let Some(val) = resolve_path_var(&name) {
                    out.push_str(&val);
                    i = if braced { end + 1 } else { end };
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
