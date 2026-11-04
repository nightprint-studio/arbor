//! Key/value rows — query parameters, headers, form fields, variables — and their **bulk-edit text**.
//!
//! The table and the text are two views of the same list, so the text form is also what the files hold:
//! one row per line, `key: value`, an optional ` # description` after the value, and `// ` in front of a row
//! that is switched off. Pasting a block of headers copied from a browser, or `a=1&b=2`, reads as rows too.

use serde::{Deserialize, Serialize};

/// The value that marks a variable whose real value lives in the keyring, never in a file.
pub const SECRET_MARK: &str = "@secret";

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl KeyValue {
    pub fn new(key: &str, value: &str) -> Self {
        Self { key: key.into(), value: value.into(), description: String::new(), enabled: true }
    }

    /// A variable whose value is kept in the keyring.
    pub fn is_secret(&self) -> bool {
        self.value.trim() == SECRET_MARK
    }
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// Percent-decoding for `a=hello%20world`; `+` is a space in a query. Invalid escapes stay as they are.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some() => {
                out.push(hex(b[i + 1]).unwrap_or(0) * 16 + hex(b[i + 2]).unwrap_or(0));
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encoding for a query or form component. A `%` that already starts an escape is kept, so a value the
/// user pre-encoded is not encoded twice.
pub fn percent_encode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    for (i, &c) in b.iter().enumerate() {
        let keep = c.is_ascii_alphanumeric()
            || matches!(c, b'-' | b'_' | b'.' | b'~')
            || (c == b'%' && i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some());
        if keep {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

/// Read rows out of text. One row per line; blank lines are skipped. A line is:
/// `key: value`, `key=value`, or — when it holds `&` and no `: ` — several `k=v` pairs of a query string.
pub fn parse_text(text: &str) -> Vec<KeyValue> {
    let mut rows = Vec::new();
    for raw in text.lines() {
        let mut line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut enabled = true;
        if let Some(rest) = line.strip_prefix("//") {
            enabled = false;
            line = rest.trim();
            if line.is_empty() {
                continue;
            }
        }
        if !line.contains(": ") && line.contains('&') && line.contains('=') {
            for pair in line.split('&').filter(|p| !p.is_empty()) {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                rows.push(KeyValue { key: percent_decode(k), value: percent_decode(v), description: String::new(), enabled });
            }
            continue;
        }
        // HTTP/2 pseudo-headers copied from DevTools start with ':' (`:authority: example.com`): the key's
        // own colon is the first one after it.
        let search_from = usize::from(line.starts_with(':'));
        let colon = line[search_from..].find(':').map(|i| i + search_from);
        let eq = line.find('=');
        let split_at = match (colon, eq) {
            (Some(c), Some(e)) => Some(c.min(e)),
            (Some(c), None) => Some(c),
            (None, Some(e)) => Some(e),
            (None, None) => None,
        };
        let (key, rest) = match split_at {
            Some(i) => (line[..i].trim(), line[i + 1..].trim()),
            None => (line, ""),
        };
        let (value, description) = match rest.split_once(" # ") {
            Some((v, d)) => (v.trim(), d.trim()),
            None => (rest, ""),
        };
        rows.push(KeyValue { key: key.into(), value: value.into(), description: description.into(), enabled });
    }
    rows
}

/// Rows as text, the inverse of [`parse_text`].
pub fn to_text(rows: &[KeyValue]) -> String {
    rows.iter()
        .map(|r| {
            let mut line = String::new();
            if !r.enabled {
                line.push_str("// ");
            }
            line.push_str(&r.key);
            line.push_str(": ");
            line.push_str(&r.value);
            if !r.description.is_empty() {
                line.push_str(" # ");
                line.push_str(&r.description);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The enabled rows as `(key, value)` pairs, in order.
pub fn enabled_pairs(rows: &[KeyValue]) -> Vec<(String, String)> {
    rows.iter().filter(|r| r.enabled && !r.key.trim().is_empty()).map(|r| (r.key.trim().to_string(), r.value.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(k: &str, v: &str) -> KeyValue {
        KeyValue::new(k, v)
    }

    #[test]
    fn lines_become_rows() {
        let rows = parse_text("Accept: application/json\nX-Trace: abc  # for support\n\n// X-Off: 1\nstato=APERTO");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], row("Accept", "application/json"));
        assert_eq!((rows[1].value.as_str(), rows[1].description.as_str()), ("abc", "for support"));
        assert!(!rows[2].enabled && rows[2].key == "X-Off");
        assert_eq!(rows[3], row("stato", "APERTO"));
    }

    #[test]
    fn a_query_string_is_split_and_decoded() {
        let rows = parse_text("stato=APERTO&q=hello%20world&page=0");
        assert_eq!(rows, vec![row("stato", "APERTO"), row("q", "hello world"), row("page", "0")]);
    }

    #[test]
    fn a_value_may_hold_colons_and_equals() {
        assert_eq!(parse_text("Authorization: Bearer a.b=c:d"), vec![row("Authorization", "Bearer a.b=c:d")]);
        assert_eq!(parse_text("url: https://x.it/a?b=1"), vec![row("url", "https://x.it/a?b=1")]);
    }

    #[test]
    fn devtools_pseudo_headers_keep_their_colon() {
        assert_eq!(parse_text(":authority: example.com"), vec![row(":authority", "example.com")]);
    }

    #[test]
    fn a_key_without_a_value_is_an_empty_value() {
        assert_eq!(parse_text("flag"), vec![row("flag", "")]);
        assert_eq!(parse_text("flag:"), vec![row("flag", "")]);
    }

    #[test]
    fn text_round_trips() {
        let rows = vec![row("a", "1"), KeyValue { enabled: false, ..row("b", "x: y") }, KeyValue { description: "note".into(), ..row("c", "3") }];
        assert_eq!(parse_text(&to_text(&rows)), rows);
        assert_eq!(to_text(&[]), "");
    }

    #[test]
    fn encoding_keeps_what_is_already_encoded() {
        assert_eq!(percent_encode("hello world&x=1"), "hello%20world%26x%3D1");
        assert_eq!(percent_encode("a%20b"), "a%20b");
        assert_eq!(percent_encode("100%"), "100%25");
        assert_eq!(percent_decode("a%2Fb+c%zz"), "a/b c%zz");
    }

    #[test]
    fn a_secret_is_a_marker_not_a_value() {
        assert!(row("token", " @secret ").is_secret());
        assert!(!row("token", "abc").is_secret());
    }

    #[test]
    fn only_enabled_named_rows_are_sent() {
        let rows = vec![row("a", "1"), KeyValue { enabled: false, ..row("b", "2") }, row("", "3")];
        assert_eq!(enabled_pairs(&rows), vec![("a".to_string(), "1".to_string())]);
    }
}
