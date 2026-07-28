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

use picus_types::prelude::DialectScope;
use serde::{Deserialize, Serialize};

use crate::dml::{DmlModel, DmlOperation};

/// What a folder of scripts is FOR. Drives which rules a target defaults to.
///
/// Defined in the leaf crate, not here: the script half *discovers* a folder's
/// role and this half *reads* it, so neither owns it. Re-exported so call sites
/// already inside `picus_ast::prelude` do not have to name a third crate.
pub use picus_types::prelude::FolderRole;

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
///
/// A target is a **file, a dialect and a set of rules** and nothing else. It used
/// to carry the id of the branch it belonged to, from when a repository was a
/// list of per-dialect branches; the destination's path is its identity and its
/// folder is where the dialect and the role came from, so the third name was
/// always a spelling of something already here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    /// Project-relative path of the destination file.
    pub file: String,
    /// **From the folder**, never from a connection or a global setting.
    ///
    /// A [`DialectScope`] rather than an `EngineKind`, and the difference is the
    /// whole safety property of writing into a portable folder: there is no
    /// `EngineKind` here to reach for, so every dialect-dependent decision in the
    /// emitter had to grow a portable answer rather than quietly defaulting to
    /// one engine. The type has no variant for an engine Picus does not support
    /// either, so such a folder cannot be a destination at all.
    ///
    /// The wire key stays `dialect`: it now carries `"oracle"`, `"postgres"` or
    /// `"generic"`.
    pub dialect: DialectScope,
    pub role: FolderRole,
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
    ///
    /// Portable destinations add the other half of that argument. A procedural
    /// block is spelled `DECLARE … BEGIN … END; /` on Oracle and `DO $$ … $$` on
    /// PostgreSQL and there is **no form both accept**, so a portable target is
    /// necessarily plain — and a version guard, which needs the block to return
    /// from, cannot hold there either. Refused with the reason, in the same
    /// mechanism, rather than emitted and hoped for.
    pub fn rule_conflict(&self) -> Option<&'static str> {
        if self.dialect.is_portable() && self.wrap == TargetWrap::Block {
            return Some(
                "a portable script cannot use a procedural block: Oracle spells it \
                 `DECLARE … BEGIN … END; /` and PostgreSQL `DO $$ … $$`, and no form runs on both",
            );
        }
        if self.guards.version.is_some() && self.wrap == TargetWrap::Plain {
            if self.dialect.is_portable() {
                return Some(
                    "a version guard needs a procedural block to return from, and a portable \
                     script cannot have one — guard the dialect-specific scripts instead",
                );
            }
            return Some("a version guard needs a procedural block to return from");
        }
        None
    }

    /// Why this target cannot receive **this** model — `None` when it can.
    ///
    /// The same mechanism as [`rule_conflict`](Self::rule_conflict), widened to
    /// the one restriction that depends on what is being written rather than on
    /// where: an upsert is `MERGE … USING DUAL` on Oracle and
    /// `INSERT … ON CONFLICT` on PostgreSQL, and a portable file can contain
    /// neither. One entry point, so a caller cannot check half of it.
    pub fn refuses(&self, model: &DmlModel) -> Option<String> {
        if let Some(conflict) = self.rule_conflict() {
            return Some(conflict.to_string());
        }
        if self.dialect.is_portable() && model.operation == DmlOperation::Upsert {
            return Some(
                "an upsert has no portable spelling: Oracle writes `MERGE … USING DUAL` and \
                 PostgreSQL `INSERT … ON CONFLICT`. Write it into the dialect folders, or use a \
                 plain INSERT here"
                    .to_string(),
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_types::prelude::EngineKind;

    fn target(wrap: TargetWrap, version: Option<VersionGuard>) -> Target {
        Target {
            id: "t1".into(),
            file: "ORACLE/UPDATES/4_13.sql".into(),
            dialect: DialectScope::One(EngineKind::Oracle),
            role: FolderRole::Update,
            enabled: true,
            wrap,
            guards: TargetGuards { version, ..TargetGuards::default() },
        }
    }

    fn portable(wrap: TargetWrap, version: Option<VersionGuard>) -> Target {
        Target {
            file: "COMUNE/parametri.sql".into(),
            dialect: DialectScope::Portable,
            ..target(wrap, version)
        }
    }

    fn model(operation: DmlOperation) -> DmlModel {
        DmlModel {
            table: "PARAMETRI".into(),
            operation,
            columns: Vec::new(),
            key_columns: Vec::new(),
            rows: Vec::new(),
            lowercase_postgres: false,
            version_table: Default::default(),
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
        // carries its own scope, so two targets of one generation can disagree.
        let a = target(TargetWrap::Block, None);
        let mut b = a.clone();
        b.dialect = DialectScope::One(EngineKind::Postgres);
        assert_ne!(a.dialect, b.dialect);
    }

    #[test]
    fn a_portable_target_cannot_be_wrapped_in_a_block() {
        // There is no procedural block both engines accept, so this is refused
        // rather than emitted as one engine's spelling and hoped for.
        let conflict = portable(TargetWrap::Block, None).rule_conflict().expect("refused");
        assert!(conflict.contains("portable"), "{conflict}");
        assert!(conflict.contains("DO $$"), "{conflict}");
        assert!(portable(TargetWrap::Plain, None).rule_conflict().is_none());
    }

    #[test]
    fn a_portable_target_cannot_carry_a_version_guard_and_says_why() {
        // It follows from the block, and the message says so rather than leaving
        // the user to work out the chain.
        let guard = VersionGuard { from: "4.12".into(), to: "4.13".into() };
        let conflict = portable(TargetWrap::Plain, Some(guard)).rule_conflict().expect("refused");
        assert!(conflict.contains("portable"), "{conflict}");
        assert!(conflict.contains("dialect-specific"), "{conflict}");
    }

    #[test]
    fn a_portable_target_refuses_an_upsert_and_accepts_the_plain_operations() {
        // The one restriction that depends on the model rather than the folder.
        let refusal = portable(TargetWrap::Plain, None)
            .refuses(&model(DmlOperation::Upsert))
            .expect("refused");
        assert!(refusal.contains("MERGE"), "{refusal}");
        assert!(refusal.contains("ON CONFLICT"), "{refusal}");

        for operation in [DmlOperation::Insert, DmlOperation::Update, DmlOperation::Delete] {
            assert!(
                portable(TargetWrap::Plain, None).refuses(&model(operation)).is_none(),
                "{operation:?} is exactly the case portable folders exist for"
            );
        }
        // …and a dialect target still takes an upsert, as it always did.
        assert!(target(TargetWrap::Plain, None).refuses(&model(DmlOperation::Upsert)).is_none());
    }

    #[test]
    fn refuses_covers_everything_rule_conflict_does() {
        // One entry point, so a caller cannot check half of it.
        let guard = VersionGuard { from: "4.12".into(), to: "4.13".into() };
        let t = target(TargetWrap::Plain, Some(guard));
        assert_eq!(t.refuses(&model(DmlOperation::Insert)).as_deref(), t.rule_conflict());
    }
}
