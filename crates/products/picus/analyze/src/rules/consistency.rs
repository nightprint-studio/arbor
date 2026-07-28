//! `CONS001` / `CONS004` — the two ways two dialects drift apart.
//!
//! Everything here compares **lanes**: the folders that play one role for one
//! dialect, against the folders that play the same role for another. The folders
//! that initialise Oracle and the folders that initialise PostgreSQL are
//! counterparts because they are both `init`, not because somebody spelled them
//! the same way; a repository that calls one of them `SETUP`, or splits it across
//! `INIZIALIZZAZIONE/2024/ORA` and `INIZIALIZZAZIONE/2025/ORA`, compares just as
//! well.
//!
//! And nothing here talks about a folder no ancestor declares a dialect for.
//! `picus-project` refuses to guess an engine, and a rule that compared an
//! unclassified folder with the Oracle ones would report every object in the
//! repository as missing from it — a first run that produces nothing but noise is
//! a tool nobody opens twice.
//!
//! ## Portable folders are in **every** lane
//!
//! A folder declared portable holds SQL that runs on both engines, so what it
//! writes is present on both. `FolderNode::is_in_lane` therefore puts it in the
//! Oracle lane and the PostgreSQL lane at once — the first thing in the model to
//! belong to more than one — and two consequences follow that are worth stating
//! rather than discovering:
//!
//! * **`CONS001` cannot report it as a gap.** `coverage_of` sums a lane's
//!   folders, so a portable folder's statements are added to *both* dialects'
//!   totals and neither reads zero. That is the intended answer, not
//!   double-counting: the sums are per dialect and never added together, and a
//!   row that really is installed on both engines really does cover both.
//! * **The one-finding-per-object-per-dialect dedup is untouched.** It is keyed
//!   on the dialect that is *missing* something, and a portable folder only ever
//!   removes gaps. It cannot produce a second finding, only prevent a first.
//!
//! `CONS004` deliberately leaves portable folders out — see the note there.
//!
//! The other axis — one dialect's initialisation against its own updates — is
//! `CONS002`/`CONS003`, in [`crate::rules::propagation`].

use std::collections::{BTreeMap, BTreeSet};

use picus_inventory::prelude::ObjectEntry;
use picus_parse::prelude::{line_col, DmlOperation, EngineKind};
use picus_project::prelude::FolderNode;
use picus_types::prelude::FolderRole;

use crate::compare::{self, RowFingerprint};
use crate::context::{engine_label, Context};
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

/// The roles a cross-dialect comparison is meaningful for, most significant
/// first. `Ignored` is absent by construction: it is what a folder gets when
/// nobody could tell what it was for, and comparing two of those says nothing.
const COMPARED_ROLES: [FolderRole; 4] =
    [FolderRole::Init, FolderRole::Data, FolderRole::Update, FolderRole::Routines];

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    coverage_gaps(context, output);
    divergent_content(context, output);
}

// ── CONS001 — covered here, not covered there ────────────────────────────────

fn coverage_gaps(context: &Context<'_>, output: &mut Output) {
    let dialects = context.dialects();
    for entry in &context.inventory.objects {
        // A package has no PostgreSQL counterpart to be missing from. Reporting
        // one would put a permanent, unfixable finding at the top of every
        // report of every Oracle-first repository.
        if !entry.kind.exists_in_both_engines() {
            continue;
        }
        // One finding per object per dialect, at the most significant role where
        // the gap shows. A table absent from a whole dialect would otherwise be
        // reported once for init, once for data and once for update — three rows
        // for one problem and one fix.
        let mut already: BTreeSet<EngineKind> = BTreeSet::new();
        for role in COMPARED_ROLES {
            for (missing, covering) in gaps_at(entry, context, &dialects, role) {
                if already.insert(missing) {
                    output.findings.push(missing_finding(entry, context, missing, covering, role));
                }
            }
        }
    }
}

/// Dialects that do nothing with `entry` at `role` while another dialect does,
/// paired with the dialect that does.
fn gaps_at(
    entry: &ObjectEntry,
    context: &Context<'_>,
    dialects: &[EngineKind],
    role: FolderRole,
) -> Vec<(EngineKind, EngineKind)> {
    let participating: Vec<EngineKind> =
        dialects.iter().copied().filter(|d| !context.lane(*d, role).is_empty()).collect();
    // Nothing to compare against: only one dialect has a folder for this role.
    if participating.len() < 2 {
        return Vec::new();
    }
    let covered: Vec<EngineKind> =
        participating.iter().copied().filter(|d| coverage_of(entry, context, *d, role) > 0).collect();
    let Some(reference) = covered.first().copied() else { return Vec::new() };
    participating
        .into_iter()
        .filter(|d| coverage_of(entry, context, *d, role) == 0)
        .map(|d| (d, reference))
        .collect()
}

