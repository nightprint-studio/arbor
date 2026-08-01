//! The link graph: forward edges, their reversal (backlinks), the links that
//! resolve to nothing, and unlinked mentions.
//!
//! Two invariants this module exists to guarantee:
//!
//! 1. **Backlinks are exactly the reversed forward edges.** There is one pass
//!    that fills both directions, so the two can never drift.
//! 2. **An unresolved link is data, not an error.** In Obsidian a `[[Foo]]`
//!    with no `Foo` is how you create a note; the graph reports them as a
//!    first-class list rather than dropping them (docs/garrulus-design.md §5.2).

use std::collections::BTreeMap;

use garrulus_vault::prelude::NoteId;
use serde::{Deserialize, Serialize};

use crate::note_view::NoteView;
use crate::text::Snippet;

/// Normalise a wikilink target or a note title into the key both sides agree on.
///
/// Trims, drops a trailing `.md` and lowercases: `[[nota.md]]`, `[[Nota]]` and
/// `[[ NOTA ]]` all land on the same note.
pub fn link_key(raw: &str) -> String {
    let trimmed = raw.trim();
    let stem = trimmed.strip_suffix(".md").unwrap_or(trimmed);
    stem.trim().to_lowercase()
}

/// The last path segment of a link key — `progetti/arbor` also answers to `arbor`.
pub fn leaf_key(raw: &str) -> String {
    let key = link_key(raw);
    match key.rsplit_once(['/', '\\']) {
        Some((_, leaf)) => leaf.to_owned(),
        None => key,
    }
}

/// Maps a written link target onto the note it means.
#[derive(Debug, Default, Clone)]
pub struct Resolver {
    by_key: BTreeMap<String, NoteId>,
}

impl Resolver {
    /// Build from `(id, title)` pairs.
    ///
    /// Each note registers both its full key and its leaf key; the first note to
    /// claim a key keeps it, so a duplicate title resolves deterministically
    /// (and gets reported by [`crate::problems`] instead of silently winning).
    pub fn build(notes: impl IntoIterator<Item = (NoteId, String)>) -> Self {
        let mut by_key: BTreeMap<String, NoteId> = BTreeMap::new();
        for (id, title) in notes {
            for key in [link_key(&title), leaf_key(&title)] {
                if key.is_empty() {
                    continue;
                }
                by_key.entry(key).or_insert_with(|| id.clone());
            }
        }
        Self { by_key }
    }

    /// The note a written target points at, if it exists.
    pub fn resolve(&self, target: &str) -> Option<&NoteId> {
        self.by_key.get(&link_key(target)).or_else(|| self.by_key.get(&leaf_key(target)))
    }

    /// Number of distinct keys registered.
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Whether nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// A resolved link from one note to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// The note the link was written in.
    pub from: NoteId,
    /// The note it points at.
    pub to: NoteId,
    /// The `#heading` fragment, if any.
    pub heading: Option<String>,
    /// The `|alias` the link was displayed as, if any.
    pub alias: Option<String>,
    /// `true` for `![[embed]]` transclusions.
    pub embed: bool,
}

/// The same edge seen from the destination. Field-for-field identical to
/// [`Edge`] on purpose: it *is* the reversed edge, and a separate type only
/// keeps the two directions from being confused at call sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backlink {
    /// The note that links here.
    pub from: NoteId,
    /// The note being linked (the one whose panel this appears in).
    pub to: NoteId,
    /// The `#heading` fragment, if any.
    pub heading: Option<String>,
    /// The `|alias` the link was displayed as, if any.
    pub alias: Option<String>,
    /// `true` for `![[embed]]` transclusions.
    pub embed: bool,
}

impl From<&Edge> for Backlink {
    fn from(e: &Edge) -> Self {
        Self {
            from: e.from.clone(),
            to: e.to.clone(),
            heading: e.heading.clone(),
            alias: e.alias.clone(),
            embed: e.embed,
        }
    }
}

/// A `[[Foo]]` with no `Foo` — an offer to create the note, not a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedLink {
    /// The note holding the dangling link.
    pub from: NoteId,
    /// The target exactly as written.
    pub target: String,
    /// The `#heading` fragment, if any.
    pub heading: Option<String>,
    /// `true` for `![[embed]]` transclusions.
    pub embed: bool,
}

/// A note whose text names another note's title without linking to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mention {
    /// The note the mention was found in.
    pub from: NoteId,
    /// The note being named.
    pub to: NoteId,
    /// The sentence it was found in, for the "menzioni non collegate" panel.
    pub snippet: Snippet,
}

/// Forward edges, backlinks and unresolved links over the whole vault.
#[derive(Debug, Default, Clone)]
pub struct LinkGraph {
    forward: BTreeMap<NoteId, Vec<Edge>>,
    backward: BTreeMap<NoteId, Vec<Backlink>>,
    unresolved: Vec<UnresolvedLink>,
}

