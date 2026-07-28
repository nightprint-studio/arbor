//! Declared suppressions: `-- picus: ignore DML001 — full reload on install`.
//!
//! Three rules, and each of them is a decision rather than an implementation
//! detail:
//!
//! 1. **The reason is mandatory.** A suppression with nothing after the rule id
//!    silences nothing at all, and is reported back to the person who wrote it
//!    ([`RejectedSuppression`]). Silencing without a motivation is how a
//!    consistency report becomes a list of things somebody once decided to stop
//!    looking at.
//! 2. **A suppressed finding stays visible.** It keeps its place in the report
//!    with the reason attached; the interface hides it behind a toggle. Deleting
//!    it here would make the reason unreadable, which defeats point 1.
//! 3. **Scope is where the comment sits**, not the whole file — unless the
//!    comment is in the file's header, where there is nothing else it could mean.

use std::collections::HashMap;

use picus_parse::prelude::{line_col, ParsedFile};

use crate::finding::Finding;
use crate::rule::RuleId;

/// What a suppression comment reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The comment is in the file's header, before any statement.
    ///
    /// Which is two things at once, and both readings are honoured: for a rule
    /// about the file it speaks for the file, and for a rule about a statement
    /// it speaks for the statement it sits above — carried here as that
    /// statement's line span, `None` in a file that has no statements at all.
    Header { first_statement: Option<(usize, usize)> },
    /// 1-based, inclusive: the statement the comment is attached to.
    Statement { first: usize, last: usize },
}

impl Scope {
    /// Does this suppression reach a finding of `rule` at `line`?
    pub fn covers(self, rule: RuleId, line: Option<usize>) -> bool {
        match self {
            Scope::Header { first_statement } => {
                if rule.is_file_scoped() {
                    return true;
                }
                match (first_statement, line) {
                    (Some((first, last)), Some(line)) => line >= first && line <= last,
                    // Nothing below it, so nothing else it could mean.
                    (None, _) => true,
                    (Some(_), None) => false,
                }
            }
            // A finding with no line is about the whole file, and a comment
            // pinned to one statement does not speak for the file.
            Scope::Statement { first, last } => {
                line.is_some_and(|line| line >= first && line <= last)
            }
        }
    }
}

/// One usable suppression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suppression {
    pub rule: RuleId,
    /// Why — never empty, by construction.
    pub reason: String,
    /// 1-based line the comment is on.
    pub line: usize,
    pub scope: Scope,
}

/// A suppression comment that was written but does not silence anything.
///
/// Reported rather than ignored: the alternative is a comment that looks like it
/// works, and the author only finds out when the finding they thought they had
/// handled turns up in a review.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedSuppression {
    /// Project-relative path.
    pub file: String,
    pub line: usize,
    /// The comment as written.
    pub text: String,
    /// Written for the person who wrote the comment.
    pub problem: String,
}

/// Every suppression in one file, plus the ones that do not work.
pub fn scan(
    path: &str,
    source: &str,
    parsed: &ParsedFile,
) -> (Vec<Suppression>, Vec<RejectedSuppression>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();

    for (offset, text) in line_comments(source) {
        let Some(body) = suppression_body(&text) else { continue };
        let line = line_col(source, offset).0;
        match read_body(body) {
            Ok((rule, reason)) => {
                accepted.push(Suppression { rule, reason, line, scope: scope_of(offset, source, parsed) })
            }
            Err(problem) => rejected.push(RejectedSuppression {
                file: path.to_string(),
                line,
                text: text.trim().to_string(),
                problem,
            }),
        }
    }
    (accepted, rejected)
}

/// Attach reasons to the findings a set of suppressions covers.
///
/// Findings are **kept**, never removed. `by_file` is keyed by the same
/// project-relative path a finding carries.
pub fn apply(findings: &mut [Finding], by_file: &HashMap<String, Vec<Suppression>>) {
    for finding in findings.iter_mut() {
        let Some(suppressions) = by_file.get(&finding.file) else { continue };
        let matched = suppressions
            .iter()
            .find(|s| s.rule == finding.rule && s.scope.covers(finding.rule, finding.line));
        if let Some(suppression) = matched {
            finding.suppressed_because = Some(suppression.reason.clone());
        }
    }
}

/// Every `--` comment in the source, as (byte offset of the `--`, text to the
/// end of the line).
///
/// A small state machine rather than a line scan, because a `--` inside a string
/// literal is not a comment and a suppression that only works outside quotes is
/// the kind of subtlety nobody should have to know about. Block comments are
/// skipped for the same reason.
///
/// Known limit: PostgreSQL dollar-quoting (`$$ … $$`) is not tracked. A `--` in
/// a function body is therefore read as a comment, which at worst notices a
/// suppression written inside one — a benign error, and the opposite of the
/// damaging one (missing a suppression the author did write).
fn line_comments(source: &str) -> Vec<(usize, String)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        // `''` is an escaped quote, not the end of the string.
                        if bytes.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i += 2;
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                let end = source[i..].find('\n').map(|n| i + n).unwrap_or(bytes.len());
                out.push((i, source[i..end].to_string()));
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

