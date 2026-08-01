//! The YAML frontmatter fence, and the minimal YAML subset behind it.
//!
//! Two separate jobs, deliberately kept apart:
//!
//! 1. [`split_frontmatter`] finds the `---` fence. It is pure slicing — no
//!    allocation, no interpretation — and it is what guarantees the rest of the
//!    pipeline never sees the fence and never mistakes it for a thematic break.
//! 2. [`parse_front_map`] reads the block into ordered key/value pairs.
//!
//! ## Why hand-rolled YAML
//!
//! `garrulus-ast` is a leaf crate (serde + thiserror), so it cannot parse YAML,
//! and this crate has no YAML dependency either. That is fine, because the
//! parsed map is **a cache, not the record**: the untouched source text travels
//! alongside it in `Frontmatter::raw` and is what gets written back, so a
//! construct this parser does not understand costs a query filter, never a
//! byte of the user's file. The subset covered is the one Obsidian vaults
//! actually contain: scalars, block and inline lists, one level of nested map.

use garrulus_ast::prelude::{FrontValue, Frontmatter};

/// Split `src` into `(frontmatter body, document body, byte offset of the body)`.
///
/// The frontmatter body is the text *between* the fences and **includes its
/// trailing newline**; that convention is what lets the writer reproduce the
/// block byte-for-byte with `"---\n" + raw + "---\n"`.
///
/// Returns `(None, src, 0)` unless `src` opens with a `---` line and a closing
/// `---` (or `...`) line follows. An unterminated fence is *not* frontmatter:
/// treating it as such would swallow the whole note.
pub fn split_frontmatter(src: &str) -> (Option<&str>, &str, usize) {
    let Some(after_open) = fence_line(src, 0) else {
        return (None, src, 0);
    };
    let mut cursor = after_open;
    while cursor <= src.len() {
        if let Some(after_close) = fence_line(src, cursor) {
            return (Some(&src[after_open..cursor]), &src[after_close..], after_close);
        }
        match src[cursor..].find('\n') {
            Some(offset) => cursor += offset + 1,
            None => break,
        }
    }
    (None, src, 0)
}

/// Build the AST frontmatter from the raw block returned by
/// [`split_frontmatter`], keeping the source text alongside the parsed pairs.
pub fn build_frontmatter(raw: Option<&str>) -> Frontmatter {
    // `from_source`, never `from_entries`: it is the constructor that keeps the
    // raw text alongside the pairs, and that pairing IS the byte-stable round
    // trip. `from_entries` marks the block edited, which would make the writer
    // re-serialise every note it ever reads.
    match raw {
        Some(raw) => Frontmatter::from_source(raw, parse_front_map(raw)),
        None => Frontmatter::empty(),
    }
}

/// Parse the frontmatter block into **ordered** key/value pairs.
///
/// Order is preserved because the frontmatter form in the UI renders fields in
/// file order; re-sorting them would shuffle the user's own layout every save.
pub fn parse_front_map(yaml: &str) -> Vec<(String, FrontValue)> {
    let lines: Vec<&str> = yaml.lines().collect();
    let mut at = 0;
    parse_map(&lines, &mut at, 0)
}

/// If `src[from..]` starts with a `---` / `...` fence line, the offset just past
/// that line's newline.
fn fence_line(src: &str, from: usize) -> Option<usize> {
    let rest = src.get(from..)?;
    let end = rest.find('\n').map_or(rest.len(), |i| i + 1);
    let line = rest[..end].trim_end_matches(['\n', '\r']);
    // Only ever exactly the fence: `--- foo` is a setext heading, not a fence.
    if line == "---" || (from > 0 && line == "...") {
        Some(from + end)
    } else {
        None
    }
}

fn parse_map(lines: &[&str], at: &mut usize, indent: usize) -> Vec<(String, FrontValue)> {
    let mut out = Vec::new();
    while *at < lines.len() {
        let line = lines[*at];
        if skippable(line) {
            *at += 1;
            continue;
        }
        let depth = indent_of(line);
        if depth < indent {
            break;
        }
        if depth > indent {
            // Content deeper than the key that should have introduced it. Not
            // recoverable, and `raw` still holds it — move on rather than loop.
            *at += 1;
            continue;
        }
        let Some((key, rest)) = split_key(line.trim_start()) else {
            *at += 1;
            continue;
        };
        *at += 1;
        if !rest.trim().is_empty() {
            out.push((key, scalar(rest.trim())));
            continue;
        }
        out.push((key, nested(lines, at, indent)));
    }
    out
}

