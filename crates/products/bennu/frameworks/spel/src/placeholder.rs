//! Property placeholders — `${key}`, `${key:default}`, and the nested form `${a.${env}.c}`.
//!
//! The grammar Spring's `PropertySourcesPlaceholderConfigurer` resolves. Three facts the
//! editor needs from it and cannot get from Java's own syntax:
//!
//! 1. **Where the key is** — so `${app.timeout:30}` navigates to `app.timeout` in
//!    `application.yml` and hovers with its value, without dragging the `:30` along.
//! 2. **Whether there is a default** — a placeholder with one can never be "missing", so
//!    the unresolved-key check must not look at it.
//! 3. **Whether the braces close** — the one thing here that is unambiguously an error.
//!
//! Nesting is real (`${${platform}.url}`) and is why this is a scanner rather than a
//! regex: the closing brace of the outer placeholder is not the first `}` in the text.
//! A placeholder whose key contains a nested one is marked [`Placeholder::nested`], and
//! the inner ones are returned as their own entries — the caller colours all of them and
//! resolves none of the composed keys, which is the honest answer for a key that is only
//! known at runtime.

/// One `${…}` occurrence, with the sub-spans a consumer navigates and colours by.
///
/// Every offset is a byte offset into the scanned text, half-open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// Start of the `$` of `${`.
    pub start: usize,
    /// End of the closing `}` (exclusive). For an unterminated placeholder this is the
    /// end of the scanned text — the span still covers what the user typed.
    pub end: usize,
    /// The key text (`app.timeout`), empty for `${}` or `${:default}`. Verbatim: no
    /// trimming, because a key with a stray space *is* a different key.
    pub key: String,
    /// Span of [`Self::key`] in the scanned text.
    pub key_start: usize,
    /// End of the key span (exclusive) — the `:` position when there is a default.
    pub key_end: usize,
    /// The default value after the first top-level `:`, if written. `Some("")` for the
    /// explicit-empty default `${key:}`, which is a real and different thing from no
    /// default at all: it makes the placeholder always resolvable.
    pub default: Option<String>,
    /// Span of the default value (equal offsets when there is none).
    pub default_start: usize,
    pub default_end: usize,
    /// Whether the closing `}` was found.
    pub terminated: bool,
    /// Whether the key contains a nested `${…}` — the composed key isn't statically
    /// known, so it must never be reported as missing.
    pub nested: bool,
}

impl Placeholder {
    /// Whether this placeholder names a key that can be looked up statically: it closed,
    /// it has a non-empty key, and the key isn't composed from a nested placeholder.
    pub fn is_resolvable_key(&self) -> bool {
        self.terminated && !self.nested && !self.key.is_empty()
    }
}

/// An unambiguously broken placeholder. Only the closing brace is checked — see the
/// module docs on why nothing else is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceholderIssue {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// Every `${…}` in `text`, outermost first, each nested one following its parent.
///
/// Never fails: unterminated input yields a placeholder with `terminated: false` rather
/// than being dropped, so the editor still colours what is there and the issue list can
/// explain it.
pub fn placeholders(text: &str) -> Vec<Placeholder> {
    let mut out = Vec::new();
    scan_into(text, 0, &mut out);
    out
}

/// The innermost placeholder whose span covers `offset`, if any. What a hover / go-to at
/// a caret asks for: with `${a.${b}.c}` and the caret inside `${b}`, the answer is `b`,
/// not the composed outer key.
pub fn placeholder_at(text: &str, offset: usize) -> Option<Placeholder> {
    placeholders(text)
        .into_iter()
        .filter(|p| offset >= p.start && offset <= p.end)
        // The later entries in the list are the more deeply nested ones (parents are
        // pushed before their children), so the last match is the innermost.
        .next_back()
}

/// The unterminated placeholders in `text`, as reportable issues.
pub fn issues(text: &str) -> Vec<PlaceholderIssue> {
    placeholders(text)
        .into_iter()
        .filter(|p| !p.terminated)
        .map(|p| PlaceholderIssue {
            start: p.start,
            end: p.end,
            message: "Unclosed property placeholder — expected `}`".to_string(),
        })
        .collect()
}

/// Scan `text` for `${…}` starting at `base` (the byte offset `text` sits at in the
/// original string), appending each find and recursing into nested ones.
fn scan_into(text: &str, base: usize, out: &mut Vec<Placeholder>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] != b'$' || b[i + 1] != b'{' {
            i += 1;
            continue;
        }
        let body_start = i + 2;
        let (body_end, terminated) = match_body(b, body_start);
        let inner = &text[body_start..body_end];
        let (key, default) = split_default(inner);

        let key_start = base + body_start;
        let key_end = key_start + key.len();
        let (default_start, default_end) = match default {
            // `+ 1` skips the `:` separator itself.
            Some(d) => (key_end + 1, key_end + 1 + d.len()),
            None => (key_end, key_end),
        };
        let nested = key.contains("${");

        out.push(Placeholder {
            start: base + i,
            end: base + if terminated { body_end + 1 } else { body_end },
            key: key.to_string(),
            key_start,
            key_end,
            default: default.map(str::to_string),
            default_start,
            default_end,
            terminated,
            nested,
        });

        // Recurse into the body so nested placeholders are reported too — both halves,
        // since `${a:${fallback}}` puts one in the default.
        if !inner.is_empty() {
            scan_into(inner, base + body_start, out);
        }
        i = body_end.max(i + 2);
    }
}

