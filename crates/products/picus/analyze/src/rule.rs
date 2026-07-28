//! [`RuleId`] and [`Severity`] — the closed set, and how loud each member is.
//!
//! The identifiers are a contract with the interface (`RuleId` in
//! `src/lib/types/picus/index.ts` is the same fourteen as a union) and with the
//! scripts themselves: a `-- picus: ignore DML001 — …` comment names one of
//! these, so renaming a member silently un-suppresses somebody's file. They do
//! not get renamed.

use serde::Serialize;

/// How much attention a finding deserves.
///
/// Two levels, not five. A scale nobody can rank consistently turns into "sort
/// by severity and read the top three", which is the same as two levels with
/// extra steps. `Blocking` orders first so the report reads worst-first without
/// a comparator anywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The installation is wrong, or will be. Fix before shipping.
    Blocking,
    /// Probably wrong, sometimes deliberate. Worth a look.
    Review,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Blocking => "blocking",
            Severity::Review => "review",
        }
    }
}

/// The fourteen rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RuleId {
    /// An object one branch touches and the other does not.
    Cons001,
    /// A datum the initialisation writes and no update script ever writes.
    Cons002,
    /// A datum an update script writes and the initialisation never writes.
    Cons003,
    /// The same object filled in differently in the two branches.
    Cons004,
    /// A statement written in the dialect the folder is not.
    ///
    /// Its own prefix on purpose: it is not a disagreement between two branches,
    /// it is one script that will not run.
    Dia001,
    /// An update script that writes without checking where it started from.
    Ver001,
    /// An update script that never carries the version forward.
    Ver002,
    /// A hole or an overlap in the chain of update files.
    Ver003,
    /// The same row inserted twice in one script.
    Dup001,
    /// The same object created in two places in one branch.
    Dup002,
    /// A file whose encoding drifted from what its folder expects.
    Enc001,
    /// A character the folder's encoding cannot represent.
    Enc002,
    /// A `DELETE` or an `UPDATE` with no `WHERE`.
    Dml001,
    /// An `INSERT` with no column list.
    Dml002,
}

