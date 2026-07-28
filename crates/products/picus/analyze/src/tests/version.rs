//! VER001 / VER002 / VER003.

use picus_project::prelude::NamingScheme;

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

const GUARDED: &str = "DECLARE v VARCHAR2(30);\n\
     BEGIN\n\
       SELECT VERSIONE INTO v FROM VERSIONE_DB;\n\
       IF v = '4.12' THEN\n\
         INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');\n\
         UPDATE VERSIONE_DB SET VERSIONE = '4.13';\n\
       END IF;\n\
     END;";

// ── VER001 ───────────────────────────────────────────────────────────────────

#[test]
fn an_update_script_that_writes_without_reading_the_version_is_reported() {
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Ver001);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].line, Some(1));
    assert!(findings[0].consequence.contains("second time"), "{}", findings[0].consequence);
}

#[test]
fn the_same_statement_in_an_initialisation_folder_needs_no_guard() {
    // An init script runs on an empty database: a starting-version guard there
    // is a condition that is never true. A rule that fired here would teach
    // people the report is wrong about init folders.
    let repo = Fixture::build(&[(
        "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
        "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');",
    )]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Ver001).is_empty());
    assert!(open_of(&report, RuleId::Ver002).is_empty());
}

#[test]
fn a_guarded_update_script_is_clean() {
    let repo = Fixture::build(&[("ORACLE/AGGIORNAMENTO/4_12__4_13.sql", GUARDED)]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Ver001).is_empty(), "{:?}", report.findings);
    assert!(open_of(&report, RuleId::Ver002).is_empty());
}

#[test]
fn carrying_the_version_forward_is_not_the_same_as_checking_it() {
    // The closing UPDATE names the version table and writes it. Counting the
    // mention alone would make every VER002-clean file VER001-clean too, and the
    // half-guarded script — which is the common real bug — would never be seen.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');\n\
         UPDATE VERSIONE_DB SET VERSIONE = '4.13';",
    )]);
    let report = repo.report();
    assert_eq!(open_of(&report, RuleId::Ver001).len(), 1, "the guard is still missing");
    assert!(open_of(&report, RuleId::Ver002).is_empty(), "the bump is there");
}

#[test]
fn an_update_file_that_changes_nothing_is_not_missing_a_guard() {
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "-- documentation only\nSELECT VERSIONE FROM VERSIONE_DB;",
    )]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Ver001).is_empty());
    assert!(open_of(&report, RuleId::Ver002).is_empty());
}

// ── VER002 ───────────────────────────────────────────────────────────────────

#[test]
fn an_update_script_that_never_bumps_the_version_is_reported() {
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "DECLARE v VARCHAR2(30);\n\
         BEGIN\n\
           SELECT VERSIONE INTO v FROM VERSIONE_DB;\n\
           IF v = '4.12' THEN\n\
             INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');\n\
           END IF;\n\
         END;",
    )]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Ver002);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].consequence.contains("stalls"), "{}", findings[0].consequence);
    assert!(open_of(&report, RuleId::Ver001).is_empty(), "the guard IS there");
}

#[test]
fn a_project_with_no_version_table_skips_both_guard_rules_out_loud() {
    // Silence here would look exactly like a repository whose update scripts are
    // all guarded, which is the opposite of the truth.
    let repo = Fixture::build(&[(
        "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
        "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');",
    )])
    .configured(|config| config.version_table.table = String::new());
    let report = repo.report();
    assert!(open_of(&report, RuleId::Ver001).is_empty());
    assert!(report.was_skipped(RuleId::Ver001));
    assert!(report.was_skipped(RuleId::Ver002));
    // Each of the two says *why*, in terms of the thing that is missing. Scoped to
    // these two rules on purpose: other rules stand down for reasons of their own —
    // `CONS002` is not a question under the default initialisation model — and a
    // blanket assertion over every skipped line would be a test about a list that
    // has nothing to do with version tables.
    for rule in [RuleId::Ver001, RuleId::Ver002] {
        let line = report.skipped.iter().find(|s| s.rule == rule).expect("skipped");
        assert!(line.reason.contains("version table"), "{rule}: {}", line.reason);
    }
}

// ── VER003 ───────────────────────────────────────────────────────────────────

fn update_files(names: &[&str]) -> Vec<(String, String)> {
    names
        .iter()
        .map(|name| {
            (format!("ORACLE/AGGIORNAMENTO/{name}"), "-- nothing in particular\n".to_string())
        })
        .collect()
}

