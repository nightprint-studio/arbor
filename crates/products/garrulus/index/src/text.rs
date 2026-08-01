//! The inverted word index and snippet extraction.
//!
//! Word -> notes, plus the raw body of every note kept alongside so that a hit
//! can be rendered with the line it was found on. At a personal vault's scale
//! (thousands of notes) a `BTreeMap` of postings is instant and rebuildable in
//! milliseconds; the `fst` + mmap store in `bennu-index` is the documented
//! upgrade path if that ever stops being true (docs/garrulus-design.md §5.2).

use std::collections::{BTreeMap, BTreeSet};

use garrulus_vault::prelude::NoteId;
use serde::{Deserialize, Serialize};

/// How much text a snippet shows around the first match.
const SNIPPET_BUDGET: usize = 160;

/// A half-open byte range inside some rendered string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRange {
    /// Inclusive start, in bytes.
    pub start: usize,
    /// Exclusive end, in bytes.
    pub end: usize,
}

/// A slice of note body around a match, ready to render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    /// The excerpt, with a leading/trailing ellipsis when it was cut.
    pub text: String,
    /// Ranges to highlight, as byte offsets **into `text`**, not into the body.
    pub ranges: Vec<MatchRange>,
}

/// Split text into the lowercased alphanumeric runs the index stores.
///
/// Punctuation, markdown syntax and wikilink brackets all fall out as
/// separators, which is exactly what we want: `[[Nota]]` indexes as `nota`.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Find `needle` inside `haystack`, ignoring case, returning the byte range in
/// `haystack`.
///
/// Written by hand instead of `haystack.to_lowercase().find(..)` because
/// lowercasing can change a string's length (`İ` -> `i̇`), which would make the
/// returned offset unusable for slicing the original.
pub fn find_ignore_case(haystack: &str, needle: &str) -> Option<MatchRange> {
    if needle.is_empty() {
        return None;
    }
    let needle: Vec<char> = needle.chars().map(fold).collect();
    let hay: Vec<(usize, char)> = haystack.char_indices().map(|(i, c)| (i, fold(c))).collect();
    if needle.len() > hay.len() {
        return None;
    }
    for start in 0..=(hay.len() - needle.len()) {
        if hay[start..start + needle.len()].iter().map(|&(_, c)| c).eq(needle.iter().copied()) {
            let begin = hay[start].0;
            let end = hay
                .get(start + needle.len())
                .map(|&(i, _)| i)
                .unwrap_or(haystack.len());
            return Some(MatchRange { start: begin, end });
        }
    }
    None
}

/// One-character case fold, matching [`crate::fuzzy`]'s approximation.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Word -> notes, plus the bodies snippets are cut from.
#[derive(Debug, Default, Clone)]
pub struct TextIndex {
    postings: BTreeMap<String, BTreeSet<NoteId>>,
    /// Terms currently attributed to each note, so an update can retract them.
    terms: BTreeMap<NoteId, Vec<String>>,
    bodies: BTreeMap<NoteId, String>,
}

impl TextIndex {
    /// An index over nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace everything known about `id`.
    ///
    /// `metadata` is the note's title/headings/tags/frontmatter text and
    /// `body` its raw source; both are tokenised into the same postings, so a
    /// query matches a note whether the word is in its title or its prose.
    pub fn upsert(&mut self, id: &NoteId, metadata: &str, body: &str) {
        self.remove(id);
        let mut words: Vec<String> = tokenize(metadata);
        words.extend(tokenize(body));
        words.sort();
        words.dedup();
        for w in &words {
            self.postings.entry(w.clone()).or_default().insert(id.clone());
        }
        self.terms.insert(id.clone(), words);
        self.bodies.insert(id.clone(), body.to_owned());
    }

    /// Forget a note entirely.
    pub fn remove(&mut self, id: &NoteId) {
        self.bodies.remove(id);
        for word in self.terms.remove(id).unwrap_or_default() {
            if let Some(set) = self.postings.get_mut(&word) {
                set.remove(id);
                if set.is_empty() {
                    self.postings.remove(&word);
                }
            }
        }
    }

    /// The stored source of a note, if one was ever supplied.
    pub fn body(&self, id: &NoteId) -> Option<&str> {
        self.bodies.get(id).map(String::as_str)
    }

    /// Number of notes with stored text.
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether the index holds nothing.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Notes containing `term`, or — when `prefix` is set — any word starting
    /// with it. Prefix mode is what makes search feel live while typing.
    pub fn notes_with(&self, term: &str, prefix: bool) -> BTreeSet<NoteId> {
        if !prefix {
            return self.postings.get(term).cloned().unwrap_or_default();
        }
        self.postings
            .range(term.to_owned()..)
            .take_while(|(w, _)| w.starts_with(term))
            .flat_map(|(_, ids)| ids.iter().cloned())
            .collect()
    }