impl RuleId {
    /// Every rule, in report order.
    pub const ALL: [RuleId; 14] = [
        RuleId::Cons001,
        RuleId::Cons002,
        RuleId::Cons003,
        RuleId::Cons004,
        RuleId::Dia001,
        RuleId::Ver001,
        RuleId::Ver002,
        RuleId::Ver003,
        RuleId::Dup001,
        RuleId::Dup002,
        RuleId::Enc001,
        RuleId::Enc002,
        RuleId::Dml001,
        RuleId::Dml002,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RuleId::Cons001 => "CONS001",
            RuleId::Cons002 => "CONS002",
            RuleId::Cons003 => "CONS003",
            RuleId::Cons004 => "CONS004",
            RuleId::Dia001 => "DIA001",
            RuleId::Ver001 => "VER001",
            RuleId::Ver002 => "VER002",
            RuleId::Ver003 => "VER003",
            RuleId::Dup001 => "DUP001",
            RuleId::Dup002 => "DUP002",
            RuleId::Enc001 => "ENC001",
            RuleId::Enc002 => "ENC002",
            RuleId::Dml001 => "DML001",
            RuleId::Dml002 => "DML002",
        }
    }

    /// Read a rule id out of a suppression comment. Case-insensitive, because a
    /// comment is typed by a person and `dml001` is unmistakably `DML001`.
    pub fn parse(text: &str) -> Option<RuleId> {
        RuleId::ALL.into_iter().find(|r| text.eq_ignore_ascii_case(r.as_str()))
    }

    /// Is this rule about a **file** (or a folder), rather than about one
    /// statement in it?
    ///
    /// It decides what a suppression written in a file's header means. A header
    /// comment is ambiguous by nature — it sits both at the top of the file and
    /// immediately above the first statement — and the honest reading depends on
    /// what could possibly be suppressed: `-- picus: ignore ENC001` in a header
    /// can only be about the file, because the encoding is not a property of any
    /// statement; `-- picus: ignore DML001` in the same position is about the
    /// `DELETE` underneath it, because that is what it is touching.
    ///
    /// Deciding by rule rather than by position honours both readings without
    /// either of them over-reaching.
    pub fn is_file_scoped(self) -> bool {
        match self {
            // A missing object, a divergent one, an unguarded script, a broken
            // chain, an encoding: none of these is a fact about a statement, and
            // several report no line at all.
            RuleId::Cons001
            | RuleId::Cons004
            | RuleId::Ver001
            | RuleId::Ver002
            | RuleId::Ver003
            | RuleId::Enc001
            | RuleId::Enc002 => true,
            // `CONS002`/`CONS003` are statement-scoped, and that is the point
            // rather than a detail: "this row is deliberately only in the
            // initialisation" can only be declared on the INSERT that writes it,
            // so the rules have to anchor where a person can write the comment.
            RuleId::Cons002
            | RuleId::Cons003
            | RuleId::Dia001
            | RuleId::Dup001
            | RuleId::Dup002
            | RuleId::Dml001
            | RuleId::Dml002 => false,
        }
    }

    /// How loud this rule is. Fixed per rule rather than per finding: a severity
    /// that varied with circumstance would be a severity nobody could filter on.
    pub fn severity(self) -> Severity {
        match self {
            // The installation ends up wrong: a branch missing a change, a
            // script that cannot run, a guard that is not there, a key that
            // collides, a character that cannot be written.
            RuleId::Cons001
            | RuleId::Cons002
            | RuleId::Cons003
            | RuleId::Cons004
            | RuleId::Dia001
            | RuleId::Ver001
            | RuleId::Ver002
            | RuleId::Ver003
            | RuleId::Dup001
            | RuleId::Enc002 => Severity::Blocking,
            // Legitimate often enough that a blocking verdict would train people
            // to ignore the report.
            RuleId::Dup002 | RuleId::Enc001 | RuleId::Dml001 | RuleId::Dml002 => Severity::Review,
        }
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_words_match_the_frontend_union() {
        // `src/lib/types/picus/index.ts` declares exactly these fourteen, and a
        // suppression comment in someone's script spells one of them.
        for rule in RuleId::ALL {
            assert_eq!(serde_json::to_string(&rule).unwrap(), format!("\"{}\"", rule.as_str()));
        }
        assert_eq!(RuleId::ALL.len(), 14);
    }

    #[test]
    fn no_two_rules_share_a_wire_word() {
        // The ids moved once (the old `CONS002`/`CONS003` became `CONS004` and
        // `DIA001`); a second move that collided would silently make one rule
        // un-suppressable and the other suppressable by the wrong comment.
        let mut words: Vec<&str> = RuleId::ALL.iter().map(|r| r.as_str()).collect();
        words.sort_unstable();
        let count = words.len();
        words.dedup();
        assert_eq!(words.len(), count, "two rules answer to the same id");
    }

    #[test]
    fn a_rule_id_round_trips_through_a_comment() {
        for rule in RuleId::ALL {
            assert_eq!(RuleId::parse(rule.as_str()), Some(rule));
            assert_eq!(RuleId::parse(&rule.as_str().to_lowercase()), Some(rule));
        }
        assert_eq!(RuleId::parse("DML003"), None);
        assert_eq!(RuleId::parse(""), None);
    }

    #[test]
    fn the_rules_that_report_no_line_are_all_file_scoped() {
        // A rule that anchors at a whole file could never be suppressed from a
        // statement, so the two classifications have to agree.
        for rule in [RuleId::Cons001, RuleId::Ver003, RuleId::Enc001] {
            assert!(rule.is_file_scoped(), "{rule} anchors at a file");
        }
        for rule in [RuleId::Dml001, RuleId::Dup001, RuleId::Dia001, RuleId::Cons002, RuleId::Cons003]
        {
            assert!(!rule.is_file_scoped(), "{rule} anchors at a statement");
        }
    }

    #[test]
    fn blocking_sorts_before_review() {
        assert!(Severity::Blocking < Severity::Review);
        assert_eq!(serde_json::to_string(&Severity::Review).unwrap(), "\"review\"");
    }
}
