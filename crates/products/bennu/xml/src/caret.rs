//! Where the caret is, in XML's terms.
//!
//! Four places, and they take completely different answers: an element name, an attribute name,
//! an attribute value, or text between tags. Getting the boundary between them wrong is what
//! makes a completion list feel random, so the classification is its own module with its own
//! tests rather than a branch inside the completion function.
//!
//! ## Why the element name is found by looking back, not by asking the scanner
//!
//! Because of `<`. The instant it is typed there is no tag to find — `<` followed by a newline
//! is not markup and the scanner is right to say so — and that instant is precisely when the
//! user wants the list of children. So an element name is recognised from the text: the run of
//! name characters before the caret, and a `<` or `</` immediately before *that*.
//!
//! It happens to be simpler as well as more correct: the same rule covers `<`, `<dep`, `</`, and
//! `</dep` without four cases.

use bennu_complete::prelude::{safe_offset, token_before};

use crate::scan::Scan;

/// What the caret is in the middle of writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caret {
    /// An element name, after `<` or `</`.
    ElementName {
        /// The element this one would go inside. Empty at the document root.
        parent: String,
        /// What has been typed of the name.
        prefix: String,
        /// Where the name starts — the span a completion replaces.
        start: usize,
        /// This is a closing tag, so the only right answer is the element being closed.
        closing: bool,
    },
    /// An attribute name inside a tag.
    AttrName { element: String, prefix: String, start: usize },
    /// An attribute value.
    AttrValue { element: String, attr: String, prefix: String, start: usize },
    /// Text between tags.
    Content {
        /// The innermost open element. Empty before the root.
        element: String,
    },
}

