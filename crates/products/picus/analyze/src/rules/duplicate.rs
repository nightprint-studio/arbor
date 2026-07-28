//! `DUP001` / `DUP002` — the same thing done twice.
//!
//! Both rules spend most of their code on **not** firing, because "twice" is
//! ambiguous in a repository that deliberately says everything twice:
//!
//! * `PARAMETRI` created by the Oracle scripts and again by the PostgreSQL ones
//!   is the product's entire premise, not a duplicate. `DUP002` therefore compares
//!   **within one dialect** — and folders no ancestor declares a dialect for form
//!   a group of their own rather than being lumped in with somebody's;
//! * a table created in the initialisation folder and altered by four update
//!   scripts is an ordinary, healthy repository. `DUP002` counts **creations**,
//!   never definitions;
//! * an Oracle package spec and its body carry the same name by construction, so
//!   they are told apart by the exact kind the source declared.

use std::collections::BTreeMap;

use picus_inventory::prelude::ObjectSite;
use picus_types::prelude::FolderRole;
use picus_parse::prelude::{DialectScope, DmlOperation, DmlShape, ParsedFile, StatementKind};

use crate::compare::{self, RowFingerprint};
use crate::context::Context;
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    duplicate_rows(context, output);
    duplicate_definitions(context, output);
}

// ── DUP001 — the same row inserted twice in one script ───────────────────────

/// What makes two INSERTs comparable: the same table, and both naming their
/// columns or both not.
///
/// `INSERT INTO T (A, B)` and `INSERT INTO T VALUES (…)` write the same table,
/// but a positional row and a named row are not the same fact — lining them up
/// would be a guess about the table's physical column order, which is precisely
/// what `DML002` exists to say nobody should make.
///
/// Column *order* is deliberately not part of the signature: two named INSERTs
/// listing the same columns in a different order write the same row, and the
/// fingerprint pairs the values by name.
type Signature = (String, bool);

fn duplicate_rows(context: &Context<'_>, output: &mut Output) {
    for (script, _) in context.project.placed() {
        // First occurrence of each row, per signature, in source order.
        let mut seen: BTreeMap<(Signature, RowFingerprint), usize> = BTreeMap::new();
        for statement in &script.parsed.statements {
            // `TRUNCATE` is a statement, not DML, so it never reaches the loop
            // below — and it is the most emphatic "forget what was in there"
            // there is. It clears every table it names.
            if statement.kind == StatementKind::Truncate {
                for object in &statement.references {
                    let table = object.folded_name();
                    seen.retain(|((seen_table, _), _), _| *seen_table != table);
                }
            }
            // Walked in source order, DML included, so a DELETE between two
            // INSERTs is seen between them. See `forgets`.
            for shape in &statement.dml {
                if context.excludes(&shape.table.folded_name()) {
                    continue;
                }
                if forgets(shape) {
                    let table = shape.table.folded_name();
                    seen.retain(|((seen_table, _), _), _| *seen_table != table);
                    continue;
                }
                if shape.operation != DmlOperation::Insert {
                    continue;
                }
                let Some(rows) = compare::comparable_rows(shape) else { continue };
                let signature = signature_of(shape);
                for (row, fingerprint) in shape.rows.iter().zip(rows) {
                    let key = (signature.clone(), fingerprint.clone());
                    match seen.get(&key) {
                        None => {
                            seen.insert(key, row.range.start);
                        }
                        Some(first) => output.findings.push(duplicate_row_finding(
                            script.parsed,
                            script.path,
                            &shape.table.folded_name(),
                            &fingerprint,
                            *first,
                            row.range.start,
                        )),
                    }
                }
            }
        }
    }
}

/// Does this statement mean "whatever was in that table, forget it"?
///
/// The pattern this exists for is the ordinary way to make an update script
/// re-runnable:
///
/// ```sql
/// INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 10);
/// …
/// DELETE FROM PARAMETRI WHERE COD = 'SOGLIA';
/// INSERT INTO PARAMETRI (COD, VALORE) VALUES ('SOGLIA', 15);
/// ```
///
/// The two INSERTs are identical in shape and the second is not a mistake — it is
/// the point. Without this the rule reported every reload-and-reinsert block in
/// the repository, which is most of them, and a rule that fires on the house
/// style is a rule people switch off.
///
/// **The WHERE clause is deliberately not examined.** Deciding whether
/// `WHERE COD = 'SOGLIA'` covers a particular row means evaluating a predicate
/// against values, which is a query planner, and getting it subtly wrong would
/// mean reporting a duplicate that is not one. Every delete on the table clears
/// every remembered row of that table instead: the rule gives up the ability to
/// catch a genuine duplicate written *after* an unrelated delete, and in exchange
/// it never accuses a correct script. That is the right way round — a missed
/// duplicate fails loudly at install time on the key, a false one costs trust.
fn forgets(shape: &DmlShape) -> bool {
    matches!(shape.operation, DmlOperation::Delete)
}

