//! [`Index`] — the one object the backend holds, owning every other piece.
//!
//! Lifecycle: [`Index::build`] at vault open, [`Index::upsert`] on save,
//! [`Index::remove`] on delete. Nothing here reads the disk; the caller hands
//! over notes it has already parsed, together with their source text when it has
//! it (see [`Index::upsert_with_body`]).
//!
//! **Derived state is recomputed, not patched.** The resolver, link graph, tag
//! buckets and unlinked mentions are rebuilt from the stored views after every
//! mutation. That is O(vault) per save, which at a personal vault's size is
//! nothing, and it is the only shape of this code where backlinks cannot drift
//! out of sync with forward edges. The word index *is* incremental, because it
//! holds every word of every note and re-tokenising all of it on each keystroke
//! of a save would be the one part that shows.

use std::collections::{BTreeMap, BTreeSet};

use garrulus_vault::prelude::{Note, NoteId};
use serde::{Deserialize, Serialize};

use crate::fuzzy;
use crate::graph::{Backlink, Edge, LinkGraph, Mention, Resolver, UnresolvedLink};
use crate::note_view::NoteView;
use crate::problems::{self, Problem};
use crate::query::{self, Query, SortField, SortOrder};
use crate::text::{self, MatchRange, Snippet, TextIndex};

/// Titles shorter than this are not scanned for as unlinked mentions: matching
/// a two-letter note title against every body produces noise, not findings.
const MIN_MENTION_TITLE: usize = 3;

/// One search or quick-switch result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    /// The note found.
    pub id: NoteId,
    /// Its title, so the caller can render without a second lookup.
    pub title: String,
    /// Higher is better. Only meaningful within one result list.
    pub score: i32,
    /// Byte ranges in `title` that the query matched — what the UI underlines.
    pub title_matches: Vec<MatchRange>,
    /// A body excerpt, when the query had text and the body was indexed.
    pub snippet: Option<Snippet>,
}

/// The vault index.
#[derive(Debug, Default, Clone)]
pub struct Index {
    views: BTreeMap<NoteId, NoteView>,
    text: TextIndex,
    resolver: Resolver,
    graph: LinkGraph,
    mentions: BTreeMap<NoteId, Vec<Mention>>,
    by_tag: BTreeMap<String, Vec<NoteId>>,
}

impl Index {
    /// An index over an empty vault.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from parsed notes, with no body text.
    ///
    /// Full-text search and snippets stay empty until the caller supplies
    /// sources through [`Index::build_with_bodies`] or [`Index::upsert_with_body`].
    pub fn build(notes: Vec<Note>) -> Self {
        Self::build_with_bodies(notes.into_iter().map(|n| (n, String::new())).collect())
    }

    /// Build from parsed notes paired with their raw source. The form the
    /// backend actually uses at vault open.
    pub fn build_with_bodies(notes: Vec<(Note, String)>) -> Self {
        let mut index = Self::new();
        for (note, body) in notes {
            index.stage(NoteView::from(&note), body);
        }
        index.rebuild_derived();
        index
    }

    /// Insert or replace a note, keeping any body text already stored for it.
    ///
    /// Preserving the body is what lets a metadata-only refresh (a type being
    /// applied, a rename) leave full-text search intact.
    pub fn upsert(&mut self, note: Note) {
        let view = NoteView::from(&note);
        let body = self.text.body(&view.id).unwrap_or_default().to_owned();
        self.stage(view, body);
        self.rebuild_derived();
    }

    /// Insert or replace a note together with its source text.
    pub fn upsert_with_body(&mut self, note: Note, body: impl Into<String>) {
        self.stage(NoteView::from(&note), body.into());
        self.rebuild_derived();
    }

    /// Drop a note and every trace of it.
    pub fn remove(&mut self, id: &NoteId) {
        self.views.remove(id);
        self.text.remove(id);
        self.rebuild_derived();
    }

    /// Write a view and its body into the stores, without recomputing anything.
    fn stage(&mut self, view: NoteView, body: String) {
        self.text.upsert(&view.id, &view.metadata_text(), &body);
        self.views.insert(view.id.clone(), view);
    }

    // ── Reads ───────────────────────────────────────────────────────────────

    /// Number of indexed notes.
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// Whether the vault has no indexed notes.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// The stored projection of a note.
    pub fn view(&self, id: &NoteId) -> Option<&NoteView> {
        self.views.get(id)
    }

    /// Every indexed note, in id order.
    pub fn views(&self) -> impl Iterator<Item = &NoteView> {
        self.views.values()
    }