fn chain_report(names: &[&str]) -> crate::report::Report {
    let owned = update_files(names);
    let borrowed: Vec<(&str, &str)> =
        owned.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    Fixture::build(&borrowed).report()
}

#[test]
fn a_hole_in_the_chain_is_reported_on_the_file_that_cannot_start() {
    let report = chain_report(&["4_11__4_12.sql", "4_13__4_14.sql"]);
    let findings = open_of(&report, RuleId::Ver003);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].file, "ORACLE/AGGIORNAMENTO/4_13__4_14.sql");
    assert!(findings[0].title.contains("hole"), "{}", findings[0].title);
}

#[test]
fn an_unbroken_chain_is_clean_and_is_not_sorted_as_text() {
    // The bug a string sort produces: 4.9 after 4.12, so the chain reads as
    // broken on a repository that is perfectly fine.
    let report = chain_report(&["4_9__4_10.sql", "4_10__4_11.sql", "4_11__4_12.sql"]);
    assert!(open_of(&report, RuleId::Ver003).is_empty(), "{:?}", report.findings);
}

#[test]
fn two_files_installing_the_same_version_are_reported() {
    let report = chain_report(&["4_11__4_12.sql", "4_11__4_12b.sql"]);
    let findings = open_of(&report, RuleId::Ver003);
    // The second file does not match the default pattern (`4_12b` is not
    // numeric), so this repository has one update file and no chain at all.
    assert!(findings.is_empty());

    let report = chain_report(&["4_11__4_12.sql", "4_10__4_12.sql"]);
    let findings = open_of(&report, RuleId::Ver003);
    assert!(
        findings.iter().any(|f| f.title.contains("both install 4.12")),
        "{:?}",
        findings.iter().map(|f| &f.title).collect::<Vec<_>>()
    );
}

#[test]
fn an_overlap_is_worded_as_an_overlap_not_as_a_hole() {
    let report = chain_report(&["4_10__4_13.sql", "4_11__4_14.sql"]);
    let findings = open_of(&report, RuleId::Ver003);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].title.contains("overlap"), "{}", findings[0].title);
}

#[test]
fn a_scheme_with_no_starting_version_reports_itself_as_skipped() {
    // The case `docs/picus-design.md` §6.1 calls out by name: without a `from`
    // there is no chain, only a list, and the rule must say so rather than pass.
    let owned = update_files(&["V4_12__add_threshold.sql", "V4_14__add_index.sql"]);
    let borrowed: Vec<(&str, &str)> =
        owned.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    let report = Fixture::build(&borrowed)
        .configured(|config| {
            config.naming = NamingScheme {
                pattern: r"(?i)^V(?P<to>\d+(?:_\d+)*)__.+\.sql$".to_string(),
                template: "V{to}__change.sql".to_string(),
                separator: '_',
            };
        })
        .report();

    assert!(open_of(&report, RuleId::Ver003).is_empty());
    let skipped = report
        .skipped
        .iter()
        .find(|s| s.rule == RuleId::Ver003)
        .expect("VER003 must report itself as skipped");
    assert!(skipped.reason.contains("starts from"), "{}", skipped.reason);
    assert_eq!(skipped.scope, "ORACLE/AGGIORNAMENTO");
}

#[test]
fn a_folder_where_nothing_matches_the_pattern_is_skipped_rather_than_passed() {
    let report = chain_report(&["install_tutto.sql", "rollback.sql"]);
    assert!(open_of(&report, RuleId::Ver003).is_empty());
    let skipped =
        report.skipped.iter().find(|s| s.rule == RuleId::Ver003).expect("skipped, not silent");
    assert!(skipped.reason.contains("pattern"), "{}", skipped.reason);
}

#[test]
fn an_invalid_pattern_is_skipped_with_the_users_own_pattern_in_the_reason() {
    let owned = update_files(&["4_11__4_12.sql"]);
    let borrowed: Vec<(&str, &str)> =
        owned.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    let report = Fixture::build(&borrowed)
        .configured(|config| config.naming.pattern = "^(?P<to>[unclosed".to_string())
        .report();
    let skipped = report.skipped.iter().find(|s| s.rule == RuleId::Ver003).expect("skipped");
    assert!(skipped.reason.contains("[unclosed"), "{}", skipped.reason);
}