/// The part of a comment after the `-- picus:` marker, if it is one.
fn suppression_body(comment: &str) -> Option<&str> {
    let rest = comment.trim_start_matches('-').trim_start();
    let rest = strip_prefix_ignore_case(rest, "picus:")?.trim_start();
    strip_prefix_ignore_case(rest, "ignore")
        .filter(|r| r.is_empty() || r.starts_with(char::is_whitespace))
}

/// `DML001 — reason` → the rule and the reason.
fn read_body(body: &str) -> Result<(RuleId, String), String> {
    let body = body.trim();
    let (word, rest) = match body.find(char::is_whitespace) {
        Some(i) => (&body[..i], &body[i..]),
        None => (body, ""),
    };
    let Some(rule) = RuleId::parse(word) else {
        return Err(if word.is_empty() {
            "this suppression names no rule — write `-- picus: ignore DML001 — why`".to_string()
        } else {
            format!("`{word}` is not a Picus rule, so this comment silences nothing")
        });
    };
    // The separator between the rule and the reason is whatever the author felt
    // like typing: an em dash, a hyphen, a colon. None of them is the reason.
    let reason = rest.trim_start_matches(|c: char| c.is_whitespace() || "-–—:,".contains(c)).trim();
    if reason.is_empty() {
        return Err(format!(
            "`{rule}` is suppressed with no reason, so it is not suppressed — \
             the reason is the point, and it stays visible in the report"
        ));
    }
    Ok((rule, reason.to_string()))
}

/// Which statement a comment is attached to.
///
/// The convention is the ordinary one: a comment belongs to the thing it sits
/// above, or to the thing it sits on the same line as. The header is the
/// exception, and it is the useful one — it is the only place from which a
/// file-wide rule can be silenced.
fn scope_of(offset: usize, source: &str, parsed: &ParsedFile) -> Scope {
    let lines = |start: usize, end: usize| {
        (line_col(source, start).0, line_col(source, end.saturating_sub(1).max(start)).0)
    };
    let Some(first) = parsed.statements.first() else {
        return Scope::Header { first_statement: None };
    };
    if offset < first.range.start {
        return Scope::Header {
            first_statement: Some(lines(first.range.start, first.range.end)),
        };
    }
    let span = |start: usize, end: usize| {
        let (first, last) = lines(start, end);
        Scope::Statement { first, last }
    };

    if let Some(inside) = parsed.statements.iter().find(|s| s.range.contains(offset)) {
        return span(inside.range.start, inside.range.end);
    }
    // Trailing on the same line as the statement that just ended: `INSERT …; --
    // picus: ignore DML002 — …` is about that INSERT, not about the next one.
    let comment_line = line_col(source, offset).0;
    if let Some(previous) = parsed.statements.iter().rev().find(|s| s.range.end <= offset) {
        if line_col(source, previous.range.end.saturating_sub(1)).0 == comment_line {
            return span(previous.range.start, previous.range.end);
        }
    }
    match parsed.statements.iter().find(|s| s.range.start >= offset) {
        Some(next) => span(next.range.start, next.range.end),
        // A trailing comment after the last statement, on its own line: there is
        // nothing below it, so the file is the only thing it can be about.
        None => Scope::Header { first_statement: None },
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len()).filter(|head| head.eq_ignore_ascii_case(prefix))?;
    text.get(prefix.len()..)
}

#[cfg(test)]
mod tests {
    use super::*;
    use picus_parse::prelude::{EngineKind, SqlParser};

    fn scan_oracle(source: &str) -> (Vec<Suppression>, Vec<RejectedSuppression>) {
        let parsed = SqlParser::new().parse(source, EngineKind::Oracle);
        scan("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", source, &parsed)
    }

