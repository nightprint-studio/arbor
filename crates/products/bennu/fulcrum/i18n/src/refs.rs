//! Where the project reads a label — the other half of the question the catalogue answers.
//!
//! "Which labels exist" comes from the `i18n/` trees. "Which are used, and which are used but do
//! not exist" can only come from the code, and in a fulcrum project the code is **two** kinds of
//! file:
//!
//! - **`.ron` content.** A node, a crystal, a cutscene declares a *label* rather than a string:
//!   `(id: "drill", name: "tree:nodes.drill.name", …)`. This is where most of the references are,
//!   and it is why a label check that only read `.rs` would miss the ones that matter — a content
//!   file is edited far more often than the code that draws it.
//! - **`.rs` code.** `t_key!("battle.damage")`, `key!("ui:effect.heal", amount = 3)`,
//!   `text_or(label, label)`.
//!
//! ## Recognised by shape, not by API
//!
//! A reference is a **string literal whose whole content is `category:dotted.key`**. Deliberately
//! not "the argument of a known macro": the engine's own fallback is `text_or(label, label)`, which
//! means any string may be a label and one without a `:` is definitionally not one. Matching the
//! shape therefore finds every call site, including the ones through helpers this crate has never
//! heard of — and it is the same rule the engine uses to decide whether to resolve a string at all.
//!
//! The shape is tighter than it looks, which is what keeps it from firing on ordinary text:
//!
//! - exactly one `:`, so a Rust path (`crate::foo`) and a URL are out;
//! - identifier characters only either side, so `12:30` and a sentence with a colon are out;
//! - no whitespace anywhere, so a label is never found inside prose.

use serde::Serialize;

/// One place a label is referenced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LabelRef {
    pub label: String,
    /// Byte offset of the label **inside** the quotes — what an underline covers.
    pub start: usize,
    pub end: usize,
    /// 1-based line.
    pub line: u32,
}

/// Whether this crate reads references out of `path` at all.
pub fn supports(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".rs") || lower.ends_with(".ron")
}

/// Every label referenced in `text`.
pub fn labels_in(path: &str, text: &str) -> Vec<LabelRef> {
    if !supports(path) {
        return Vec::new();
    }
    string_literals(text)
        .into_iter()
        .filter(|(content, _, _)| looks_like_label(content))
        .map(|(content, start, end)| LabelRef {
            label: content.to_string(),
            start,
            end,
            line: line_of(text, start),
        })
        .collect()
}

/// The label under `offset`, when the caret is inside one. What go-to-declaration and hover ask.
pub fn label_at(path: &str, text: &str, offset: usize) -> Option<LabelRef> {
    labels_in(path, text).into_iter().find(|r| offset >= r.start && offset <= r.end)
}

/// The text before the caret inside a string literal, when that string could become a label.
///
/// Completion's question, and it has to accept the **incomplete** shape — `menu:it` is not a label
/// yet, and neither is `men`, but both are what somebody typing one has on screen. So the test here
/// is only "an identifier, optionally with one `:` in it", not [`looks_like_label`].
pub fn label_prefix_at(path: &str, text: &str, offset: usize) -> Option<String> {
    if !supports(path) {
        return None;
    }
    let (content, start, end) = string_literals(text)
        .into_iter()
        .find(|(_, s, e)| offset >= *s && offset <= *e)?;
    let upto = offset.checked_sub(start)?;
    if upto > content.len() {
        return None;
    }
    let prefix = &content[..upto];
    let _ = end;
    let ok = !prefix.contains(char::is_whitespace)
        && prefix.matches(':').count() <= 1
        && prefix.chars().all(is_label_char);
    ok.then(|| prefix.to_string())
}

