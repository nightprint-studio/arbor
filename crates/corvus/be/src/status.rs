//! `status` domain — the single workdir-status query, served **out-of-process**.
//!
//! Byte-identical to the shell's in-process `get_status`: reads the shell-pushed
//! `status.detect_renames` tuning (via [`status_detect_renames`](crate::repo::status_detect_renames))
//! and runs the pure `corvus-git` scan on the repo opened by the pushed path. The
//! in-process copy read the config *before* taking the repos lock to avoid
//! nesting the two mutexes; here the config is a separate pushed `Value` and
//! `open` is independent, so there is no nesting to avoid. No hooks fire.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::RepoStatus;

use crate::repo::{open, status_detect_renames};

#[arbor_rpc::handler]
fn get_status(state: &CorvusState, tab_id: String) -> Result<RepoStatus, String> {
    let detect_renames = status_detect_renames(state);
    let repo = open(state, &tab_id)?;
    corvus_git::status::get_status_with(&repo, detect_renames).map_err(|e| e.to_string())
}