/// Find the end of a placeholder body opened at `from` (just after `${`), tracking nested
/// `${` so the outer placeholder closes on ITS brace. Returns the body end (exclusive of
/// the closing `}`) and whether that brace was actually found.
fn match_body(b: &[u8], from: usize) -> (usize, bool) {
    let mut depth = 0usize;
    let mut i = from;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() && b[i + 1] == b'{' {
            depth += 1;
            i += 2;
            continue;
        }
        if b[i] == b'}' {
            if depth == 0 {
                return (i, true);
            }
            depth -= 1;
        }
        i += 1;
    }
    (b.len(), false)
}

/// Split a placeholder body into `(key, default)` at the first `:` that is not inside a
/// nested `${…}` — `${a:${b:c}}` defaults to the whole `${b:c}`, not to `${b`.
fn split_default(body: &str) -> (&str, Option<&str>) {
    let b = body.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() && b[i + 1] == b'{' {
            depth += 1;
            i += 2;
            continue;
        }
        if b[i] == b'}' && depth > 0 {
            depth -= 1;
        } else if b[i] == b':' && depth == 0 {
            return (&body[..i], Some(&body[i + 1..]));
        }
        i += 1;
    }
    (body, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_key() {
        let p = &placeholders("${app.timeout}")[0];
        assert_eq!(p.key, "app.timeout");
        assert!(p.terminated && p.is_resolvable_key());
        assert_eq!(p.default, None);
        assert_eq!((p.start, p.end), (0, 14));
        assert_eq!(&"${app.timeout}"[p.key_start..p.key_end], "app.timeout");
    }

    #[test]
    fn key_with_default_splits_at_the_first_colon() {
        let text = "${db.url:jdbc:postgresql://localhost/x}";
        let p = &placeholders(text)[0];
        assert_eq!(p.key, "db.url");
        // The colons INSIDE the JDBC url belong to the default, not to another split.
        assert_eq!(p.default.as_deref(), Some("jdbc:postgresql://localhost/x"));
        assert_eq!(&text[p.default_start..p.default_end], "jdbc:postgresql://localhost/x");
    }

    #[test]
    fn explicit_empty_default_is_not_no_default() {
        let p = &placeholders("${maybe:}")[0];
        assert_eq!(p.key, "maybe");
        assert_eq!(p.default.as_deref(), Some(""), "`${{k:}}` always resolves — it has a default");
    }

    #[test]
    fn nested_placeholder_yields_both_and_marks_the_outer() {
        let text = "${${platform}.url}";
        let ps = placeholders(text);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].key, "${platform}.url");
        assert!(ps[0].nested, "a composed key is not statically resolvable");
        assert!(!ps[0].is_resolvable_key());
        assert_eq!(ps[1].key, "platform");
        assert!(ps[1].is_resolvable_key());
        assert_eq!(&text[ps[1].start..ps[1].end], "${platform}");
    }

    #[test]
    fn nested_inside_the_default_is_found_too() {
        let ps = placeholders("${a:${b}}");
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].key, "a");
        assert_eq!(ps[0].default.as_deref(), Some("${b}"));
        assert_eq!(ps[1].key, "b");
    }

    #[test]
    fn unterminated_is_reported_not_dropped() {
        let ps = placeholders("prefix ${app.name");
        assert_eq!(ps.len(), 1);
        assert!(!ps[0].terminated);
        assert!(!ps[0].is_resolvable_key());
        assert_eq!(issues("prefix ${app.name").len(), 1);
        assert!(issues("${ok}").is_empty());
    }

    #[test]
    fn several_in_one_string() {
        let ps = placeholders("jdbc://${host}:${port}/db");
        assert_eq!(ps.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["host", "port"]);
    }

    #[test]
    fn text_without_placeholders_is_empty() {
        assert!(placeholders("plain text, $ and { alone, and a lone }").is_empty());
    }

    #[test]
    fn placeholder_at_returns_the_innermost() {
        let text = "${a.${b}.c}";
        let inner_caret = text.find("b").unwrap();
        assert_eq!(placeholder_at(text, inner_caret).unwrap().key, "b");
        assert_eq!(placeholder_at(text, 1).unwrap().key, "a.${b}.c");
        assert!(placeholder_at("no braces here", 3).is_none());
    }

    #[test]
    fn non_ascii_around_a_placeholder_keeps_spans_sliceable() {
        let text = "città ${app.città}";
        let p = &placeholders(text)[0];
        // The whole point of byte spans: they must still be char boundaries.
        assert_eq!(&text[p.key_start..p.key_end], "app.città");
        assert_eq!(&text[p.start..p.end], "${app.città}");
    }
}
