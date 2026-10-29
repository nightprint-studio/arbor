//! File reservations: two conversations running at once must not edit the same file.
//!
//! The registry itself is pure and lives in `bennu-assistant` (`Reservations`); this is the process-wide
//! instance and the one door the write tools go through. A run reserves a file the first time it changes
//! it and holds it until the run ends (`release_run`, called by `execute` even if the run panics); a
//! conflict fails at once with the holder named, so the model can wait or work on something else.
//! Reads never come here. A proposal waiting for the user is not a reservation — it only leaves the soft
//! note `runs::pending_elsewhere` already gives.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Instant;

use bennu_assistant::prelude::Reservations;

use super::ToolHost;

static RESERVATIONS: LazyLock<Reservations> = LazyLock::new(Reservations::default);

/// A run ended (finished, cancelled, crashed): everything it held is free again.
pub(in crate::assistant) fn release_run(run_id: &str) {
    RESERVATIONS.release_run(run_id);
}

/// Who holds `file` other than `except` (a conversation id), if anyone — for `file_owner` and `request_change`.
pub(in crate::assistant) fn holder_of(file: &str, except: &str) -> Option<bennu_assistant::prelude::ReservationConflict> {
    RESERVATIONS.holder(file, except, Instant::now())
}

/// A conversation was deleted: so is whatever it held.
pub(in crate::assistant) fn release_conversation(conversation: &str) {
    RESERVATIONS.release_conversation(conversation);
}

/// Reserve `files` for a caller that is not a tool call (applying an isolated copy back to the project).
/// All or nothing, like the tools.
pub(in crate::assistant) fn reserve_for(run_key: &str, conversation: &str, title: &str, files: &[PathBuf]) -> Result<(), String> {
    let list: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
    RESERVATIONS.reserve(run_key, conversation, title, &list, Instant::now()).map_err(|c| c.to_string())?;
    // Applying a copy writes these files: that is a change by this conversation.
    list.iter().for_each(|f| {
        crate::build_queue::journal().record(f, conversation, title);
    });
    Ok(())
}

impl ToolHost {
    /// Reserve every file in `files` for this run, or none of them. Idempotent for files this conversation
    /// already holds, so it is safe to call per file and per set.
    pub(super) fn reserve_files(&self, files: &[&Path]) -> Result<(), String> {
        let list: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
        let s = &self.session;
        RESERVATIONS.reserve(&s.run_id, s.owner_conversation(), &s.title, &list, Instant::now()).map_err(|c| c.to_string())?;
        // Who wrote what: a failed build is read against this to point at the conversation that changed a file.
        // Proposals are not written yet, so only a mode that applies edits records.
        if s.applies_edits() {
            list.iter().for_each(|f| {
                crate::build_queue::journal().record(f, s.owner_conversation(), &s.title);
            });
        }
        Ok(())
    }
}
