//! Rules a repository has switched off — `[analysis] disabled_rules`.
//!
//! The mechanism is one line of filtering, and every test here is about the half
//! that is not: a rule that was told not to run has to **say so**. A report with
//! nothing in it is the product's entire output, and if "clean" and "not looked
//! at" render the same way then the clean one stops meaning anything.
//!
//! Distinct from a suppression comment on purpose. A suppression is written next
//! to one statement, carries a reason, and leaves the finding visible with that
//! reason attached ([`crate::suppress`]). Switching a rule off is a decision about
//! the whole repository — "our views reference tables that live in another
//! repository, so `CONS001` has nothing useful to say here" — and there is no one
//! statement to write it beside.

use crate::rule::{rule_settings_problems, RuleId};
use crate::testing::Fixture;
use crate::tests::open_of;

/// A repository with a real, reportable problem: `PARAMETRI` is touched by the
/// Oracle scripts and by nothing on the PostgreSQL side.
fn one_sided() -> Fixture {
    Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/02_LISTINI.sql",
            "insert into listini (cod) values ('STD2026');",
        ),
    ])
}

#[test]
fn a_rule_the_project_switches_off_produces_nothing() {
    let before = one_sided().report();
    assert!(!open_of(&before, RuleId::Cons001).is_empty(), "the fixture has to fire first");

    let after = one_sided()
        .configured(|c| c.analysis.disabled_rules.push("CONS001".to_string()))
        .report();
    assert!(after.findings.iter().all(|f| f.rule != RuleId::Cons001));
}

#[test]
fn a_rule_that_was_switched_off_says_so_rather_than_looking_clean() {
    let report = one_sided()
        .configured(|c| c.analysis.disabled_rules.push("CONS001".to_string()))
        .report();
    assert!(report.was_skipped(RuleId::Cons001), "{:?}", report.skipped);

    let reason = &report.skipped.iter().find(|s| s.rule == RuleId::Cons001).expect("skipped").reason;
    // It has to name the place the decision is written, or the reader is told
    // that something is off and not where to turn it back on.
    assert!(reason.contains("project.toml"), "{reason}");
}

#[test]
fn switching_one_rule_off_leaves_its_neighbours_alone() {
    // The filter is keyed on the rule, and a report that lost unrelated findings
    // when one rule was disabled would be the worst possible version of this
    // feature: silent, and in the direction of missing things.
    let report = one_sided()
        .configured(|c| c.analysis.disabled_rules.push("DML002".to_string()))
        .report();
    assert!(!open_of(&report, RuleId::Cons001).is_empty());
    assert!(report.was_skipped(RuleId::Dml002));
    assert!(!report.was_skipped(RuleId::Cons001));
}

#[test]
fn a_disabled_rule_is_listed_once_even_when_it_had_another_reason_to_stand_down() {
    // `CONS002` is already not running under the default initialisation model. A
    // project that *also* disables it must not produce two skip lines with two
    // different explanations — the actionable one wins.
    let report = one_sided()
        .configured(|c| c.analysis.disabled_rules.push("CONS002".to_string()))
        .report();
    let lines: Vec<_> = report.skipped.iter().filter(|s| s.rule == RuleId::Cons002).collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].reason.contains("project.toml"), "{}", lines[0].reason);
}

#[test]
fn the_id_is_read_the_way_a_person_types_it() {
    let report = one_sided()
        .configured(|c| c.analysis.disabled_rules.push("  cons001 ".to_string()))
        .report();
    assert!(report.was_skipped(RuleId::Cons001));
}

#[test]
fn an_id_that_silences_nothing_is_reported_to_whoever_wrote_it() {
    // The ids are held as plain strings so a typo degrades to "this line does
    // nothing" instead of failing the parse and resetting every other setting in
    // the file. That degradation is only honest if somebody is told.
    use picus_project::prelude::AnalysisSettings;

    let settings = AnalysisSettings {
        disabled_rules: vec!["CONS001".into(), "CONS009".into(), "cons001".into()],
        ..AnalysisSettings::default()
    };
    let problems = rule_settings_problems(&settings);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("CONS009")), "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("more than once")), "{problems:?}");

    assert!(rule_settings_problems(&AnalysisSettings::default()).is_empty());
}

// ── Objects excluded from the rules ──────────────────────────────────────────

/// One table that is a special case, and one that is not — so a test can show
/// the exclusion reaching the first and leaving the second alone.
fn two_tables() -> Fixture {
    Fixture::build(&[
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO STAGING_IMPORT (COD) VALUES ('X');\n\
             INSERT INTO CATALOGO_WIDGET (CHIAVE) VALUES ('ricerca');\n\
             DELETE FROM STAGING_IMPORT;\n\
             UPDATE CATALOGO_WIDGET SET ORDINE = 1;",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "insert into parametri (cod) values ('X');",
        ),
    ])
}

#[test]
fn an_excluded_object_produces_no_findings_of_any_rule() {
    let before = two_tables().report();
    assert!(
        before.findings.iter().any(|f| f.title.contains("STAGING_IMPORT")
            || f.consequence.contains("STAGING_IMPORT")),
        "the fixture has to say something about it first"
    );

    let after = two_tables()
        .configured(|c| c.analysis.excluded_objects = vec!["staging_import".to_string()])
        .report();
    assert!(
        !after.findings.iter().any(|f| f.title.contains("STAGING_IMPORT")
            || f.consequence.contains("STAGING_IMPORT")),
        "{:?}",
        after.findings.iter().map(|f| &f.title).collect::<Vec<_>>()
    );
}

#[test]
fn excluding_one_object_leaves_every_other_one_checked() {
    // The reason this exists rather than "switch the rule off": silencing one
    // table must not stop the rule watching the rest.
    let report = two_tables()
        .configured(|c| c.analysis.excluded_objects = vec!["STAGING_IMPORT".to_string()])
        .report();
    assert!(
        report.findings.iter().any(|f| f.title.contains("CATALOGO_WIDGET")
            || f.consequence.contains("CATALOGO_WIDGET")),
        "{:?}",
        report.findings.iter().map(|f| &f.title).collect::<Vec<_>>()
    );
}

#[test]
fn the_name_is_matched_the_way_the_scripts_are_compared() {
    // Folded like every other identifier in the product: an unquoted name given
    // in either case matches, and a quoted one keeps its contents exactly.
    for written in ["STAGING_IMPORT", "staging_import", " Staging_Import "] {
        let report = two_tables()
            .configured(|c| c.analysis.excluded_objects = vec![written.to_string()])
            .report();
        assert!(
            !report.findings.iter().any(|f| f.title.contains("STAGING_IMPORT")),
            "{written:?} should have excluded it"
        );
    }
}
