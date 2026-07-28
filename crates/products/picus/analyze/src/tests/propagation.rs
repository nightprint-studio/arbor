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
///
/// Declared `mirrored` — the reading in which **both** directions are questions —
/// because that is what the bulk of this module is about. The default reading asks
/// only one of them, and it has its own tests at the bottom.
fn maintained(init: &str, update: &str) -> Fixture {
    Fixture::build(&[
        ("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql", init),
        ("ORACLE/AGGIORNAMENTO/4_12__4_13.sql", update),
    ])
    .mirrored()
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
    )])
    .mirrored();
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
    ])
    .mirrored();
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

// ── which direction is even a question ───────────────────────────────────────

/// The fixture that fires in both directions under `mirrored`: one row only the
/// initialisation has, one row only the update has.
fn diverged() -> Fixture {
    Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
             INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOLO_INIT', 1);",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);\n\
             INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOLO_UPDATE', 2);",
        ),
    ])
}

#[test]
fn by_default_only_the_direction_that_is_always_a_bug_is_checked() {
    // The default reading: the initialisation is kept at the latest version. A row
    // it holds that no update carries is a first-release row and there is no update
    // for the beginning — reporting those is what produced hundreds of findings on
    // a real repository, all of them describing correct behaviour.
    //
    // The reverse is never correct behaviour and stays on.
    let report = diverged().report();
    assert!(open_of(&report, RuleId::Cons002).is_empty(), "the install-only row is not a finding");

    let backwards = open_of(&report, RuleId::Cons003);
    assert_eq!(backwards.len(), 1, "{backwards:?}");
    assert!(backwards[0].title.contains("SOLO_UPDATE"), "{}", backwards[0].title);

    // …and the same repository read the other way reports both, so the test above
    // is not passing because the comparison is broken.
    let both = diverged().mirrored().report();
    assert_eq!(open_of(&both, RuleId::Cons002).len(), 1);
    assert_eq!(open_of(&both, RuleId::Cons003).len(), 1);
}

#[test]
fn a_direction_that_is_not_checked_is_reported_rather_than_silently_absent() {
    // The whole value of the report is that a clean one means something. A rule
    // that produced nothing because it was told not to run must never be
    // indistinguishable from one that ran and found nothing.
    let report = diverged().report();
    assert!(report.was_skipped(RuleId::Cons002), "{:?}", report.skipped);
    assert!(!report.was_skipped(RuleId::Cons003));

    let reason = &report.skipped.iter().find(|s| s.rule == RuleId::Cons002).expect("skipped").reason;
    // Written for the person who could turn it back on: it has to name both what
    // the project declared and where to change it.
    assert!(reason.contains("latest version"), "{reason}");
    assert!(reason.contains("project settings"), "{reason}");
}

#[test]
fn a_value_that_drifted_is_not_described_as_a_row_nobody_wrote() {
    // The asymmetry the initialisation model introduces. With both directions on,
    // a changed value produces two findings and the pair makes it obvious the row
    // exists on both sides. Under the default only one direction runs, and the
    // survivor used to say "the initialisation never inserts it" about a row the
    // initialisation plainly does insert — which reads as a lie to anyone who
    // opens the file, and there were a great many of them.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA, CONFIG) \
             VALUES ('ricerca_avanzata', 'Ricerca', '{\"v\":1}');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA, CONFIG) \
             VALUES ('ricerca_avanzata', 'Ricerca', '{\"v\":2}');",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons003);
    assert_eq!(findings.len(), 1, "{findings:?}");

    let finding = findings[0];
    // The title names the column the two halves disagree about, and does not
    // claim the row is absent.
    assert!(finding.title.contains("CONFIG"), "{}", finding.title);
    assert!(!finding.title.contains("alone"), "{}", finding.title);
    // The consequence says what actually happens: which value you end up with
    // depends on when you installed.
    assert!(
        finding.consequence.contains("installed before or after"),
        "{}",
        finding.consequence
    );
    assert!(finding.consequence.contains("drifted"), "{}", finding.consequence);
    // …and it points at the other row rather than at the folder, because the
    // other row is the thing to go and look at.
    assert_eq!(
        finding.also_at.as_deref(),
        Some("ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql:1")
    );
}

