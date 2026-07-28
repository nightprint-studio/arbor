//! `CONS002` / `CONS003` — a datum that lives in one half of a dialect's install
//! story and not in the other.
//!
//! These two compare **roles inside one dialect**, never one dialect against
//! another. A repository has two ways to arrive at a running database and they
//! must arrive at the same one:
//!
//! * the **install** half — the `init` and `data` folders — is what a database
//!   created from nothing gets. Both roles are folded together because the line
//!   between "schema and its seed" and "reference data" is where somebody chose
//!   to put an INSERT, and a rule that moved when a file moved would be a rule
//!   about the layout rather than about the data;
//! * the **upgrade** half — the `update` folders — is what a database that
//!   already exists gets.
//!
//! `CONS002` is a datum the install half has and no update ever inserts: the
//! installations then diverge by age. `CONS003` is the mirror, and it is the
//! worse one — a *fresh* install comes out incomplete, because it never had the
//! update applied and the initialisation never had the row.
//!
//! ## What "a datum" is
//!
//! One **row of an INSERT**, in the comparison form [`crate::compare`] produces:
//! `(column, value)` pairs, sorted, with numbers compared numerically and
//! strings exactly. Two rows are the same datum when those pairs are equal.
//! `UPDATE` and `DELETE` are deliberately not data here — they change a row that
//! is already there, and the question this pair of rules asks is whether the row
//! is there at all.
//!
//! ## Which of the two is even a question
//!
//! That depends on what the initialisation folders *are*, and no amount of reading
//! the SQL settles it — it is a fact about how the team works, so the project
//! states it (`[analysis] initialisation`, see
//! [`InitialisationModel`](picus_project::prelude::InitialisationModel)).
//!
//! On the default reading — the initialisation is kept at the latest version —
//! only `CONS003` is a question. A row the initialisation holds and no update
//! carries is simply a row from the first release, and there is no update for the
//! beginning; reporting those is what makes a first run print hundreds of findings
//! that are all correct behaviour. The reverse stays on, because it is never
//! correct behaviour: a row only an update writes means the newest installation is
//! the one missing something every older one has.
//!
//! A direction that is off is **reported as not run**, never quietly absent.
//!
//! ## Where it stands down, and why
//!
//! The install half is cumulative and the update half is a chain of deltas, so
//! most of what the initialisation writes was never any update's business. Three
//! gates keep that out of the report:
//!
//! 1. **The table must be loaded by both halves.** A table only the
//!    initialisation ever inserts into is seed data from before the update
//!    folder existed, not a propagation somebody forgot — and it is the one
//!    signal available without reading the repository's history.
//! 2. **Only the columns both halves write are compared.** An update that
//!    carries one extra column is not a different row, and reporting it twice —
//!    once in each direction — would be two findings for one row that is
//!    perfectly fine.
//! 3. **An unreadable statement stands the table down**, exactly as it does
//!    across dialects: a computed cell (`SYSDATE`, `now()`, a sequence), an
//!    `INSERT … SELECT`, or a row with no column list. A difference nobody can
//!    close is worse than a difference nobody was told about.
//!
//! Past those, the escape hatch is the declared suppression, which is why both
//! rules anchor at the **statement** that holds the datum: it is the only place
//! a person can write `-- picus: ignore CONS002 — this row predates the updates`.

use std::collections::{BTreeMap, BTreeSet};

use picus_parse::prelude::{DmlOperation, EngineKind};
use picus_project::prelude::FolderNode;
use picus_types::prelude::FolderRole;

use crate::compare::{self, RowFingerprint};
use crate::context::{engine_label, Context};
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    // Which of the two directions is meaningful is a fact about how the team
    // works, not about the SQL — see `InitialisationModel`. The project states it,
    // and a direction that is off is *reported* as not run rather than quietly
    // absent, so a report of a repository whose halves are deliberately unequal
    // can never be mistaken for a clean one.
    let model = context.initialisation_model();
    let forward = model.expects_installed_rows_in_updates();
    let backward = model.expects_updated_rows_in_the_initialisation();
    if !forward {
        output.skip(RuleId::Cons002, "", stood_down(model, "the initialisation writes"));
    }
    if !backward {
        output.skip(RuleId::Cons003, "", stood_down(model, "an update writes"));
    }
    if !forward && !backward {
        return;
    }

    for (_, pair) in collect(context) {
        compare_halves(&pair, forward, backward, output);
    }
}

