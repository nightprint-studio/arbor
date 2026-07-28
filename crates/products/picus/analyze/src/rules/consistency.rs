//! `CONS001` / `CONS004` — the two ways two branches drift apart.
//!
//! Everything here compares **folders of the same role**, never folders of the
//! same name and never a whole branch against a whole branch. `ORACLE/
//! INIZIALIZZAZIONE` and `POSTGRES/INIZIALIZZAZIONE` are counterparts because
//! they are both `init`, not because somebody spelled them the same way; a
//! project whose PostgreSQL branch calls it `SETUP` compares just as well.
//!
//! And nothing here talks about a branch whose dialect is unknown. `picus-project`
//! refuses to guess an engine, and a rule that compared a `COMMON/` folder with
//! the Oracle branch would report every object in the repository as missing from
//! it — a first run that produces nothing but noise is a tool nobody opens twice.
//!
//! The other axis — one branch's initialisation against its own updates — is
//! `CONS002`/`CONS003`, in [`crate::rules::propagation`].

use std::collections::{BTreeMap, BTreeSet};

use picus_inventory::prelude::{ObjectEntry, Placement};
use picus_parse::prelude::{line_col, DmlOperation, EngineKind};
use picus_project::prelude::Branch;
use picus_types::prelude::FolderRole;

use crate::compare::{self, RowFingerprint};
use crate::context::{branch_label, folders_with_role, Context};
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

/// The roles a cross-branch comparison is meaningful for, most significant
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
    let branches = context.dialect_branches();
    for entry in &context.inventory.objects {
        // A package has no PostgreSQL counterpart to be missing from. Reporting
        // one would put a permanent, unfixable finding at the top of every
        // report of every Oracle-first repository.
        if !entry.kind.exists_in_both_engines() {
            continue;
        }
        // One finding per object per branch, at the most significant role where
        // the gap shows. A table absent from a whole branch would otherwise be
        // reported once for init, once for data and once for update — three rows
        // for one problem and one fix.
        let mut already: BTreeSet<&str> = BTreeSet::new();
        for role in COMPARED_ROLES {
            for (branch, covering) in gaps_at(entry, &branches, role) {
                if already.insert(branch.id.as_str()) {
                    output.findings.push(missing_finding(entry, branch, covering, role));
                }
            }
        }
    }
}

/// Branches that do nothing with `entry` at `role` while another dialect's
/// branch does, paired with the branch that does.
fn gaps_at<'a>(
    entry: &ObjectEntry,
    branches: &[&'a Branch],
    role: FolderRole,
) -> Vec<(&'a Branch, &'a Branch)> {
    let participating: Vec<&'a Branch> =
        branches.iter().copied().filter(|b| has_role(b, role)).collect();
    let dialects: BTreeSet<EngineKind> = participating.iter().filter_map(|b| b.dialect).collect();
    // Nothing to compare against: one branch, or several branches that are all
    // the same engine.
    if participating.len() < 2 || dialects.len() < 2 {
        return Vec::new();
    }
    let covered: Vec<&'a Branch> =
        participating.iter().copied().filter(|b| coverage_of(entry, b, role) > 0).collect();
    let Some(reference) = covered.first().copied() else { return Vec::new() };
    participating
        .into_iter()
        .filter(|b| coverage_of(entry, b, role) == 0)
        // Only against a branch of a *different* engine: two Oracle branches
        // disagreeing is a different question from a dialect gap.
        .filter(|b| b.dialect != reference.dialect)
        .map(|b| (b, reference))
        .collect()
}

fn missing_finding(
    entry: &ObjectEntry,
    missing: &Branch,
    covering: &Branch,
    role: FolderRole,
) -> Finding {
    let folder = folders_with_role(missing, role).next();
    // The anchor is the folder, not a file inside it: no file in it is the one
    // that should have had the statement, and naming one would be a guess
    // dressed up as advice. The jump the user wants is `alsoAt`, which points at
    // the branch that does do it.
    let path = folder.map(|f| f.path.as_str()).unwrap_or(missing.path.as_str());
    let elsewhere = entry.sites_in(&covering.id, role).next().map(|s| s.location());

    let mut draft = Finding::new(
        RuleId::Cons001,
        Anchor::file(path, &missing.id),
        format!("{} is not touched by the {} branch", entry.name, branch_label(missing)),
        gap_consequence(entry, missing, covering, role),
    )
    .fix(format!("Generate for {} too", branch_label(missing)));
    if let Some(location) = elsewhere {
        draft = draft.also_at(location);
    }
    draft.build()
}

