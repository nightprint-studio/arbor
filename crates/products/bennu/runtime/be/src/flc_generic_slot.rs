//! A habit is the text most often passed at `(callee, position)`. When that position takes a type variable
//! (`<T> Matcher<T> helper(T value)`), the project's usual text says what `T` was most often, not what it is *here*:
//! the call around it decides (`.field(helper(|))` inside a slot that wants a `Matcher<ColorB>`). So the usual text
//! is kept only when its own type fits the type the outer call makes of the variable, and when nothing settles the
//! variable the habit is still offered, but as a guess: less confident in the ghost text, behind the provider's rows
//! in the popup.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use bennu_java::prelude::{TypeRef, TypeResolver};
use bennu_query::prelude::{fits_type, generic_slot, written_type, GenericSlot};

use crate::index_service::IndexService;

/// Habit texts judged per call: each one is parsed into the file, so a place with many distinct texts is cut here.
const MAX_JUDGED: usize = 16;
/// What a habit's confidence is scaled by while its type is a guess.
const GUESS_FACTOR: f32 = 0.5;

pub(crate) struct GenericSlotFit {
    resolver: Arc<dyn TypeResolver + Send + Sync>,
    source: String,
    start: usize,
    offset: usize,
    /// `None` when nothing settles the variable.
    wanted: Option<TypeRef>,
    judged: RefCell<HashMap<String, bool>>,
}

/// The fit of a habit text at the caret, when the argument there fills a type-variable parameter; `None` for a concrete
/// parameter (the declaration decides, as before) and when the file has no index to ask.
pub(crate) fn at(file: &str, source: &str, offset: usize) -> Option<GenericSlotFit> {
    let (_, _, start) = bennu_query::prelude::call_site_name_and_index(source, offset)?;
    let resolver = IndexService::global().caret_resolver_for(file)?;
    let wanted = match generic_slot(source, offset, &*resolver) {
        GenericSlot::Fixed => return None,
        GenericSlot::Settled(t) => Some(t),
        GenericSlot::Free => None,
    };
    Some(GenericSlotFit { resolver, source: source.to_string(), start, offset, wanted, judged: RefCell::new(HashMap::new()) })
}

impl GenericSlotFit {
    /// Whether nothing settles the variable.
    pub(crate) fn is_free(&self) -> bool {
        self.wanted.is_none()
    }

    /// Whether the habit `text` may be offered: its type fits the settled one (a text that cannot be typed is not
    /// accused), or the variable is free.
    pub(crate) fn admits(&self, text: &str) -> bool {
        let Some(wanted) = &self.wanted else { return true };
        let mut judged = self.judged.borrow_mut();
        if let Some(known) = judged.get(text) {
            return *known;
        }
        let verdict = judged.len() < MAX_JUDGED
            && written_type(&self.source, self.start, self.offset, text, &*self.resolver).map_or(true, |t| fits_type(&t, wanted, &*self.resolver));
        judged.insert(text.to_string(), verdict);
        verdict
    }

    /// The confidence a habit keeps: all of it where the type is settled, less where it is a guess.
    pub(crate) fn confidence(&self, share: f32) -> f32 {
        if self.is_free() { share * GUESS_FACTOR } else { share }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flc_habits::Habits;
    use bennu_complete::prelude::MatchCase;
    use bennu_proto::prelude::CompletionItem;

    /// Six files that pass `Size.SMALL` to `same` and two that pass `Color.RED`: the dominant text is the wrong one
    /// wherever the outer call wants a `Color`.
    fn project() -> Habits {
        let mut h = Habits::default();
        for i in 0..6 {
            h.add_file(&format!("S{i}.java"), "class A {\n    void f() {\n        q.size(Helper.same(Size.SMALL));\n    }\n}\n");
        }
        for i in 0..2 {
            h.add_file(&format!("C{i}.java"), "class A {\n    void f() {\n        q.color(Helper.same(Color.RED));\n    }\n}\n");
        }
        h
    }

    const USE: &str = "class U {\n    void f() {\n        q.color(Helper.same(\n    }\n}\n";

    fn caret() -> usize {
        USE.find("same(").unwrap() + 5
    }

    #[test]
    fn the_dominant_text_of_another_type_is_not_offered() {
        let h = project();
        assert_eq!(h.suggest("U.java", USE, caret()).map(|x| x.text).as_deref(), Some("Size.SMALL"));
        // two Colors are too few sightings to lead: dropping the dominant one leaves no habit, not the wrong one
        assert!(h.suggest_fit("U.java", USE, caret(), &|t| t.starts_with("Color.")).is_none());
        let got = h.suggest_fit("U.java", USE, caret(), &|t| t.starts_with("Size.")).expect("a fitting habit");
        assert_eq!(got.text, "Size.SMALL");
    }

    #[test]
    fn a_text_of_the_wrong_type_gives_way_to_the_next_one() {
        let mut h = Habits::default();
        for i in 0..6 {
            h.add_file(&format!("S{i}.java"), "class A {\n    void f() {\n        q.size(Helper.same(Size.SMALL));\n        q.color(Helper.same(Color.RED));\n        q.color(Helper.same(Color.RED));\n    }\n}\n");
        }
        // 12 Color.RED against 6 Size.SMALL
        assert_eq!(h.suggest("U.java", USE, caret()).unwrap().text, "Color.RED");
        let got = h.suggest_fit("U.java", USE, caret(), &|t| t.starts_with("Size.")).expect("the fitting one leads");
        assert_eq!(got.text, "Size.SMALL");
    }

    #[test]
    fn popup_rows_are_filtered_and_a_demoted_habit_comes_after_the_providers_rows() {
        let h = project();
        let provider = || vec![CompletionItem { label: "Color.GREEN".into(), preselect: true, ..Default::default() }];
        let at = caret();

        let mut items = provider();
        h.rows_fit("U.java", USE, at, MatchCase::default(), &mut items, &|t| t.starts_with("Color."), false);
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Color.GREEN"], "no Size row");

        let mut items = provider();
        h.rows_fit("U.java", USE, at, MatchCase::default(), &mut items, &|_| true, true);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels.first(), Some(&"Color.GREEN"), "{labels:?}");
        assert!(labels.contains(&"Size.SMALL"));
        assert!(items[0].preselect && items.iter().skip(1).all(|i| !i.preselect), "the guess is not preselected");
    }
}