/// Why one direction is not being checked, written for the person who could turn
/// it back on.
fn stood_down(model: picus_project::prelude::InitialisationModel, writes: &str) -> String {
    format!(
        "this project declares that {} — so a row {writes} is not expected on the other side. \
         Change the initialisation model in the project settings to check it.",
        model.describe()
    )
}

/// Which way into a database a folder serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Half {
    /// What a database created from nothing gets: `init` and `data`.
    Install,
    /// What a database that already exists gets: `update`.
    Upgrade,
}

/// `routines` and `ignored` are neither: a procedure is not a datum, and a
/// folder nobody could identify has no place in either story.
fn half_of(role: FolderRole) -> Option<Half> {
    match role {
        FolderRole::Init | FolderRole::Data => Some(Half::Install),
        FolderRole::Update => Some(Half::Upgrade),
        FolderRole::Routines | FolderRole::Ignored => None,
    }
}

/// Everything one half of one dialect inserts into one table.
#[derive(Debug)]
struct Side<'a> {
    /// Every row, with the first place it is written. Not yet reduced to the
    /// shared columns — that can only be done once both halves are known.
    rows: Vec<(RowFingerprint, Anchor)>,
    columns: BTreeSet<String>,
    /// False once a statement turned out to be unreadable. The whole half stands
    /// down then: the rows that *can* be read are a subset, and comparing a
    /// subset reports differences that are an artefact of what this crate can
    /// decode.
    readable: bool,
    /// A folder of this half, for the jump to where the datum is not.
    folder: Option<&'a FolderNode>,
}

impl Side<'_> {
    fn new() -> Self {
        Side { rows: Vec::new(), columns: BTreeSet::new(), readable: true, folder: None }
    }
}

/// One table, seen from both halves of one dialect.
#[derive(Debug)]
struct Pair<'a> {
    dialect: EngineKind,
    table: String,
    install: Side<'a>,
    upgrade: Side<'a>,
}

impl<'a> Pair<'a> {
    fn new(dialect: EngineKind, table: String) -> Self {
        Pair { dialect, table, install: Side::new(), upgrade: Side::new() }
    }

    fn side_mut(&mut self, half: Half) -> &mut Side<'a> {
        match half {
            Half::Install => &mut self.install,
            Half::Upgrade => &mut self.upgrade,
        }
    }
}

/// Every `(dialect, table)` either half of the repository inserts into.
///
/// A folder no ancestor declares a dialect for takes part in nothing: it has no
/// install story of its own to be inconsistent with.
///
/// A **portable** folder takes part in *every* dialect's story, which is why this
/// iterates `placement.dialects()` rather than reading one. A repository whose
/// initialisation is portable and whose updates are per-dialect still has an
/// Oracle install-versus-upgrade comparison and a PostgreSQL one, and skipping
/// the portable half would leave both with nothing to compare against — a rule
/// that quietly stops running is worse than one that reports.
fn collect<'a>(context: &Context<'a>) -> BTreeMap<(EngineKind, String), Pair<'a>> {
    let mut tables: BTreeMap<(EngineKind, String), Pair<'a>> = BTreeMap::new();
    for (script, placement) in context.project.placed() {
        let Some(half) = half_of(placement.effective_role()) else { continue };
        for statement in &script.parsed.statements {
            for shape in statement.dml.iter().filter(|d| d.operation == DmlOperation::Insert) {
                let table = shape.table.folded_name();
                for dialect in placement.dialects().iter().copied() {
                    let key = (dialect, table.clone());
                    let pair =
                        tables.entry(key).or_insert_with(|| Pair::new(dialect, table.clone()));
                    let side = pair.side_mut(half);
                    side.folder.get_or_insert(placement.folder);
                    side.columns.extend(compare::written_columns(shape));
                    // A row with no column list cannot be reduced to shared
                    // columns at all: lining it up against a named row would be a
                    // guess about the table's physical column order, which is
                    // exactly what `DML002` exists to say nobody should make.
                    if !shape.has_column_list {
                        side.readable = false;
                        continue;
                    }
                    match compare::comparable_rows(shape) {
                        Some(rows) => {
                            for (row, fingerprint) in shape.rows.iter().zip(rows) {
                                let line = script.parsed.line_of(row.range.start);
                                let anchor = Anchor::at(script.path, line);
                                side.rows.push((fingerprint, anchor));
                            }
                        }
                        None => side.readable = false,
                    }
                }
            }
        }
    }
    tables
}