/// What actually goes wrong, which depends entirely on what the folder is for.
fn gap_consequence(
    entry: &ObjectEntry,
    missing: &Branch,
    covering: &Branch,
    role: FolderRole,
) -> String {
    let count = coverage_of(entry, covering, role);
    let statements =
        if count == 1 { "1 statement".to_string() } else { format!("{count} statements") };
    let (there, here) = (branch_label(covering), branch_label(missing));
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
            "The {there} branch loads {name} with {statements} of reference data that {here} never \
             loads: the two installations answer the same query differently.",
            name = entry.name
        ),
        FolderRole::Routines => format!(
            "{name} exists as a routine only on {there}. Anything that calls it — a trigger, a \
             report, another procedure — fails at runtime on {here}.",
            name = entry.name
        ),
        FolderRole::Ignored => format!("{} is absent from the {here} branch.", entry.name),
    }
}

// ── CONS004 — both branches do it, differently ───────────────────────────────

/// What one branch writes into one object at one role.
#[derive(Debug)]
struct Written {
    dialect: EngineKind,
    label: String,
    columns: BTreeSet<String>,
    /// `None` once any statement turned out to be incomparable — a computed
    /// value, or an `INSERT … SELECT`. The columns survive that; the rows do not.
    rows: Option<BTreeSet<RowFingerprint>>,
    anchor: Anchor,
}

fn divergent_content(context: &Context<'_>, output: &mut Output) {
    // Keyed by the role's wire word rather than by `FolderRole`, which is not
    // `Ord` — unlike `EngineKind` and `ObjectKind`, its two neighbours in the
    // same vocabulary.
    let mut written: BTreeMap<(String, &'static str), BTreeMap<String, Written>> = BTreeMap::new();
    for (script, placement) in context.project.placed() {
        let Some(dialect) = placement.branch.dialect else { continue };
        if !COMPARED_ROLES.contains(&placement.folder.role) {
            continue;
        }
        for statement in &script.parsed.statements {
            for shape in statement.dml.iter().filter(|d| d.operation == DmlOperation::Insert) {
                let key = (shape.table.folded_name(), placement.folder.role.as_str());
                let per_branch = written.entry(key).or_default();
                let slot = per_branch.entry(placement.branch.id.clone()).or_insert_with(|| {
                    Written {
                        dialect,
                        label: branch_label(placement.branch),
                        columns: BTreeSet::new(),
                        rows: Some(BTreeSet::new()),
                        anchor: anchor_of(script.source, script.path, placement, shape.table.range.start),
                    }
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

    for ((table, role), per_branch) in written {
        if let Some(finding) = divergence(&table, role, &per_branch) {
            output.findings.push(finding);
        }
    }
}

fn divergence(
    table: &str,
    role: &str,
    per_branch: &BTreeMap<String, Written>,
) -> Option<Finding> {
    let mut sides = per_branch.values();
    let reference = sides.next()?;
    let other = sides.find(|w| w.dialect != reference.dialect)?;

    // Report on the side that ends up with less: that is the installation which
    // will be missing something, and it is where the fix goes.
    let (short, long) = if other.columns.len() < reference.columns.len() {
        (other, reference)
    } else {
        (reference, other)
    };

    if short.columns != long.columns {
        let missing: Vec<&str> =
            long.columns.difference(&short.columns).map(String::as_str).collect();
        if !missing.is_empty() {
            return Some(
                Finding::new(
                    RuleId::Cons004,
                    short.anchor.clone(),
                    format!("{table} is filled in differently in the two branches"),
                    format!(
                        "The {long_label} branch writes {columns} into {table} and the {short_label} \
                         branch never does, so the same row exists on both engines with those \
                         columns left at their default on {short_label}.",
                        long_label = long.label,
                        short_label = short.label,
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
            "the {} branch inserts {} and the {} branch does not",
            long.label,
            compare::render(there),
            short.label
        ),
        (None, Some(here)) => format!(
            "the {} branch inserts {}, which the {} branch does not",
            short.label,
            compare::render(here),
            long.label
        ),
        (None, None) => return None,
    };
    Some(
        Finding::new(
            RuleId::Cons004,
            short.anchor.clone(),
            format!("{table} is populated differently in the two branches"),
            format!(
                "Both branches load {table} in their {role} scripts, but not with the same rows: \
                 {detail}. The two installations disagree about data the application reads as fact.",
            ),
        )
        .also_at(long.anchor.location())
        .build(),
    )
}

// ── shared ───────────────────────────────────────────────────────────────────

fn has_role(branch: &Branch, role: FolderRole) -> bool {
    branch.folders.iter().any(|f| f.role == role)
}

fn coverage_of(entry: &ObjectEntry, branch: &Branch, role: FolderRole) -> usize {
    folders_with_role(branch, role)
        .map(|f| entry.coverage_in(&picus_inventory::prelude::coverage_key(&branch.id, &f.id)))
        .sum()
}

fn anchor_of(source: &str, path: &str, placement: Placement<'_>, offset: usize) -> Anchor {
    Anchor::at(path, &placement.branch.id, line_col(source, offset).0)
}
