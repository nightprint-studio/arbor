//! `search` domain — served **out-of-process** by corvus-be.
//!
//! Same handler as the shell's in-process copy (`crate::ipc::corvus::search`),
//! but the context is [`CorvusState`]: the repo is opened by the shell-pushed
//! path. The revwalk is the shared [`corvus_git::search`] crate, so the matched
//! commits, the result shape, and the errors are identical. Read-only — fires no
//! hooks. (The brief-lock-then-reopen the in-process copy does is moot here:
//! there is no `RepoManager` lock, the handler opens its own repo.)

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{SearchQuery, SearchResult};

use crate::repo::open;

#[arbor_rpc::handler]
fn search_commits(
    state: &CorvusState,
    tab_id: String,
    query: SearchQuery,
) -> Result<Vec<SearchResult>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::search::search_commits(&repo, &query).map_err(|e| e.to_string())
}
