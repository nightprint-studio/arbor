//! CONS001 / CONS004 — one dialect against the other.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

// ── CONS001 ──────────────────────────────────────────────────────────────────

#[test]
fn an_object_one_dialect_never_touches_is_reported_against_that_dialect() {
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));\nCREATE TABLE LISTINI (COD VARCHAR2(30));",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert_eq!(findings.len(), 1);
    // Anchored at the PostgreSQL folder that should have had the statement.
    assert_eq!(findings[0].file, "POSTGRES/INIZIALIZZAZIONE");
    assert!(findings[0].title.contains("LISTINI"));
    assert!(findings[0].title.contains("PostgreSQL"), "{}", findings[0].title);
    // The jump the user wants is the dialect that DOES do it.
    assert_eq!(
        findings[0].also_at.as_deref(),
        Some("ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql:2")
    );
    assert!(findings[0].fix_label.is_some());
}

#[test]
fn a_package_is_never_reported_as_missing_from_postgresql() {
    // Packages are Oracle-only. A finding here would be permanent, unfixable and
    // at the top of the report of every Oracle-first repository there is.
    let repo = Fixture::build(&[
        (
            "ORACLE/PROCEDURE/PKG.sql",
            "CREATE PACKAGE PKG_CLIENTI AS PROCEDURE P; END;",
        ),
        ("POSTGRES/PROCEDURE/fn.sql", "create function f() returns integer as $$ begin return 1; end; $$ language plpgsql;"),
    ]);
    let report = repo.report();
    let missing: Vec<&str> =
        open_of(&report, RuleId::Cons001).iter().map(|f| f.title.as_str()).collect();
    assert!(
        !missing.iter().any(|t| t.contains("PKG_CLIENTI")),
        "a package has no PostgreSQL counterpart to be missing from: {missing:?}"
    );
}

#[test]
fn a_folder_whose_engine_is_unknown_is_left_out_of_the_comparison() {
    // `picus-project` refuses to guess a folder's engine. A rule that compared
    // COMMON/ with the Oracle folders would report every object as missing from
    // it, and the first run would be nothing but noise.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        ("COMMON/notes.sql", "-- nothing here"),
    ]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons001).is_empty());
}

#[test]
fn a_role_only_one_dialect_has_is_not_a_gap() {
    // Oracle keeps its routines in a folder; PostgreSQL has no such folder at
    // all. That is a layout difference, not a missing object, and there is
    // nothing to compare against.
    let repo = Fixture::build(&[
        (
            "ORACLE/PROCEDURE/P.sql",
            "CREATE PROCEDURE RICALCOLA AS BEGIN NULL; END;",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
    ]);
    let report = repo.report();
    let titles: Vec<&str> =
        open_of(&report, RuleId::Cons001).iter().map(|f| f.title.as_str()).collect();
    assert!(!titles.iter().any(|t| t.contains("RICALCOLA")), "{titles:?}");
}

#[test]
fn an_object_absent_from_a_whole_dialect_is_one_finding_not_one_per_role() {
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE LISTINI (COD VARCHAR2(30));",
        ),
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO LISTINI (COD) VALUES ('STD2026');",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
        ("POSTGRES/AGGIORNAMENTO/4_12__4_13.sql", "-- nothing"),
    ]);
    let report = repo.report();
    let listini: Vec<_> = open_of(&report, RuleId::Cons001)
        .into_iter()
        .filter(|f| f.title.contains("LISTINI"))
        .collect();
    assert_eq!(listini.len(), 1, "one problem, one fix, one row: {listini:?}");
}

#[test]
fn two_dialects_that_agree_produce_nothing() {
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
    ]);
    assert!(open_of(&repo.report(), RuleId::Cons001).is_empty());
}

// ── CONS004 ──────────────────────────────────────────────────────────────────

#[test]
fn the_same_table_loaded_with_different_rows_is_reported() {
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA_SCONTO', 15);",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
            "insert into parametri (cod, valore) values ('SOGLIA_SCONTO', 10);",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons004);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].consequence.contains("SOGLIA_SCONTO"), "{}", findings[0].consequence);
    assert!(findings[0].also_at.is_some());
}