/// The value introduced by a `key:` with nothing after the colon.
fn nested(lines: &[&str], at: &mut usize, indent: usize) -> FrontValue {
    let Some(next) = lines[*at..].iter().find(|l| !skippable(l)) else {
        return FrontValue::Str(String::new());
    };
    let depth = indent_of(next);
    // A block list may sit at the parent's own indent (`tags:\n- a`) or deeper;
    // a nested map must be deeper, otherwise it is the next sibling key.
    if next.trim_start().starts_with("- ") && depth >= indent {
        FrontValue::List(parse_list(lines, at, depth))
    } else if depth > indent {
        FrontValue::Map(parse_map(lines, at, depth))
    } else {
        FrontValue::Str(String::new())
    }
}

fn parse_list(lines: &[&str], at: &mut usize, indent: usize) -> Vec<FrontValue> {
    let mut out = Vec::new();
    while *at < lines.len() {
        let line = lines[*at];
        if skippable(line) {
            *at += 1;
            continue;
        }
        let trimmed = line.trim_start();
        if indent_of(line) != indent || !trimmed.starts_with("- ") {
            break;
        }
        out.push(scalar(trimmed[2..].trim()));
        *at += 1;
    }
    out
}

fn scalar(text: &str) -> FrontValue {
    if let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        let items = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(scalar)
            .collect();
        return FrontValue::List(items);
    }
    if let Some(unquoted) = unquote(text) {
        return FrontValue::Str(unquoted);
    }
    match text {
        "true" | "yes" => return FrontValue::Bool(true),
        "false" | "no" => return FrontValue::Bool(false),
        "null" | "~" | "" => return FrontValue::Str(String::new()),
        _ => {}
    }
    // `2026-07-31` must stay a string, so only text that is *entirely* numeric
    // syntax becomes a number.
    if looks_numeric(text) {
        if let Ok(n) = text.parse::<f64>() {
            return FrontValue::Num(n);
        }
    }
    FrontValue::Str(text.to_string())
}

fn unquote(text: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            let inner = &text[1..text.len() - 1];
            return Some(if quote == '"' {
                inner.replace("\\\"", "\"").replace("\\\\", "\\")
            } else {
                inner.replace("''", "'")
            });
        }
    }
    None
}

fn looks_numeric(text: &str) -> bool {
    let body = text.strip_prefix(['-', '+']).unwrap_or(text);
    !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '-' || c == '+')
        && body.chars().any(|c| c.is_ascii_digit())
        && body.matches('.').count() <= 1
}

/// `key: rest` → `("key", "rest")`. The colon must be followed by a space or end
/// of line, so `url: https://x` splits once and keeps the scheme intact.
fn split_key(line: &str) -> Option<(String, &str)> {
    let bytes = line.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b':' && bytes.get(i + 1).map_or(true, |n| *n == b' ' || *n == b'\t') {
            let key = line[..i].trim();
            let key = unquote(key).unwrap_or_else(|| key.to_string());
            if key.is_empty() {
                return None;
            }
            return Some((key, &line[i + 1..]));
        }
    }
    None
}

