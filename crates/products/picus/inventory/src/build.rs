//! [`Inventory::build`] — the walk that turns parsed files into rows.
//!
//! One pass over every placed script. For each statement, the objects it names
//! are folded to their comparison form and de-duplicated, so the coverage cell
//! counts statements rather than mentions. The sites are collected in the same
//! pass, keyed by `(statement, defining)` so a statement that names the same
//! object as both a definition and a reference keeps both facts and nothing else
//! is repeated.

use std::collections::{BTreeMap, BTreeSet};

use picus_parse::prelude::{ObjectRef, Statement, StatementKind};

use crate::entry::{ObjectEntry, ObjectSite};
use crate::input::{ParsedProject, ParsedScript, Placement};
use crate::kind::{InventoryKind, Namespace};
use crate::wire::InventoryObject;

/// The whole index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    /// Every coverage column, in tree order. Kept alongside the rows so a caller
    /// can render the table without re-deriving the header from the project.
    pub keys: Vec<String>,
    /// Rows, ordered by kind then name — a stable order that does not depend on
    /// which file happened to be parsed first.
    pub objects: Vec<ObjectEntry>,
}

impl Inventory {
    pub fn build(project: &ParsedProject<'_>) -> Inventory {
        let keys = project.coverage_keys();
        let mut rows: BTreeMap<(Namespace, String), ObjectEntry> = BTreeMap::new();

        for (script, placement) in project.placed() {
            for (index, statement) in script.parsed.statements.iter().enumerate() {
                index_statement(&mut rows, &keys, script, placement, index, statement);
            }
        }

        // Ordered by kind then name, which is the order the view groups them in.
        // Done here rather than by the map's key so the *resolved* kind decides:
        // a row that started as a table reference and turned out to be a view has
        // to sit with the views.
        let mut objects: Vec<ObjectEntry> = rows.into_values().collect();
        objects.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
        Inventory { keys, objects }
    }

    pub fn find(&self, kind: InventoryKind, name: &str) -> Option<&ObjectEntry> {
        self.objects.iter().find(|o| o.kind == kind && o.name == name)
    }

    /// The shape the inventory view renders.
    pub fn wire(&self) -> Vec<InventoryObject> {
        self.objects.iter().map(InventoryObject::from_entry).collect()
    }
}

/// Index one statement's objects. Split out because the outer loop is only a
/// walk and this is where every decision is.
fn index_statement(
    rows: &mut BTreeMap<(Namespace, String), ObjectEntry>,
    keys: &[String],
    script: &ParsedScript<'_>,
    placement: Placement<'_>,
    index: usize,
    statement: &Statement,
) {
    let coverage_key = placement.coverage_key();
    // A statement counts once per object however many times it names it, so the
    // coverage cell is a count of statements and stays comparable between a
    // terse Oracle script and a chatty PostgreSQL one.
    let mut counted: BTreeSet<(Namespace, String)> = BTreeSet::new();
    let mut sited: BTreeSet<(Namespace, String, bool)> = BTreeSet::new();

    let creating = statement.kind == StatementKind::Create;
    let occurrences = statement
        .defines
        .iter()
        .map(|r| (r, true))
        .chain(statement.references.iter().map(|r| (r, false)));

    for (object, defining) in occurrences {
        let Some(kind) = InventoryKind::from_parse(object.kind) else { continue };
        let name = folded(object);
        if name.is_empty() {
            continue;
        }
        let space = kind.namespace();
        // Both sets are consulted before the row is touched, so a statement that
        // names the same object again — the common case in a real script — costs
        // two lookups and no allocation at all. Reaching for the row first would
        // clone the name on every mention rather than on every new fact.
        let counts = counted.insert((space, name.clone()));
        let sites = sited.insert((space, name.clone(), defining));
        if !counts && !sites {
            continue;
        }
        let row = rows
            .entry((space, name.clone()))
            .or_insert_with(|| new_entry(kind, &name, keys));

        // A reference carries no kind — the grammar reports every `FROM x` as a
        // table — so the row takes the most informative mention it has seen. A
        // definition settles it outright; short of one, anything that is not the
        // `table` fallback beats the fallback. Without this a view read fifty
        // times and created once is two rows, and the fifty-strong one says
        // `table`.
        if defining || kind.confidence() > row.kind.confidence() {
            row.kind = kind;
        }

        if counts {
            *row.coverage.entry(coverage_key.clone()).or_insert(0) += 1;
        }
        if sites {
            row.sites.push(ObjectSite {
                path: script.path.to_string(),
                folder_path: placement.folder.path.clone(),
                scope: placement.scope(),
                role: placement.effective_role(),
                statement_index: index,
                range: object.range,
                line: script.parsed.line_of(object.range.start),
                declared_kind: object.kind,
                defining,
                creating: defining && creating,
            });
        }
    }
}

fn new_entry(kind: InventoryKind, name: &str, keys: &[String]) -> ObjectEntry {
    ObjectEntry {
        name: name.to_string(),
        kind,
        // Seeded with every column at zero. A missing key and a zero would read
        // the same to a caller that used `.get()`, but not to one that iterates
        // the map — and "this folder has none" is the answer CONS001 is looking
        // for, so it has to be present rather than inferred from an absence.
        coverage: keys.iter().map(|k| (k.clone(), 0usize)).collect(),
        sites: Vec::new(),
    }
}