#[test]
fn the_same_row_spelled_in_two_dialects_is_not_a_divergence() {
    // Oracle doubles the quote, PostgreSQL dollar-quotes, and the identifiers
    // fold in opposite directions. All of that is spelling, not data.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, DESCR) VALUES ('SOGLIA', 'l''ora');",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
            "insert into parametri (descr, cod) values ($$l's$$, 'SOGLIA');",
        ),
    ]);
    // The values genuinely differ here, so assert the mechanism on equal ones:
    let same = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, DESCR) VALUES ('SOGLIA', 'l''ora');",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
            "insert into parametri (descr, cod) values ($$l'ora$$, 'SOGLIA');",
        ),
    ]);
    assert!(open_of(&same.report(), RuleId::Cons004).is_empty());
    // …and that the differing pair is still caught, so the test above is not
    // passing because the comparison is switched off.
    assert_eq!(open_of(&repo.report(), RuleId::Cons004).len(), 1);
}

#[test]
fn a_computed_value_makes_the_rows_incomparable_rather_than_different() {
    // SYSDATE and now() are the same intention and never the same value. Claiming
    // the two dialects diverge here would be a finding nobody can ever close.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, DATA_AGG) VALUES ('SOGLIA', SYSDATE);",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
            "insert into parametri (cod, data_agg) values ('SOGLIA', now());",
        ),
    ]);
    assert!(open_of(&repo.report(), RuleId::Cons004).is_empty());
}

#[test]
fn a_column_one_dialect_never_writes_is_reported_even_when_the_values_are_computed() {
    // The column set survives what the row comparison cannot read, and it is the
    // more useful half of the answer anyway.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
            "INSERT INTO PARAMETRI (COD, DESCR, DATA_AGG) VALUES ('SOGLIA', 'x', SYSDATE);",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
            "insert into parametri (cod, data_agg) values ('SOGLIA', now());",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons004);
    assert_eq!(findings.len(), 1);
    // Reported on the side that ends up with less — the PostgreSQL script.
    assert!(findings[0].file.starts_with("POSTGRES/"), "{}", findings[0].file);
    assert!(findings[0].consequence.contains("DESCR"), "{}", findings[0].consequence);
}


// ── Portable folders: one file that counts for both engines ─────────────────

#[test]
fn a_portable_folder_satisfies_cons001_on_both_sides_at_once() {
    // The property the whole feature rests on. `COMUNE` is declared portable, so
    // its INSERT is present on Oracle *and* on PostgreSQL — and neither dialect
    // may be reported as missing the object it fills.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
        (
            "COMUNE/DATI/01_parametri.sql",
            "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA');",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert!(
        findings.iter().all(|f| !f.file.starts_with("COMUNE")),
        "a portable folder is never the side that is missing something: {findings:?}"
    );
    // Neither dialect is reported as failing to load PARAMETRI in its data role:
    // the portable folder is in both lanes and covers both.
    assert!(
        !findings.iter().any(|f| f.title.contains("PARAMETRI")),
        "{findings:?}"
    );
}

#[test]
fn without_the_portable_declaration_the_same_layout_reports_a_gap() {
    // The control for the test above: an ordinary Oracle data folder covers only
    // Oracle, so PostgreSQL genuinely is missing the rows and is told so. Without
    // this, "no findings" above could just mean the rule never ran.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
        ("ORACLE/DATI/01_parametri.sql", "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA');"),
        ("POSTGRES/DATI/01_altro.sql", "insert into listini (cod) values ('X');"),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert!(
        findings.iter().any(|f| f.title.contains("PARAMETRI") && f.title.contains("PostgreSQL")),
        "{findings:?}"
    );
}

#[test]
fn a_portable_folder_alone_leaves_no_dialect_uncovered() {
    // A repository whose data is written once, portably, and nowhere else. Both
    // dialects participate — the folder is in both lanes — and both are covered,
    // so the report is empty rather than reporting each dialect against itself.
    let repo = Fixture::build(&[
        (
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
        ),
        (
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table parametri (cod varchar(30));",
        ),
        ("COMUNE/DATI/a.sql", "INSERT INTO PARAMETRI (COD) VALUES ('A');"),
        ("COMUNE/DATI/b.sql", "INSERT INTO PARAMETRI (COD) VALUES ('B');"),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert!(findings.is_empty(), "{findings:?}");
}
