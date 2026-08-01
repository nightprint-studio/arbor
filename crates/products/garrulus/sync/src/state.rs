//! [`SyncState`] — what the one control in the title bar shows.
//!
//! There is exactly one place sync state is displayed
//! (`docs/garrulus-design.md` §4.3), so there is exactly one type describing it,
//! and the rule that turns raw counts into it is a pure function with a test
//! rather than a chain of `if`s inside a handler.

use serde::{Deserialize, Serialize};

/// The state of the vault against its remote.
///
/// Serialised externally tagged (`"synced"`, `{"has-changes": 3}`,
/// `{"diverged": {"ahead": 3, "behind": 2}}`) so the frontend can match on it
/// without a discriminant field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncState {
    /// Everything here is there, and vice versa.
    Synced,
    /// `n` notes are edited locally and not yet sent.
    HasChanges(u32),
    /// `n` notes are waiting to come in.
    Behind(u32),
    /// `n` local changes are committed but not pushed.
    Ahead(u32),
    /// Both directions have moved.
    Diverged {
        /// Units to send.
        ahead: u32,
        /// Units to receive.
        behind: u32,
    },
    /// `n` conflicts are sitting in the vault waiting to be resolved.
    Conflict(u32),
    /// The remote is configured but not reachable right now.
    Offline,
    /// No remote has been configured for this vault.
    NoRemote,
}

impl SyncState {
    /// Does this state need the user to do something?
    ///
    /// Drives whether the button reads as accent/warning/danger rather than
    /// muted — colour is state here, never decoration (CLAUDE.md).
    pub fn needs_action(&self) -> bool {
        !matches!(self, SyncState::Synced | SyncState::Offline)
    }

    /// A stable kebab-case tag for the frontend's icon/colour lookup.
    pub fn tag(&self) -> &'static str {
        match self {
            SyncState::Synced => "synced",
            SyncState::HasChanges(_) => "has-changes",
            SyncState::Behind(_) => "behind",
            SyncState::Ahead(_) => "ahead",
            SyncState::Diverged { .. } => "diverged",
            SyncState::Conflict(_) => "conflict",
            SyncState::Offline => "offline",
            SyncState::NoRemote => "no-remote",
        }
    }
}

/// The raw counts a probe collects, before they mean anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateInputs {
    /// Is a remote configured at all?
    pub has_remote: bool,
    /// Did the probe reach it?
    pub reachable: bool,
    /// Notes edited locally and not yet committed/mirrored.
    pub dirty_notes: u32,
    /// Conflict side files sitting in the vault.
    pub conflicts: u32,
    /// Local commits the remote does not have.
    pub ahead_commits: u32,
    /// Remote commits this machine does not have.
    pub behind_commits: u32,
}

impl StateInputs {
    /// Everything waiting to leave this machine, committed or not.
    ///
    /// The user does not distinguish "edited" from "committed but not pushed" —
    /// both are *note che devo mandare* — so the button does not either.
    pub fn outgoing(&self) -> u32 {
        self.dirty_notes + self.ahead_commits
    }
}

/// Turn counts into the state the button shows.
///
/// Precedence is deliberate and is the whole of the rule: an unresolved
/// conflict outranks everything (it is the only state that can lose text if
/// ignored), a missing remote outranks unreachability, and unreachability
/// outranks counts that a fetch could not have refreshed anyway.
pub fn classify(i: StateInputs) -> SyncState {
    if !i.has_remote {
        return SyncState::NoRemote;
    }
    if i.conflicts > 0 {
        return SyncState::Conflict(i.conflicts);
    }
    if !i.reachable {
        return SyncState::Offline;
    }
    let outgoing = i.outgoing();
    match (outgoing, i.behind_commits) {
        (0, 0) => SyncState::Synced,
        (0, behind) => SyncState::Behind(behind),
        (out, 0) if i.dirty_notes > 0 => SyncState::HasChanges(out),
        (out, 0) => SyncState::Ahead(out),
        (ahead, behind) => SyncState::Diverged { ahead, behind },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn online() -> StateInputs {
        StateInputs { has_remote: true, reachable: true, ..Default::default() }
    }

    #[test]
    fn no_remote_beats_everything() {
        let i = StateInputs { conflicts: 3, dirty_notes: 2, ..Default::default() };
        assert_eq!(classify(i), SyncState::NoRemote);
    }

    #[test]
    fn a_conflict_beats_being_offline() {
        let i = StateInputs { has_remote: true, reachable: false, conflicts: 1, ..online() };
        assert_eq!(classify(i), SyncState::Conflict(1));
    }

    #[test]
    fn offline_hides_stale_counts() {
        let i = StateInputs { reachable: false, dirty_notes: 4, ..online() };
        assert_eq!(classify(i), SyncState::Offline);
    }

    #[test]
    fn dirty_and_committed_work_count_together() {
        let i = StateInputs { dirty_notes: 2, ahead_commits: 1, ..online() };
        assert_eq!(classify(i), SyncState::HasChanges(3));
        let i = StateInputs { ahead_commits: 2, ..online() };
        assert_eq!(classify(i), SyncState::Ahead(2));
    }

    #[test]
    fn both_directions_diverge() {
        let i = StateInputs { dirty_notes: 3, behind_commits: 2, ..online() };
        assert_eq!(classify(i), SyncState::Diverged { ahead: 3, behind: 2 });
    }

    #[test]
    fn quiet_vault_is_synced() {
        assert_eq!(classify(online()), SyncState::Synced);
        assert!(!SyncState::Synced.needs_action());
        assert!(SyncState::Behind(1).needs_action());
        assert_eq!(SyncState::Diverged { ahead: 1, behind: 1 }.tag(), "diverged");
    }
}