#[test]
fn a_row_the_other_half_has_nothing_like_is_still_reported_as_missing() {
    // The other branch, so the wording above is a distinction and not a rename.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('ricerca', 'Ricerca');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('ricerca', 'Ricerca');\n\
             INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('esporta', 'Esporta');",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons003);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].title.contains("by an update alone"), "{}", findings[0].title);
    assert!(findings[0].consequence.contains("never inserts it"), "{}", findings[0].consequence);
}

#[test]
fn two_rows_that_share_nothing_are_not_called_a_drift() {
    // The near-match is "shares at least one column value". Two unrelated rows in
    // the same table share none, and pairing them would invent a relationship —
    // and point the reader at a line that has nothing to do with the finding.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('ricerca', 'Ricerca');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('ricerca', 'Ricerca');\n\
             INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('stampa', 'Stampa');",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons003);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].title.contains("alone"), "{}", findings[0].title);
}

#[test]
fn a_row_a_later_update_rewrote_is_not_reported_against_the_initialisation() {
    // The update half is a chain of deltas, not a bag of INSERTs. Version 1.11
    // writes a row, 1.13 rewrites it, and the initialisation — kept at the latest
    // version — carries what 1.13 left. The 1.11 row is in no initialisation, and
    // it should not be: it has not existed since 1.13.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, CONFIG) VALUES ('ricerca', '{\"v\":3}');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_11__4_12.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, CONFIG) VALUES ('ricerca', '{\"v\":1}');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "DELETE FROM CATALOGO_WIDGET WHERE CHIAVE = 'ricerca';\n\
             INSERT INTO CATALOGO_WIDGET (CHIAVE, CONFIG) VALUES ('ricerca', '{\"v\":3}');",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons003);
    assert!(findings.is_empty(), "the 4.11 value was superseded by 4.13: {findings:?}");
}

#[test]
fn the_last_word_of_the_updates_is_still_checked() {
    // The other way round: the newest update leaves a value the initialisation
    // does not have. Nothing supersedes it, so it is a real divergence and the
    // rule has to keep reporting it — otherwise the fix above is a way of
    // switching CONS003 off.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, CONFIG) VALUES ('ricerca', '{\"v\":1}');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_11__4_12.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, CONFIG) VALUES ('ricerca', '{\"v\":1}');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, CONFIG) VALUES ('ricerca', '{\"v\":3}');",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons003);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].file.ends_with("4_12__4_13.sql"), "{}", findings[0].file);
    assert!(findings[0].title.contains("CONFIG"), "{}", findings[0].title);
}

#[test]
fn two_unrelated_rows_do_not_supersede_each_other() {
    // The near-match alone is not enough — the later row also has to be one the
    // initialisation actually has. Without that condition, two different rows
    // that happen to share a column value would silence each other and the rule
    // would go quiet on exactly the repositories it is for.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, GRUPPO) VALUES ('ricerca', 'base');",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_11__4_12.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, GRUPPO) VALUES ('esporta', 'base');\n\
             INSERT INTO CATALOGO_WIDGET (CHIAVE, GRUPPO) VALUES ('stampa', 'base');",
        ),
    ]);
    let report = repo.report();
    // Both are genuinely absent from the initialisation; neither replaced the other.
    assert_eq!(open_of(&report, RuleId::Cons003).len(), 2);
}

#[test]
fn a_project_that_maintains_the_two_halves_separately_checks_neither() {
    use picus_project::prelude::InitialisationModel;
    let report = diverged()
        .configured(|c| c.analysis.initialisation = InitialisationModel::Independent)
        .report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());
    assert!(report.was_skipped(RuleId::Cons002));
    assert!(report.was_skipped(RuleId::Cons003));
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