fn missing_finding(
    entry: &ObjectEntry,
    context: &Context<'_>,
    missing: EngineKind,
    covering: EngineKind,
    role: FolderRole,
) -> Finding {
    let lane = context.lane(missing, role);
    // The anchor is the folder, not a file inside it: no file in it is the one
    // that should have had the statement, and naming one would be a guess
    // dressed up as advice. The jump the user wants is `alsoAt`, which points at
    // the dialect that does do it.
    let path = lane.first().map(|f| f.path.as_str()).unwrap_or("");
    let elsewhere = entry.sites_in(covering, role).next().map(|s| s.location());

    let mut draft = Finding::new(
        RuleId::Cons001,
        Anchor::file(path),
        format!("{} is not touched by the {} scripts", entry.name, engine_label(missing)),
        gap_consequence(entry, context, missing, covering, role),
    )
    .fix(format!("Generate for {} too", engine_label(missing)));
    if let Some(location) = elsewhere {
        draft = draft.also_at(location);
    }
    draft.build()
}

/// What actually goes wrong, which depends entirely on what the folder is for.
fn gap_consequence(
    entry: &ObjectEntry,
    context: &Context<'_>,
    missing: EngineKind,
    covering: EngineKind,
    role: FolderRole,
) -> String {
    let count = coverage_of(entry, context, covering, role);
    let statements =
        if count == 1 { "1 statement".to_string() } else { format!("{count} statements") };
    let (there, here) = (engine_label(covering), engine_label(missing));
    match role {
        FolderRole::Init => format!(
            "The {there} initialisation runs {statements} against {name}; the {here} one runs none, \
             so a fresh {here} install comes up without it while a fresh {there} install has it.",
            name = entry.name
        ),
        FolderRole::Update => format!(
            "The {there} update scripts change {name} in {statements}; the {here} update scripts \
             never do, so a {here} database upgraded with these scripts silently stays on the old \
             shape while {there} moves on.",
            name = entry.name
        ),
        FolderRole::Data => format!(
            "The {there} scripts load {name} with {statements} of reference data that {here} never \
             loads: the two installations answer the same query differently.",
            name = entry.name
        ),
        FolderRole::Routines => format!(
            "{name} exists as a routine only on {there}. Anything that calls it — a trigger, a \
             report, another procedure — fails at runtime on {here}.",
            name = entry.name
        ),
        FolderRole::Ignored => format!("{} is absent from the {here} scripts.", entry.name),
    }
}

// ── CONS004 — both dialects do it, differently ───────────────────────────────

/// What one dialect writes into one object at one role.
#[derive(Debug)]
struct Written {
    columns: BTreeSet<String>,
    /// `None` once any statement turned out to be incomparable — a computed
    /// value, or an `INSERT … SELECT`. The columns survive that; the rows do not.
    rows: Option<BTreeSet<RowFingerprint>>,
    anchor: Anchor,
}