    #[test]
    fn a_reasoned_suppression_is_read_with_its_reason() {
        let source = "-- picus: ignore DML001 — full reload on install\nDELETE FROM PARAMETRI;";
        let (accepted, rejected) = scan_oracle(source);
        assert!(rejected.is_empty());
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].rule, RuleId::Dml001);
        assert_eq!(accepted[0].reason, "full reload on install");
    }

    #[test]
    fn a_suppression_with_no_reason_silences_nothing_and_says_so() {
        // The whole point of the mechanism is the reason; without one it must
        // not work, and the author has to be told why.
        let (accepted, rejected) = scan_oracle("-- picus: ignore DML001\nDELETE FROM PARAMETRI;");
        assert!(accepted.is_empty());
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].problem.contains("no reason"));

        // …and neither does one with a separator and nothing after it.
        let (accepted, rejected) = scan_oracle("-- picus: ignore DML001 —  \nDELETE FROM T;");
        assert!(accepted.is_empty());
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn an_unknown_rule_is_reported_rather_than_ignored() {
        let (accepted, rejected) = scan_oracle("-- picus: ignore DML009 — because\nDELETE FROM T;");
        assert!(accepted.is_empty());
        assert!(rejected[0].problem.contains("DML009"));
    }

    #[test]
    fn every_separator_people_actually_type_is_accepted() {
        for separator in ["—", "-", ":", "–", "", ","] {
            let source = format!("-- picus: ignore DML001 {separator} because\nDELETE FROM T;");
            let (accepted, _) = scan_oracle(&source);
            assert_eq!(accepted.len(), 1, "separator {separator:?}");
            assert_eq!(accepted[0].reason, "because", "separator {separator:?}");
        }
    }

    #[test]
    fn a_marker_inside_a_string_literal_is_not_a_suppression() {
        // A parser that scanned lines would read this as a suppression and
        // silently disable a rule for the statement below it.
        let source = "INSERT INTO T (D) VALUES ('-- picus: ignore DML001 — nope');\nDELETE FROM T;";
        let (accepted, rejected) = scan_oracle(source);
        assert!(accepted.is_empty());
        assert!(rejected.is_empty());
    }

    #[test]
    fn a_header_suppression_speaks_for_the_file_when_the_rule_is_about_the_file() {
        let source = "-- picus: ignore ENC001 — legacy import\nINSERT INTO T (A) VALUES (1);";
        let (accepted, _) = scan_oracle(source);
        assert!(matches!(accepted[0].scope, Scope::Header { .. }));
        assert!(accepted[0].scope.covers(RuleId::Enc001, None));
        assert!(accepted[0].scope.covers(RuleId::Enc001, Some(999)));
    }

    #[test]
    fn a_header_suppression_of_a_statement_rule_covers_only_the_statement_below_it() {
        // The same position, the other reading: this one is touching the DELETE,
        // and letting it reach the whole file would silence the rest by accident.
        let source = "-- picus: ignore DML001 — deliberate\nDELETE FROM T;\nDELETE FROM U;";
        let (accepted, _) = scan_oracle(source);
        assert!(accepted[0].scope.covers(RuleId::Dml001, Some(2)));
        assert!(!accepted[0].scope.covers(RuleId::Dml001, Some(3)));
    }

    #[test]
    fn a_suppression_above_a_statement_covers_only_that_statement() {
        let source = "INSERT INTO T (A) VALUES (1);\n\
                      -- picus: ignore DML001 — deliberate\n\
                      DELETE FROM T;\n\
                      DELETE FROM U;";
        let (accepted, _) = scan_oracle(source);
        assert_eq!(accepted.len(), 1);
        // Line 3 is the DELETE it is attached to; line 4 is the other one.
        assert!(accepted[0].scope.covers(RuleId::Dml001, Some(3)));
        assert!(!accepted[0].scope.covers(RuleId::Dml001, Some(4)));
        assert!(!accepted[0].scope.covers(RuleId::Dml001, Some(1)));
        // …and it cannot silence a file-wide finding, which has no line.
        assert!(!accepted[0].scope.covers(RuleId::Enc001, None));
    }

    #[test]
    fn a_trailing_suppression_belongs_to_the_statement_it_follows() {
        let source = "DELETE FROM T; -- picus: ignore DML001 — deliberate\nDELETE FROM U;";
        let (accepted, _) = scan_oracle(source);
        assert!(accepted[0].scope.covers(RuleId::Dml001, Some(1)));
        assert!(!accepted[0].scope.covers(RuleId::Dml001, Some(2)));
    }

    #[test]
    fn a_suppression_inside_a_block_covers_the_block() {
        let source = "BEGIN\n  -- picus: ignore DML001 — reset\n  DELETE FROM T;\nEND;";
        let (accepted, _) = scan_oracle(source);
        assert_eq!(accepted.len(), 1);
        assert!(accepted[0].scope.covers(RuleId::Dml001, Some(3)));
    }

    #[test]
    fn an_ordinary_comment_is_left_alone() {
        let (accepted, rejected) =
            scan_oracle("-- picus: generated PARAMETRI (4.12 -> 4.13)\nDELETE FROM T;\n-- note");
        assert!(accepted.is_empty());
        assert!(rejected.is_empty());
    }

    #[test]
    fn applying_a_suppression_keeps_the_finding() {
        use crate::finding::{Anchor, Finding};
        let mut findings = vec![Finding::new(
            RuleId::Dml001,
            Anchor::at("a.sql", "ora", 3),
            "DELETE without a WHERE clause",
            "…",
        )
        .build()];
        let mut by_file = HashMap::new();
        by_file.insert(
            "a.sql".to_string(),
            vec![Suppression {
                rule: RuleId::Dml001,
                reason: "deliberate".to_string(),
                line: 2,
                scope: Scope::Statement { first: 3, last: 3 },
            }],
        );
        apply(&mut findings, &by_file);
        assert_eq!(findings.len(), 1, "a suppressed finding is silenced, not deleted");
        assert_eq!(findings[0].suppressed_because.as_deref(), Some("deliberate"));
    }
}