fn signature_of(shape: &DmlShape) -> Signature {
    (shape.table.folded_name(), shape.has_column_list)
}

fn duplicate_row_finding(
    parsed: &ParsedFile,
    path: &str,
    table: &str,
    fingerprint: &RowFingerprint,
    first: usize,
    second: usize,
) -> Finding {
    let first_line = parsed.line_of(first);
    let line = parsed.line_of(second);
    Finding::new(
        RuleId::Dup001,
        Anchor::at(path, line),
        format!("The same row is inserted into {table} twice"),
        format!(
            "Line {first_line} already inserts {row}. On a table with a key over those columns the \
             second INSERT fails, and because these scripts run as one unit everything after it in \
             the file is never applied.",
            row = compare::render(fingerprint)
        ),
    )
    .also_at(format!("{path}:{first_line}"))
    .fix("Remove the duplicate")
    .build()
}

// ── DUP002 — the same object created in two places ───────────────────────────

/// Which way into a database a folder serves — the same split
/// [`crate::rules::propagation`] compares along, and for the same reason: what a
/// fresh install runs and what an existing database runs are two different lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Half {
    /// What a database created from nothing gets.
    ///
    /// `routines` belongs here, unlike in [`crate::rules::propagation`], and the
    /// difference is the subject: that rule compares *data*, and a procedure is
    /// not a datum. This one compares *definitions*, and a routines folder holds
    /// the current definition of everything in it — which is precisely what a
    /// fresh install needs and what an update script re-issues.
    Install,
    /// `update`: what a database that already exists gets.
    Upgrade,
    /// `ignored` — no story at all, and compared only with itself.
    Neither,
}

fn half_of(role: FolderRole) -> Half {
    match role {
        FolderRole::Init | FolderRole::Data | FolderRole::Routines => Half::Install,
        FolderRole::Update => Half::Upgrade,
        FolderRole::Ignored => Half::Neither,
    }
}

fn duplicate_definitions(context: &Context<'_>, output: &mut Output) {
    // Under a cumulative initialisation, a definition in the install half and
    // another in the upgrade half is the model working, not a duplicate: the
    // initialisation carries the object's *current* shape and the update that
    // introduced it carries the same `CREATE`, because that is how an existing
    // database gets it. Comparing across the two halves then reports every object
    // the repository has ever added — which is what happens on a real repository,
    // and it is the same false positive `CONS002` had for the same reason.
    //
    // The comparison that stays meaningful is **within one half**: two files that
    // both create the object on a fresh install genuinely fight, and whichever
    // runs last wins.
    let split_by_half = !context.initialisation_model().expects_installed_rows_in_updates();

    for entry in &context.inventory.objects {
        if context.excludes(&entry.name) {
            continue;
        }
        // Keyed by scope **and** by the exact kind the source declared: two
        // dialects creating the same table is the point of the repository, and a
        // package spec is not its body.
        //
        // A portable folder is its own key rather than being folded into either
        // dialect. Creating a table portably *and* creating it in the Oracle
        // folder is a genuine duplicate — which is what `DUP001` reports across
        // scopes; here the question is only "twice within the same origin".
        let mut by_origin: BTreeMap<
            (Option<DialectScope>, picus_parse::prelude::ObjectKind, Option<Half>),
            Vec<&ObjectSite>,
        > = BTreeMap::new();
        for site in entry.creations() {
            // `CREATE OR REPLACE` is a statement of intent, not a spelling. Two
            // files that both create an object are usually a race whose winner is
            // decided by file order; two files that both *replace* it are doing
            // precisely what the syntax exists for.
            //
            // The case that forced it, and it is the house style of every
            // repository this product is for: each update script defines a
            // throwaway wrapper — `CREATE OR REPLACE FUNCTION aggiornamento()`,
            // holding "if the version is X, do this, then set it to Y" — calls it
            // and moves on. Two hundred update scripts then declared the same
            // function two hundred times and the rule reported all but the first.
            if site.replacing {
                continue;
            }
            let half = split_by_half.then(|| half_of(site.role));
            by_origin.entry((site.scope, site.declared_kind, half)).or_default().push(site);
        }
        for (_, sites) in by_origin {
            let Some((first, rest)) = sites.split_first() else { continue };
            for site in rest {
                output.findings.push(
                    Finding::new(
                        RuleId::Dup002,
                        Anchor::at(&site.path, site.line),
                        format!("{} is created in two places", entry.name),
                        format!(
                            "`{}` also creates it. Whichever file the installer runs last decides \
                             what is actually in the database, so the result depends on file order \
                             rather than on which definition is the current one.",
                            first.path
                        ),
                    )
                    .also_at(first.location())
                    .build(),
                );
            }
        }
    }
}
