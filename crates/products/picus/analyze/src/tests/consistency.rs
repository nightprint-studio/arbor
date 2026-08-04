//! CONS001 / CONS004 — one dialect against the other.

use crate::compare::column_key;
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

// ── How a column name is matched across the two dialects ─────────────────────

#[test]
fn a_quoted_column_on_one_side_is_the_same_column_as_an_unquoted_one_on_the_other() {
    // PostgreSQL folds an unquoted identifier to lower case and Oracle to upper,
    // so a team writing `"etichetta"` on one side and `ETICHETTA` on the other has
    // written the same column twice, each in its own engine's canonical form.
    // Comparing the spellings reported CONS004 on every such table — and the give
    // away was in the message, which printed the table upper case and the columns
    // lower case, side by side.
    let repo = Fixture::build(&[
        (
            "ORACLE/DATI/widget.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA, ORDINE) \
             VALUES ('ricerca', 'Ricerca', 10);",
        ),
        (
            "POSTGRES/DATI/widget.sql",
            "insert into catalogo_widget (\"chiave\", \"etichetta\", \"ordine\") \
             values ('ricerca', 'Ricerca', 10);",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons004);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_column_only_one_dialect_writes_is_still_reported() {
    // The rule still has to work, or the fix above is just a way of switching it
    // off quietly.
    let repo = Fixture::build(&[
        (
            "ORACLE/DATI/widget.sql",
            "INSERT INTO CATALOGO_WIDGET (CHIAVE, ETICHETTA) VALUES ('ricerca', 'Ricerca');",
        ),
        (
            "POSTGRES/DATI/widget.sql",
            "insert into catalogo_widget (\"chiave\", \"etichetta\", \"ordine\") \
             values ('ricerca', 'Ricerca', 10);",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons004);
    assert_eq!(findings.len(), 1, "{findings:?}");
    // Named in the comparison form, so the reader is not left wondering whether
    // the case is the difference.
    assert!(findings[0].consequence.contains("ORDINE"), "{}", findings[0].consequence);
}

#[test]
fn the_comparison_form_of_a_column_ignores_quoting_and_case() {
    use picus_parse::prelude::{ByteRange, ColumnRef};
    let column = |name: &str| ColumnRef { name: name.to_string(), range: ByteRange::new(0, 0) };
    assert_eq!(column_key(&column("etichetta")), "ETICHETTA");
    assert_eq!(column_key(&column("ETICHETTA")), "ETICHETTA");
    assert_eq!(column_key(&column("\"etichetta\"")), "ETICHETTA");
    assert_eq!(column_key(&column("\"Etichetta\"")), "ETICHETTA");
}

#[test]
fn a_finding_says_what_kind_of_thing_it_is_about() {
    // A repository whose folders are called AGGIORNAMENTO can have a table called
    // AGGIORNAMENTO too — an update log is exactly the sort of thing that gets
    // that name. "AGGIORNAMENTO is not touched by the Oracle scripts", anchored at
    // a folder path ending in AGGIORNAMENTO, read as a claim about the folder.
    let repo = Fixture::build(&[
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO PARAMETRI (COD) VALUES ('A');",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "insert into parametri (cod) values ('A');\n\
             insert into aggiornamento (versione) values ('4.13');",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].title.starts_with("The table AGGIORNAMENTO"),
        "{}",
        findings[0].title
    );
}

#[test]
fn a_table_a_view_only_reads_is_not_a_gap() {
    // The case this exists for: a view over a table that another repository
    // installs. The PostgreSQL views read it, the Oracle ones do not happen to,
    // and CONS001 reported it as a table the Oracle scripts never touch — a gap
    // in scripts that never installed it in the first place, and one nobody could
    // close by writing anything.
    let repo = Fixture::build(&[
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "CREATE VIEW V_ORDINI AS SELECT ID FROM ORDINI;",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "create view v_ordini as select o.id, c.descr from catalogo_esterno c join ordini o on o.cat = c.id;",
        ),
    ]);
    let report = repo.report();
    let titles: Vec<&str> =
        open_of(&report, RuleId::Cons001).iter().map(|f| f.title.as_str()).collect();
    assert!(!titles.iter().any(|t| t.contains("CATALOGO_ESTERNO")), "{titles:?}");
}

#[test]
fn a_table_one_dialect_writes_and_the_other_does_not_is_still_a_gap() {
    // The rule has to keep working, or the exemption above is a way of switching
    // CONS001 off for every table.
    let repo = Fixture::build(&[
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO MECATALOGO (ID) VALUES (1);",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "select id from mecatalogo;",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert!(
        findings.iter().any(|f| f.title.contains("MECATALOGO") && f.title.contains("PostgreSQL")),
        "{:?}",
        findings.iter().map(|f| &f.title).collect::<Vec<_>>()
    );
}

#[test]
fn a_drop_or_a_truncate_counts_as_touching_the_table() {
    // Neither leaves DML behind and neither defines anything, so both arrive as
    // plain references — but emptying a table on one engine and not the other is
    // exactly the divergence this rule is for.
    let repo = Fixture::build(&[
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "TRUNCATE TABLE MECATALOGO;",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "select id from mecatalogo;",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert!(
        findings.iter().any(|f| f.title.contains("MECATALOGO")),
        "{:?}",
        findings.iter().map(|f| &f.title).collect::<Vec<_>>()
    );
}

#[test]
fn an_engine_dictionary_view_is_not_an_object_this_repository_owns() {
    // `user_tab_cols` is an Oracle script asking Oracle about itself. Nobody wrote
    // a CREATE for it, no PostgreSQL script will ever have a counterpart —
    // PostgreSQL answers the same question from information_schema — so a row for
    // it in the inventory could only ever read as an unclosable gap.
    let repo = Fixture::build(&[
        (
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "DECLARE n NUMBER;\n\
             BEGIN\n\
               SELECT COUNT(*) INTO n FROM user_tab_cols WHERE table_name = 'CATALOGO_WIDGET';\n\
               IF n = 0 THEN\n\
                 EXECUTE IMMEDIATE 'ALTER TABLE CATALOGO_WIDGET ADD (ORDINE NUMBER)';\n\
               END IF;\n\
             END;",
        ),
        (
            "POSTGRES/AGGIORNAMENTO/4_12__4_13.sql",
            "alter table catalogo_widget add column ordine integer;",
        ),
    ]);
    let report = repo.report();
    let titles: Vec<&str> =
        open_of(&report, RuleId::Cons001).iter().map(|f| f.title.as_str()).collect();
    assert!(
        !titles.iter().any(|t| t.to_uppercase().contains("USER_TAB_COLS")),
        "{titles:?}"
    );
}

#[test]
fn a_project_that_does_not_compare_its_dialects_says_so_rather_than_going_quiet() {
    use picus_project::prelude::AnalysisSettings;
    let repo = || {
        Fixture::build(&[
            (
                "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
                "CREATE TABLE CATALOGO_WIDGET (CHIAVE VARCHAR2(30));",
            ),
            (
                "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
                "create table parametri (cod varchar(30));",
            ),
        ])
    };
    // It fires by default: this is what the product is for.
    assert!(!open_of(&repo().report(), RuleId::Cons001).is_empty());

    let report = repo()
        .configured(|c| {
            c.analysis = AnalysisSettings { compare_dialects: false, ..c.analysis.clone() }
        })
        .report();
    assert!(open_of(&report, RuleId::Cons001).is_empty());
    assert!(open_of(&report, RuleId::Cons004).is_empty());
    // …and both are listed as rules that did not run, naming the setting, because
    // a report that quietly stopped comparing reads exactly like a clean one.
    assert!(report.was_skipped(RuleId::Cons001));
    assert!(report.was_skipped(RuleId::Cons004));
    let reason = &report.skipped.iter().find(|s| s.rule == RuleId::Cons001).expect("skipped").reason;
    assert!(reason.contains("project settings"), "{reason}");

    // The rest of the report is untouched — that is the whole point of the switch.
    assert!(!report.skipped.iter().any(|s| s.rule == RuleId::Dup001));
}
