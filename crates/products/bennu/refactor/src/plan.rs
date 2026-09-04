//! What a refactoring produces before anything is written, and what it says when it will not.
//!
//! ## Plan, then edit
//!
//! Every refactoring here answers in two steps, and the split is the whole safety story. Planning
//! reads the buffer and decides; applying is a list of byte-range replacements. Nothing in this
//! crate touches a file, so a plan can be shown, discarded, or fed back with a different name
//! without anything having happened.
//!
//! ## Refusing is a result, not an error
//!
//! A refactoring that cannot be done safely has to say **why**, in the words of the code in front
//! of the user: *"the selection assigns two locals that are read afterwards"* is actionable and
//! *"cannot extract"* is not. So [`Refusal`] carries a sentence, and the editor shows it on a
//! greyed row rather than hiding the offer — which is also what the language servers do for their
//! own refactorings, and the reason the two read the same way in one menu.

use serde::{Deserialize, Serialize};

/// One byte-range replacement. The only thing any refactoring ever produces.
///
/// Byte offsets, like every other span on the bennu wire: the frontend maps them against the buffer
/// it holds and applies them through CodeMirror, so a refactoring is undone like any other edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
    /// What this edit is for, so a preview can group and label them: `"call"`, `"declaration"`,
    /// `"body"`, `"use"`, `"import"`.
    pub reason: String,
}

impl RefactorEdit {
    pub fn new(start: usize, end: usize, text: impl Into<String>, reason: &str) -> Self {
        Self { start, end, text: text.into(), reason: reason.to_string() }
    }
}

/// A refactoring, planned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Stable id of the refactoring that produced it (`"extract-method"`).
    pub id: String,
    /// What the menu row says (`"Extract method"`).
    pub label: String,
    /// The edits, **in descending start order** so a caller can apply them one after another
    /// without re-mapping offsets. See [`Plan::sorted`] — this is an invariant, not a convention.
    pub edits: Vec<RefactorEdit>,
    /// The name the refactoring introduces, when it introduces one. The editor offers it for
    /// renaming before applying, which is the only interaction any of these need.
    pub name: Option<String>,
    /// Where the caret should land afterwards — the introduced name, so it can be typed over.
    pub caret: Option<usize>,
    /// A type this plan could not name on its own; see [`TypeSlot`].
    #[serde(default)]
    pub type_slot: Option<TypeSlot>,
    /// A `throws` clause this plan could only guess at; see [`ThrowsSlot`].
    #[serde(default)]
    pub throws_slot: Option<ThrowsSlot>,
}

impl Plan {
    /// Build a plan with its edits in the order they must be applied.
    ///
    /// **Descending**, and that is not a style choice: applying an edit shifts every offset after
    /// it, so a caller working forwards has to re-map the rest after each one. Working backwards,
    /// nothing it has yet to apply has moved. Every consumer would otherwise have to know that,
    /// and one of them would eventually not.
    pub fn new(id: &str, label: &str, mut edits: Vec<RefactorEdit>) -> Self {
        // Descending by start, and **by end within the same start** — which is not a tie-break
        // detail. An *extract variable* whose expression begins its own statement produces an
        // insertion at X and a replacement of `X..X+n`: apply the insertion first and the
        // replacement then overwrites the text just inserted, silently, and the buffer is corrupt.
        // Widest first means every edit at that offset is consumed before anything is inserted
        // there. Seen on `this.params.add(param);`, which is as ordinary as Java gets.
        reorder(&mut edits);
        Self {
            id: id.to_string(),
            label: label.to_string(),
            edits,
            name: None,
            caret: None,
            type_slot: None,
            throws_slot: None,
        }
    }

    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn caret_at(mut self, offset: usize) -> Self {
        self.caret = Some(offset);
        self
    }

    pub fn needing_throws(mut self, slot: ThrowsSlot) -> Self {
        self.throws_slot = Some(slot);
        self
    }

    pub fn needing_type(mut self, slot: TypeSlot) -> Self {
        self.type_slot = Some(slot);
        self
    }

