//! `VER001` / `VER002` / `VER003` — the version guard, the version bump, and the
//! chain between the files.
//!
//! These three fire **only in folders whose role is `update`**, and that is a
//! structural rule rather than a convenience: an initialisation script runs on an
//! empty database, where a starting-version guard would be a condition that is
//! never true, and reporting one would teach people that the report is wrong
//! about init folders (`docs/picus-design.md` §1).
//!
//! The unit `VER001` and `VER002` judge is the **file**. In these repositories an
//! update script *is* the block: it is one transition, applied whole, and the
//! guard is at the top of it. Reporting per statement would produce fifty
//! findings for one missing `IF`, all with the same fix.

use std::collections::BTreeMap;

use picus_parse::prelude::{DmlOperation, Statement, StatementKind};
use picus_project::prelude::{
    CompiledNaming, FolderNode, ScriptFile, Version, VersionRange,
};

use crate::context::Context;
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    guards(context, output);
    chain(context, output);
}

// ── VER001 / VER002 — per update file ────────────────────────────────────────

fn guards(context: &Context<'_>, output: &mut Output) {
    let Some(version_table) = context.version_table.as_deref() else {
        // Not a pass. A project that has emptied the version-table name has
        // switched these two rules off, and a report that stayed silent about it
        // would look like a project whose update scripts are all guarded.
        output.skip(
            RuleId::Ver001,
            "",
            "the project declares no version table, so there is nothing an update script could \
             check before it writes — set one in the project settings to turn this rule on",
        );
        output.skip(
            RuleId::Ver002,
            "",
            "the project declares no version table, so there is nothing an update script could \
             carry forward — set one in the project settings to turn this rule on",
        );
        return;
    };

    for (script, placement) in context.project.placed() {
        if placement.effective_role() != picus_types::prelude::FolderRole::Update {
            continue;
        }
        let statements = &script.parsed.statements;
        let Some(first_change) = statements.iter().find(|s| changes_something(s)) else {
            // An update file that changes nothing needs no guard and no bump. A
            // rule that fired here would fire on every README-in-SQL-form.
            continue;
        };
        let anchor = |offset: usize| {
            Anchor::at(script.path, script.parsed.line_of(offset))
        };

        if !reads_version(statements, version_table) {
            output.findings.push(
                Finding::new(
                    RuleId::Ver001,
                    anchor(first_change.range.start),
                    "This update script never checks which version it is starting from",
                    format!(
                        "It writes without reading {version_table} first, so running it a second \
                         time re-applies everything: on a database already upgraded, the changes \
                         land again and overwrite values that were correct.",
                    ),
                )
                .fix("Add the version guard")
                .build(),
            );
        }

        if !writes_version(statements, version_table) {
            let last = statements.last().map(|s| s.range.start).unwrap_or(0);
            output.findings.push(
                Finding::new(
                    RuleId::Ver002,
                    anchor(last),
                    "This update script never carries the version forward",
                    format!(
                        "It applies its changes and leaves {version_table} on the old value, so the \
                         next update refuses to start and the installation stalls one version \
                         behind — with the changes already applied.",
                    ),
                )
                .fix("Add the closing UPDATE")
                .build(),
            );
        }
    }
}

/// Does this statement change anything at all — data or schema?
fn changes_something(statement: &Statement) -> bool {
    if matches!(
        statement.kind,
        StatementKind::Create
            | StatementKind::Alter
            | StatementKind::Drop
            | StatementKind::Truncate
            | StatementKind::Insert
            | StatementKind::Update
            | StatementKind::Delete
            | StatementKind::Merge
    ) {
        return true;
    }
    // A `DECLARE … BEGIN … END` is a `Block`, and everything it does is nested.
    !statement.dml.is_empty()
}

/// Does the file **read** the version table anywhere?
///
/// Counted as "named more often than it is written to". A closing
/// `UPDATE VERSIONE_DB SET …` names it once and writes it once, so it nets zero
/// and does not pass for a guard; a `SELECT VERSIONE INTO v FROM VERSIONE_DB`
/// names it without writing it and does. Without the subtraction, `VER002` being
/// satisfied would silently satisfy `VER001` too, and the two would never be able
/// to disagree — which is exactly what a real half-guarded script looks like.
fn reads_version(statements: &[Statement], version_table: &str) -> bool {
    let mentions: usize = statements
        .iter()
        .flat_map(|s| s.references.iter())
        .filter(|r| r.folded_name() == version_table)
        .count();
    mentions > writes(statements, version_table)
}

fn writes_version(statements: &[Statement], version_table: &str) -> bool {
    writes(statements, version_table) > 0
}

