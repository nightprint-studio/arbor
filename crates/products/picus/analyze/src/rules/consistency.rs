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
//! ## Portable scripts are in **every** lane
//!
//! A script declared portable runs on both engines, so what it writes is present
//! on both. It is therefore in the Oracle lane and the PostgreSQL lane at once —
//! the first thing in the model to belong to more than one — and two consequences
//! follow that are worth stating rather than discovering:
//!
//! * **`CONS001` cannot report it as a gap.** `lane_touches` asks the whole lane,
//!   so a portable script answers for *both* dialects and neither reads zero.
//!   That is the intended answer, not double-counting: the question is asked per
//!   dialect and the answers are never added together, and a row that really is
//!   installed on both engines really does cover both.
//! * **The one-finding-per-object-per-dialect dedup is untouched.** It is keyed
//!   on the dialect that is *missing* something, and a portable script only ever
//!   removes gaps. It cannot produce a second finding, only prevent a first.
//!
//! ## The lane is asked of the files, not of the folders
//!
//! In an untidy repository one folder holds both engines, told apart only by the
//! file names, and its coverage *column* is the folder's. So the lane question is
//! asked of the **sites** — each of which carries the scope of the file it was
//! found in — rather than summed out of that column, which would credit one
//! dialect with what the other's scripts did. See `lane_touches`.
//!
//! `CONS004` deliberately leaves portable folders out — see the note there.
//!
//! The other axis — one dialect's initialisation against its own updates — is
//! `CONS002`/`CONS003`, in [`crate::rules::propagation`].

use std::collections::{BTreeMap, BTreeSet};

use picus_inventory::prelude::ObjectEntry;
use picus_parse::prelude::{DmlOperation, EngineKind};
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
    // Whether the two dialects are comparable at all is a fact about the
    // repository, and on some there is no honest answer but "no": two halves that
    // have diverged in layout, in table names, in who generated them. Both rules
    // then produce a wall of findings that is accurate and unactionable, while the
    // rest of the report — the version chain, the duplicates, the dangerous DML,
    // the encodings — is worth having on its own.
    if !context.config.analysis.compare_dialects {
        for rule in [RuleId::Cons001, RuleId::Cons004] {
            output.skip(
                rule,
                "",
                "this project does not compare its two dialects against each other — turn the \
                 comparison back on in the project settings",
            );
        }
        return;
    }
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
        if !entry.kind.exists_in_both_engines() || context.excludes(&entry.name) {
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
        participating.iter().copied().filter(|d| lane_touches(entry, *d, role)).collect();
    let Some(reference) = covered.first().copied() else { return Vec::new() };
    participating
        .into_iter()
        .filter(|d| !lane_touches(entry, *d, role))
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
    let elsewhere = entry.writes_in(covering, role).next().map(|s| s.location());

    // The kind is named, and it is not decoration. A repository whose folders are
    // called `AGGIORNAMENTO` can perfectly well have a *table* called
    // `AGGIORNAMENTO` — an update log is exactly the sort of thing that gets that
    // name — and "AGGIORNAMENTO is not touched by the Oracle scripts", anchored at
    // a folder path ending in `AGGIORNAMENTO/2026/ORA`, reads as a claim about
    // the folder. It is a claim about the table, and one word settles it.
    let mut draft = Finding::new(
        RuleId::Cons001,
        Anchor::file(path),
        format!(
            "The {kind} {name} is not touched by the {engine} scripts",
            kind = entry.kind.as_str(),
            name = entry.name,
            engine = engine_label(missing)
        ),
        gap_consequence(entry, missing, covering, role),
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
    missing: EngineKind,
    covering: EngineKind,
    role: FolderRole,
) -> String {
    let count = lane_statements(entry, covering, role);
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
                if context.excludes(&shape.table.folded_name()) {
                    continue;
                }
                let key = (shape.table.folded_name(), placement.effective_role().as_str());
                let per_dialect = written.entry(key).or_default();
                let slot = per_dialect.entry(dialect).or_insert_with(|| Written {
                    columns: BTreeSet::new(),
                    rows: Some(BTreeSet::new()),
                    anchor: Anchor::at(
                        script.path,
                        script.parsed.line_of(shape.table.range.start),
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

/// Does the whole lane do anything at all with this object?
///
/// Asked of the **sites** — every place the object was named, each carrying the
/// scope of the file it was named in — rather than summed out of the coverage
/// map, and the difference matters in exactly one situation: a folder holding
/// both an `*_ORA.sql` and a `*_POS.sql` is in both lanes, and its coverage
/// *column* is the folder's. Summing that column into each lane would credit the
/// Oracle side with what the PostgreSQL scripts did and report a real gap as
/// covered — a false negative, which is the one kind of wrong answer this rule
/// must never give.
///
/// Still the whole lane and not one folder of it: a repository that splits its
/// updates over `2024/ORA` and `2025/ORA` has one update story, and looking at
/// either half alone would report the other as a gap.
///
/// A portable script is in every lane, so it answers for Oracle and for
/// PostgreSQL. Not double-counting: these are per-dialect questions that are
/// never added together, and one portable `INSERT` genuinely does put the row in
/// both installations.
fn lane_touches(entry: &ObjectEntry, dialect: EngineKind, role: FolderRole) -> bool {
    entry.writes_in(dialect, role).next().is_some()
}

/// How many statements the lane runs against this object — for the sentence that
/// explains the gap, and for nothing else.
///
/// Counted per **statement**, not per site: a statement that both defines and
/// references an object leaves two sites behind and has still done one thing to
/// it, which is the same thing the coverage cell counts. Separate from
/// [`lane_touches`] because that one is asked for every object in the repository
/// and only ever needs to know whether the answer is zero.
fn lane_statements(entry: &ObjectEntry, dialect: EngineKind, role: FolderRole) -> usize {
    entry
        .writes_in(dialect, role)
        .map(|site| (site.path.as_str(), site.statement_index))
        .collect::<BTreeSet<_>>()
        .len()
}