    /// Apply the plan to a source string. The reference implementation, and what the tests here
    /// check against — the editor applies the same edits through its own buffer.
    pub fn apply(&self, source: &str) -> String {
        let mut out = source.to_string();
        for edit in &self.edits {
            let (start, end) = (edit.start.min(out.len()), edit.end.min(out.len()));
            out.replace_range(start..end, &edit.text);
        }
        out
    }

    /// Restore the application order after edits have been added.
    ///
    /// A consumer that appends an edit — the backend adds the `import` line — has to put the list
    /// back in order, and doing that with its own `sort_by` is how the tie-break below silently
    /// went missing once already. One rule, in one place, reachable from both.
    pub fn reorder(&mut self) {
        reorder(&mut self.edits);
    }

    /// Whether the edits hold the descending invariant. Cheap, and used by the tests that would
    /// otherwise only catch a violation as a corrupted string.
    pub fn sorted(&self) -> bool {
        self.edits.windows(2).all(|w| w[0].start >= w[1].end || w[0].start == w[1].start)
    }
}

/// Descending by start, and by end within the same start — see [`Plan::new`] for why the second
/// half is load-bearing rather than a tidy-up.
fn reorder(edits: &mut [RefactorEdit]) {
    edits.sort_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));
}

/// A type the plan needs written into the source and could not name by reading the text.
///
/// The introduced local of an *extract variable* is the case: `var x = repo.findAll();` needs the
/// static type of a call, which is a question for the resolver — the classpath, the JDK, the
/// project index — and this crate has none of that on purpose. So the plan names the span whose
/// type it needs and the caller, which does have a resolver, fills it in.
///
/// The alternative was writing `var` and being done. It is wrong on a Java 8 project, which is most
/// of the code this editor exists for, and it is worse than wrong on any project whose style forbids
/// it: a refactoring that quietly changes how the codebase is written is one nobody uses twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeSlot {
    /// The expression whose type is wanted.
    pub start: usize,
    pub end: usize,
    /// Where in the plan's edit text the type goes — the byte offset inside
    /// `edits[edit_index].text` of the placeholder.
    pub edit_index: usize,
    pub at: usize,
    /// What is written there until the caller replaces it, so a plan applied without a resolver
    /// still produces something (and something that compiles from Java 10 on).
    pub placeholder: String,
    /// Whether the placeholder is NOT an acceptable answer.
    ///
    /// `var` stands in fine for `var rows = load();` — it compiles, it is what javac would infer,
    /// and a project on Java 10+ would have written it anyway. It does not stand in at all where
    /// the expression may have no value: naming a whole statement, `obj.setName(x);`, gives
    /// `var setName = obj.setName(x);` and `void` has nothing to infer from. There the caller
    /// either names the type or must not apply the plan.
    #[serde(default)]
    pub required: bool,
}

impl Plan {
    /// Write a resolved type into the slot, and clear it.
    ///
    /// The whole exchange with the caller that has a resolver: it reads
    /// [`TypeSlot::start`]/[`TypeSlot::end`], infers, and hands the spelling back.
    pub fn fill_type(&mut self, type_name: &str) {
        let Some(slot) = self.type_slot.take() else { return };
        let Some(edit) = self.edits.get_mut(slot.edit_index) else { return };
        if edit.text[slot.at..].starts_with(&slot.placeholder) {
            edit.text.replace_range(slot.at..slot.at + slot.placeholder.len(), type_name);
        }
        // The caret was measured against the placeholder; a longer or shorter spelling moves it.
        if let Some(caret) = self.caret.as_mut() {
            let written = edit.start + slot.at;
            if *caret > written {
                *caret = (*caret + type_name.len()).saturating_sub(slot.placeholder.len());
            }
        }
    }
}

/// A `throws` clause the plan wrote from the text alone, and the span whose real one the caller
/// should work out.
///
/// *Extract method* has to give the moved body a `throws`, and reading the tree can only tell it
/// what the enclosing method already declares plus what a surrounding `try` catches. That is sound
/// where those cover it and silently short where they do not — a `try` INSIDE the selection, a
/// `@SneakyThrows` method — and a short `throws` is not a cosmetic difference: it is a call site
/// that stops compiling.
///
/// So the plan writes its guess and names the range it was guessing about. A caller with a resolver
/// replaces it with the exact set; one without keeps the guess, which is what it would have had
/// anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThrowsSlot {
    /// The ORIGINAL span of the statements that moved — what to analyse.
    pub start: usize,
    pub end: usize,
    /// Where in the plan's edits the clause sits, as for [`TypeSlot`].
    pub edit_index: usize,
    pub at: usize,
    /// The clause written from the text alone, e.g. `" throws IOException"` — possibly empty.
    pub placeholder: String,
}