fn divergent_content(context: &Context<'_>, output: &mut Output) {
    // Keyed by the role's wire word so the map key stays a plain pair of things
    // that print, which is what the message below needs anyway.
    let mut written: BTreeMap<(String, &'static str), BTreeMap<EngineKind, Written>> =
        BTreeMap::new();
    for (script, placement) in context.project.placed() {
        // A portable folder is deliberately not a side of this comparison. It
        // writes the same rows to both engines by construction, so it can never
        // be the *source* of a divergence between them — and putting it on both
        // sides would compare it against itself and report nothing while hiding
        // the real difference. A genuine gap is still caught: whatever a dialect
        // folder writes that its counterpart does not is reported exactly as
        // before, because the portable rows are absent from both sides equally.
        let Some(dialect) = placement.effective_dialect() else { continue };
        if !COMPARED_ROLES.contains(&placement.effective_role()) {
            continue;
        }
        for statement in &script.parsed.statements {
            for shape in statement.dml.iter().filter(|d| d.operation == DmlOperation::Insert) {
                let key = (shape.table.folded_name(), placement.effective_role().as_str());
                let per_dialect = written.entry(key).or_default();
                let slot = per_dialect.entry(dialect).or_insert_with(|| Written {
                    columns: BTreeSet::new(),
                    rows: Some(BTreeSet::new()),
                    anchor: Anchor::at(
                        script.path,
                        line_col(script.source, shape.table.range.start).0,
                    ),
                });
                slot.columns.extend(compare::written_columns(shape));
                match (slot.rows.as_mut(), compare::comparable_rows(shape)) {
                    (Some(known), Some(rows)) => known.extend(rows),
                    // One incomparable statement poisons the row comparison for
                    // the whole object: the rows we *can* read are a subset, and
                    // comparing a subset reports differences that are an artefact
                    // of what this crate can decode.
                    (Some(_), None) => slot.rows = None,
                    (None, _) => {}
                }
            }
        }
    }

    for ((table, role), per_dialect) in written {
        if let Some(finding) = divergence(&table, role, &per_dialect) {
            output.findings.push(finding);
        }
    }
}

fn divergence(
    table: &str,
    role: &str,
    per_dialect: &BTreeMap<EngineKind, Written>,
) -> Option<Finding> {
    let mut sides = per_dialect.iter();
    let (reference_dialect, reference) = sides.next()?;
    let (other_dialect, other) = sides.next()?;

    // Report on the side that ends up with less: that is the installation which
    // will be missing something, and it is where the fix goes.
    let ((short_dialect, short), (long_dialect, long)) =
        if other.columns.len() < reference.columns.len() {
            ((other_dialect, other), (reference_dialect, reference))
        } else {
            ((reference_dialect, reference), (other_dialect, other))
        };
    let (short_label, long_label) = (engine_label(*short_dialect), engine_label(*long_dialect));

    if short.columns != long.columns {
        let missing: Vec<&str> =
            long.columns.difference(&short.columns).map(String::as_str).collect();
        if !missing.is_empty() {
            return Some(
                Finding::new(
                    RuleId::Cons004,
                    short.anchor.clone(),
                    format!("{table} is filled in differently in the two dialects"),
                    format!(
                        "The {long_label} scripts write {columns} into {table} and the {short_label} \
                         ones never do, so the same row exists on both engines with those columns \
                         left at their default on {short_label}.",
                        columns = missing.join(", ")
                    ),
                )
                .also_at(long.anchor.location())
                .build(),
            );
        }
    }

    let (Some(short_rows), Some(long_rows)) = (short.rows.as_ref(), long.rows.as_ref()) else {
        return None;
    };
    if short_rows == long_rows {
        return None;
    }
    let only_there: Vec<&RowFingerprint> = long_rows.difference(short_rows).collect();
    let only_here: Vec<&RowFingerprint> = short_rows.difference(long_rows).collect();
    let detail = match (only_there.first(), only_here.first()) {
        (Some(there), _) => format!(
            "the {long_label} scripts insert {} and the {short_label} ones do not",
            compare::render(there)
        ),
        (None, Some(here)) => format!(
            "the {short_label} scripts insert {}, which the {long_label} ones do not",
            compare::render(here)
        ),
        (None, None) => return None,
    };
    Some(
        Finding::new(
            RuleId::Cons004,
            short.anchor.clone(),
            format!("{table} is populated differently in the two dialects"),
            format!(
                "Both dialects load {table} in their {role} scripts, but not with the same rows: \
                 {detail}. The two installations disagree about data the application reads as fact.",
            ),
        )
        .also_at(long.anchor.location())
        .build(),
    )
}

// ── shared ───────────────────────────────────────────────────────────────────

/// How many statements the whole lane runs against this object.
///
/// Summed across the lane's folders rather than read from one of them: a
/// repository that splits its updates over `2024/ORA` and `2025/ORA` has one
/// update story, and counting either half alone would report the other as a gap.
///
/// A portable folder is in every lane, so its statements are counted once for
/// Oracle and once for PostgreSQL. Not double-counting: these sums are per
/// dialect and are never added to each other, and one portable `INSERT` genuinely
/// does put the row in both installations.
fn coverage_of(
    entry: &ObjectEntry,
    context: &Context<'_>,
    dialect: EngineKind,
    role: FolderRole,
) -> usize {
    context.lane(dialect, role).iter().map(|f: &&FolderNode| entry.coverage_in(&f.path)).sum()
}