impl LinkGraph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk every note's links once, filling both directions and collecting the
    /// dangling ones.
    ///
    /// Rebuilt wholesale rather than patched incrementally: at vault scale this
    /// is microseconds, and "recompute from the views" is the only version of
    /// this code that cannot get the two directions out of sync.
    pub fn build<'a, I>(views: I, resolver: &Resolver) -> Self
    where
        I: IntoIterator<Item = &'a NoteView>,
    {
        let mut graph = Self::new();
        for view in views {
            for link in &view.links {
                match resolver.resolve(&link.target) {
                    Some(to) => graph.insert_edge(Edge {
                        from: view.id.clone(),
                        to: to.clone(),
                        heading: link.heading.clone(),
                        alias: link.alias.clone(),
                        embed: link.embed,
                    }),
                    None => graph.unresolved.push(UnresolvedLink {
                        from: view.id.clone(),
                        target: link.target.clone(),
                        heading: link.heading.clone(),
                        embed: link.embed,
                    }),
                }
            }
        }
        graph
    }

    /// Record an edge in both directions. The only writer of either map.
    fn insert_edge(&mut self, edge: Edge) {
        self.backward.entry(edge.to.clone()).or_default().push(Backlink::from(&edge));
        self.forward.entry(edge.from.clone()).or_default().push(edge);
    }

    /// Links written in `id`.
    pub fn outgoing(&self, id: &NoteId) -> &[Edge] {
        self.forward.get(id).map_or(&[], Vec::as_slice)
    }

    /// Links pointing at `id`.
    pub fn backlinks(&self, id: &NoteId) -> &[Backlink] {
        self.backward.get(id).map_or(&[], Vec::as_slice)
    }

    /// Whether `from` already links to `to`, in any form. Used to keep an
    /// already-linked note out of the unlinked-mentions list.
    pub fn links_to(&self, from: &NoteId, to: &NoteId) -> bool {
        self.outgoing(from).iter().any(|e| &e.to == to)
    }

    /// Every dangling link in the vault.
    pub fn unresolved(&self) -> &[UnresolvedLink] {
        &self.unresolved
    }

    /// Every resolved edge, in note order.
    pub fn edges(&self) -> impl Iterator<Item = &Edge> {
        self.forward.values().flatten()
    }

    /// Total number of resolved edges.
    pub fn edge_count(&self) -> usize {
        self.forward.values().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_view::{note_id, LinkRef, NoteView};

    fn note(id: &str, title: &str, links: &[&str]) -> NoteView {
        let mut v = NoteView::new(note_id(id), title);
        v.links = links.iter().map(|t| LinkRef::plain(*t)).collect();
        v
    }

    fn vault() -> Vec<NoteView> {
        vec![
            note("a", "Alfa", &["Beta", "Gamma", "Fantasma"]),
            note("b", "Beta", &["Alfa"]),
            note("c", "Gamma", &[]),
        ]
    }

    fn built() -> LinkGraph {
        let views = vault();
        let resolver = Resolver::build(views.iter().map(|v| (v.id.clone(), v.title.clone())));
        LinkGraph::build(&views, &resolver)
    }

    #[test]
    fn link_keys_ignore_case_extension_and_padding() {
        assert_eq!(link_key("  Nota.md "), "nota");
        assert_eq!(link_key("Progetti/Arbor"), "progetti/arbor");
        assert_eq!(leaf_key("Progetti/Arbor.md"), "arbor");
        assert_eq!(leaf_key("Arbor"), "arbor");
    }

    #[test]
    fn a_target_resolves_by_full_key_or_by_leaf() {
        let views = vec![note("a", "Progetti/Arbor", &[])];
        let r = Resolver::build(views.iter().map(|v| (v.id.clone(), v.title.clone())));
        assert_eq!(r.resolve("progetti/arbor"), Some(&note_id("a")));
        assert_eq!(r.resolve("Arbor"), Some(&note_id("a")));
        assert_eq!(r.resolve("Altro"), None);
    }

    #[test]
    fn backlinks_are_exactly_the_reversed_forward_edges() {
        let g = built();

        let mut from_forward: Vec<Backlink> = g.edges().map(Backlink::from).collect();
        let mut from_backward: Vec<Backlink> =
            g.backward.values().flatten().cloned().collect();
        from_forward.sort_by(|x, y| (&x.from, &x.to).cmp(&(&y.from, &y.to)));
        from_backward.sort_by(|x, y| (&x.from, &x.to).cmp(&(&y.from, &y.to)));

        assert_eq!(from_forward, from_backward);
        assert_eq!(g.edge_count(), 3);
    }

    #[test]
    fn every_edge_appears_in_its_destinations_backlinks() {
        let g = built();
        for edge in g.edges() {
            assert!(
                g.backlinks(&edge.to).iter().any(|b| b.from == edge.from),
                "{:?} -> {:?} missing from backlinks",
                edge.from,
                edge.to
            );
        }
        assert_eq!(g.backlinks(&note_id("a")).len(), 1);
        assert_eq!(g.backlinks(&note_id("c")).len(), 1);
        assert!(g.backlinks(&note_id("mai-esistito")).is_empty());
    }

    #[test]
    fn a_link_with_no_destination_is_reported_not_dropped() {
        let g = built();
        assert_eq!(g.unresolved().len(), 1);
        assert_eq!(g.unresolved()[0].target, "Fantasma");
        assert_eq!(g.unresolved()[0].from, note_id("a"));
    }

    #[test]
    fn links_to_sees_a_written_edge() {
        let g = built();
        assert!(g.links_to(&note_id("a"), &note_id("b")));
        assert!(!g.links_to(&note_id("c"), &note_id("a")));
    }
}