fn compare_halves(pair: &Pair<'_>, forward: bool, backward: bool, output: &mut Output) {
    if !pair.install.readable || !pair.upgrade.readable {
        return;
    }
    // Gate 1: both halves have to load this table. A table only the
    // initialisation seeds predates the update folder as far as anything
    // readable from the tree can tell, and reporting it would put every seeded
    // row in the report on the first run.
    let (Some(install_folder), Some(upgrade_folder)) = (pair.install.folder, pair.upgrade.folder)
    else {
        return;
    };
    if pair.install.rows.is_empty() || pair.upgrade.rows.is_empty() {
        return;
    }

    // Gate 2: compare on what both halves actually write.
    let shared: BTreeSet<&str> = pair
        .install
        .columns
        .intersection(&pair.upgrade.columns)
        .map(String::as_str)
        .collect();
    if shared.is_empty() {
        return;
    }
    let installed = reduce(&pair.install.rows, &shared);
    let upgraded = reduce(&pair.upgrade.rows, &shared);

    let label = engine_label(pair.dialect);
    if forward {
        for (datum, anchor) in &installed {
            if upgraded.contains_key(datum) {
                continue;
            }
            output.findings.push(never_propagated(pair, label, datum, anchor, upgrade_folder));
        }
    }
    if backward {
        for (datum, anchor) in &upgraded {
            if installed.contains_key(datum) {
                continue;
            }
            output.findings.push(never_seeded(pair, label, datum, anchor, install_folder));
        }
    }
}

/// Rows reduced to the columns both halves write, first occurrence winning.
///
/// The first occurrence is the one worth pointing at: a row written twice in one
/// half is `DUP001`'s business, and reporting the second copy here would make
/// one missing datum look like two.
fn reduce(
    rows: &[(RowFingerprint, Anchor)],
    shared: &BTreeSet<&str>,
) -> BTreeMap<RowFingerprint, Anchor> {
    let mut out: BTreeMap<RowFingerprint, Anchor> = BTreeMap::new();
    for (fingerprint, anchor) in rows {
        let datum: RowFingerprint = fingerprint
            .iter()
            .filter(|(column, _)| shared.contains(column.as_str()))
            .cloned()
            .collect();
        if datum.is_empty() {
            continue;
        }
        out.entry(datum).or_insert_with(|| anchor.clone());
    }
    out
}

// ── CONS002 — in the initialisation, in no update ────────────────────────────

fn never_propagated(
    pair: &Pair<'_>,
    label: &str,
    datum: &RowFingerprint,
    anchor: &Anchor,
    upgrade_folder: &FolderNode,
) -> Finding {
    let row = compare::render(datum);
    Finding::new(
        RuleId::Cons002,
        anchor.clone(),
        format!("{row} is inserted into {} by the initialisation alone", pair.table),
        format!(
            "A {label} database created from scratch has this row. One that was already running \
             gets here through `{upgrade}`, where nothing inserts it, so it never arrives — and the \
             same query answers differently depending on how old the installation is. A row that \
             predates the update folder is fine: declare it with `-- picus: ignore CONS002 — why`.",
            upgrade = upgrade_folder.path
        ),
    )
    .also_at(upgrade_folder.path.clone())
    .build()
}

// ── CONS003 — in an update, in no initialisation ─────────────────────────────

fn never_seeded(
    pair: &Pair<'_>,
    label: &str,
    datum: &RowFingerprint,
    anchor: &Anchor,
    install_folder: &FolderNode,
) -> Finding {
    let row = compare::render(datum);
    Finding::new(
        RuleId::Cons003,
        anchor.clone(),
        format!("{row} is inserted into {} by an update alone", pair.table),
        format!(
            "This update adds the row to a {label} database that already exists. A database \
             installed from scratch runs `{install}`, which never inserts it, and never runs this \
             update either — so the newest installation is the one missing a row every older one \
             has, and it stays missing until somebody notices.",
            install = install_folder.path
        ),
    )
    .also_at(install_folder.path.clone())
    .build()
}
