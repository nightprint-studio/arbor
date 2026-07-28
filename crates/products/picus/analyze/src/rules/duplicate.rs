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
use picus_parse::prelude::{DialectScope, DmlOperation, DmlShape, ParsedFile};

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
            for shape in statement.dml.iter().filter(|d| d.operation == DmlOperation::Insert) {
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

fn duplicate_definitions(context: &Context<'_>, output: &mut Output) {
    for entry in &context.inventory.objects {
        // Keyed by scope **and** by the exact kind the source declared: two
        // dialects creating the same table is the point of the repository, and a
        // package spec is not its body.
        //
        // A portable folder is its own key rather than being folded into either
        // dialect. Creating a table portably *and* creating it in the Oracle
        // folder is a genuine duplicate — which is what `DUP001` reports across
        // scopes; here the question is only "twice within the same origin".
        let mut by_origin: BTreeMap<
            (Option<DialectScope>, picus_parse::prelude::ObjectKind),
            Vec<&ObjectSite>,
        > = BTreeMap::new();
        for site in entry.creations() {
            by_origin.entry((site.scope, site.declared_kind)).or_default().push(site);
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
