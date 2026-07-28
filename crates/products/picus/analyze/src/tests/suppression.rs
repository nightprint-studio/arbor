//! Declared suppressions, end to end.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

#[test]
fn a_declared_suppression_silences_a_finding_and_keeps_it_visible() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "-- picus: ignore DML001 — full reload of the parameter table on install\n\
         DELETE FROM PARAMETRI;",
    )]);
    let report = repo.report();
    // Silenced…
    assert!(open_of(&report, RuleId::Dml001).is_empty());
    // …but still there, with the reason attached, which is the entire point.
    let all: Vec<_> = report.of_rule(RuleId::Dml001).collect();
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].suppressed_because.as_deref(),
        Some("full reload of the parameter table on install")
    );
    assert_eq!(report.suppressed_count(), 1);
}

#[test]
fn a_suppression_with_no_reason_silences_nothing_and_the_author_is_told() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "-- picus: ignore DML001\nDELETE FROM PARAMETRI;",
    )]);
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Dml001).len(), 1, "the reason is the point");
    assert_eq!(report.rejected_suppressions.len(), 1);
    assert_eq!(report.rejected_suppressions[0].line, 1);
    assert!(report.rejected_suppressions[0].problem.contains("no reason"));
}

#[test]
fn a_suppression_only_covers_the_statement_it_is_attached_to() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "-- picus: ignore DML001 — deliberate reset\n\
         DELETE FROM PARAMETRI;\n\
         DELETE FROM LISTINI;",
    )]);
    let report = repo.report();
    let open = open_of(&report, RuleId::Dml001);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].line, Some(3), "the second DELETE is not covered");
}

#[test]
fn a_suppression_for_one_rule_does_not_touch_another() {
    let repo = Fixture::build(&[(
        "POSTGRES/INIZIALIZZAZIONE/03_clienti.sql",
        "-- picus: ignore DML001 — deliberate\ninsert into clienti values ('X');",
    )]);
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Dml002).len(), 1);
}

#[test]
fn a_header_suppression_can_silence_a_rule_that_is_about_the_whole_file() {
    // ENC001 has no statement to attach to; the header is the only place a
    // suppression for it could sit, and it has to work from there.
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "-- picus: ignore ENC001 — imported from the supplier as UTF-8, converted on load\n\
         INSERT INTO PARAMETRI (COD) VALUES ('X');",
    )])
    .encoded("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", "UTF-8", "windows-1252");
    let report = repo.report();
    assert!(open_of(&report, RuleId::Enc001).is_empty());
    assert_eq!(report.suppressed_count(), 1);
}

#[test]
fn a_suppression_written_inside_a_string_does_not_work() {
    // A line scanner would read this as a suppression and quietly disable a rule
    // for the statement below it.
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "INSERT INTO PARAMETRI (COD, DESCR) VALUES ('X', '-- picus: ignore DML001 — nope');\n\
         DELETE FROM PARAMETRI;",
    )]);
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Dml001).len(), 1);
    assert!(report.rejected_suppressions.is_empty());
}

#[test]
fn an_unknown_rule_id_in_a_comment_is_reported_rather_than_ignored() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "-- picus: ignore DELETE001 — typo\nDELETE FROM PARAMETRI;",
    )]);
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Dml001).len(), 1);
    assert!(report.rejected_suppressions[0].problem.contains("DELETE001"));
}