/// The comparison form of an object's name.
///
/// **Unqualified on purpose.** `picus-parse` can give `APP.PARAMETRI`, but the
/// Oracle scripts qualify with the owning user and the PostgreSQL ones with
/// `public`, inconsistently and usually not at all. Keying on the qualified name
/// would make `PARAMETRI` and `public.parametri` two objects and every row in the
/// repository would look half-missing.
fn folded(object: &ObjectRef) -> String {
    object.folded_name()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ParsedScript;
    use crate::testing::{parsed, project};
    use picus_parse::prelude::{EngineKind, ParsedFile};

    struct Fixture {
        parses: Vec<(String, String, ParsedFile)>,
    }

    impl Fixture {
        fn new(files: &[(&str, &str, EngineKind)]) -> Fixture {
            Fixture {
                parses: files
                    .iter()
                    .map(|(path, src, engine)| {
                        (path.to_string(), src.to_string(), parsed(src, *engine))
                    })
                    .collect(),
            }
        }

        fn scripts(&self) -> Vec<ParsedScript<'_>> {
            self.parses
                .iter()
                .map(|(path, src, parse)| ParsedScript {
                    path: path.as_str(),
                    source: src.as_str(),
                    parsed: parse,
                })
                .collect()
        }
    }

    #[test]
    fn the_same_table_in_both_dialects_is_one_row() {
        // This is the product's whole premise: Oracle writes PARAMETRI,
        // PostgreSQL writes parametri, and they are the same object.
        let f = Fixture::new(&[
            (
                "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
                "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
                EngineKind::Oracle,
            ),
            (
                "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
                "create table parametri (cod varchar(30));",
                EngineKind::Postgres,
            ),
        ]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);

        assert_eq!(inventory.objects.len(), 1);
        let row = &inventory.objects[0];
        assert_eq!(row.name, "PARAMETRI");
        assert_eq!(row.kind, InventoryKind::Table);
        assert_eq!(row.coverage_in("ORACLE/INIZIALIZZAZIONE"), 1);
        assert_eq!(row.coverage_in("POSTGRES/INIZIALIZZAZIONE"), 1);
    }

    #[test]
    fn a_view_is_one_row_however_many_times_it_is_read() {
        // The grammar reports every `FROM x` as a *table* reference, because at
        // that point it cannot know what `x` turned out to be. Keying rows on that
        // kind gave a view created once and read three times two rows: a `view`
        // with one statement in it and a `table` with three — and the second one,
        // the wrong one, is the one a reader sees.
        let f = Fixture::new(&[(
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE VIEW V_ORDINI AS SELECT * FROM ORDINI;\n\
             SELECT * FROM V_ORDINI;\n\
             SELECT COUNT(*) FROM V_ORDINI;",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);

        let view = inventory.find(InventoryKind::View, "V_ORDINI").expect("one row for the view");
        assert_eq!(view.coverage_in("ORACLE/INIZIALIZZAZIONE"), 3, "every mention counts on it");
        assert!(
            inventory.find(InventoryKind::Table, "V_ORDINI").is_none(),
            "and there is no second row calling it a table"
        );
        // The table it is built over is still its own row, and still a table.
        assert!(inventory.find(InventoryKind::Table, "ORDINI").is_some());
    }

    #[test]
    fn a_trigger_may_share_a_name_with_a_table() {
        // Triggers have their own namespace in both engines, so these are two
        // objects and folding them into one row would hide whichever came second.
        let f = Fixture::new(&[(
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE AUDIT_ORDINI (ID NUMBER);\n\
             CREATE TRIGGER AUDIT_ORDINI BEFORE INSERT ON ORDINI BEGIN NULL; END;",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);

        assert!(inventory.find(InventoryKind::Table, "AUDIT_ORDINI").is_some());
        assert!(inventory.find(InventoryKind::Trigger, "AUDIT_ORDINI").is_some());
    }

    #[test]
    fn a_quoted_lowercase_name_is_not_the_unquoted_one() {
        // In both engines `"parametri"` and `parametri` are genuinely different
        // objects, and folding them together would hide a real bug.
        let f = Fixture::new(&[(
            "POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql",
            "create table \"parametri\" (cod varchar(30));\ncreate table parametri (cod varchar(30));",
            EngineKind::Postgres,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);

        let names: Vec<&str> = inventory.objects.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["PARAMETRI", "parametri"]);
    }

    #[test]
    fn a_qualified_name_is_the_same_object_as_a_bare_one() {
        // Oracle qualifies with the owning user, PostgreSQL with `public`, and
        // both do it inconsistently. Keying on the qualifier would make every
        // table look half-missing.
        let f = Fixture::new(&[
            (
                "ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql",
                "INSERT INTO APP.PARAMETRI (COD) VALUES ('X');",
                EngineKind::Oracle,
            ),
            (
                "POSTGRES/INIZIALIZZAZIONE/02_parametri.sql",
                "insert into public.parametri (cod) values ('X');",
                EngineKind::Postgres,
            ),
        ]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        assert_eq!(inventory.objects.len(), 1);
        assert_eq!(inventory.objects[0].name, "PARAMETRI");
    }

    #[test]
    fn a_statement_naming_a_table_four_times_counts_once() {
        // Coverage is "how many statements touch this", not "how often is it
        // mentioned" — otherwise a verbose folder always looks better covered.
        let f = Fixture::new(&[(
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "UPDATE PARAMETRI SET VALORE = (SELECT MAX(VALORE) FROM PARAMETRI) \
             WHERE COD IN (SELECT COD FROM PARAMETRI WHERE COD LIKE 'S%');",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        assert_eq!(inventory.objects[0].coverage_in("ORACLE/AGGIORNAMENTO"), 1);
    }

    #[test]
    fn a_gap_between_dialects_is_a_zero_that_is_actually_there() {
        let f = Fixture::new(&[(
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO');",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        let row = &inventory.objects[0];
        // Every column of the project is present, so the interesting `0` is a
        // value and not an absence.
        assert_eq!(row.coverage.len(), 4);
        assert_eq!(row.coverage_in("ORACLE/AGGIORNAMENTO"), 1);
        assert_eq!(row.coverage_in("POSTGRES/AGGIORNAMENTO"), 0);
        assert!(row.coverage.contains_key("POSTGRES/INIZIALIZZAZIONE"));
    }

    #[test]
    fn an_alter_is_a_definition_but_not_a_creation() {
        let f = Fixture::new(&[
            (
                "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
                "CREATE TABLE PARAMETRI (COD VARCHAR2(30));",
                EngineKind::Oracle,
            ),
            (
                "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
                "ALTER TABLE PARAMETRI ADD (DESCR VARCHAR2(200));",
                EngineKind::Oracle,
            ),
        ]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        let row = &inventory.objects[0];
        assert_eq!(row.sites.iter().filter(|s| s.defining).count(), 2);
        // …but only one creation, which is what keeps DUP002 quiet about a table
        // that has simply been maintained.
        assert_eq!(row.creations().count(), 1);
    }

    #[test]
    fn a_drop_names_an_object_without_defining_it() {
        let f = Fixture::new(&[(
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "DROP TABLE LISTINI;",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        let row = inventory.find(InventoryKind::Table, "LISTINI").expect("indexed");
        assert!(row.sites.iter().all(|s| !s.defining));
    }

    #[test]
    fn a_package_spec_and_its_body_share_a_row_but_not_a_declared_kind() {
        let f = Fixture::new(&[(
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE PACKAGE PKG_CLIENTI AS PROCEDURE P; END;\n\
             CREATE PACKAGE BODY PKG_CLIENTI AS PROCEDURE P IS BEGIN NULL; END; END;",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        let row = inventory.find(InventoryKind::Package, "PKG_CLIENTI").expect("indexed");
        let kinds: BTreeSet<_> = row.creations().map(|s| s.declared_kind).collect();
        assert_eq!(kinds.len(), 2, "spec and body must stay distinguishable");
    }

    #[test]
    fn rows_are_ordered_by_kind_and_name_not_by_which_file_came_first() {
        let f = Fixture::new(&[(
            "ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql",
            "CREATE TABLE ZULU (A NUMBER);\nCREATE TABLE ALFA (A NUMBER);\n\
             CREATE VIEW V_ALFA AS SELECT * FROM ALFA;",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        let listed: Vec<String> =
            inventory.objects.iter().map(|o| format!("{}:{}", o.kind.as_str(), o.name)).collect();
        assert_eq!(listed, ["table:ALFA", "table:ZULU", "view:V_ALFA"]);
    }

    #[test]
    fn a_file_the_tree_does_not_hold_contributes_nothing() {
        let f = Fixture::new(&[(
            "ORACLE/SOMEWHERE_ELSE/x.sql",
            "CREATE TABLE GHOST (A NUMBER);",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        assert!(inventory.objects.is_empty());
        assert_eq!(joined.orphans().len(), 1);
    }

    #[test]
    fn an_insert_three_blocks_deep_still_reaches_the_inventory() {
        // The reason `Statement::dml` is a list and the walker descends: in a
        // real Oracle upgrade the INSERT is inside DECLARE … BEGIN … END.
        let f = Fixture::new(&[(
            "ORACLE/AGGIORNAMENTO/4_12__4_13.sql",
            "DECLARE v NUMBER; BEGIN IF 1 = 1 THEN \
             INSERT INTO PARAMETRI (COD) VALUES ('SOGLIA_SCONTO'); END IF; END;",
            EngineKind::Oracle,
        )]);
        let project = project();
        let joined = ParsedProject::new(&project, f.scripts());
        let inventory = Inventory::build(&joined);
        let row = inventory.find(InventoryKind::Table, "PARAMETRI").expect("indexed");
        assert_eq!(row.coverage_in("ORACLE/AGGIORNAMENTO"), 1);
    }
}
