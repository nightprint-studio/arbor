//! The single coupling point between the index and `garrulus-vault`.
//!
//! Every other module in this crate works on [`NoteView`], an owned projection
//! of a note carrying exactly what the index needs. The reason is blunt: if the
//! shape of `Note` moves, only this file has to move with it — the graph, the
//! word index, the scorer, the query language and the problem report are all
//! written against `NoteView` and stay untouched.

use std::collections::BTreeMap;

use garrulus_ast::prelude::FrontValue;
use garrulus_vault::prelude::{Note, NoteId, TypeId};
use serde::{Deserialize, Serialize};

/// Build a [`NoteId`] from its string form.
///
/// Centralised so that the id constructor is named once in the whole crate.
pub fn note_id(raw: &str) -> NoteId {
    NoteId::from(raw.to_owned())
}

/// Build a [`TypeId`] from its string form (what a `type:bug` filter parses to).
pub fn type_id(raw: &str) -> TypeId {
    TypeId::from(raw.to_owned())
}

/// One outgoing wikilink, flattened out of the note's AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRef {
    /// The link target as written, e.g. `Progetti/Arbor` in `[[Progetti/Arbor#Sync|qui]]`.
    pub target: String,
    /// The `#heading` part, if the link pointed at a section.
    pub heading: Option<String>,
    /// The `|alias` part, if the link was renamed at the call site.
    pub alias: Option<String>,
    /// `true` for `![[embed]]` transclusions.
    pub embed: bool,
}

impl LinkRef {
    /// A bare link to `target`, with no heading, alias or embedding.
    pub fn plain(target: impl Into<String>) -> Self {
        Self { target: target.into(), heading: None, alias: None, embed: false }
    }
}

/// A note reduced to the fields the index actually reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteView {
    /// Identity of the note inside the vault.
    pub id: NoteId,
    /// Display title — what the quick switcher matches against.
    pub title: String,
    /// The note type it was classified as, if any.
    pub kind: Option<TypeId>,
    /// Tags, without the leading `#`.
    pub tags: Vec<String>,
    /// Outgoing wikilinks, in document order.
    pub links: Vec<LinkRef>,
    /// Heading texts, in document order — searchable, and the outline's source.
    pub headings: Vec<String>,
    /// Frontmatter flattened to strings, which is what `key:value` filters compare.
    pub fields: BTreeMap<String, String>,
}

impl NoteView {
    /// A view with nothing but an id and a title. Used by tests and by callers
    /// that only need a note to exist as a link destination.
    pub fn new(id: NoteId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            kind: None,
            tags: Vec::new(),
            links: Vec::new(),
            headings: Vec::new(),
            fields: BTreeMap::new(),
        }
    }

    /// Everything about the note that is searchable but is not its body:
    /// title, headings, tags and frontmatter values, concatenated for tokenising.
    pub fn metadata_text(&self) -> String {
        let mut out = String::with_capacity(self.title.len() + 64);
        out.push_str(&self.title);
        for h in &self.headings {
            out.push(' ');
            out.push_str(h);
        }
        for t in &self.tags {
            out.push(' ');
            out.push_str(t);
        }
        for v in self.fields.values() {
            out.push(' ');
            out.push_str(v);
        }
        out
    }

    /// Case-insensitive tag lookup, so `#Bug` and `#bug` are the same bucket.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }
}

impl From<&Note> for NoteView {
    fn from(note: &Note) -> Self {
        Self {
            id: note.id.clone(),
            title: note.title.clone(),
            kind: note.kind.clone(),
            // `tag_names` rather than the raw tags: it drops the spans the index
            // has no use for AND deduplicates, which a tag bucket wants.
            tags: note.tag_names(),
            links: note
                .links
                .iter()
                .map(|l| LinkRef {
                    target: l.target.clone(),
                    heading: l.heading.clone(),
                    alias: l.alias.clone(),
                    embed: l.embed,
                })
                .collect(),
            headings: note.headings.iter().map(|h| h.text.clone()).collect(),
            fields: flatten_frontmatter(note.frontmatter.iter()),
        }
    }
}

/// Flatten `key -> FrontValue` pairs into the `key -> String` map that
/// `Filter::Field` compares against.
///
/// Nested maps collapse to the empty string rather than to a debug rendering:
/// a filter that silently matched `{...}` would be worse than one that never
/// matches at all.
pub fn flatten_frontmatter<'a, I>(pairs: I) -> BTreeMap<String, String>
where
    I: IntoIterator<Item = (&'a str, &'a FrontValue)>,
{
    pairs.into_iter().map(|(k, v)| (k.to_lowercase(), front_value_to_string(v))).collect()
}

/// Render a frontmatter value the way a query filter should see it.
pub fn front_value_to_string(value: &FrontValue) -> String {
    match value {
        FrontValue::Str(s) => s.clone(),
        FrontValue::Num(n) => format_number(*n),
        FrontValue::Bool(b) => b.to_string(),
        FrontValue::List(items) => {
            items.iter().map(front_value_to_string).collect::<Vec<_>>().join(", ")
        }
        // Nested maps have no scalar rendering worth filtering on.
        _ => String::new(),
    }
}

/// Print a YAML number without the `.0` tail, so `priority: 3` filters as `3`.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_text_covers_every_searchable_non_body_field() {
        let mut v = NoteView::new(note_id("a"), "Titolo");
        v.headings.push("Sezione".into());
        v.tags.push("bug".into());
        v.fields.insert("stato".into(), "aperto".into());

        let text = v.metadata_text();
        for expected in ["Titolo", "Sezione", "bug", "aperto"] {
            assert!(text.contains(expected), "{text:?} is missing {expected:?}");
        }
    }

    #[test]
    fn tags_match_case_insensitively() {
        let mut v = NoteView::new(note_id("a"), "T");
        v.tags.push("Bug".into());
        assert!(v.has_tag("bug"));
        assert!(v.has_tag("BUG"));
        assert!(!v.has_tag("bugs"));
    }

    #[test]
    fn numbers_lose_their_zero_fraction() {
        assert_eq!(front_value_to_string(&FrontValue::Num(3.0)), "3");
        assert_eq!(front_value_to_string(&FrontValue::Num(-1.0)), "-1");
        assert_eq!(front_value_to_string(&FrontValue::Num(1.5)), "1.5");
    }

    #[test]
    fn lists_flatten_to_a_comma_joined_string() {
        let v = FrontValue::List(vec![
            FrontValue::Str("uno".into()),
            FrontValue::Bool(true),
            FrontValue::Num(2.0),
        ]);
        assert_eq!(front_value_to_string(&v), "uno, true, 2");
    }

    #[test]
    fn frontmatter_keys_are_lowercased_so_filters_are_case_insensitive() {
        let key = "Stato".to_string();
        let value = FrontValue::Str("aperto".into());
        let map = flatten_frontmatter([(key.as_str(), &value)]);
        assert_eq!(map.get("stato").map(String::as_str), Some("aperto"));
    }
}
