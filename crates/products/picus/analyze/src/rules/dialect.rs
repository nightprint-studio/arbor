//! `DIA001` — a statement written in the dialect its folder is not.
//!
//! Its own prefix rather than a third `CONS`, because it is a different kind of
//! claim: the `CONS` rules say two places in the repository disagree, and this
//! one says a single script will not run. Nothing needs to be compared to know
//! it — `NVL` in a PostgreSQL file is wrong on its own.
//!
//! Blocking, and it means it literally: the script stops at the offending
//! statement and everything below it is never applied.

use std::collections::BTreeSet;

use picus_parse::prelude::line_col;
use picus_types::prelude::FolderRole;

use crate::context::{engine_label, Context};
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    for (script, placement) in context.project.placed() {
        let Some(dialect) = placement.branch.dialect else { continue };
        if placement.folder.role == FolderRole::Ignored {
            continue;
        }
        // The parser reports "foreign" relative to the engine it was *given*. If
        // the caller parsed this file as something other than its branch's
        // dialect, the list means nothing and reporting from it would be worse
        // than reporting nothing.
        if script.parsed.engine != dialect {
            continue;
        }
        for statement in &script.parsed.statements {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for construct in &statement.foreign {
                if !seen.insert(construct.construct) {
                    // One finding per construct per statement. A statement using
                    // NVL four times is one thing to rewrite, not four.
                    continue;
                }
                let line = line_col(script.source, construct.range.start).0;
                output.findings.push(
                    Finding::new(
                        RuleId::Dia001,
                        Anchor::at(script.path, &placement.branch.id, line),
                        format!(
                            "{} syntax in a {} script",
                            engine_label(construct.belongs_to),
                            engine_label(dialect)
                        ),
                        format!(
                            "{}. Run against {}, this script stops here with a syntax error and \
                             everything after it is never applied.",
                            construct.message,
                            engine_label(dialect)
                        ),
                    )
                    .build(),
                );
            }
        }
    }
}
