//! `search` domain — everything the index answers.
//!
//! All four handlers are reads over the in-memory index, so they take the read
//! guard and nothing else; none of them touches the disk. The index being a cache
//! is what makes that safe: a stale answer here is a stale answer, never a wrong
//! write.

use garrulus_core::prelude::{
    parse_query, Backlink, GarrulusState, Hit, Mention, NoteId, UnresolvedLink,
};
use serde::Serialize;

use crate::vault_io;

/// How many quick-switcher hits are worth sending when the caller does not say.
/// The switcher shows a scrolling list, not a report — past this the ranking is
/// what matters, not the tail.
const QUICK_SWITCH_LIMIT: usize = 50;

/// A note's incoming edges: the links that point at it, and the places its title
/// appears as plain text without one.
#[derive(Debug, Clone, Serialize)]
pub struct Backlinks {
    /// Vault-relative path the report is about.
    pub path: String,
    /// Notes linking here.
    pub backlinks: Vec<Backlink>,
    /// Notes mentioning the title without linking — one click from becoming a
    /// link, which is the whole point of surfacing them.
    pub unlinked_mentions: Vec<Mention>,
}

/// What is wrong with the vault, as far as the link graph can tell.
#[derive(Debug, Clone, Serialize)]
pub struct VaultProblems {
    /// `[[Foo]]` with no `Foo`. First-class rather than an error: in this dialect
    /// an unresolved link is how a note gets created.
    pub unresolved: Vec<UnresolvedLink>,
    /// Notes nothing links to and which link nowhere.
    pub orphans: Vec<NoteId>,
}

/// Full-text + structured search: `type:bug stato:aperto testo libero`.
///
/// The query string is parsed by the index (the syntax is its contract, tested
/// there); this handler only decides that an empty query means an empty result
/// rather than the whole vault.
#[arbor_rpc::handler]
fn garrulus_search(state: &GarrulusState, query: String) -> Result<Vec<Hit>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed = parse_query(&query);
    Ok(state.index_read()?.search(&parsed))
}

/// Fuzzy title match for the quick switcher (`Ctrl+O`).
#[arbor_rpc::handler]
fn garrulus_quick_switch(
    state: &GarrulusState,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<Hit>, String> {
    let mut hits = state.index_read()?.quick_switch(&query);
    hits.truncate(limit.unwrap_or(QUICK_SWITCH_LIMIT).max(1));
    Ok(hits)
}

/// Backlinks + unlinked mentions for one note.
#[arbor_rpc::handler]
fn garrulus_backlinks(state: &GarrulusState, path: String) -> Result<Backlinks, String> {
    // The note's id comes from the note itself (it may be a frontmatter uid), so
    // the vault is read first — and its guard dropped — before the index is.
    let note = vault_io::with_vault(state, |v| vault_io::load_note(v, &path))?;
    let index = state.index_read()?;
    Ok(Backlinks {
        path,
        backlinks: index.backlinks(&note.id).into_iter().cloned().collect(),
        unlinked_mentions: index.unlinked_mentions(&note.id).into_iter().cloned().collect(),
    })
}

/// The vault's link-graph problems, for the Problems panel.
#[arbor_rpc::handler]
fn garrulus_problems(state: &GarrulusState) -> Result<VaultProblems, String> {
    let index = state.index_read()?;
    Ok(VaultProblems {
        unresolved: index.unresolved().into_iter().cloned().collect(),
        orphans:    index.orphans().into_iter().cloned().collect(),
    })
}
