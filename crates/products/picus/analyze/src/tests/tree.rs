//! The rules against the shape real repositories have: the role at the top of
//! the tree, the dialect at the bottom.
//!
//! ```text
//! AGGIORNAMENTO/2024/ORA    update, Oracle
//! AGGIORNAMENTO/2024/POS    update, PostgreSQL
//! AGGIORNAMENTO/2025/ORA    update, Oracle
//! ```
//!
//! Nothing in here is a special case. A lane is `(dialect, role)`, both come
//! from wherever in the tree they were declared, and a lane with two folders in
//! it is one story — which is precisely what the previous model, where a lane was
//! a top-level branch, could not say.

use crate::rule::RuleId;
use crate::testing::Fixture;
use crate::tests::open_of;

#[test]
fn a_dialect_declared_at_the_bottom_of_the_tree_is_compared_like_any_other() {
    let repo = Fixture::build(&[
        (
            "AGGIORNAMENTO/2024/ORA/4_12.sql",
            "INSERT INTO LISTINI (COD) VALUES ('STD2026');",
        ),
        ("AGGIORNAMENTO/2024/POS/4_12.sql", "-- nothing here"),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons001);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].title.contains("LISTINI"));
    assert!(findings[0].title.contains("PostgreSQL"), "{}", findings[0].title);
    assert_eq!(findings[0].file, "AGGIORNAMENTO/2024/POS");
}

#[test]
fn two_folders_of_one_lane_are_one_story() {
    // The failure the flat model produced: `2024/ORA` and `2025/ORA` are both the
    // Oracle update lane, and reading either alone reports the other as a gap.
    let repo = Fixture::build(&[
        ("AGGIORNAMENTO/2024/ORA/4_12.sql", "INSERT INTO LISTINI (COD) VALUES ('A');"),
        ("AGGIORNAMENTO/2025/ORA/4_13.sql", "-- nothing new here"),
        ("AGGIORNAMENTO/2024/POS/4_12.sql", "insert into listini (cod) values ('A');"),
    ]);
    assert!(open_of(&repo.report(), RuleId::Cons001).is_empty());
}

#[test]
fn a_leaf_nobody_could_identify_takes_part_in_nothing() {
    // `MSQ` matches no keyword Picus knows, so it has no dialect. Comparing it
    // against the Oracle folder beside it would report every object in the
    // repository as missing from it.
    let repo = Fixture::build(&[
        ("AGGIORNAMENTO/2024/ORA/4_12.sql", "INSERT INTO LISTINI (COD) VALUES ('A');"),
        ("AGGIORNAMENTO/2024/MSQ/4_12.sql", "-- nothing here"),
    ]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons001).is_empty());
    assert!(open_of(&report, RuleId::Cons004).is_empty());
}

#[test]
fn the_two_halves_of_one_dialect_are_read_across_the_whole_tree() {
    // CONS002/CONS003 compare install against upgrade **within one dialect**, and
    // the two halves are now three levels apart in different subtrees.
    let repo = Fixture::build(&[
        (
            "INIZIALIZZAZIONE/2024/ORA/01.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 15);",
        ),
        (
            "AGGIORNAMENTO/2024/ORA/4_12.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 15);\n\
             INSERT INTO PARAMETRI (COD, VALORE) VALUES ('MAX_RIGHE', 50);",
        ),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Cons003);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].title.contains("MAX_RIGHE"));
    assert_eq!(findings[0].also_at.as_deref(), Some("INIZIALIZZAZIONE/2024/ORA"));
}

#[test]
fn one_dialects_install_story_is_never_read_as_anothers() {
    // The Oracle initialisation is not the PostgreSQL updates' initialisation,
    // however close together they sit in the tree.
    let repo = Fixture::build(&[
        (
            "INIZIALIZZAZIONE/2024/ORA/01.sql",
            "INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 15);",
        ),
        (
            "AGGIORNAMENTO/2024/POS/4_12.sql",
            "insert into parametri (cod, valore) values ('SOGLIA', 15);",
        ),
    ]);
    let report = repo.report();
    assert!(open_of(&report, RuleId::Cons002).is_empty());
    assert!(open_of(&report, RuleId::Cons003).is_empty());
}

#[test]
fn the_version_chain_is_read_per_folder_wherever_the_folder_is() {
    // Each year folder is its own chain of files; a hole in one of them is a
    // finding, and the folder's depth has nothing to do with it.
    let repo = Fixture::build(&[
        ("AGGIORNAMENTO/2024/ORA/4_11__4_12.sql", "UPDATE PARAMETRI SET VALORE = 1;"),
        ("AGGIORNAMENTO/2024/ORA/4_13__4_14.sql", "UPDATE PARAMETRI SET VALORE = 2;"),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Ver003);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].title.contains("hole"), "{}", findings[0].title);
    assert_eq!(findings[0].file, "AGGIORNAMENTO/2024/ORA/4_13__4_14.sql");
}

#[test]
fn an_object_created_twice_in_one_dialect_is_a_duplicate_across_the_whole_tree() {
    let repo = Fixture::build(&[
        ("INIZIALIZZAZIONE/2024/ORA/01.sql", "CREATE TABLE LISTINI (COD VARCHAR2(30));"),
        ("INIZIALIZZAZIONE/2025/ORA/01.sql", "CREATE TABLE LISTINI (COD VARCHAR2(30));"),
        // …and the same table created for another dialect is the point of the
        // repository, not a third finding.
        ("INIZIALIZZAZIONE/2024/POS/01.sql", "create table listini (cod varchar(30));"),
    ]);
    let report = repo.report();
    let findings = open_of(&report, RuleId::Dup002);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].file, "INIZIALIZZAZIONE/2025/ORA/01.sql");
}