    /// The note a written `[[target]]` points at.
    pub fn resolve(&self, target: &str) -> Option<&NoteId> {
        self.resolver.resolve(target)
    }

    /// Links written *in* `id`.
    pub fn outgoing(&self, id: &NoteId) -> &[Edge] {
        self.graph.outgoing(id)
    }

    /// Links pointing *at* `id` — the reversed forward edges, by construction.
    pub fn backlinks(&self, id: &NoteId) -> Vec<&Backlink> {
        self.graph.backlinks(id).iter().collect()
    }

    /// Notes whose text names `id`'s title without linking to it.
    pub fn unlinked_mentions(&self, id: &NoteId) -> Vec<&Mention> {
        self.mentions.get(id).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Every `[[Foo]]` in the vault with no `Foo` behind it.
    pub fn unresolved(&self) -> Vec<&UnresolvedLink> {
        self.graph.unresolved().iter().collect()
    }

    /// Notes carrying `tag`, case-insensitively.
    pub fn by_tag(&self, tag: &str) -> Vec<&NoteId> {
        self.by_tag.get(&tag.to_lowercase()).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Every tag in the vault, lowercased and sorted.
    pub fn tags(&self) -> Vec<&String> {
        self.by_tag.keys().collect()
    }

    /// Notes nothing links to.
    pub fn orphans(&self) -> Vec<&NoteId> {
        self.views.keys().filter(|id| self.graph.backlinks(id).is_empty()).collect()
    }

    /// The vault-problems report.
    pub fn problems(&self) -> Vec<Problem> {
        problems::collect(self.views.values(), &self.graph)
    }

    // ── Search ──────────────────────────────────────────────────────────────

    /// Fuzzy title search for the quick switcher.
    ///
    /// Ranked by match quality, then by title length (a shorter title that
    /// matches as well is the one you meant), then alphabetically so the list is
    /// stable between keystrokes.
    pub fn quick_switch(&self, q: &str) -> Vec<Hit> {
        let needle = q.trim();
        let mut hits: Vec<Hit> = self
            .views
            .values()
            .filter(|v| fuzzy::matches(needle, &v.title))
            .filter_map(|v| {
                fuzzy::score(needle, &v.title).map(|m| Hit {
                    id: v.id.clone(),
                    title: v.title.clone(),
                    score: m.score,
                    title_matches: char_ranges(&v.title, &m.positions),
                    snippet: None,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score.cmp(&a.score).then(a.title.len().cmp(&b.title.len())).then(a.title.cmp(&b.title))
        });
        hits
    }

    /// Full-text plus structured search.
    ///
    /// Filters narrow first (they are cheap and exact), then the free text is
    /// matched through the word index. A hit's score favours notes whose title
    /// also matches, so `sync` finds the note *called* "Sync" before the twenty
    /// notes that merely say the word.
    pub fn search(&self, query: &Query) -> Vec<Hit> {
        let terms = query.text.as_deref().map(text::tokenize).unwrap_or_default();
        let candidates = self.candidates(&terms);

        let mut hits: Vec<Hit> = candidates
            .into_iter()
            .filter_map(|id| self.views.get(&id))
            .filter(|v| query::matches_filters(v, &query.filters))
            .map(|v| self.make_hit(v, query.text.as_deref(), &terms))
            .collect();

        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.title.cmp(&b.title)));
        if let Some(sort) = &query.sort {
            self.apply_sort(&mut hits, &sort.field, sort.order);
        }
        hits
    }

    /// Notes the free text could possibly match — everything when there is none.
    fn candidates(&self, terms: &[String]) -> BTreeSet<NoteId> {
        if terms.is_empty() {
            self.views.keys().cloned().collect()
        } else {
            self.text.search(terms)
        }
    }

    fn make_hit(&self, view: &NoteView, raw_text: Option<&str>, terms: &[String]) -> Hit {
        let title_match = raw_text.and_then(|t| fuzzy::score(t, &view.title));
        let body_hit = terms.iter().any(|t| view.title.to_lowercase().contains(t.as_str()));
        Hit {
            id: view.id.clone(),
            title: view.title.clone(),
            // A title match is worth more than a body match, but a body-only
            // note still scores above zero so it never sorts below nothing.
            score: title_match.as_ref().map_or(0, |m| m.score) + if body_hit { 8 } else { 0 },
            title_matches: title_match
                .map(|m| char_ranges(&view.title, &m.positions))
                .unwrap_or_default(),
            snippet: self.text.body(&view.id).and_then(|b| text::snippet(b, terms)),
        }
    }

    /// Reorder by an explicit `sort:` term.
    ///
    /// `Modified` and `Created` are deliberately no-ops: the index holds no
    /// timestamps (it is rebuilt from note content alone), so the backend, which
    /// has the `stat` results, applies those two itself.
    fn apply_sort(&self, hits: &mut [Hit], field: &SortField, order: SortOrder) {
        match field {
            SortField::Title => hits.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
            SortField::Field(key) => hits.sort_by(|a, b| {
                let get = |h: &Hit| {
                    self.views.get(&h.id).and_then(|v| v.fields.get(key)).cloned().unwrap_or_default()
                };
                get(a).to_lowercase().cmp(&get(b).to_lowercase())
            }),
            SortField::Modified | SortField::Created => return,
        }
        if order == SortOrder::Desc {
            hits.reverse();
        }
    }

    // ── Derived state ───────────────────────────────────────────────────────

    fn rebuild_derived(&mut self) {
        self.resolver =
            Resolver::build(self.views.values().map(|v| (v.id.clone(), v.title.clone())));
        self.graph = LinkGraph::build(self.views.values(), &self.resolver);
        self.by_tag = build_tag_buckets(self.views.values());
        // Mentions are last: they read the graph to skip already-linked notes.
        let mentions = self.compute_mentions();
        self.mentions = mentions;
    }

    /// Find, for every note, the notes that name its title in prose without
    /// linking to it.
    ///
    /// The word index does the narrowing: only notes containing every word of
    /// the title are even opened, which keeps this an O(vault) pass rather than
    /// a title-by-body cross product.
    fn compute_mentions(&self) -> BTreeMap<NoteId, Vec<Mention>> {
        let mut out: BTreeMap<NoteId, Vec<Mention>> = BTreeMap::new();
        for target in self.views.values() {
            if target.title.chars().count() < MIN_MENTION_TITLE {
                continue;
            }
            let terms = text::tokenize(&target.title);
            if terms.is_empty() {
                continue;
            }
            for source in self.text.search(&terms) {
                if source == target.id || self.graph.links_to(&source, &target.id) {
                    continue;
                }
                let Some(body) = self.text.body(&source) else { continue };
                if text::find_ignore_case(body, &target.title).is_none() {
                    continue;
                }
                let Some(snippet) = text::snippet(body, &[target.title.clone()]) else { continue };
                out.entry(target.id.clone()).or_default().push(Mention {
                    from: source,
                    to: target.id.clone(),
                    snippet,
                });
            }
        }
        out
    }
}

/// Tag -> notes, lowercased, deduplicated, in note order.
fn build_tag_buckets<'a>(views: impl Iterator<Item = &'a NoteView>) -> BTreeMap<String, Vec<NoteId>> {
    let mut out: BTreeMap<String, Vec<NoteId>> = BTreeMap::new();
    for view in views {
        for tag in &view.tags {
            let bucket = out.entry(tag.to_lowercase()).or_default();
            if bucket.last() != Some(&view.id) {
                bucket.push(view.id.clone());
            }
        }
    }
    out
}

/// Turn per-character match offsets into merged byte ranges, so adjacent
/// matched characters render as one underline instead of five.
fn char_ranges(haystack: &str, positions: &[usize]) -> Vec<MatchRange> {
    let mut ranges: Vec<MatchRange> = Vec::new();
    for &start in positions {
        let end = next_boundary(haystack, start);
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(MatchRange { start, end }),
        }
    }
    ranges
}

