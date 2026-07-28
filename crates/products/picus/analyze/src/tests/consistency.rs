//! CONS001 / CONS004 — one branch against the other.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

// ── CONS001 ──────────────────────────────────────────────────────────────────

#[test]
fn an_object_one_branch_never_touches_is_reported_against_that_branch() {
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
    assert_eq!(findings[0].branch_id, "pg");
    assert!(findings[0].title.contains("LISTINI"));
    // The jump the user wants is the branch that DOES do it.
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
fn a_branch_whose_engine_is_unknown_is_left_out_of_the_comparison() {
    // `picus-project` refuses to guess a branch's engine. A rule that compared
    // COMMON/ with the Oracle branch would report every object as missing from
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
fn a_role_only_one_branch_has_is_not_a_gap() {
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
fn an_object_absent_from_a_whole_branch_is_one_finding_not_one_per_role() {
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
fn two_branches_that_agree_produce_nothing() {
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
    // the two branches diverge here would be a finding nobody can ever close.
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
fn a_column_one_branch_never_writes_is_reported_even_when_the_values_are_computed() {
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
    assert_eq!(findings[0].branch_id, "pg");
    assert!(findings[0].consequence.contains("DESCR"), "{}", findings[0].consequence);
}

