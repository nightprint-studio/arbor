//! The Alt+Enter fixes.
//!
//! Each one re-derives its problem from the buffer rather than trusting the span it was handed: the
//! squiggle was drawn a keystroke ago, and a fix whose analysis no longer agrees offers nothing.

use bennu_ext::prelude::{ExtEdit, ExtIntention, ExtProblem};
use bennu_facts::prelude::JavaFacts;
use bennu_intentions::prelude::{insert_import_edit, Edit};

use crate::checks::{findings, unmapped, CODE_UNKNOWN_SOURCE, CODE_UNKNOWN_TARGET, CODE_UNMAPPED};
use crate::mapper::Mapper;
use crate::table::TypeTable;

pub const INTENT_IGNORE_UNMAPPED: &str = "mapstruct.ignore-unmapped";
pub const INTENT_DID_YOU_MEAN: &str = "mapstruct.did-you-mean";

/// How far a typo may be from the property it meant. Two covers a transposition and a slip; three
/// starts suggesting a different property that happens to be short.
const MAX_DISTANCE: usize = 2;

pub fn intentions(
    source: &str,
    facts: &JavaFacts,
    mappers: &[Mapper],
    table: &TypeTable,
    problems: &[ExtProblem],
) -> Vec<ExtIntention> {
    let mut out = Vec::new();
    for problem in problems {
        let offer = match problem.code.as_str() {
            CODE_UNMAPPED => ignore_unmapped(source, facts, mappers, table, problem),
            CODE_UNKNOWN_TARGET | CODE_UNKNOWN_SOURCE => did_you_mean(source, mappers, table, problem),
            _ => None,
        };
        out.extend(offer);
    }
    out
}

/// One `@Mapping(target = "x", ignore = true)` per unmapped property, on the lines above the method —
/// the explicit "yes, on purpose" that silences the warning and documents the decision where the
/// next reader will look.
fn ignore_unmapped(
    source: &str,
    facts: &JavaFacts,
    mappers: &[Mapper],
    table: &TypeTable,
    problem: &ExtProblem,
) -> Option<ExtIntention> {
    let method = mappers.iter().flat_map(|m| m.methods.iter()).find(|m| m.name_offset == problem.start)?;
    let names = unmapped(method, table)?;
    if names.is_empty() {
        return None;
    }
    let decl = method.anns.decl_start;
    let line_start = source.get(..decl)?.rfind('\n').map_or(0, |i| i + 1);
    let indent = &source[line_start..decl];
    // Something else shares the line — the layout is not one to guess an insertion into.
    if !indent.chars().all(|c| c == ' ' || c == '\t') {
        return None;
    }
    let (annotation, import) = mapping_reference(source, facts);
    let lines: String =
        names.iter().map(|n| format!("{indent}@{annotation}(target = \"{n}\", ignore = true)\n")).collect();

    let mut edits = Vec::new();
    if let Some(e) = import {
        edits.push(ExtEdit::replace(e.start, e.end, e.replacement));
    }
    edits.push(ExtEdit::insert(line_start, lines));
    let label = match names.as_slice() {
        [one] => format!("Ignore unmapped target property `{one}`"),
        _ => format!("Ignore {} unmapped target properties", names.len()),
    };
    Some(ExtIntention { id: INTENT_IGNORE_UNMAPPED.to_string(), label, edits })
}

/// How to write `@Mapping` in this file: by its simple name (importing it when needed), or qualified
/// when the simple name already means something else here.
fn mapping_reference(source: &str, facts: &JavaFacts) -> (&'static str, Option<Edit>) {
    let reachable = facts.imports.iter().any(|i| i == "org.mapstruct.Mapping" || i == "org.mapstruct.*");
    if reachable {
        return ("Mapping", None);
    }
    let taken = facts.imports.iter().any(|i| i.ends_with(".Mapping")) || facts.types.iter().any(|t| t.name == "Mapping");
    if taken {
        return ("org.mapstruct.Mapping", None);
    }
    ("Mapping", insert_import_edit(source, "org.mapstruct.Mapping"))
}

/// The one property a typo was plainly meant to be. Two candidates equally close is not a typo this
/// fix can settle.
fn did_you_mean(source: &str, mappers: &[Mapper], table: &TypeTable, problem: &ExtProblem) -> Option<ExtIntention> {
    let typed = source.get(problem.start..problem.end)?;
    let found = findings(mappers, table);
    let finding = found
        .iter()
        .find(|f| f.diag.code == problem.code && f.diag.start == problem.start && f.diag.end == problem.end)?;
    let mut close = finding.candidates.iter().filter(|c| edit_distance(typed, c) <= MAX_DISTANCE);
    let only = close.next()?;
    if close.next().is_some() {
        return None;
    }
    Some(ExtIntention {
        id: INTENT_DID_YOU_MEAN.to_string(),
        label: format!("Change to `{only}`"),
        edits: vec![ExtEdit::replace(problem.start, problem.end, only.clone())],
    })
}

/// Levenshtein distance, over chars.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != *cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_is_the_usual_one() {
        assert_eq!(edit_distance("nmae", "name"), 2);
        assert_eq!(edit_distance("name", "name"), 0);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("email", "emial"), 2);
    }
}