impl Plan {
    /// Write the real `throws` clause into the slot, and clear it.
    ///
    /// `clause` includes its leading space and the `throws` keyword, or is empty for a method that
    /// throws nothing — the same shape as the placeholder it replaces.
    pub fn fill_throws(&mut self, clause: &str) {
        let Some(slot) = self.throws_slot.take() else { return };
        let Some(edit) = self.edits.get_mut(slot.edit_index) else { return };
        if !edit.text[slot.at..].starts_with(&slot.placeholder) {
            return;
        }
        edit.text.replace_range(slot.at..slot.at + slot.placeholder.len(), clause);
    }
}

/// Why a refactoring is not on offer here.
///
/// Always a sentence about *this* code, never a category. The editor shows it on the row, greyed —
/// see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    /// The refactoring that would have applied.
    pub id: String,
    pub label: String,
    /// One sentence, in the words of the code in front of the user.
    pub reason: String,
}

impl Refusal {
    pub fn new(id: &str, label: &str, reason: impl Into<String>) -> Self {
        Self { id: id.to_string(), label: label.to_string(), reason: reason.into() }
    }
}

/// What a refactoring answered: a plan, a refusal, or silence.
///
/// Silence is the third case and it is not the same as a refusal: *extract method* has nothing to
/// say about a caret sitting in an import, and saying "cannot extract a method from an import"
/// would fill the menu with rows about everything the user is not doing. A refusal is for a
/// refactoring the user is plainly reaching for and cannot have.
pub type Outcome = Option<Result<Plan, Refusal>>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant every consumer stands on: apply the edits in order, and nothing has moved.
    #[test]
    fn edits_apply_back_to_front_without_remapping() {
        let source = "abcdefgh";
        let plan = Plan::new(
            "t",
            "t",
            vec![
                RefactorEdit::new(0, 1, "X", "a"),
                RefactorEdit::new(6, 8, "YY", "b"),
                RefactorEdit::new(3, 4, "Z", "c"),
            ],
        );
        assert!(plan.sorted());
        assert_eq!(plan.apply(source), "XbcZefYY");
    }

    /// Regression: an insertion and a replacement that begin at the same byte. Applied in the wrong
    /// order the insertion is overwritten and the buffer is silently corrupt — the shape every
    /// `extract variable` on an expression that starts its own statement produces.
    #[test]
    fn an_insertion_and_a_replacement_at_the_same_offset_do_not_overwrite_each_other() {
        let source = "this.params.add(param);";
        let plan = Plan::new(
            "extract-variable",
            "Extract variable",
            vec![
                RefactorEdit::new(0, 0, "var p = this.params;\n", "declaration"),
                RefactorEdit::new(0, "this.params".len(), "p", "use"),
            ],
        );
        assert_eq!(plan.apply(source), "var p = this.params;\np.add(param);");
    }

    #[test]
    fn a_filled_type_replaces_its_placeholder_and_moves_the_caret_with_it() {
        let mut plan = Plan::new("t", "t", vec![RefactorEdit::new(10, 10, "var name = x;", "declaration")])
            .caret_at(10 + "var ".len())
            .needing_type(TypeSlot {
                start: 0,
                end: 1,
                edit_index: 0,
                at: 0,
                placeholder: "var".to_string(),
                            required: false,
            });
        plan.fill_type("List<String>");
        assert_eq!(plan.edits[0].text, "List<String> name = x;");
        assert_eq!(plan.caret, Some(10 + "List<String> ".len()));
        assert!(plan.type_slot.is_none());
    }

    /// A plan whose type nobody filled still applies, and still compiles from Java 10 on.
    #[test]
    fn an_unfilled_slot_leaves_the_placeholder() {
        let plan = Plan::new("t", "t", vec![RefactorEdit::new(0, 0, "var name = x;", "declaration")]);
        assert_eq!(plan.apply(""), "var name = x;");
    }
}
