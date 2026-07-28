//! Properties of the report as a whole, rather than of any one rule.

use crate::rule::Severity;
use crate::testing::Fixture;

/// A repository with something wrong in most of the ways there are.
fn messy() -> Fixture {
    Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));\nCREATE TABLE LISTINI (COD VARCHAR2(30));",
        ),
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "DELETE FROM PARAMETRI;\n\
             INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_11__4_12.sql",
            "INSERT INTO LISTINI (COD) VALUES ('STD2026');\n\
             INSERT INTO LISTINI (COD) VALUES ('STD2026');",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
            "insert into parametri values ('SOGLIA_SCONTO', 10);",
        ),
        ("POSTGRES/AGGIORNAMENTO/4_13__4_14.sql", "update parametri set valore = 20;"),
    ])
}

#[test]
fn the_report_is_the_same_twice_running() {
    // The interface keys its rows on the finding id and remembers where the user
    // was. A report that reshuffled itself between runs would throw that away.
    let repo = messy();
    let first = repo.report();
    let second = repo.report();
    assert_eq!(first, second);
    assert!(!first.findings.is_empty());
}

#[test]
fn findings_are_ordered_worst_first_then_in_reading_order() {
    let report = messy().report();
    let mut previous: Option<(Severity, &str, usize)> = None;
    for finding in &report.findings {
        let current = (finding.severity, finding.file.as_str(), finding.line.unwrap_or(0));
        if let Some(before) = previous {
            assert!(before <= current, "{before:?} came before {current:?}");
        }
        previous = Some(current);
    }
}

#[test]
fn every_finding_says_what_goes_wrong_rather_than_restating_the_rule() {
    // The one property the whole report is judged on. A consequence that repeats
    // the title, or that says something "should" be a certain way, tells the
    // reader nothing they did not already know from the rule's name.
    let report = messy().report();
    assert!(report.findings.len() > 4, "the fixture must actually trip several rules");
    for finding in &report.findings {
        assert_ne!(finding.consequence, finding.title, "{}", finding.rule);
        assert!(
            finding.consequence.len() > 60,
            "{} has a consequence too short to say anything: {:?}",
            finding.rule,
            finding.consequence
        );
        for weasel in ["should be", "must be", "is not consistent", "is recommended"] {
            assert!(
                !finding.consequence.contains(weasel),
                "{} restates the rule instead of its effect: {:?}",
                finding.rule,
                finding.consequence
            );
        }
    }
}

#[test]
fn every_finding_points_at_a_real_place_in_the_project() {
    let repo = messy();
    let report = repo.report();
    let known: Vec<&str> = repo
        .project
        .walk()
        .flat_map(|f| f.files.iter().map(|x| x.path.as_str()).chain(std::iter::once(f.path.as_str())))
        .collect();
    for finding in &report.findings {
        assert!(
            known.contains(&finding.file.as_str()),
            "{} anchors at {:?}, which is not in the tree",
            finding.rule,
            finding.file
        );
    }
}

#[test]
fn an_empty_repository_produces_an_empty_report() {
    let report = Fixture::build(&[]).report();
    assert!(report.findings.is_empty());
    assert!(report.rejected_suppressions.is_empty());
    assert_eq!(report.count(Severity::Blocking), 0);
}

#[test]
fn the_counts_ignore_what_has_been_declared() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "-- picus: ignore DML001 — full reload on install\nDELETE FROM PARAMETRI;",
    )]);
    let report = repo.report();
    assert_eq!(report.count(Severity::Review), 0);
    assert_eq!(report.suppressed_count(), 1);
    assert_eq!(report.findings.len(), 1);
}
