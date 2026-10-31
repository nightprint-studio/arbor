//! The whole argument list of the call whose `(` the caret has just typed, from the project's index.
//!
//! The work is `bennu_query::prelude::argument_list_with`: the callee's parameters against the names in scope, one list
//! or nothing. This is the seam to the backend: the file's resolver, and the answer in the shape the ghost text uses.
//! Pure lookups on the index, no model and no disk: it is fast enough for the keystroke path.

use crate::index_service::IndexService;

/// What to draw after the `(`.
pub(crate) struct ArgList {
    pub(crate) text: String,
    /// 0..1.
    pub(crate) confidence: f32,
}

/// The argument list for the call opened at `offset` of `source`, or `None` when the caret is not right after a `(`
/// of a resolvable call, a slot has no confident choice, or the index is not up yet.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<ArgList> {
    if !source.get(..offset)?.ends_with('(') {
        return None;
    }
    let resolver = IndexService::global().caret_resolver_for(file)?;
    // two names fit a slot equally: the one the project always passes there (a file does not vote for itself)
    let habit = |callee: &str, index: usize| crate::flc_habits::habit_name_in(file, callee, index).map(|(name, _, _)| name);
    // the overload the user chose in the completion list a moment ago decides between overloads
    let picked = bennu_query::prelude::call_site_name_and_index(source, offset).and_then(|(name, _, _)| crate::completion_picked::picked_overload(file, &name));
    let list = bennu_query::prelude::argument_list_for(source, offset, &*resolver, &habit, picked.as_deref())?;
    Some(ArgList { text: list.text, confidence: list.confidence })
}

/// The arguments that remain once the first ones are written: `store.upload(first, |` → `doc);`. The first may be whatever the
/// user decided, or was unsure of; each of the rest is as certain as its type makes it.
pub(crate) fn rest(file: &str, source: &str, offset: usize) -> Option<ArgList> {
    if !source.get(..offset)?.trim_end().ends_with(',') {
        return None;
    }
    let resolver = IndexService::global().caret_resolver_for(file)?;
    let habit = |callee: &str, index: usize| crate::flc_habits::habit_name_in(file, callee, index).map(|(name, _, _)| name);
    let list = bennu_query::prelude::argument_rest(source, offset, &*resolver, &habit)?;
    Some(ArgList { text: list.text, confidence: list.confidence })
}

/// `list.toArray(|` → `Item[]::new` (what is left of it after the part already typed), when the collection says what
/// its elements are.
pub(crate) fn array_constructor(file: &str, source: &str, offset: usize) -> Option<ArgList> {
    let resolver = IndexService::global().caret_resolver_for(file)?;
    let whole = bennu_query::prelude::array_constructor_argument(source, offset, &*resolver)?;
    let (_, _, start) = bennu_query::prelude::call_site_name_and_index(source, offset)?;
    let typed = source.get(start..offset)?.trim_start();
    let rest = whole.strip_prefix(typed).filter(|r| !r.is_empty())?;
    Some(ArgList { text: rest.to_string(), confidence: 0.9 })
}

/// The expression that makes the class the caret's position wants (`Pair.of(`, `tabellato(`, `new Foo(`), minus the
/// part already typed.
pub(crate) fn creation(file: &str, source: &str, offset: usize) -> Option<ArgList> {
    let resolver = IndexService::global().caret_resolver_for(file)?;
    let whole = bennu_query::prelude::expected_creation(source, offset, &*resolver)?;
    let (_, _, start) = bennu_query::prelude::call_site_name_and_index(source, offset)?;
    let typed = source.get(start..offset)?.trim_start();
    let rest = whole.strip_prefix(typed).filter(|r| !r.is_empty())?;
    Some(ArgList { text: rest.to_string(), confidence: 0.7 })
}

/// `getForObject(url, String|` → `.class`: a type name written where the parameter is a `Class<?>`, which is the literal
/// being started. Only a whole, known type name directly before the caret, and a slot every overload agrees is a class.
pub(crate) fn class_literal(file: &str, source: &str, offset: usize) -> Option<ArgList> {
    let head = source.get(..offset)?;
    let word: String = head.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect::<Vec<_>>().into_iter().rev().collect();
    if word.is_empty() || !word.starts_with(|c: char| c.is_ascii_uppercase()) || head[..head.len() - word.len()].trim_end().ends_with('.') {
        return None;
    }
    if !IndexService::global().type_resolves(file, source, &word) {
        return None;
    }
    let resolver = IndexService::global().caret_resolver_for(file)?;
    let wanted = bennu_query::prelude::argument_expected_type(source, offset, &*resolver)?;
    (wanted.binary_name == "java/lang/Class").then(|| ArgList { text: ".class".to_string(), confidence: 0.9 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_caret_right_after_a_parenthesis_is_asked() {
        assert!(suggest("/nowhere/A.java", "class A { void f() { g", 21).is_none());
        assert!(suggest("/nowhere/A.java", "short", 99).is_none());
    }

    #[test]
    fn without_an_index_for_the_file_there_is_no_answer() {
        assert!(suggest("/nowhere/A.java", "class A { void f() { g(", 22).is_none());
    }
}
