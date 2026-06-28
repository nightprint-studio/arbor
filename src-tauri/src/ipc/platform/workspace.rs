//! `workspace` platform backend — only the one-shot migration report now.
//!
//! The repo registry + workspace store + tab snapshots moved out-of-process to
//! corvus-be (ADR-1: each backend owns its own `repo_registry` + `workspaces`).
//! Every workspace query / mutation / background runner is now an
//! `#[arbor_rpc::handler]` in `corvus-be` (`crate::workspace::*` over there),
//! routed on the `corvus` program; the frontend calls them via `corvus(...)`.
//!
//! What stays here: `take_migration_report`. The legacy on-disk migration runs
//! in the shell's `AppState::new` (before corvus-be exists), so its report is a
//! shell read — it never had a corvus-be twin.

use crate::error::AppError;
use crate::ipc::platform;
use crate::workspace::migration;
use crate::AppState;

#[platform::handler(program = "platform")]
fn take_migration_report(state: &AppState) -> Result<Option<migration::MigrationReport>, AppError> {
    let mut slot = state
        .migration_report
        .lock()
        .map_err(|_| AppError::MutexPoisoned("migration_report".into()))?;
    Ok(slot.take())
}