/// Byte offset just past the character starting at `start`.
fn next_boundary(s: &str, start: usize) -> usize {
    s[start..].chars().next().map_or(start, |c| start + c.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_view::{note_id, type_id, LinkRef};
    use crate::query::parse_query;

    /// The tests drive the index through `NoteView`s directly: constructing a
    /// `garrulus_vault::Note` would pull the whole vault crate into unit tests
    /// that are about ranking and graph shape, not about parsing.
    fn seeded() -> Index {
        let mut index = Index::new();
        let mut sync = NoteView::new(note_id("sync"), "Sync");
        sync.tags = vec!["Infra".into()];
        sync.kind = Some(type_id("nota"));

        let mut bug = NoteView::new(note_id("bug"), "Bug di sincronizzazione");
        bug.tags = vec!["infra".into(), "bug".into()];
        bug.kind = Some(type_id("bug"));
        bug.fields.insert("stato".into(), "aperto".into());
        bug.links = vec![LinkRef::plain("Sync"), LinkRef::plain("Fantasma")];

        let diario = NoteView::new(note_id("diario"), "Diario");

        index.stage(sync, "il tema qui e la sincronizzazione fra i due PC".into());
        index.stage(bug, "quando il push fallisce".into());
        index.stage(diario, "oggi ho sistemato Sync a mano".into());
        index.rebuild_derived();
        index
    }

    #[test]
    fn quick_switch_ranks_the_tighter_title_first() {
        let ids: Vec<String> =
            seeded().quick_switch("sync").iter().map(|h| h.title.clone()).collect();
        assert_eq!(ids, vec!["Sync"]);

        let ids: Vec<String> = seeded().quick_switch("s").iter().map(|h| h.title.clone()).collect();
        assert_eq!(ids.first().map(String::as_str), Some("Sync"));
    }

    #[test]
    fn quick_switch_with_an_empty_query_lists_the_whole_vault() {
        assert_eq!(seeded().quick_switch("").len(), 3);
    }

    #[test]
    fn quick_switch_highlights_merge_adjacent_characters() {
        let hits = seeded().quick_switch("bu");
        let ranges = &hits[0].title_matches;
        assert_eq!(ranges, &[MatchRange { start: 0, end: 2 }]);
    }

    #[test]
    fn backlinks_are_the_reverse_of_the_outgoing_edges() {
        let index = seeded();
        assert_eq!(index.outgoing(&note_id("bug")).len(), 1);
        let back = index.backlinks(&note_id("sync"));
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].from, note_id("bug"));
        assert!(index.backlinks(&note_id("bug")).is_empty());
    }

    #[test]
    fn a_dangling_target_lands_in_unresolved() {
        let index = seeded();
        assert_eq!(index.unresolved().len(), 1);
        assert_eq!(index.unresolved()[0].target, "Fantasma");
    }

    #[test]
    fn an_unlinked_mention_is_found_and_a_linked_one_is_not() {
        let index = seeded();
        let mentions = index.unlinked_mentions(&note_id("sync"));
        // "diario" says "Sync" without linking; "bug" links, so it is excluded.
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].from, note_id("diario"));
    }

    #[test]
    fn tags_bucket_case_insensitively() {
        let index = seeded();
        assert_eq!(index.by_tag("INFRA").len(), 2);
        assert_eq!(index.by_tag("bug"), vec![&note_id("bug")]);
        assert!(index.by_tag("assente").is_empty());
        assert_eq!(index.tags(), vec!["bug", "infra"]);
    }

    #[test]
    fn orphans_are_the_notes_nothing_points_at() {
        let orphans: Vec<NoteId> = seeded().orphans().into_iter().cloned().collect();
        assert_eq!(orphans, vec![note_id("bug"), note_id("diario")]);
    }

    #[test]
    fn search_applies_filters_and_free_text_together() {
        let index = seeded();
        let hits = index.search(&parse_query("type:bug"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, note_id("bug"));

        let hits = index.search(&parse_query("type:bug stato:chiuso"));
        assert!(hits.is_empty());

        let hits = index.search(&parse_query("sincronizzazione"));
        let found: BTreeSet<NoteId> = hits.iter().map(|h| h.id.clone()).collect();
        assert_eq!(found, [note_id("bug"), note_id("sync")].into());
    }

    #[test]
    fn a_title_match_outranks_a_body_only_match() {
        let hits = seeded().search(&parse_query("sync"));
        assert_eq!(hits[0].id, note_id("sync"));
    }

    #[test]
    fn a_body_hit_carries_a_snippet() {
        let hits = seeded().search(&parse_query("push"));
        assert_eq!(hits.len(), 1);
        let s = hits[0].snippet.as_ref().expect("body hit should have a snippet");
        assert!(s.text.contains("push"));
    }

    #[test]
    fn sort_title_overrides_the_score_ordering() {
        let hits = seeded().search(&parse_query("sort:-title"));
        let titles: Vec<String> = hits.iter().map(|h| h.title.clone()).collect();
        assert_eq!(titles, vec!["Sync", "Diario", "Bug di sincronizzazione"]);
    }

    #[test]
    fn removing_a_note_removes_its_edges_too() {
        let mut index = seeded();
        index.remove(&note_id("bug"));
        assert_eq!(index.len(), 2);
        assert!(index.backlinks(&note_id("sync")).is_empty());
        assert!(index.unresolved().is_empty());
    }

    #[test]
    fn problems_surface_through_the_index() {
        let found = seeded().problems();
        assert!(found.iter().any(|p| matches!(p, Problem::BrokenLink { .. })));
        assert!(found.iter().any(|p| matches!(p, Problem::Untyped { note } if note == &note_id("diario"))));
    }
}
