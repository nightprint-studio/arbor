//! [`Inline`] — the span-level content of a block.
//!
//! Named by intent, never by spelling: [`Inline::Strong`] rather than "double
//! asterisk", [`Inline::WikiLink`] rather than "double bracket". A reader for a
//! second format produces the same variants, and the index, the outline and every
//! exporter keep working without knowing which reader ran.
//!
//! [`Inline::WikiLink`] is the one variant that is not CommonMark, and it is the
//! centre of the vault: `[[target#heading|alias]]` and `![[embed]]` are how notes
//! point at each other. It is modelled with its parts already split apart because
//! every consumer needs the parts — the index needs `target`, the editor renders
//! `alias`, transclusion needs `heading` and `embed`.

use serde::{Deserialize, Serialize};

use crate::span::Span;

/// One piece of span-level content.
///
/// Nesting is by construction: emphasis wraps a `Vec<Inline>` rather than being a
/// pair of markers, so "bold inside a link label" is a tree and not a parsing
/// accident that has to be recovered later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Inline {
    /// Literal text, already unescaped.
    Text(String),
    /// Emphasised run (markdown's single `*`/`_`).
    Emph(Vec<Inline>),
    /// Strongly emphasised run (markdown's double `**`/`__`).
    Strong(Vec<Inline>),
    /// Struck-through run.
    Strike(Vec<Inline>),
    /// Highlighted run (Obsidian's `==…==`).
    Highlight(Vec<Inline>),
    /// Verbatim code, already stripped of its delimiters.
    Code(String),
    /// A link to another note in the same vault.
    WikiLink {
        /// The note being pointed at, as written — resolution against the vault
        /// happens in `garrulus-index`, not here. An empty target means the link
        /// is intra-note (`[[#Heading]]`).
        target: String,
        /// Heading or block anchor inside the target, without its `#`.
        heading: Option<String>,
        /// Display text the user supplied after `|`. `None` means "show the
        /// target"; deciding *how* to show it is the renderer's business.
        alias: Option<String>,
        /// `true` for `![[…]]`: transclude the target rather than link to it.
        embed: bool,
        /// Byte range of the whole link, delimiters included.
        span: Span,
    },
    /// An ordinary link: a URL, a relative path, or anything else.
    Link {
        /// Destination exactly as written; never resolved or normalised here.
        href: String,
        /// Link text, which may itself be styled.
        label: Vec<Inline>,
        /// Byte range of the whole link.
        span: Span,
    },
    /// An embedded image or media reference written in the non-wiki form.
    Image {
        /// Source path or URL exactly as written.
        src: String,
        /// Alternative text.
        alt: String,
        /// Byte range of the whole image.
        span: Span,
    },
    /// An inline `#tag`, stored **without** the leading `#`. Nested tags keep
    /// their slashes (`progetto/arbor`) because the hierarchy is the tag.
    Tag {
        /// Tag path without the `#`.
        name: String,
        /// Byte range of the tag, `#` included.
        span: Span,
    },
    /// A hard line break inside a block.
    Break,
}

impl Inline {
    /// Convenience constructor for the overwhelmingly common case.
    pub fn text(s: impl Into<String>) -> Self {
        Inline::Text(s.into())
    }

    /// The byte range of this inline, for the variants that carry one.
    ///
    /// Styling variants have no span of their own: their extent is their
    /// children's, and reconstructing it would invent precision the reader never
    /// reported. Consumers that need it can fold [`Span::join`] over the children.
    pub fn span(&self) -> Option<Span> {
        match self {
            Inline::WikiLink { span, .. }
            | Inline::Link { span, .. }
            | Inline::Image { span, .. }
            | Inline::Tag { span, .. } => Some(*span),
            _ => None,
        }
    }

    /// The nested inlines this one wraps, empty for the leaf variants.
    pub fn children(&self) -> &[Inline] {
        match self {
            Inline::Emph(kids)
            | Inline::Strong(kids)
            | Inline::Strike(kids)
            | Inline::Highlight(kids)
            | Inline::Link { label: kids, .. } => kids,
            _ => &[],
        }
    }

    /// Mutable view of [`Inline::children`], for AST refactors.
    ///
    /// Returns the owning `Vec` rather than a slice so a rewrite can insert and
    /// remove, not only replace — splicing a transclusion in is an insert.
    pub fn children_mut(&mut self) -> Option<&mut Vec<Inline>> {
        match self {
            Inline::Emph(kids)
            | Inline::Strong(kids)
            | Inline::Strike(kids)
            | Inline::Highlight(kids)
            | Inline::Link { label: kids, .. } => Some(kids),
            _ => None,
        }
    }
}

/// The text of a run of inlines with all styling dropped.
///
/// What a heading contributes to the outline, what a link's label contributes to
/// a search hit, what a note's H1 contributes to its title. Deliberately *not* a
/// `Display` impl: this is a lossy projection, not a rendering, and calling it
/// `to_string` would invite it into places where a [`crate::io::Writer`] belongs.
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_plain_text(inlines, &mut out);
    out
}

fn push_plain_text(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emph(kids)
            | Inline::Strong(kids)
            | Inline::Strike(kids)
            | Inline::Highlight(kids)
            | Inline::Link { label: kids, .. } => push_plain_text(kids, out),
            // A wikilink reads as what the user chose to see.
            Inline::WikiLink { alias, target, .. } => {
                out.push_str(alias.as_deref().unwrap_or(target))
            }
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Tag { name, .. } => {
                out.push('#');
                out.push_str(name);
            }
            Inline::Break => out.push(' '),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_flattens_nesting() {
        let inlines = vec![
            Inline::text("un "),
            Inline::Strong(vec![Inline::text("titolo "), Inline::Emph(vec![Inline::text("misto")])]),
        ];
        assert_eq!(plain_text(&inlines), "un titolo misto");
    }

    #[test]
    fn plain_text_prefers_the_alias_of_a_wikilink() {
        let with_alias = vec![Inline::WikiLink {
            target: "Nota lunga".into(),
            heading: None,
            alias: Some("corta".into()),
            embed: false,
            span: Span::new(0, 22),
        }];
        assert_eq!(plain_text(&with_alias), "corta");

        let without = vec![Inline::WikiLink {
            target: "Nota lunga".into(),
            heading: Some("Sezione".into()),
            alias: None,
            embed: true,
            span: Span::EMPTY,
        }];
        assert_eq!(plain_text(&without), "Nota lunga");
    }

    #[test]
    fn only_the_anchored_variants_report_a_span() {
        assert_eq!(
            Inline::Tag { name: "arbor".into(), span: Span::new(4, 10) }.span(),
            Some(Span::new(4, 10))
        );
        assert_eq!(Inline::Strong(vec![Inline::text("x")]).span(), None);
    }

    #[test]
    fn children_exposes_the_wrapped_run() {
        let link = Inline::Link {
            href: "https://example.invalid".into(),
            label: vec![Inline::text("qui")],
            span: Span::EMPTY,
        };
        assert_eq!(link.children(), &[Inline::text("qui")]);
        assert!(Inline::Break.children().is_empty());
    }
}
