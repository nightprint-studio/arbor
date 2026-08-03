//! Turning a Spring expression into coloured spans.
//!
//! Shared by the Java and the XML side, because `${app.timeout}` means the same thing in
//! `@Value("…")` and in `<property value="…"/>` and should not look like two different
//! languages depending on which file it is in.
//!
//! Kinds are namespaced strings rather than an enum: the frontend maps a kind it does not
//! recognise to a neutral class, so a new one can be added here without a frontend change.

use bennu_ext::prelude::ExtHighlight;
use bennu_spel::prelude::{self as spel, TokenKind};

/// Placeholder and SpEL spans inside `text`, offset by `base` (the position of `text` in
/// the file).
pub fn expression_highlights(text: &str, base: usize, out: &mut Vec<ExtHighlight>) {
    for p in spel::placeholders(text) {
        out.push(span(base + p.start, base + p.end, "spring.placeholder"));
        if p.key_end > p.key_start {
            out.push(span(base + p.key_start, base + p.key_end, "spring.placeholder.key"));
        }
        if p.default.is_some() && p.default_end > p.default_start {
            out.push(span(
                base + p.default_start,
                base + p.default_end,
                "spring.placeholder.default",
            ));
        }
    }
    for e in spel::expressions(text) {
        out.push(span(base + e.start, base + e.end, "spring.spel"));
        for t in &e.tokens {
            let kind = match t.kind {
                TokenKind::BeanRef => "spring.spel.bean",
                TokenKind::Variable => "spring.spel.variable",
                TokenKind::TypeRef => "spring.spel.type",
                TokenKind::Keyword => "spring.spel.keyword",
                TokenKind::String => "spring.spel.string",
                TokenKind::Number => "spring.spel.number",
                // A placeholder nested in an expression was already coloured by the pass
                // above; identifiers, operators and punctuation keep the surrounding
                // colour rather than turning one expression into a rainbow.
                _ => continue,
            };
            out.push(span(base + t.start, base + t.end, kind));
        }
    }
}

/// The `{var}` segments of a request-mapping path.
pub fn path_var_highlights(text: &str, base: usize, out: &mut Vec<ExtHighlight>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'{' {
            i += 1;
            continue;
        }
        match b[i..].iter().position(|&c| c == b'}') {
            Some(rel) => {
                out.push(span(base + i, base + i + rel + 1, "spring.path-var"));
                i += rel + 1;
            }
            None => break,
        }
    }
}

fn span(start: usize, end: usize, kind: &str) -> ExtHighlight {
    ExtHighlight { start, end, kind: kind.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        expression_highlights(text, 0, &mut out);
        out.into_iter().map(|h| (h.kind, text[h.start..h.end].to_string())).collect()
    }

    #[test]
    fn a_placeholder_yields_whole_key_and_default() {
        let hs = kinds("${app.timeout:30}");
        assert!(hs.contains(&("spring.placeholder".into(), "${app.timeout:30}".into())));
        assert!(hs.contains(&("spring.placeholder.key".into(), "app.timeout".into())));
        assert!(hs.contains(&("spring.placeholder.default".into(), "30".into())));
    }

    #[test]
    fn spel_tokens_that_carry_meaning_get_a_kind() {
        let hs = kinds("#{@svc.find(T(Math).max(1,2), 'x') and true}");
        assert!(hs.contains(&("spring.spel.bean".into(), "@svc".into())));
        assert!(hs.contains(&("spring.spel.type".into(), "T".into())));
        assert!(hs.contains(&("spring.spel.string".into(), "'x'".into())));
        assert!(hs.contains(&("spring.spel.keyword".into(), "and".into())));
        assert!(hs.contains(&("spring.spel.number".into(), "1".into())));
        assert!(!hs.iter().any(|(k, _)| k == "spring.spel.punct"), "structure keeps its colour");
    }

    #[test]
    fn plain_text_is_not_coloured() {
        assert!(kinds("just a value").is_empty());
        assert!(kinds("100").is_empty());
    }

    #[test]
    fn path_variables_are_spanned_including_their_braces() {
        let mut out = Vec::new();
        path_var_highlights("/orders/{id}/items/{itemId}", 0, &mut out);
        let text = "/orders/{id}/items/{itemId}";
        let spans: Vec<_> = out.iter().map(|h| &text[h.start..h.end]).collect();
        assert_eq!(spans, ["{id}", "{itemId}"]);
    }

    #[test]
    fn an_unclosed_brace_stops_the_path_scan_rather_than_spanning_the_rest() {
        let mut out = Vec::new();
        path_var_highlights("/a/{id", 0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn spans_are_offset_by_the_base() {
        let file = "@Value(\"${a.b}\")";
        let base = file.find("${").unwrap();
        let mut out = Vec::new();
        expression_highlights("${a.b}", base, &mut out);
        let whole = out.iter().find(|h| h.kind == "spring.placeholder").unwrap();
        assert_eq!(&file[whole.start..whole.end], "${a.b}");
    }
}