fn skippable(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#')
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_fence_leaves_the_source_untouched() {
        let src = "# Titolo\n\ntesto";
        assert_eq!(split_frontmatter(src), (None, src, 0));
    }

    #[test]
    fn a_fence_must_be_the_very_first_line() {
        let src = "\n---\ntitle: x\n---\n";
        assert_eq!(split_frontmatter(src), (None, src, 0));
    }

    #[test]
    fn splits_and_reports_the_body_offset() {
        let src = "---\ntitle: x\n---\n# H\n";
        let (front, body, at) = split_frontmatter(src);
        assert_eq!(front, Some("title: x\n"));
        assert_eq!(body, "# H\n");
        assert_eq!(at, 17);
        assert_eq!(&src[at..], body);
    }

    #[test]
    fn the_body_offset_is_a_byte_offset_not_a_char_offset() {
        // `à` and `é` are two bytes each: a char-counting implementation would
        // report 22 here instead of 24, and every span in the note would drift.
        let src = "---\ncittà: perché\n---\nCorpo\n";
        let (front, body, at) = split_frontmatter(src);
        assert_eq!(front, Some("città: perché\n"));
        assert_eq!(body, "Corpo\n");
        assert_eq!(at, 24);
        assert_eq!(&src[at..], body);
    }

    #[test]
    fn accepts_crlf_and_the_dot_terminator() {
        let (front, body, _) = split_frontmatter("---\r\na: 1\r\n...\r\nx\r\n");
        assert_eq!(front, Some("a: 1\r\n"));
        assert_eq!(body, "x\r\n");
    }

    #[test]
    fn an_unterminated_fence_is_not_frontmatter() {
        let src = "---\ntitle: x\nmai chiuso\n";
        assert_eq!(split_frontmatter(src), (None, src, 0));
    }

    #[test]
    fn an_empty_fence_yields_an_empty_block() {
        let (front, body, at) = split_frontmatter("---\n---\nx");
        assert_eq!(front, Some(""));
        assert_eq!(body, "x");
        assert_eq!(at, 8);
    }

    #[test]
    fn a_thematic_break_in_the_body_does_not_close_a_fence_that_never_opened() {
        let src = "testo\n\n---\n\naltro";
        assert_eq!(split_frontmatter(src).0, None);
    }

    #[test]
    fn reads_scalars_in_file_order() {
        let map = parse_front_map("title: Nota\nnum: 3\ndone: true\ndue: 2026-07-31\n");
        assert_eq!(
            map,
            vec![
                ("title".into(), FrontValue::Str("Nota".into())),
                ("num".into(), FrontValue::Num(3.0)),
                ("done".into(), FrontValue::Bool(true)),
                // A date must survive as text, not become 2026.
                ("due".into(), FrontValue::Str("2026-07-31".into())),
            ]
        );
    }

    #[test]
    fn reads_block_and_inline_lists() {
        let map = parse_front_map("tags:\n  - a\n  - b\naliases: [x, y]\n");
        assert_eq!(
            map[0].1,
            FrontValue::List(vec![
                FrontValue::Str("a".into()),
                FrontValue::Str("b".into())
            ])
        );
        assert_eq!(
            map[1].1,
            FrontValue::List(vec![
                FrontValue::Str("x".into()),
                FrontValue::Str("y".into())
            ])
        );
    }

    #[test]
    fn a_list_may_sit_at_the_keys_own_indent() {
        let map = parse_front_map("tags:\n- uno\n- due\nnext: 1\n");
        assert_eq!(map.len(), 2);
        assert_eq!(map[1].0, "next");
    }

    #[test]
    fn reads_a_nested_map_and_returns_to_the_parent_level() {
        let map = parse_front_map("meta:\n  autore: cm\n  peso: 2\ntitle: x\n");
        assert_eq!(
            map[0].1,
            FrontValue::Map(vec![
                ("autore".into(), FrontValue::Str("cm".into())),
                ("peso".into(), FrontValue::Num(2.0)),
            ])
        );
        assert_eq!(map[1].0, "title");
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let map = parse_front_map("# commento\n\ntitle: x\n");
        assert_eq!(map, vec![("title".into(), FrontValue::Str("x".into()))]);
    }

    #[test]
    fn keeps_a_url_intact_and_unquotes_strings() {
        let map = parse_front_map("url: https://example.org/a\nq: \"a: b\"\n");
        assert_eq!(map[0].1, FrontValue::Str("https://example.org/a".into()));
        assert_eq!(map[1].1, FrontValue::Str("a: b".into()));
    }

    #[test]
    fn an_empty_key_yields_an_empty_string_not_a_panic() {
        assert_eq!(
            parse_front_map("cover:\ntitle: x\n"),
            vec![
                ("cover".into(), FrontValue::Str(String::new())),
                ("title".into(), FrontValue::Str("x".into())),
            ]
        );
    }
}