/// Whether a string literal's whole content is a label — see the module doc for why the test is
/// this strict.
pub fn looks_like_label(s: &str) -> bool {
    let Some((category, key)) = s.split_once(':') else { return false };
    if category.is_empty() || key.is_empty() || key.contains(':') {
        return false;
    }
    let ident = |seg: &str| {
        !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    // The category has to START with a letter, which is the rule that separates a label from a
    // **time**: `12:30` passes every other test here. A category is a file name, and one that
    // begins with a digit is not something anyone writes — whereas `12:30` is in every game that
    // shows a clock.
    let starts_like_a_name =
        category.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    starts_like_a_name && ident(category) && key.split('.').all(ident)
}

fn is_label_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == ':'
}

fn line_of(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

/// Every string literal in `text`, as `(content, content_start, content_end)`.
///
/// One scanner for both languages, because the literal syntax they share is the only part that
/// matters here: `"…"` with `\` escapes, and Rust's `r#"…"#` (which RON also accepts). A literal
/// carrying an escape is skipped rather than unescaped — a label has no escapes in it, so a string
/// that needs unescaping is not one, and unescaping would put the offsets out of step with the
/// file for nothing.
fn string_literals(text: &str) -> Vec<(&str, usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // A line comment: nothing quoted on the rest of the line is code. Skipping them keeps
            // a commented-out label out of the usage count, which is the honest answer — the
            // engine will never read it.
            b'/' if bytes[i..].starts_with(b"//") => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes[i..].starts_with(b"/*") => {
                i += 2;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    if bytes[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if bytes[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'r' if raw_hashes(&bytes[i..]).is_some() => {
                let hashes = raw_hashes(&bytes[i..]).unwrap_or(0);
                let start = i + 2 + hashes;
                let mut j = start;
                let mut closed = None;
                while j < bytes.len() {
                    if bytes[j] == b'"'
                        && bytes.len() >= j + 1 + hashes
                        && bytes[j + 1..j + 1 + hashes].iter().all(|b| *b == b'#')
                    {
                        closed = Some(j);
                        break;
                    }
                    j += 1;
                }
                match closed {
                    Some(end) => {
                        if let Some(s) = text.get(start..end) {
                            out.push((s, start, end));
                        }
                        i = end + 1 + hashes;
                    }
                    None => break,
                }
            }
            b'"' => {
                let start = i + 1;
                let mut j = start;
                let mut escaped = false;
                let mut closed = None;
                while j < bytes.len() {
                    match bytes[j] {
                        b'\\' => {
                            escaped = true;
                            j += 2;
                        }
                        b'"' => {
                            closed = Some(j);
                            break;
                        }
                        b'\n' => break, // an unterminated literal; do not swallow the file
                        _ => j += 1,
                    }
                }
                match closed {
                    Some(end) => {
                        if !escaped {
                            if let Some(s) = text.get(start..end) {
                                out.push((s, start, end));
                            }
                        }
                        i = end + 1;
                    }
                    None => {
                        i = j.max(i + 1);
                    }
                }
            }
            _ => i += 1,
        }
    }
    out
}

fn raw_hashes(bytes: &[u8]) -> Option<usize> {
    if bytes.first() != Some(&b'r') {
        return None;
    }
    let hashes = bytes[1..].iter().take_while(|b| **b == b'#').count();
    (bytes.get(1 + hashes) == Some(&b'"')).then_some(hashes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ron_field_holding_a_label_is_a_reference() {
        let src = r#"(id: "drill", name: "tree:nodes.drill.name", desc: "tree:nodes.drill.desc")"#;
        let found = labels_in("/p/content/tree.ron", src);
        assert_eq!(
            found.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            ["tree:nodes.drill.name", "tree:nodes.drill.desc"],
        );
        // The span covers the label and not the quotes.
        assert_eq!(&src[found[0].start..found[0].end], "tree:nodes.drill.name");
    }

    #[test]
    fn a_macro_argument_is_a_reference_without_knowing_the_macro() {
        let src = "let k = t_key!(\"ui:effect.damage\", amount = n);\n";
        let found = labels_in("/p/src/ui.rs", src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].label, "ui:effect.damage");
        assert_eq!(found[0].line, 1);
    }

    /// The engine's own fallback is `text_or(label, label)`, so a bare string may be a label — and
    /// the shape is the only thing that can tell.
    #[test]
    fn a_bare_string_of_the_right_shape_counts() {
        let found = labels_in("/p/src/ui.rs", "draw(\"menu:items.new_game\");");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_string_without_a_colon_is_not_a_label() {
        assert!(labels_in("/p/src/x.rs", "let s = \"Piccone+\";").is_empty());
        assert!(labels_in("/p/src/x.rs", "let s = \"Tool.Pick\";").is_empty());
    }

    /// The false positives that would otherwise flood a Rust file.
    #[test]
    fn rust_paths_urls_and_times_are_not_labels() {
        for s in [
            "let p = \"crate::foo::bar\";",
            "let u = \"https://example.com\";",
            "let t = \"12:30\";",
            "let t = \"1:30\";",
            "let m = \"error: something went wrong\";",
            "let q = \"a: b\";",
        ] {
            assert!(labels_in("/p/src/x.rs", s).is_empty(), "matched in `{s}`");
        }
    }

    #[test]
    fn a_raw_string_is_scanned_too() {
        let src = "const L: &str = r#\"menu:items.quit\"#;";
        let found = labels_in("/p/src/x.rs", src);
        assert_eq!(found.len(), 1);
        assert_eq!(&src[found[0].start..found[0].end], "menu:items.quit");
    }

    /// A commented-out label is not a use: the engine will never read it, so counting it would make
    /// a dead label look alive.
    #[test]
    fn a_commented_out_label_is_not_a_use() {
        assert!(labels_in("/p/src/x.rs", "// draw(\"menu:old.thing\");\n").is_empty());
        assert!(labels_in("/p/src/x.rs", "/* draw(\"menu:old.thing\"); */\n").is_empty());
    }

    #[test]
    fn the_label_under_the_caret_is_the_one_asked_for() {
        let src = r#"(name: "tree:nodes.drill.name", desc: "tree:nodes.drill.desc")"#;
        let at = src.find("drill.desc").unwrap();
        let r = label_at("/p/x.ron", src, at).expect("a label");
        assert_eq!(r.label, "tree:nodes.drill.desc");
        // Outside every literal there is nothing to ask about.
        assert!(label_at("/p/x.ron", src, 0).is_none());
    }

    #[test]
    fn a_prefix_is_offered_while_it_is_still_incomplete() {
        let src = "draw(\"menu:it\");";
        let at = src.find("it").unwrap() + 2;
        assert_eq!(label_prefix_at("/p/src/x.rs", src, at).as_deref(), Some("menu:it"));
        // And before the colon has been typed.
        let src2 = "draw(\"men\");";
        let at2 = src2.find("men").unwrap() + 3;
        assert_eq!(label_prefix_at("/p/src/x.rs", src2, at2).as_deref(), Some("men"));
    }

    #[test]
    fn a_prose_string_offers_no_prefix() {
        let src = "draw(\"hello there\");";
        let at = src.find("there").unwrap() + 2;
        assert_eq!(label_prefix_at("/p/src/x.rs", src, at), None);
    }

    #[test]
    fn a_file_kind_we_do_not_read_yields_nothing() {
        assert!(labels_in("/p/i18n/it/menu.toml", "a = \"menu:x.y\"\n").is_empty());
        assert!(!supports("/p/x.java"));
    }

    #[test]
    fn an_escaped_literal_is_skipped_rather_than_unescaped() {
        // Not a label anyway — the point is that the scanner does not mangle offsets trying.
        let found = labels_in("/p/src/x.rs", "let s = \"a\\\"b:c\";");
        assert!(found.is_empty());
    }

    #[test]
    fn an_unterminated_literal_does_not_swallow_the_file() {
        let src = "let a = \"oops\nlet b = \"menu:items.quit\";\n";
        let found = labels_in("/p/src/x.rs", src);
        assert_eq!(found.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["menu:items.quit"]);
    }

    #[test]
    fn lines_are_one_based() {
        let src = "\n\ndraw(\"menu:a.b\");";
        assert_eq!(labels_in("/p/src/x.rs", src)[0].line, 3);
    }
}
