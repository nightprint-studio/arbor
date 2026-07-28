//! `DIA001` — a statement the folder it sits in cannot run.
//!
//! Its own prefix rather than a third `CONS`, because it is a different kind of
//! claim: the `CONS` rules say two places in the repository disagree, and this
//! one says a single script will not run. Nothing needs to be compared to know
//! it — `NVL` in a PostgreSQL file is wrong on its own.
//!
//! Blocking, and it means it literally: the script stops at the offending
//! statement and everything below it is never applied.
//!
//! ## In a portable folder the rule inverts, and gets better
//!
//! A folder declared **portable** promises its scripts run on Oracle *and* on
//! PostgreSQL. So there, a construct belonging to **either** engine is a finding
//! — `MERGE … FROM DUAL` and `ON CONFLICT`, `SYSDATE` and `now()` alike — because
//! each of them keeps the promise on exactly one engine and breaks it on the
//! other. The parser does the inversion (`DialectScope::permits_syntax_of` is
//! false for both dialects under `Portable`), so this module only has to say the
//! right thing about it.
//!
//! And the right thing is about **the promise, not the construct's home**:
//! "PostgreSQL syntax in a portable script" reads like a category error, whereas
//! "this folder is declared portable and this line only runs on PostgreSQL" is
//! the sentence a maintainer can act on.

use std::collections::BTreeSet;

use picus_parse::prelude::line_col;
use picus_types::prelude::FolderRole;

use crate::context::{engine_label, Context};
use crate::finding::{Anchor, Finding};
use crate::report::Output;
use crate::rule::RuleId;

pub(crate) fn run(context: &Context<'_>, output: &mut Output) {
    for (script, placement) in context.project.placed() {
        let Some(scope) = placement.scope() else { continue };
        if placement.effective_role() == FolderRole::Ignored {
            continue;
        }
        // The parser reports "foreign" relative to the scope it was *given*. If
        // the caller parsed this file as something other than its folder's scope,
        // the list means nothing and reporting from it would be worse than
        // reporting nothing.
        if script.parsed.scope != scope {
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
                let home = engine_label(construct.belongs_to);
                let (title, detail) = if scope.is_portable() {
                    (
                        format!("{home}-only syntax in a portable script"),
                        format!(
                            "{}. This folder is declared portable, so its scripts are meant to run \
                             on every engine — but this line runs on {home} and stops with a \
                             syntax error everywhere else, with everything after it never applied.",
                            construct.message
                        ),
                    )
                } else {
                    let here = engine_label(scope.dialect().expect("a non-portable scope has one"));
                    (
                        format!("{home} syntax in a {here} script"),
                        format!(
                            "{}. Run against {here}, this script stops here with a syntax error \
                             and everything after it is never applied.",
                            construct.message
                        ),
                    )
                };
                output.findings.push(
                    Finding::new(RuleId::Dia001, Anchor::at(script.path, line), title, detail)
                        .build(),
                );
            }
        }
    }
}
