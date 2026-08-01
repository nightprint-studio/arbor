//! [`Frontmatter`] — the note's typed metadata, and the byte-stability invariant.
//!
//! The hard rule this module exists to enforce:
//!
//! > **Frontmatter that nobody edited must be written back byte-for-byte.**
//!
//! Reformatting YAML on save would turn every note in the vault into a diff the
//! first time Garrulus opened it, and a sync history where every note changed on
//! day one is a history nobody can read. Since Garrulus is explicitly a *second
//! client on an Obsidian vault*, the other client's formatting — its quoting, its
//! indentation, its comments, its key order — is not ours to normalise.
//!
//! So [`Frontmatter`] carries two things: the **raw source text** exactly as it
//! was read, and the **parsed entries** for everything that wants to read a field.
//! [`Frontmatter::source`] hands the raw text back until somebody calls a mutating
//! method, at which point it returns `None` and the writer has to serialise. A
//! [`crate::io::Writer`] that ignores `source` is a bug, not a style choice.

use serde::{Deserialize, Serialize};

/// A frontmatter value.
///
/// Deliberately smaller than YAML: notes carry scalars, lists and the occasional
/// nested map, and modelling anchors, tags or multi-document streams would buy
/// nothing while making every consumer defend against shapes no note has. Values
/// the reader cannot express land as [`FrontValue::Str`] — and the raw source is
/// still there, so nothing is lost on the way back out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrontValue {
    /// A scalar string.
    Str(String),
    /// A number. One numeric type, because YAML's int/float distinction is not
    /// a distinction any note-level consumer acts on.
    Num(f64),
    /// A boolean.
    Bool(bool),
    /// A sequence.
    List(Vec<FrontValue>),
    /// A nested mapping. Ordered pairs rather than a `BTreeMap` for the same
    /// reason the top level is ordered: key order is part of what round-trips.
    Map(Vec<(String, FrontValue)>),
}

impl FrontValue {
    /// The value as a string, for the scalar case.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FrontValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The value as a number.
    pub fn as_num(&self) -> Option<f64> {
        match self {
            FrontValue::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// The value as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FrontValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The value as a sequence.
    pub fn as_list(&self) -> Option<&[FrontValue]> {
        match self {
            FrontValue::List(items) => Some(items),
            _ => None,
        }
    }

    /// The value as a nested mapping.
    pub fn as_map(&self) -> Option<&[(String, FrontValue)]> {
        match self {
            FrontValue::Map(entries) => Some(entries),
            _ => None,
        }
    }
}

impl From<&str> for FrontValue {
    fn from(value: &str) -> Self {
        FrontValue::Str(value.to_string())
    }
}

impl From<String> for FrontValue {
    fn from(value: String) -> Self {
        FrontValue::Str(value)
    }
}

impl From<bool> for FrontValue {
    fn from(value: bool) -> Self {
        FrontValue::Bool(value)
    }
}

impl From<f64> for FrontValue {
    fn from(value: f64) -> Self {
        FrontValue::Num(value)
    }
}

/// The note's frontmatter: an ordered key → [`FrontValue`] map that remembers the
/// text it came from.
///
/// Key order is preserved on purpose — it is how the user arranged the block, and
/// a `BTreeMap` would silently re-sort every note it touched.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frontmatter {
    /// The exact source text of the frontmatter body, fences excluded, as read.
    /// `None` for a document built by hand.
    #[serde(default)]
    raw: Option<String>,
    /// Parsed entries in document order.
    ///
    /// A pair list rather than a JSON object on the wire: order is part of the
    /// contract and an object would leave it to whatever the deserialiser feels
    /// like doing.
    #[serde(default)]
    entries: Vec<(String, FrontValue)>,
    /// Set the moment anything is written. Once true, [`Frontmatter::source`]
    /// stops offering the raw text — it no longer describes these entries.
    #[serde(default)]
    edited: bool,
}

impl Frontmatter {
    /// Frontmatter for a note that has none.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Frontmatter as read from a note: the raw body plus what was parsed out of
    /// it. The pair is what makes the byte-stable round trip possible, so a
    /// [`crate::io::Reader`] must use this constructor rather than
    /// [`Frontmatter::from_entries`].
    pub fn from_source(raw: impl Into<String>, entries: Vec<(String, FrontValue)>) -> Self {
        Self { raw: Some(raw.into()), entries, edited: false }
    }

    /// Frontmatter assembled in memory — a template instantiation, a refactor
    /// result. It has no source text, so a writer will always serialise it.
    pub fn from_entries(entries: Vec<(String, FrontValue)>) -> Self {
        Self { raw: None, entries, edited: true }
    }

    /// The verbatim source to write back, or `None` when the entries have been
    /// edited and the writer must serialise them instead.
    ///
    /// **This is the byte-stability invariant.** A writer that skips this check
    /// reformats every untouched note in the vault.
    pub fn source(&self) -> Option<&str> {
        if self.edited {
            None
        } else {
            self.raw.as_deref()
        }
    }