fn writes(statements: &[Statement], version_table: &str) -> usize {
    statements
        .iter()
        .flat_map(|s| s.dml.iter())
        .filter(|d| {
            // A DELETE does not carry a version forward, it removes one.
            matches!(
                d.operation,
                DmlOperation::Insert | DmlOperation::Update | DmlOperation::Merge
            ) && d.table.folded_name() == version_table
        })
        .count()
}

// ── VER003 — the chain across the files ──────────────────────────────────────

fn chain(context: &Context<'_>, output: &mut Output) {
    // Every update folder in the tree, at whatever depth and whatever dialect —
    // including one no ancestor declares a dialect for. A hole in a version chain
    // is a fact about the files' own names, and it is just as wrong in a folder
    // nobody has classified yet.
    for folder in
        context.folders().filter(|f| f.effective_role == picus_types::prelude::FolderRole::Update)
    {
        check_folder(context, folder, output);
    }
}

fn check_folder(context: &Context<'_>, folder: &FolderNode, output: &mut Output) {
    if folder.files.is_empty() {
        return;
    }
    let scheme = context.naming_for(folder);
    let naming: CompiledNaming = match scheme.compile() {
        Ok(naming) => naming,
        Err(error) => {
            output.skip(RuleId::Ver003, &folder.path, error.to_string());
            return;
        }
    };
    // The rule the design document calls out by name: without a starting version
    // there is no chain, only a list, and a rule that cannot run must say so
    // rather than pass.
    if !naming.tracks_starting_version() {
        output.skip(
            RuleId::Ver003,
            &folder.path,
            format!(
                "the naming pattern for `{}` records only the version a file installs, not the one \
                 it starts from, so there is no chain to find a hole in — add a (?P<from>…) group \
                 to the pattern if these files do carry both",
                folder.path
            ),
        );
        return;
    }

    let mut ordered: Vec<(&ScriptFile, VersionRange)> = folder
        .files
        .iter()
        .filter_map(|file| naming.parse(&file.name).map(|range| (file, range)))
        .collect();
    if ordered.is_empty() {
        output.skip(
            RuleId::Ver003,
            &folder.path,
            format!(
                "no file in `{}` matches the project's update-file pattern, so the version chain \
                 could not be read at all — check the pattern in the project settings",
                folder.path
            ),
        );
        return;
    }
    ordered.sort_by(|a, b| (&a.1.to, &a.0.path).cmp(&(&b.1.to, &b.0.path)));

    let mut installs: BTreeMap<String, &ScriptFile> = BTreeMap::new();
    for (file, range) in &ordered {
        if let Some(previous) = installs.insert(range.to.to_string(), file) {
            output.findings.push(duplicate_target(file, previous, &range.to));
        }
    }

    for pair in ordered.windows(2) {
        let (previous, previous_range) = &pair[0];
        let (file, range) = &pair[1];
        let Some(from) = range.from.as_ref() else { continue };
        if *from == previous_range.to {
            continue;
        }
        output.findings.push(broken_link(file, previous, from, &previous_range.to));
    }
}

fn duplicate_target(
    file: &ScriptFile,
    previous: &ScriptFile,
    version: &Version,
) -> Finding {
    Finding::new(
        RuleId::Ver003,
        Anchor::file(&file.path),
        format!("Two update files both install {version}"),
        format!(
            "`{}` and `{}` both claim to bring the database to {version}. Whichever runs second \
             finds the version already there and is skipped, so half the change is never applied \
             and nothing says which half.",
            previous.name, file.name
        ),
    )
    .also_at(previous.path.clone())
    .build()
}

fn broken_link(
    file: &ScriptFile,
    previous: &ScriptFile,
    from: &Version,
    previous_to: &Version,
) -> Finding {
    let (title, consequence) = if from > previous_to {
        (
            format!("The update chain has a hole between {previous_to} and {from}"),
            format!(
                "`{}` leaves the database at {previous_to} and `{}` refuses to start on anything \
                 but {from}. An installation that follows these files in order stops at \
                 {previous_to} and never reaches the end.",
                previous.name, file.name
            ),
        )
    } else {
        (
            format!("Two update files overlap between {from} and {previous_to}"),
            format!(
                "`{}` already takes the database past {from}, and `{}` starts from there again — \
                 the statements in the overlap are applied twice, or not at all, depending on which \
                 order somebody runs them in.",
                previous.name, file.name
            ),
        )
    };
    Finding::new(RuleId::Ver003, Anchor::file(&file.path), title, consequence)
        .also_at(previous.path.clone())
        .build()
}
