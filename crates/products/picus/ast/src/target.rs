//! [`Target`] — one file a generation is written into, and the rules that apply
//! there.
//!
//! **This is where the dialect lives.** Not on the model, not on the session, not
//! anywhere global: on the destination. A target's dialect comes from the folder it
//! belongs to, so one generation produces N files, each correct on its own terms.
//!
//! The rules are per target for the same reason. A version guard belongs on an
//! update script and is meaningless on an initialisation script — there is no
//! earlier version to protect against on a fresh install — so a rule that made
//! sense for one role must never propagate to another.

use picus_types::prelude::EngineKind;
use serde::{Deserialize, Serialize};

/// What a folder of scripts is FOR. Drives which rules a target defaults to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderRole {
    /// Runs on a fresh install. Bare statements, no guards.
    Init,
    /// Runs on an existing database. Guarded, and carries the version forward.
    Update,
    /// Packages, procedures, functions, triggers.
    Routines,
    /// Reference rows loaded alongside the schema.
    Data,
    Ignored,
}

/// How a target wraps the statements it receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetWrap {
    /// Bare statements, one after another.
    Plain,
    /// A procedural block, which is what makes guards possible at all.
    Block,
}

/// Run only when the database is at `from`, then carry it to `to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionGuard {
    pub from: String,
    pub to: String,
}

/// The rules that apply to one destination.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetGuards {
    /// Run only from a known starting version, and stamp the resulting one.
    /// Requires [`TargetWrap::Block`] — a guard needs somewhere to return from.
    #[serde(default)]
    pub version: Option<VersionGuard>,
    /// Skip rows already present, matched on the comparison key.
    #[serde(default)]
    pub skip_if_present: bool,
    /// Bail out when the table isn't there.
    #[serde(default)]
    pub require_object: bool,
    /// Savepoint and roll back on error.
    #[serde(default)]
    pub transactional: bool,
}

/// One file the generation is written into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    /// Project-relative path of the destination file.
    pub file: String,
    /// **From the folder**, never from a connection or a global setting.
    pub dialect: EngineKind,
    pub role: FolderRole,
    pub branch_id: String,
    #[serde(default)]
    pub enabled: bool,
    pub wrap: TargetWrap,
    #[serde(default)]
    pub guards: TargetGuards,
}

impl Target {
    /// Is this target's rule set coherent?
    ///
    /// One rule genuinely constrains another: a version guard has to be able to
    /// `RETURN` early, and there is nothing to return from outside a block. Rather
    /// than silently emitting a guard that cannot work, the caller is told.
    pub fn rule_conflict(&self) -> Option<&'static str> {
        if self.guards.version.is_some() && self.wrap == TargetWrap::Plain {
            return Some("a version guard needs a procedural block to return from");
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(wrap: TargetWrap, version: Option<VersionGuard>) -> Target {
        Target {
            id: "t1".into(),
            file: "ORACLE/UPDATES/4_13.sql".into(),
            dialect: EngineKind::Oracle,
            role: FolderRole::Update,
            branch_id: "ora".into(),
            enabled: true,
            wrap,
            guards: TargetGuards { version, ..TargetGuards::default() },
        }
    }

    #[test]
    fn a_version_guard_outside_a_block_is_reported_not_emitted() {
        let guard = VersionGuard { from: "4.12".into(), to: "4.13".into() };
        assert!(target(TargetWrap::Plain, Some(guard.clone())).rule_conflict().is_some());
        assert!(target(TargetWrap::Block, Some(guard)).rule_conflict().is_none());
        assert!(target(TargetWrap::Plain, None).rule_conflict().is_none());
    }

    #[test]
    fn the_dialect_travels_with_the_target() {
        // The invariant, asserted where it can actually be asserted: a target
        // carries its own dialect, so two targets of one generation can disagree.
        let a = target(TargetWrap::Block, None);
        let mut b = a.clone();
        b.dialect = EngineKind::Postgres;
        assert_ne!(a.dialect, b.dialect);
    }
}