    /// Whether any field has been written since the frontmatter was read.
    pub fn is_edited(&self) -> bool {
        self.edited
    }

    /// Whether the note carries no frontmatter fields at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of top-level fields.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The value of `key`, if present.
    pub fn get(&self, key: &str) -> Option<&FrontValue> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Whether `key` is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// The fields in document order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &FrontValue)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The field names in document order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    /// Set `key`, replacing in place when it already exists and appending
    /// otherwise — so editing one field never reshuffles the block.
    ///
    /// Marks the frontmatter edited: the raw source no longer describes it.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<FrontValue>) {
        let key = key.into();
        let value = value.into();
        match self.entries.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.entries.push((key, value)),
        }
        self.edited = true;
    }

    /// Remove `key`, returning what was there. Marks the frontmatter edited only
    /// when something was actually removed — a no-op removal must not cost the
    /// note its byte-stable round trip.
    pub fn remove(&mut self, key: &str) -> Option<FrontValue> {
        let idx = self.entries.iter().position(|(k, _)| k == key)?;
        self.edited = true;
        Some(self.entries.remove(idx).1)
    }

    /// Rename a field in place, keeping its position and value. Used when a note
    /// type renames one of its fields across the vault.
    pub fn rename_key(&mut self, from: &str, to: impl Into<String>) -> bool {
        let Some(slot) = self.entries.iter_mut().find(|(k, _)| k == from) else {
            return false;
        };
        slot.0 = to.into();
        self.edited = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "titolo: 'Bug — crash all''avvio'\ntipo: bug\ntag:\n  - arbor\n";

    fn read() -> Frontmatter {
        Frontmatter::from_source(
            RAW,
            vec![
                ("titolo".into(), FrontValue::Str("Bug — crash all'avvio".into())),
                ("tipo".into(), FrontValue::Str("bug".into())),
                ("tag".into(), FrontValue::List(vec![FrontValue::from("arbor")])),
            ],
        )
    }

    #[test]
    fn untouched_frontmatter_hands_back_its_source_byte_for_byte() {
        let fm = read();
        assert!(!fm.is_edited());
        assert_eq!(fm.source(), Some(RAW));
        assert_eq!(fm.source().expect("untouched").as_bytes(), RAW.as_bytes());
    }

    #[test]
    fn reading_a_field_does_not_count_as_touching_it() {
        let fm = read();
        assert_eq!(fm.get("tipo").and_then(FrontValue::as_str), Some("bug"));
        assert!(fm.contains_key("tag"));
        assert_eq!(fm.keys().collect::<Vec<_>>(), ["titolo", "tipo", "tag"]);
        assert_eq!(fm.source(), Some(RAW));
    }

    #[test]
    fn writing_a_field_withdraws_the_verbatim_source() {
        let mut fm = read();
        fm.set("stato", "aperto");
        assert!(fm.is_edited());
        assert_eq!(fm.source(), None, "an edited block must be serialised, not echoed");
    }

    #[test]
    fn set_replaces_in_place_and_never_reshuffles() {
        let mut fm = read();
        fm.set("tipo", "improvement");
        fm.set("stato", "aperto");
        assert_eq!(fm.keys().collect::<Vec<_>>(), ["titolo", "tipo", "tag", "stato"]);
        assert_eq!(fm.get("tipo").and_then(FrontValue::as_str), Some("improvement"));
    }

    #[test]
    fn removing_a_missing_key_keeps_the_note_byte_stable() {
        let mut fm = read();
        assert!(fm.remove("inesistente").is_none());
        assert!(!fm.is_edited());
        assert_eq!(fm.source(), Some(RAW));

        assert!(fm.remove("tipo").is_some());
        assert!(fm.is_edited());
        assert_eq!(fm.len(), 2);
    }

    #[test]
    fn rename_keeps_the_position_and_reports_whether_it_hit() {
        let mut fm = read();
        assert!(fm.rename_key("tipo", "type"));
        assert_eq!(fm.keys().collect::<Vec<_>>(), ["titolo", "type", "tag"]);
        assert!(!fm.rename_key("assente", "x"));
    }

    #[test]
    fn hand_built_frontmatter_has_no_source_to_echo() {
        let fm = Frontmatter::from_entries(vec![("tipo".into(), FrontValue::from("bug"))]);
        assert_eq!(fm.source(), None);
        assert!(Frontmatter::empty().is_empty());
    }

    #[test]
    fn the_wire_shape_survives_a_json_round_trip_with_its_order() {
        let fm = read();
        let json = serde_json::to_string(&fm).expect("serialise");
        let back: Frontmatter = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, fm);
        // Order and the raw source cross the RPC seam intact, which is what lets
        // the frontend edit one field and the backend still write the rest back
        // untouched.
        assert_eq!(back.keys().collect::<Vec<_>>(), ["titolo", "tipo", "tag"]);
        assert_eq!(back.source(), Some(RAW));
    }
}