impl Caret {
    /// The element the caret is writing about — for hover and go-to, which do not care which of
    /// the four positions it is.
    pub fn element(&self) -> &str {
        match self {
            Caret::ElementName { prefix, .. } => prefix,
            Caret::AttrName { element, .. }
            | Caret::AttrValue { element, .. }
            | Caret::Content { element } => element,
        }
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')
}

/// Classify the caret. `None` where nothing may be offered: inside a comment, a CDATA section or
/// a processing instruction — a caret in prose, where answering would be answering about text.
pub fn classify(scan: &Scan, source: &str, offset: usize) -> Option<Caret> {
    let offset = safe_offset(source, offset)?;
    if scan.inert_at(offset) {
        return None;
    }

    // An element name, wherever it is being written. Checked first because the `<` case has no
    // tag for the scanner to have found.
    let (start, prefix) = token_before(source, offset, is_name_char);
    let before = &source[..start];
    if before.ends_with("</") || before.ends_with('<') {
        let closing = before.ends_with("</");
        let open_at = start - if closing { 2 } else { 1 };
        return Some(Caret::ElementName {
            parent: scan.parent_at(open_at).unwrap_or_default().to_string(),
            prefix: prefix.to_string(),
            start,
            closing,
        });
    }

    match scan.tag_at(offset) {
        Some(tag) if offset > tag.name_end => {
            let element = tag.name.clone();
            if let Some(a) = tag.attr_value_at(offset) {
                return Some(Caret::AttrValue {
                    element,
                    attr: a.name.clone(),
                    prefix: source[a.value_start..offset].to_string(),
                    start: a.value_start,
                });
            }
            // `name=|` — past the equals but before the quote. Still a value position: offering
            // attribute names here would be offering the one thing that cannot go next.
            if before.ends_with('=') || before.ends_with("=\"") || before.ends_with("='") {
                let attr = tag
                    .attrs
                    .iter()
                    .filter(|a| a.name_end <= offset)
                    .next_back()
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                return Some(Caret::AttrValue { element, attr, prefix: String::new(), start: offset });
            }
            if let Some(a) = tag.attr_name_at(offset) {
                return Some(Caret::AttrName {
                    element,
                    prefix: source[a.name_start..offset].to_string(),
                    start: a.name_start,
                });
            }
            Some(Caret::AttrName { element, prefix: prefix.to_string(), start })
        }
        // Inside the tag but not past its name, and not after a `<` — the caret is on the `<`
        // itself. Nothing to complete from.
        Some(_) => None,
        None => Some(Caret::Content { element: scan.parent_at(offset).unwrap_or_default().to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan;

    /// Classify at the `|` in `text`, which is stripped first.
    fn at(text: &str) -> Option<Caret> {
        let offset = text.find('|').expect("mark the caret with |");
        let source = text.replace('|', "");
        classify(&scan(&source), &source, offset)
    }

    #[test]
    fn the_instant_the_angle_bracket_is_typed_is_an_element_name() {
        assert_eq!(
            at("<project>\n  <|\n</project>"),
            Some(Caret::ElementName {
                parent: "project".into(),
                prefix: String::new(),
                start: 13,
                closing: false,
            }),
        );
    }

    #[test]
    fn a_partly_typed_name_carries_the_span_it_would_replace() {
        let Some(Caret::ElementName { parent, prefix, start, closing }) =
            at("<project>\n  <depend|\n</project>")
        else {
            panic!("expected an element name")
        };
        assert_eq!((parent.as_str(), prefix.as_str(), closing), ("project", "depend", false));
        assert_eq!(start, 13);
    }

    #[test]
    fn a_closing_tag_says_so() {
        let Some(Caret::ElementName { parent, closing, .. }) = at("<a><b></|") else {
            panic!("expected an element name")
        };
        assert!(closing);
        assert_eq!(parent, "b", "the innermost open element is what is being closed");
    }

    #[test]
    fn the_three_positions_inside_a_tag_are_told_apart() {
        assert_eq!(
            at("<action |/>"),
            Some(Caret::AttrName { element: "action".into(), prefix: String::new(), start: 8 }),
        );
        assert_eq!(
            at("<action nam|/>"),
            Some(Caret::AttrName { element: "action".into(), prefix: "nam".into(), start: 8 }),
        );
        assert_eq!(
            at(r#"<action name="sav|"/>"#),
            Some(Caret::AttrValue {
                element: "action".into(),
                attr: "name".into(),
                prefix: "sav".into(),
                start: 14,
            }),
        );
    }

    /// Past the equals, before the quote. Offering attribute names here would offer the one
    /// thing that cannot come next.
    #[test]
    fn immediately_after_the_equals_is_already_a_value() {
        let Some(Caret::AttrValue { attr, prefix, .. }) = at(r#"<action name=|"#) else {
            panic!("expected a value position")
        };
        assert_eq!((attr.as_str(), prefix.as_str()), ("name", ""));
        // And inside the opening quote, which the scanner does see as a value.
        let Some(Caret::AttrValue { attr, prefix, .. }) = at(r#"<action name="|"/>"#) else {
            panic!("expected a value position")
        };
        assert_eq!((attr.as_str(), prefix.as_str()), ("name", ""));
    }

    #[test]
    fn text_between_tags_knows_which_element_it_is_in() {
        assert_eq!(
            at("<project><build>  |  </build></project>"),
            Some(Caret::Content { element: "build".into() }),
        );
        assert_eq!(at("|<project/>"), Some(Caret::Content { element: String::new() }));
    }

    #[test]
    fn a_caret_in_prose_is_not_a_completion_site() {
        assert!(at("<a><!-- <dep| --></a>").is_none());
        assert!(at("<a><![CDATA[ <dep| ]]></a>").is_none());
        assert!(at("<?xml ver|sion?>").is_none());
    }

    #[test]
    fn a_prefixed_name_is_one_token() {
        let Some(Caret::ElementName { prefix, .. }) = at("<beans><context:comp|") else {
            panic!("expected an element name")
        };
        assert_eq!(prefix, "context:comp", "the prefix is part of the name being typed");
    }
}
