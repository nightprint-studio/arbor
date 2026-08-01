//! Callout headers — the `[!NOTE]- Titolo` line that turns a block quote into a
//! callout.
//!
//! Pure string work, kept out of [`reader`](crate::reader) because it is the one
//! piece of the quote path with real syntax in it and it deserves its own tests.
//!
//! ## The fold marker
//!
//! Obsidian spells three states: `[!NOTE]` (not foldable), `[!NOTE]+`
//! (foldable, open) and `[!NOTE]-` (foldable, collapsed). The model carries a
//! single `folded: bool`, so `+` and the bare form both round-trip as "not
//! folded" and the distinction between "cannot fold" and "open" is lost. That is
//! a deliberate simplification of the model, not an oversight here: a callout
//! the reader can never collapse is a worse default than one they can.

use garrulus_ast::prelude::CalloutKind;
use std::str::FromStr;

/// A parsed `[!KIND]± Title` header.
///
/// Deliberately *not* serde-derived: nothing crosses a seam as a bare header —
/// it always arrives folded into `Block::Callout` — and deriving here would tie
/// this crate to whether `CalloutKind` happens to be serialisable.
#[derive(Debug, Clone, PartialEq)]
pub struct CalloutHeader {
    pub kind: CalloutKind,
    /// The text after the marker, or `None` when the author wrote none — which
    /// is what tells a renderer to fall back to the kind's own label.
    pub title: Option<String>,
    /// `true` only for the `-` marker.
    pub folded: bool,
}

/// Parse the first line of a de-quoted block quote as a callout header.
///
/// Returns `None` for an ordinary quote, which is the common case and must stay
/// cheap.
pub fn parse_callout_header(line: &str) -> Option<CalloutHeader> {
    let rest = line.trim_start().strip_prefix("[!")?;
    let close = rest.find(']')?;
    let kind = CalloutKind::from_str(rest[..close].trim()).ok()?;
    let after = &rest[close + 1..];
    let (folded, title) = match after.strip_prefix('-') {
        Some(t) => (true, t),
        None => (false, after.strip_prefix('+').unwrap_or(after)),
    };
    let title = title.trim();
    Some(CalloutHeader {
        kind,
        title: (!title.is_empty()).then(|| title.to_string()),
        folded,
    })
}

/// Render a callout header line (without the `> ` prefix and without a newline).
pub fn format_callout_header(kind: &CalloutKind, title: Option<&str>, folded: bool) -> String {
    let mut out = format!("[!{kind}]");
    if folded {
        out.push('-');
    }
    if let Some(title) = title.filter(|t| !t.trim().is_empty()) {
        out.push(' ');
        out.push_str(title.trim());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_bare_header() {
        assert_eq!(
            parse_callout_header("[!NOTE]"),
            Some(CalloutHeader {
                kind: CalloutKind::Note,
                title: None,
                folded: false,
            })
        );
    }

    #[test]
    fn reads_a_folded_header_with_a_title() {
        assert_eq!(
            parse_callout_header("[!warning]- Attenzione ai path"),
            Some(CalloutHeader {
                kind: CalloutKind::Warning,
                title: Some("Attenzione ai path".into()),
                folded: true,
            })
        );
    }

    #[test]
    fn a_plus_marker_is_open_not_folded() {
        let got = parse_callout_header("[!tip]+ Suggerimento").expect("header");
        assert!(!got.folded);
        assert_eq!(got.title.as_deref(), Some("Suggerimento"));
    }

    #[test]
    fn an_unknown_kind_survives_as_other() {
        assert_eq!(
            parse_callout_header("[!bug] Da sistemare").map(|h| h.kind),
            // `FromStr` normalises to uppercase so `Display` is its exact inverse.
            Some(CalloutKind::Other("BUG".into()))
        );
    }

    #[test]
    fn an_ordinary_quote_is_not_a_callout() {
        assert_eq!(parse_callout_header("Solo una citazione"), None);
        assert_eq!(parse_callout_header("[NOTE] senza bang"), None);
        assert_eq!(parse_callout_header("[!NOTE senza chiusura"), None);
    }

    #[test]
    fn formatting_is_the_inverse_of_parsing() {
        for line in ["[!NOTE]", "[!TIP] Titolo", "[!WARNING]- Piegato"] {
            let parsed = parse_callout_header(line).expect("header");
            let back = format_callout_header(&parsed.kind, parsed.title.as_deref(), parsed.folded);
            assert_eq!(back, line);
        }
    }
}
