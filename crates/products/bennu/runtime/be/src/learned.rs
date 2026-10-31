//! What Bennu has learned, as a person reads it — and what can be taken back.
//!
//! The tables behind the ghost text (usual arguments, what follows what, calls on a variable, what was done with
//! suggestions) are counters keyed by place; this turns them into groups of readable rows. Taking a row back is a *rule*, not
//! a deleted count — see [`crate::learned_rules`] — because the counts are derived from the project's files and would
//! come back with the next scan. The corrections, which are the user's own, are simply forgotten.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::learned_rules::{self, Rule};

/// Most places shown per table, and most rows per place: a page to read, not the table.
const MAX_GROUPS: usize = 300;
const MAX_ITEMS: usize = 10;

#[derive(Serialize)]
pub struct LearnedItem {
    pub text: String,
    pub count: u32,
    /// The share of the place's sightings this one holds, 0..1.
    pub share: Option<f32>,
    /// The user asked for it not to be offered.
    pub hidden: bool,
    /// What else is known about the row, where there is more than a count (what became of the suggestions).
    pub detail: Option<String>,
}

#[derive(Serialize)]
pub struct LearnedGroup {
    pub key: String,
    pub total: u32,
    /// The whole place is hidden: nothing is offered from it.
    pub hidden: bool,
    pub items: Vec<LearnedItem>,
}

#[derive(Serialize)]
pub struct LearnedSection {
    pub id: &'static str,
    /// The `kind` a rule is made with, or empty where a row can only be forgotten outright.
    pub kind: &'static str,
    pub title: &'static str,
    pub about: &'static str,
    pub groups: Vec<LearnedGroup>,
}

#[derive(Serialize)]
pub struct LearnedReport {
    pub sections: Vec<LearnedSection>,
    /// How the project lays out its code, one line per thing it is consistent about.
    pub layout: Vec<String>,
    pub learning_on: bool,
}

fn groups(kind: &str, rows: Vec<(String, Vec<(String, u32)>)>) -> Vec<LearnedGroup> {
    rows.into_iter()
        .take(MAX_GROUPS)
        .map(|(key, items)| {
            let total: u32 = items.iter().map(|(_, n)| n).sum();
            let items = items
                .into_iter()
                .take(MAX_ITEMS)
                .map(|(text, count)| LearnedItem {
                    share: (total > 0).then(|| count as f32 / total as f32),
                    hidden: learned_rules::hidden(kind, &key, &text),
                    detail: None,
                    text,
                    count,
                })
                .collect();
            LearnedGroup { hidden: learned_rules::place_hidden(kind, &key), key, total, items }
        })
        .collect()
}

/// What the counters of corrections say, one group per kind of suggestion, one row per place it was used.
fn corrections() -> Vec<LearnedGroup> {
    let mut by_origin: std::collections::BTreeMap<String, Vec<LearnedItem>> = Default::default();
    for (origin, context, total, useful, detail) in crate::learn::corrections() {
        by_origin.entry(origin).or_default().push(LearnedItem { text: context, count: total.round() as u32, share: useful, hidden: false, detail: Some(detail) });
    }
    by_origin
        .into_iter()
        .map(|(key, mut items)| {
            items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.text.cmp(&b.text)));
            let total = items.iter().map(|i| i.count).sum();
            items.truncate(MAX_ITEMS * 3);
            LearnedGroup { key, total, hidden: false, items }
        })
        .collect()
}

/// Everything the page shows.
#[arbor_rpc::handler]
fn bennu_learned_report(_ctx: &BennuState) -> Result<LearnedReport, String> {
    let sections = vec![
        LearnedSection {
            id: "habits",
            kind: "habit",
            title: "Usual arguments",
            about: "What this project passes at a place in a call, seen across its files. A row you hide is never offered again.",
            groups: groups("habit", crate::flc_habits::report(false)),
        },
        LearnedSection {
            id: "names",
            kind: "name",
            title: "Usual variables",
            about: "Which variable the project passes at a place, when it is always the same one.",
            groups: groups("name", crate::flc_habits::report(true)),
        },
        LearnedSection {
            id: "sequences",
            kind: "sequence",
            title: "What follows what",
            about: "How the line after a kind of statement usually starts. Offered on an empty line.",
            groups: groups("sequence", crate::flc_sequence::report()),
        },
        LearnedSection {
            id: "created",
            kind: "created",
            title: "Types by variable name",
            about: "The class this project creates under a variable name (`bao` is a `ByteArrayOutputStream`), offered after `new`. A row you hide is never offered again.",
            groups: groups("created", crate::flc_created::report()),
        },
        LearnedSection {
            id: "member_args",
            kind: "member_args",
            title: "Calls on a variable",
            about: "A call the project always makes on whichever variable is at hand, such as .toByteArray().",
            groups: groups("member_args", crate::flc_member_args::report()),
        },
        LearnedSection {
            id: "corrections",
            kind: "",
            title: "What you did with the suggestions",
            about: "Per kind of suggestion and place: how many were settled, and how many of them were useful as far as they went. Forgetting a row starts it over.",
            groups: corrections(),
        },
    ];
    Ok(LearnedReport { sections, layout: crate::flc_format::describe(), learning_on: crate::learn::enabled() })
}

#[derive(Deserialize)]
pub struct RuleArgs {
    pub kind: String,
    pub key: String,
    pub text: String,
}

/// Stop offering one learned row.
#[arbor_rpc::handler]
fn bennu_learned_hide(_ctx: &BennuState, args: RuleArgs) -> Result<(), String> {
    learned_rules::hide(Rule { kind: args.kind, key: args.key, text: args.text });
    Ok(())
}

/// Offer a hidden row again.
#[arbor_rpc::handler]
fn bennu_learned_restore(_ctx: &BennuState, args: RuleArgs) -> Result<(), String> {
    learned_rules::restore(&Rule { kind: args.kind, key: args.key, text: args.text });
    Ok(())
}

#[derive(Deserialize)]
pub struct CorrectionArgs {
    pub origin: String,
    pub context: String,
}

/// Forget what was counted for one kind of suggestion in one place.
#[arbor_rpc::handler]
fn bennu_learned_forget_correction(_ctx: &BennuState, args: CorrectionArgs) -> Result<(), String> {
    crate::learn::forget_correction(&args.origin, &args.context);
    Ok(())
}
