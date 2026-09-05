//! The one call the editor makes: everything that can be done here, planned.
//!
//! ## Why one call and not five
//!
//! Alt+Enter is a single gesture — *"what can you do where I am standing"* — and the answer has to
//! arrive as one list, ordered, with the refusals in it. Five calls would be five round trips, five
//! parses of the same buffer, and an order decided by whichever answered first.
//!
//! So the buffer is parsed **once** here and every refactoring is asked against the same tree. A new
//! one costs a line in [`refactorings_at`].
//!
//! ## The order
//!
//! What the user is most likely reaching for, given what they did with the mouse. A selection over
//! statements means *extract method*; a caret in an expression means *extract variable*; a caret on
//! a name means one of the inlines. Each refactoring already answers `None` where it does not
//! apply, so the order is a preference and never a filter.

use bennu_java::prelude::parse_java;

use crate::plan::{Plan, Refusal};

/// Everything on offer at a caret (`start == end`) or over a selection.
///
/// A `Err(Refusal)` in the list is deliberate and is not an error: it is a row the editor shows
/// greyed, with the reason. "Cannot extract: the selection produces `total` and `count`" tells the
/// user what to change; an absent row teaches nothing.
pub fn refactorings_at(source: &str, start: usize, end: usize) -> Vec<Result<Plan, Refusal>> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    [
        crate::extract_method::extract_method(root, source, start, end),
        crate::extract_var::extract_variable(root, source, start, end),
        crate::extract_var::extract_constant(root, source, start, end),
        crate::inline_var::inline_variable(root, source, start),
        crate::inline_method::inline_method(root, source, start),
        crate::if_statement::invert_if(root, source, start, end),
        crate::if_statement::merge_nested_if(root, source, start, end),
        crate::declaration::split_declaration(root, source, start, end),
        crate::declaration::join_declaration(root, source, start, end),
        crate::declaration::to_var(root, source, start, end),
        crate::declaration::from_var(root, source, start, end),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Just the ones that would work — for a caller that wants to act rather than to offer.
pub fn plans_at(source: &str, start: usize, end: usize) -> Vec<Plan> {
    refactorings_at(source, start, end).into_iter().filter_map(Result::ok).collect()
}

/// The plan for one refactoring by id, when it applies here. What the editor calls after the user
/// picks a row, so the plan it applies is computed against the buffer as it is *now* rather than as
/// it was when the menu opened.
pub fn plan_for(id: &str, source: &str, start: usize, end: usize) -> Option<Result<Plan, Refusal>> {
    refactorings_at(source, start, end)
        .into_iter()
        .find(|outcome| match outcome {
            Ok(plan) => plan.id == id,
            Err(refusal) => refusal.id == id,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "class A {\n    int f(int n) {\n        int base = n * 2;\n        return base + 1;\n    }\n}";

    #[test]
    fn a_selection_over_statements_offers_extracting_a_method() {
        let start = SRC.find("int base").unwrap();
        let end = SRC.find("n * 2;").unwrap() + "n * 2;".len();
        let ids: Vec<String> = plans_at(SRC, start, end).into_iter().map(|p| p.id).collect();
        assert!(ids.contains(&"extract-method".to_string()), "{ids:?}");
    }

    #[test]
    fn a_caret_in_an_expression_offers_extracting_a_variable() {
        // `base + 1`, not `n * 2`: the latter is already a declaration's initialiser, which
        // extract-variable refuses by design — a fixture that measures the refusal, not the offer.
        let at = SRC.find("base + 1").unwrap() + 1;
        let ids: Vec<String> = plans_at(SRC, at, at).into_iter().map(|p| p.id).collect();
        assert!(ids.contains(&"extract-variable".to_string()), "{ids:?}");
    }

    #[test]
    fn a_caret_on_a_local_offers_inlining_it() {
        let at = SRC.find("base + 1").unwrap() + 1;
        let ids: Vec<String> = plans_at(SRC, at, at).into_iter().map(|p| p.id).collect();
        assert!(ids.contains(&"inline-variable".to_string()), "{ids:?}");
    }

    /// A caret in something that is none of these produces nothing at all — not a list of five
    /// rows explaining why each does not apply.
    #[test]
    fn a_caret_with_nothing_to_offer_is_silent() {
        let at = SRC.find("class A").unwrap() + 2;
        assert!(refactorings_at(SRC, at, at).is_empty());
    }

    /// The buffer is re-read when a row is chosen, so a plan is never applied against text it was
    /// not computed from.
    #[test]
    fn a_plan_can_be_asked_for_again_by_id() {
        let at = SRC.find("base + 1").unwrap() + 1;
        let again = plan_for("extract-variable", SRC, at, at);
        assert!(matches!(again, Some(Ok(plan)) if plan.id == "extract-variable"));
        assert!(plan_for("no-such-refactoring", SRC, at, at).is_none());
    }

    #[test]
    fn a_source_that_does_not_parse_offers_nothing_rather_than_panicking() {
        assert!(refactorings_at("class {{{", 3, 3).is_empty());
    }
}