    /// Notes containing **every** term. The last term is matched as a prefix so
    /// that a half-typed word still narrows instead of emptying the result.
    pub fn search(&self, terms: &[String]) -> BTreeSet<NoteId> {
        let Some((last, head)) = terms.split_last() else {
            return BTreeSet::new();
        };
        let mut acc = self.notes_with(last, true);
        for term in head {
            if acc.is_empty() {
                break;
            }
            let next = self.notes_with(term, false);
            acc.retain(|id| next.contains(id));
        }
        acc
    }
}

/// Cut an excerpt of `body` around the first occurrence of any of `terms`.
///
/// Returns `None` when nothing matches, so a caller can fall back to the head
/// of the note rather than showing an empty box.
pub fn snippet(body: &str, terms: &[String]) -> Option<Snippet> {
    let first = terms.iter().filter_map(|t| find_ignore_case(body, t)).min_by_key(|r| r.start)?;
    let (window, offset) = window_around(body, first.start, SNIPPET_BUDGET);

    let mut text = String::new();
    if offset > 0 {
        text.push('…');
    }
    let lead = text.len();
    text.push_str(window);
    let ranges = terms
        .iter()
        .filter_map(|t| find_ignore_case(window, t))
        .map(|r| MatchRange { start: r.start + lead, end: r.end + lead })
        .collect();
    if offset + window.len() < body.len() {
        text.push('…');
    }
    // Newlines become spaces so the excerpt renders on one line. Safe after the
    // ranges are computed: '\n' and ' ' are both one byte, so offsets hold.
    Some(Snippet { text: text.replace('\n', " "), ranges })
}

/// A `budget`-wide slice of `body` centred on `at`, snapped to char boundaries.
/// Returns the slice and its byte offset in `body`.
fn window_around(body: &str, at: usize, budget: usize) -> (&str, usize) {
    let half = budget / 2;
    let start = floor_boundary(body, at.saturating_sub(half));
    let end = ceil_boundary(body, (start + budget).min(body.len()));
    (&body[start..end], start)
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i.min(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_view::note_id;

    #[test]
    fn tokenizer_keeps_only_alphanumeric_runs_lowercased() {
        assert_eq!(tokenize("[[Nota]] — Bug #42!"), vec!["nota", "bug", "42"]);
        assert_eq!(tokenize("   "), Vec::<String>::new());
        assert_eq!(tokenize("Però"), vec!["però"]);
    }

    #[test]
    fn case_insensitive_find_returns_offsets_into_the_original() {
        let hay = "Però NOTA di lavoro";
        let r = find_ignore_case(hay, "nota").unwrap();
        assert_eq!(&hay[r.start..r.end], "NOTA");
        assert!(find_ignore_case(hay, "assente").is_none());
        assert!(find_ignore_case(hay, "").is_none());
    }

    fn seeded() -> TextIndex {
        let mut idx = TextIndex::new();
        idx.upsert(&note_id("a"), "Nota alfa", "il testo parla di sincronizzazione");
        idx.upsert(&note_id("b"), "Nota beta", "il testo parla di conflitti");
        idx
    }

    #[test]
    fn a_term_in_metadata_or_body_both_hit() {
        let idx = seeded();
        assert_eq!(idx.notes_with("alfa", false), [note_id("a")].into());
        assert_eq!(idx.notes_with("conflitti", false), [note_id("b")].into());
        assert_eq!(idx.notes_with("nota", false), [note_id("a"), note_id("b")].into());
    }

    #[test]
    fn multi_term_search_is_an_intersection_with_a_prefix_tail() {
        let idx = seeded();
        let q = vec!["testo".to_string(), "sincro".to_string()];
        assert_eq!(idx.search(&q), [note_id("a")].into());
        let q = vec!["testo".to_string(), "parla".to_string()];
        assert_eq!(idx.search(&q).len(), 2);
        assert!(idx.search(&[]).is_empty());
    }

    #[test]
    fn removing_a_note_retracts_its_postings() {
        let mut idx = seeded();
        idx.remove(&note_id("a"));
        assert!(idx.notes_with("alfa", false).is_empty());
        assert_eq!(idx.notes_with("nota", false), [note_id("b")].into());
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn upserting_the_same_note_does_not_leave_stale_terms() {
        let mut idx = seeded();
        idx.upsert(&note_id("a"), "Nota alfa", "riscritta");
        assert!(idx.notes_with("sincronizzazione", false).is_empty());
        assert_eq!(idx.notes_with("riscritta", false), [note_id("a")].into());
    }

    #[test]
    fn snippet_highlights_the_match_inside_its_own_text() {
        let body = "prima riga\nqui parliamo di conflitti di sync\nultima riga";
        let s = snippet(body, &["conflitti".to_string()]).unwrap();
        let r = s.ranges[0];
        assert_eq!(&s.text[r.start..r.end], "conflitti");
        assert!(!s.text.contains('\n'));
    }

    #[test]
    fn snippet_is_elided_on_the_side_it_cut() {
        let body = format!("{}TROVAMI{}", "x ".repeat(200), " y".repeat(200));
        let s = snippet(&body, &["trovami".to_string()]).unwrap();
        assert!(s.text.starts_with('…') && s.text.ends_with('…'));
        assert!(snippet(&body, &["assente".to_string()]).is_none());
    }
}
