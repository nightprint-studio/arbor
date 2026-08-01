//! The vault-problems report: what the Problems panel lists.
//!
//! Everything here is an *observation*, never a repair — nothing in this crate
//! writes. And nothing here is fatal: a vault full of problems is a normal
//! vault, which is why this is a `Vec<Problem>` and not an error type.

use std::collections::BTreeMap;

use garrulus_vault::prelude::NoteId;
use serde::{Deserialize, Serialize};

use crate::graph::{link_key, LinkGraph};
use crate::note_view::NoteView;

/// How much attention a problem deserves in the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Something is broken and the user probably meant otherwise.
    Warning,
    /// Worth knowing, not worth fixing today.
    Hint,
}

/// One finding about the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Problem {
    /// A `[[target]]` that resolves to no note.
    BrokenLink {
        /// The note holding the link.
        note: NoteId,
        /// The target as written.
        target: String,
    },
    /// A note nothing links to.
    Orphan {
        /// The unreachable note.
        note: NoteId,
    },
    /// A note that matched no note type.
    Untyped {
        /// The unclassified note.
        note: NoteId,
    },
    /// A note with no usable title, which no wikilink can ever reach.
    EmptyTitle {
        /// The nameless note.
        note: NoteId,
    },
    /// Several notes competing for the same wikilink target.
    DuplicateTitle {
        /// The contested title, normalised.
        title: String,
        /// Every note claiming it, in id order.
        notes: Vec<NoteId>,
    },
}

impl Problem {
    /// How loudly the panel should show this.
    pub fn severity(&self) -> Severity {
        match self {
            Problem::BrokenLink { .. } | Problem::EmptyTitle { .. } | Problem::DuplicateTitle { .. } => {
                Severity::Warning
            }
            Problem::Orphan { .. } | Problem::Untyped { .. } => Severity::Hint,
        }
    }
}

/// Collect every problem in the vault, warnings first, then in a stable order
/// so the panel does not reshuffle itself on every rebuild.
pub fn collect<'a, I>(views: I, graph: &LinkGraph) -> Vec<Problem>
where
    I: IntoIterator<Item = &'a NoteView>,
{
    let views: Vec<&NoteView> = views.into_iter().collect();

    let mut out: Vec<Problem> = graph
        .unresolved()
        .iter()
        .map(|u| Problem::BrokenLink { note: u.from.clone(), target: u.target.clone() })
        .collect();

    for view in &views {
        if view.title.trim().is_empty() {
            out.push(Problem::EmptyTitle { note: view.id.clone() });
        }
        if view.kind.is_none() {
            out.push(Problem::Untyped { note: view.id.clone() });
        }
        if graph.backlinks(&view.id).is_empty() {
            out.push(Problem::Orphan { note: view.id.clone() });
        }
    }
    out.extend(duplicate_titles(&views));

    out.sort_by_key(|p| (p.severity(), discriminant_rank(p)));
    out
}

/// Notes sharing a normalised title: only the first one a `[[link]]` names can
/// ever win, so the rest are silently unreachable — worth saying out loud.
fn duplicate_titles(views: &[&NoteView]) -> Vec<Problem> {
    let mut buckets: BTreeMap<String, Vec<NoteId>> = BTreeMap::new();
    for view in views {
        let key = link_key(&view.title);
        if !key.is_empty() {
            buckets.entry(key).or_default().push(view.id.clone());
        }
    }
    buckets
        .into_iter()
        .filter(|(_, notes)| notes.len() > 1)
        .map(|(title, mut notes)| {
            notes.sort();
            Problem::DuplicateTitle { title, notes }
        })
        .collect()
}

/// Stable secondary sort key: group problems of the same kind together.
fn discriminant_rank(p: &Problem) -> u8 {
    match p {
        Problem::BrokenLink { .. } => 0,
        Problem::DuplicateTitle { .. } => 1,
        Problem::EmptyTitle { .. } => 2,
        Problem::Orphan { .. } => 3,
        Problem::Untyped { .. } => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Resolver;
    use crate::note_view::{note_id, type_id, LinkRef, NoteView};

    fn vault() -> Vec<NoteView> {
        let mut alfa = NoteView::new(note_id("a"), "Alfa");
        alfa.kind = Some(type_id("bug"));
        alfa.links = vec![LinkRef::plain("Beta"), LinkRef::plain("Fantasma")];

        let beta = NoteView::new(note_id("b"), "Beta");
        let doppia = NoteView::new(note_id("c"), "beta");
        let senza_titolo = NoteView::new(note_id("d"), "  ");

        vec![alfa, beta, doppia, senza_titolo]
    }

    fn collected() -> Vec<Problem> {
        let views = vault();
        let resolver = Resolver::build(views.iter().map(|v| (v.id.clone(), v.title.clone())));
        let graph = LinkGraph::build(&views, &resolver);
        collect(&views, &graph)
    }

    #[test]
    fn a_dangling_link_is_reported_as_broken() {
        let p = collected();
        assert!(p.contains(&Problem::BrokenLink {
            note: note_id("a"),
            target: "Fantasma".into()
        }));
    }

    #[test]
    fn notes_with_no_backlinks_are_orphans_and_linked_ones_are_not() {
        let p = collected();
        assert!(p.contains(&Problem::Orphan { note: note_id("a") }));
        assert!(!p.contains(&Problem::Orphan { note: note_id("b") }));
    }

    #[test]
    fn untyped_and_untitled_notes_are_reported() {
        let p = collected();
        assert!(!p.contains(&Problem::Untyped { note: note_id("a") }));
        assert!(p.contains(&Problem::Untyped { note: note_id("b") }));
        assert!(p.contains(&Problem::EmptyTitle { note: note_id("d") }));
    }

    #[test]
    fn titles_colliding_after_normalisation_are_one_finding() {
        let p = collected();
        assert!(p.contains(&Problem::DuplicateTitle {
            title: "beta".into(),
            notes: vec![note_id("b"), note_id("c")],
        }));
    }

    #[test]
    fn warnings_sort_before_hints() {
        let p = collected();
        let first_hint = p.iter().position(|x| x.severity() == Severity::Hint);
        let last_warning = p.iter().rposition(|x| x.severity() == Severity::Warning);
        if let (Some(h), Some(w)) = (first_hint, last_warning) {
            assert!(w < h, "a hint sorted before a warning");
        }
    }
}
