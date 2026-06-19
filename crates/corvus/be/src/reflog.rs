//! `reflog` domain — served **out-of-process** by corvus-be.
//!
//! Same handler (function name → method name) as the shell's in-process copy
//! (`crate::ipc::corvus::reflog`), but the context is [`CorvusState`]: the repo
//! comes from the shell-pushed registration. The pure HEAD-reflog projection is
//! the shared [`corvus_git::reflog`] crate, so the [`ReflogEntry`] shape and the
//! error strings are byte-identical to in-process.
//!
//! Read-only domain — **no hooks fire here**.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{GitRepo, ReflogEntry};

use crate::repo::repo_path;

#[arbor_rpc::handler]
fn get_reflog(
    state: &CorvusState,
    tab_id: String,
    limit: Option<usize>,
) -> Result<Vec<ReflogEntry>, String> {
    let repo = GitRepo::open(&repo_path(state, &tab_id)?).map_err(|e| e.to_string())?;
    corvus_git::reflog::get_reflog(&repo, limit).map_err(|e| e.to_string())
}
