//! CONS002 / CONS003 — one dialect's initialisation against its own updates.
//!
//! Most of these assert that nothing is produced. The install half is
//! cumulative and the update half is a chain of deltas, so a naive reading of
//! "in one and not the other" reports the entire seed dataset on the first run —
//! and a first run that is all noise is a tool nobody opens twice.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

/// The shape both rules need: a table the initialisation seeds *and* an update
/// maintains, which is the only case either of them speaks about.
fn maintained(init: &str, update: &str) -> Fixture {
    Fixture::build(&[
        ("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", init),
        ("ORACLE/AGGIORNAMENTO/4_12__4_13.sql", update),
    ])
}

// ── CONS002 — seeded on install, never propagated ────────────────────────────

#[test]
fn a_row_the_initialisation_has_and_no_update_inserts_is_reported() {
    let repo = maintained(
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
    );
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons002);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].title.contains("MAX_RIGHE"), "{}", findings[0].title);
    // It anchors where the datum is — the only place a person could declare
    // that the row is deliberately install-only.
    assert_eq!(findings[0].file, "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql");
    assert_eq!(findings[0].line, Some(2));
    // …and points at the half that never receives it.
    assert_eq!(findings[0].also_at.as_deref(), Some("ORACLE/AGGIORNAMENTO"));
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

#[test]
fn a_table_the_updates_never_load_is_not_compared_at_all() {
    // The false positive that would otherwise dominate every first run: a table
    // seeded once at install, from before the update folder existed. Nothing in
    // the tree dates a row, so the only honest signal is whether the updates are
    // in the business of maintaining this table at all.
    let repo = maintained(
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('GIORNI_STORICO', 90);",
        "INSERT INTO LISTINI (COD) VALUES ('STD2026');",
    );
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    // …and the mirror stays quiet too: LISTINI is an update-only table, which is
    // not the same claim as a row the initialisation forgot.
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

#[test]
fn a_dialect_with_no_update_folder_has_nothing_to_compare() {
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
    )]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

#[test]
fn a_row_that_is_deliberately_install_only_can_be_declared_on_its_statement() {
    // The reason both rules anchor at the statement rather than at the folder:
    // "this one predates the updates" is a fact about one INSERT, and there has
    // to be somewhere to write it.
    let repo = maintained(
        "-- picus: ignore CONS002 — seeded before the update folder existed\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
    );
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    let silenced: Vec<_> = report.of_rule(RuleId::Cons002).collect();
    assert_eq!(silenced.len(), 1, "a suppressed finding is silenced, not deleted");
    assert_eq!(
        silenced[0].suppressed_because.as_deref(),
        Some("seeded before the update folder existed")
    );
}

// ── CONS003 — added by an update, never seeded ───────────────────────────────

#[test]
fn a_row_only_an_update_inserts_is_reported_against_the_initialisation() {
    let repo = maintained(
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
    );
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons003);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].title.contains("MAX_RIGHE"), "{}", findings[0].title);
    assert_eq!(findings[0].file, "ORACLE/AGGIORNAMENTO/4_12__4_13.sql");
    assert_eq!(findings[0].also_at.as_deref(), Some("ORACLE/INIZIALIZZAZIONE"));
    // The consequence has to be about the fresh install, which is the half that
    // ends up wrong — not a restatement of where the row is.
    assert!(
        findings[0].consequence.contains("from scratch"),
        "{}",
        findings[0].consequence
    );
    assert!(open_of(&report, RuleId::Cons002).is_empty());
}

#[test]
fn a_reference_data_folder_counts_as_part_of_the_initialisation() {
    // `DATI` runs on a fresh install exactly as `INIZIALIZZAZIONE` does. A rule
    // that only looked at `init` would move the moment somebody moved an INSERT
    // between the two folders.
    let repo = Fixture::build(&[
        (
            "ORACLE/DATI/parametri.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
             INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
        ),
    ]);
    let report = repo.report();
    // Only the genuinely new row: the seeded one was found in `DATI`.
    let findings = open_of(&report, RuleId::Cons003);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].title.contains("MAX_RIGHE"));
}

// ── standing down ────────────────────────────────────────────────────────────

#[test]
fn a_computed_value_stands_the_table_down_rather_than_reporting_a_difference() {
    // Two rows stamped with SYSDATE are never the same value and never known to
    // be different either. The same abstention the cross-dialect comparison makes.
    let computed = maintained(
        "INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES ('SOGLIA_SCONTO', SYSDATE);\n\
         INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES ('MAX_RIGHE', SYSDATE);",
        "INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES ('SOGLIA_SCONTO', SYSDATE);",
    );
    let report = computed.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());

    // …and the same shape with readable values still fires, so the test above is
    // not passing because the comparison is switched off.
    let literal = maintained(
        "INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES ('SOGLIA_SCONTO', '2026-01-01');\n\
         INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES ('MAX_RIGHE', '2026-01-01');",
        "INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES ('SOGLIA_SCONTO', '2026-01-01');",
    );
    assert_eq!(open_of(&literal.report(), RuleId::Cons002).len(), 1);
}

#[test]
fn an_insert_with_no_column_list_stands_the_table_down() {
    // Which column a value belongs to is unknown, so matching it against a named
    // row would be the guess about physical column order that `DML002` exists to
    // say nobody should make.
    let repo = maintained(
        "INSERT INTO PARAMETRI VALUES ('SOGLIA_SCONTO', 15);",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
    );
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

#[test]
fn an_extra_column_on_one_side_is_not_a_missing_row() {
    // The update carries a column the initialisation does not write. That is the
    // same datum written twice, not one datum missing from each half — which is
    // what a naive comparison would report, in both directions, for every row.
    let repo = maintained(
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        "INSERT INTO PARAMETRI (COD, VALORE, DATA_AGG) VALUES ('SOGLIA_SCONTO', 15, '2026-01-01');",
    );
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

#[test]
fn the_two_halves_are_never_read_across_dialects() {
    // The Oracle initialisation is not the PostgreSQL updates' initialisation.
    // Reading them as one story would make a repository with a one-sided folder
    // layout report every row it has.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "insert into parametri (cod, valore) values ('SOGLIA_SCONTO', 15);",
        ),
    ]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

#[test]
fn a_value_that_changed_on_one_side_is_reported_from_both_ends() {
    // Pinned rather than tolerated: with no notion of a primary key, "the same
    // row with a different value" is indistinguishable from two unrelated rows.
    // Both findings are true — a fresh install gets 15, an upgraded database
    // gets 20 — and closing either one closes the disagreement.
    let repo = maintained(
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 20);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
    );
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Cons002).len(), 1);
    assert_eq!(open_of(&report, RuleId::Cons003).len(), 1);
}

#[test]
fn a_row_written_twice_in_one_half_is_one_finding() {
    // Two copies of the same INSERT are `DUP001`'s business. Reporting the
    // second copy here as well would make one missing datum look like two.
    let repo = maintained(
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);\n\
         INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
        "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
    );
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons002);
    assert_eq!(findings.len(), 1, "one datum, one thing to add: {findings:?}");
    assert_eq!(findings[0].line, Some(2), "the first place it is written");
}
