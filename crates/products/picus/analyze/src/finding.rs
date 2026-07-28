//! [`Finding`] — one thing that is wrong, in the shape the report renders.
//!
//! Two fields carry the weight and they are not interchangeable:
//!
//! * **`title`** names *what* was found, in the vocabulary of the repository —
//!   the object, the file, the folder.
//! * **`consequence`** says *what goes wrong in practice* if it is left alone.
//!   Never a restatement of the rule. "the two dialects diverge from 4.13 and a
//!   PostgreSQL install ends up without the parameter" — not "objects should be
//!   consistent between dialects". A report whose messages are rule names is a
//!   report people learn to close.

use serde::Serialize;

use crate::rule::{RuleId, Severity};

/// Where a finding points.
///
/// A **path**, and that is the whole of it. The path locates the finding in the
/// tree, and the tree is where its folder, its role and its dialect are — so a
/// finding that also carried the dialect would be carrying a second copy of a
/// fact, and second copies drift. The report groups by walking from the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// Project-relative path, POSIX separators. A file, or — for the rules that
    /// are about a folder rather than anything in it — a folder.
    pub file: String,
    /// 1-based line, when the rule can point at one. `None` for the rules that
    /// are about a whole file or a whole folder.
    pub line: Option<usize>,
}

impl Anchor {
    pub fn file(file: &str) -> Anchor {
        Anchor { file: file.to_string(), line: None }
    }

    pub fn at(file: &str, line: usize) -> Anchor {
        Anchor { file: file.to_string(), line: Some(line) }
    }

    /// `path:line`, or just `path` when there is no line. The form `alsoAt`
    /// carries and the interface shows.
    pub fn location(&self) -> String {
        match self.line {
            Some(line) => format!("{}:{}", self.file, line),
            None => self.file.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable across runs for the same problem — see [`FindingDraft::build`].
    pub id: String,
    pub rule: RuleId,
    pub severity: Severity,
    pub title: String,
    /// What goes wrong in practice. Never a rule restatement.
    pub consequence: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// The second place, for rules that pair two locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub also_at: Option<String>,
    /// Label of the corrective action, when the rule can propose a patch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_label: Option<String>,
    /// The reason from a `-- picus: ignore … — …` comment.
    ///
    /// Present means **silenced but still visible**: the finding stays in the
    /// list with the declared reason attached. Suppressing something out of
    /// existence would make the report a record of what someone once decided to
    /// stop seeing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_because: Option<String>,
}

impl Finding {
    /// Start a finding. The two messages are required arguments rather than
    /// fluent steps because a finding with no consequence is the failure this
    /// whole module exists to prevent, and an optional setter invites it.
    pub fn new(
        rule: RuleId,
        anchor: Anchor,
        title: impl Into<String>,
        consequence: impl Into<String>,
    ) -> FindingDraft {
        FindingDraft {
            rule,
            anchor,
            title: title.into(),
            consequence: consequence.into(),
            also_at: None,
            fix_label: None,
        }
    }

    pub fn is_suppressed(&self) -> bool {
        self.suppressed_because.is_some()
    }
}

/// A finding under construction.
#[derive(Debug, Clone)]
pub struct FindingDraft {
    rule: RuleId,
    anchor: Anchor,
    title: String,
    consequence: String,
    also_at: Option<String>,
    fix_label: Option<String>,
}

impl FindingDraft {
    /// The second location this finding pairs with.
    pub fn also_at(mut self, location: impl Into<String>) -> Self {
        self.also_at = Some(location.into());
        self
    }

    /// The label of the corrective action. Only where a patch can actually be
    /// proposed — a button that opens a dialogue saying "not implemented" is
    /// worse than no button.
    pub fn fix(mut self, label: impl Into<String>) -> Self {
        self.fix_label = Some(label.into());
        self
    }

    pub fn build(self) -> Finding {
        let id = finding_id(&self);
        Finding {
            id,
            rule: self.rule,
            severity: self.rule.severity(),
            title: self.title,
            consequence: self.consequence,
            file: self.anchor.file,
            line: self.anchor.line,
            also_at: self.also_at,
            fix_label: self.fix_label,
            suppressed_because: None,
        }
    }
}

/// A deterministic id for a finding.
///
/// Two properties are wanted and they pull against each other: the interface
/// keys a list row on it, so it must be **stable** for the same problem across
/// re-runs; and two different problems must not collide. Hashing the rule, the
/// place and the title gives both, and deliberately **excludes the consequence**
/// — rewording a message must not renumber the report and lose the user's
/// scroll position.
///
/// FNV-1a rather than a hashing crate: the input is a short string, the output
/// only has to be stable and well spread, and this crate has no business pulling
/// a dependency in for sixteen hex digits.
fn finding_id(draft: &FindingDraft) -> String {
    let identity = format!(
        "{}|{}|{}|{}|{}",
        draft.rule,
        draft.anchor.file,
        draft.anchor.line.unwrap_or(0),
        draft.title,
        draft.also_at.as_deref().unwrap_or("")
    );
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in identity.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{}-{hash:016x}", draft.rule.as_str().to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> FindingDraft {
        Finding::new(
            RuleId::Dml001,
            Anchor::at("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", 8),
            "DELETE without a WHERE clause",
            "The statement empties the whole table rather than a subset.",
        )
    }

    #[test]
    fn the_id_is_stable_for_the_same_problem() {
        assert_eq!(draft().build().id, draft().build().id);
        assert!(draft().build().id.starts_with("dml001-"));
    }

    #[test]
    fn rewording_the_consequence_does_not_renumber_the_report() {
        // The interface keys its rows on the id; a message edit must not look
        // like a different finding and throw the user's place away.
        let reworded = Finding::new(
            RuleId::Dml001,
            Anchor::at("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", 8),
            "DELETE without a WHERE clause",
            "Something else entirely.",
        );
        assert_eq!(draft().build().id, reworded.build().id);
    }

    #[test]
    fn two_findings_in_different_places_get_different_ids() {
        let elsewhere = Finding::new(
            RuleId::Dml001,
            Anchor::at("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", 9),
            "DELETE without a WHERE clause",
            "The statement empties the whole table rather than a subset.",
        );
        assert_ne!(draft().build().id, elsewhere.build().id);
    }

    #[test]
    fn the_wire_shape_omits_what_it_does_not_have() {
        let json = serde_json::to_value(draft().build()).unwrap();
        assert_eq!(json["rule"], "DML001");
        assert_eq!(json["severity"], "review");
        assert_eq!(json["file"], "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql");
        assert_eq!(json["line"], 8);
        // Absent, not null: the frontend's fields are optional, not nullable.
        assert!(json.get("alsoAt").is_none());
        assert!(json.get("fixLabel").is_none());
        assert!(json.get("suppressedBecause").is_none());
    }

    #[test]
    fn an_anchor_with_no_line_still_has_a_location() {
        assert_eq!(Anchor::file("A/b.sql").location(), "A/b.sql");
        assert_eq!(Anchor::at("A/b.sql", 3).location(), "A/b.sql:3");
    }
}
