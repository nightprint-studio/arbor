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
    /// The file this edit lands in, **empty for the one the refactoring was invoked in**.
    ///
    /// Empty by default and by far in the majority: every refactoring in this crate that acts on
    /// one buffer produces edits with nothing here, and a consumer that never learned about the
    /// field applies them exactly as it always did. It is filled in by the caller that resolves a
    /// [`MemberTransfer`] — the only way a plan reaches a second file — so the descending order the
    /// plan promises stays a promise **per file**, which is all a consumer applying them one file
    /// at a time needs.
    #[serde(default)]
    pub file: String,
}

impl RefactorEdit {
    pub fn new(start: usize, end: usize, text: impl Into<String>, reason: &str) -> Self {
        Self { start, end, text: text.into(), reason: reason.to_string(), file: String::new() }
    }

    /// The same edit, against another file.
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = file.into();
        self
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
    /// A claim about a type this plan depends on but cannot check; see [`TypeGuard`].
    #[serde(default)]
    pub type_guard: Option<TypeGuard>,
    /// A member this plan lifts out for a type it could not find; see [`MemberTransfer`].
    #[serde(default)]
    pub transfer: Option<MemberTransfer>,
    /// A source file this plan needs written; see [`NewSource`].
    #[serde(default)]
    pub new_source: Option<NewSource>,
    /// A Java language level the code this plan writes needs; see [`NeedsLevel`].
    #[serde(default)]
    pub needs_level: Option<NeedsLevel>,
    /// The expression a `switch` will select on, and what its type has to be; see
    /// [`SelectorGuard`].
    #[serde(default)]
    pub selector_guard: Option<SelectorGuard>,
}

/// The expression a produced `switch` selects on — and the fact that only a resolver can settle.
///
/// ## Why this one REFUSES on an unknown answer, where [`TypeGuard`] does not
///
/// A `switch` selector may be `char`, `byte`, `short`, `int`, their boxes, a `String`, or an enum —
/// and **nothing else**. Not `long`, which is the one that catches you: `if (x == 1)` reads exactly
/// the same whether `x` is an `int` or a `long`, and the `switch` written from it compiles in one
/// case and not the other. Nor can the shape of a `case` label be decided from the text: a bare
/// `POINT` is right when the labels are an enum's constants and wrong for a `static final int`,
/// where the qualifier has to stay.
///
/// So this crate reads the chain and hands the caller a question. `TypeGuard` leans the other way —
/// unknown leaves the plan standing — because there the cost of refusing on silence was twenty-two
/// good extractions per breakage prevented. Here it is the reverse: the refactoring fired thirteen
/// times on h2 and produced ten files that do not compile, every one of them a type nobody asked
/// about. On a question this narrow, "I could not tell" is a reason not to write the code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectorGuard {
    /// The subject, as a span to infer.
    pub start: usize,
    pub end: usize,
    /// The subject as the source writes it, for the sentence when it is refused.
    pub written: String,
    /// Whether the arms are labelled with **bare constant names** — legal only when the selector is
    /// the enum that declares them, and wrong for anything else.
    pub enum_labels: bool,
}

/// The types a `switch` may select on, as a resolver would name them. An enum is the case this
/// list cannot hold, and [`SelectorGuard::enum_labels`] is how it is asked about instead.
pub const SWITCHABLE: &[&str] = &[
    "char",
    "byte",
    "short",
    "int",
    "java.lang.Character",
    "java.lang.Byte",
    "java.lang.Short",
    "java.lang.Integer",
    "java.lang.String",
    "Character",
    "Byte",
    "Short",
    "Integer",
    "String",
];

/// The Java version the code this plan writes will not compile below.
///
/// The same shape as every other slot here: this crate knows **what it wrote**, the caller knows
/// **what the project is**, and neither can answer alone. Two of the refactorings write code with a
/// floor, and both floors are easy to walk past on the codebases this editor exists for:
///
/// - a method pulled into an **interface** keeps its body as a `default` one, which is **Java 8**;
/// - a `switch` over a **`String`** is **Java 7**.
///
/// A caller that does not know the project's level leaves the plan alone, which is what a caller
/// without one would have done anyway — the same rule [`TypeGuard`] follows, and for the same
/// reason: only a positive disagreement is evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsLevel {
    /// The lowest Java release that accepts it: `7`, `8`.
    pub at_least: u16,
    /// What needs it, in the words of the code — the whole of what the user is told when the
    /// project is older than that.
    pub because: String,
}

/// Read a Java language level as a pom writes it: `"1.8"` and `"8"` are both 8, `"17"` is 17.
///
/// `None` for anything that is not a version — `"toolchains"`, an empty string, a property nobody
/// substituted. Unknown is not "old": refusing a refactoring because a level could not be parsed
/// would refuse it on every project that resolves its JDK through a toolchain.
pub fn language_level(declared: &str) -> Option<u16> {
    let text = declared.trim();
    let text = text.strip_prefix("1.").unwrap_or(text);
    text.split(['.', '-']).next()?.parse().ok()
}

/// A member this plan removed, and the type it has to land in — which is in another file.
///
/// The half of a move this crate cannot do. *Pull up* usually targets a superclass that lives
/// somewhere else, and finding it means a project index, which is exactly what this crate does not
/// have. So the plan carries the removal, which is complete and correct on its own, plus everything
/// the caller needs to write the other half without parsing this file again.
///
/// The caller's contract: find `target`, check it declares each of `requires`, insert `member` at
/// the end of its body re-indented for it, and add whichever of `imports` it does not already have.
/// A target it cannot find is a **refusal**, not a plan applied halfway — a member removed from one
/// file and written into none is the worst outcome available here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberTransfer {
    /// The type the member must land in, as the source names it.
    pub target: String,
    /// Where that name is **written in this file** — the `extends` clause it was read off.
    ///
    /// Not a convenience: it is how a caller finds the type without a name lookup of its own.
    /// Go-to-declaration on that very offset already answers "which file declares this", through
    /// the same imports and the same package this file has, so a `Base` that means two different
    /// classes in two packages resolves the way the compiler resolves it rather than the way a
    /// simple-name map guesses it.
    pub target_at: usize,
    /// The member's text, dedented to column zero — the caller re-indents it for the body it goes
    /// into, which is the only place the right indentation is known.
    pub member: String,
    /// Whether `private` must become `protected` on the way — see `adapt_modifiers`. A private
    /// member is invisible to the class it came from once it is one level up, so where that class
    /// reads it the move only works widened.
    #[serde(default)]
    pub widen_private: bool,
    /// Names the member reads from the type it is leaving, and which the target must already
    /// declare. A name missing there is a member that will not compile once it lands.
    pub requires: Vec<String>,
    /// The whole `import` lines of the source file whose types the member mentions.
    pub imports: Vec<String>,
    /// The package the member is **leaving**.
    pub package: String,
    /// Whether anything still calls the member where it is. Harmless for a pull up into a class —
    /// inheritance carries it — and fatal into an **interface**, whose `static` methods are not
    /// inherited (JLS §8.4.8). Only the caller knows which kind the target is.
    #[serde(default)]
    pub still_called: bool,
    /// Whether the member is a `static` method, for that same rule.
    #[serde(default)]
    pub static_method: bool,
    /// Bare names the member reads that it does not declare itself — a field of the class it is
    /// leaving, a sibling method. Across a **package boundary** every one of them is a name that
    /// may no longer be reachable, whatever it resolved to before.
    pub bare_names: Vec<String>,
    /// Type names the member uses that **no import covers** — the ones it was reaching through its
    /// own package. In the same package they resolve; one package over they resolve to nothing, and
    /// there is no import to carry because there never was one. Sixteen of h2's thirty broken pull
    /// ups were this: `cannot find symbol: class FileMemData`.
    pub unimported_types: Vec<String>,
}

/// A source file this plan needs written, named but not placed.
///
/// This crate knows the type's name and its whole text and nothing about where a package sits on
/// disk. The caller puts it **beside the file the refactoring was invoked in**, which is what "the
/// same package" means on a filesystem — and refuses rather than overwrites when something of that
/// name is already there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSource {
    /// The type's simple name. Java requires a public top-level type and its file to share it, so
    /// the file is `<name>.java`.
    pub name: String,
    /// The whole file, `package` line included.
    pub text: String,
    /// Where that name is **written in this file**, so a caller with a reference index can ask who
    /// else uses it — see `was_nested`.
    pub name_at: usize,
    /// Whether the type was **nested** in another.
    ///
    /// It decides whether a mention from another file survives the move, and the two answers could
    /// not be further apart. A second top-level type moving to its own file stays in the same
    /// package under the same name, so every reference to it anywhere keeps working. A **nested**
    /// one does not: another file can only have reached it as `Outer.Inner` or through an
    /// `import a.b.Outer.Inner;`, and both name a member of `Outer` that is about to stop existing.
    ///
    /// This crate can rule out the mentions in the file it is given and no others. A caller with the
    /// reference index must refuse the plan when any **other** file mentions the type at all —
    /// measured: `import org.apache.commons.lang3.ClassUtils.Interfaces;` in a file two packages
    /// away, which nothing in `ClassUtils.java` could have shown.
    pub was_nested: bool,
}

/// A fact the plan is only correct under, and can only assert from the text.
///
/// *Declaration to `var`* is the whole of it so far, and the fact is: **what `var` infers here is
/// what the source wrote.** The refactoring reads the tree and can rule out the shapes where that is
/// visibly false — a diamond, a literal in a widening declaration, a lambda — but it cannot see the
/// type of `c1` in `int cp1 = c1;`, and where `c1` is a `char`, `var` narrows the declaration and
/// the next `cp1 = someInt` stops compiling.
///
/// So the plan says what it is assuming and names the span to check it against. A caller with a
/// resolver infers that span: a type that **differs** from `written` refuses the plan, and anything
/// else — nothing inferred, a type it cannot write down — leaves it standing, which is what a caller
/// without a resolver does anyway. Only a positive disagreement is evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeGuard {
    /// The expression whose inferred type must match.
    pub start: usize,
    pub end: usize,
    /// The type the source writes, as it writes it.
    pub written: String,
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
            type_guard: None,
            throws_slot: None,
            transfer: None,
            new_source: None,
            needs_level: None,
            selector_guard: None,
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

    /// Attach the fact this plan is only correct under; see [`TypeGuard`].
    pub fn guarded_by(mut self, guard: TypeGuard) -> Self {
        self.type_guard = Some(guard);
        self
    }

    /// Attach the half of a move that lands in another file; see [`MemberTransfer`].
    pub fn transferring(mut self, transfer: MemberTransfer) -> Self {
        self.transfer = Some(transfer);
        self
    }

    /// Attach the file this plan needs written; see [`NewSource`].
    pub fn creating(mut self, source: NewSource) -> Self {
        self.new_source = Some(source);
        self
    }

    /// Attach the question about the `switch`'s subject; see [`SelectorGuard`].
    pub fn selecting_on(mut self, guard: SelectorGuard) -> Self {
        self.selector_guard = Some(guard);
        self
    }

    /// Attach the Java version the code this plan writes needs; see [`NeedsLevel`].
    pub fn needing_level(mut self, at_least: u16, because: impl Into<String>) -> Self {
        self.needs_level = Some(NeedsLevel { at_least, because: because.into() });
        self
    }

    /// Apply the plan to a source string. The reference implementation, and what the tests here
    /// check against — the editor applies the same edits through its own buffer.
    /// Only the edits for the file the refactoring was invoked in — an edit carrying a
    /// [`RefactorEdit::file`] belongs to another buffer and its offsets mean nothing here.
    pub fn apply(&self, source: &str) -> String {
        let mut out = source.to_string();
        for edit in self.edits.iter().filter(|e| e.file.is_empty()) {
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
        self.edits
            .windows(2)
            .all(|w| w[0].file != w[1].file || w[0].start >= w[1].end || w[0].start == w[1].start)
    }
}

/// Descending by start, and by end within the same start — see [`Plan::new`] for why the second
/// half is load-bearing rather than a tidy-up.
fn reorder(edits: &mut [RefactorEdit]) {
    // By file first, so each file's run is contiguous and descending within itself: a consumer
    // applies one file's edits back to front, which is the only order under which nothing it has
    // yet to apply has moved. Across files the order says nothing and needs to say nothing.
    edits.sort_by(|a, b| {
        a.file.cmp(&b.file).then(b.start.cmp(&a.start)).then(b.end.cmp(&a.end))
    });
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
    /// What the caller must do when it cannot name the type.
    #[serde(default)]
    pub need: TypeNeed,
}

/// How much the plan needs the type actually written.
///
/// Three answers, because "could not name the type" covers two situations the caller has to tell
/// apart. Either **nothing was inferred** — and then `var` is exactly what javac would put there
/// anyway — or **something was inferred that must not be written**: `void`, a type variable the
/// target class never declared, a captured wildcard flattened to `Object`. Only the second is
/// evidence of anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeNeed {
    /// `var` will do. It compiles from Java 10 on and is what javac would infer.
    #[default]
    Optional,
    /// `var` will not do, whatever the reason. A field is never `var` in any Java version, and
    /// naming a whole statement gives `var setName = obj.setName(x);` where the call may be `void`
    /// and there is nothing to infer from. Not naming the type means not applying the plan.
    Required,
    /// `var` will do **only while nothing was inferred**.
    ///
    /// The target-typed positions — an argument, a `return`, the right of an assignment, an arm of a
    /// conditional — where the expression's type comes from what is expected of it.
    /// `stream.collect(Collectors.toList())` gives the inner call its type arguments; standing alone
    /// as `var c = Collectors.toList();` it re-infers to a collector over `Object` and the line it
    /// fed stops compiling. That re-inference is precisely what an unwritable answer looks like, so
    /// it is the signal to decline. An engine that inferred *nothing* has not seen that signal, and
    /// there `var` compiled 96% of the time — refusing on it too cost 22 good extractions for every
    /// broken one it prevented, measured on commons-lang3.
    RequiredOnceInferred,
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

/// The `throws` clause to write: the resolver's answer when it is complete, the plan's own guess
/// when it is not.
///
/// **Replace or keep — never add.** The guess is the enclosing method's clause plus what the `try`s
/// around the selection catch, and it is a sound UPPER bound for a reason that is easy to miss: the
/// code compiled before the refactoring, so nothing the body raises could have escaped past those.
/// Adding to it therefore cannot be right, and is actively wrong — it hands the caller an exception
/// that can never reach it, and the call site stops compiling. Measured: `throws E, Exception` on a
/// method extracted from one that declares only `throws E`.
///
/// Adding was tried again, deliberately, because the bound has a hole: a selection inside a
/// **lambda** takes its budget for checked exceptions from the functional interface it implements
/// (`void accept(byte) throws E`), which the guess — text, no resolver — cannot see. Twenty-five
/// clauses on commons-lang are missing for that reason. Adding the proven names back cost
/// **seventy** other extractions, all the same way: `E` is usually a type parameter of the METHOD
/// the body came from, so a new method declaring `throws E` names something that does not exist at
/// its own signature. 124 broken became 176. The hole is real and the fix for it is the type
/// parameter, not the clause.
///
/// Narrowing is the useful direction — dropping what the body does not actually raise — and it is
/// safe only against a complete answer. An incomplete one is missing exceptions, and a clause that
/// lost one is an extracted method that does not compile. Hence the flag rather than a merge.
///
/// Matching is by SIMPLE name because the two halves are spelled differently: the guess carries the
/// source's own words, the analysis JVM binary names.
///
/// Here rather than in the backend because two callers need it — the backend, and the measurement
/// harness, which has to do exactly what the backend does or it is measuring something else.
pub fn merge_throws(guessed: &str, proven: &[String], complete: bool, source: &str) -> String {
    if !complete {
        return guessed.to_string();
    }
    if proven.is_empty() {
        return String::new();
    }
    // Complete: the answer IS the clause. Names already in the guess keep the source's spelling —
    // the file wrote them that way and the signature should read like its neighbours.
    let guessed_names: Vec<String> = guessed
        .trim()
        .trim_start_matches("throws")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let simple = |n: &str| n.rsplit(['.', '/', '$']).next().unwrap_or(n).to_string();
    let names: Vec<String> = proven
        .iter()
        .map(|binary| {
            guessed_names
                .iter()
                .find(|n| simple(n) == simple(binary))
                .cloned()
                .unwrap_or_else(|| written_name(binary, source))
        })
        .collect();
    format!(" throws {}", names.join(", "))
}

/// How a binary name should be written in this file: short when the file already imports it (or it
/// is `java.lang`), dotted otherwise — a `throws` clause is not a reason to add an import.
pub fn written_name(binary: &str, source: &str) -> String {
    let dotted = binary.replace('/', ".").replace('$', ".");
    let simple = dotted.rsplit('.').next().unwrap_or(&dotted).to_string();
    let imported = source.contains(&format!("import {dotted};"))
        || (binary.starts_with("java/lang/") && binary.matches('/').count() == 2);
    if imported {
        simple
    } else {
        dotted
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

    /// A pom writes the level four different ways, and "unknown" must not read as "old".
    #[test]
    fn a_language_level_is_read_the_way_a_pom_writes_it() {
        assert_eq!(language_level("1.8"), Some(8));
        assert_eq!(language_level("8"), Some(8));
        assert_eq!(language_level("17"), Some(17));
        assert_eq!(language_level(" 21 "), Some(21));
        // Not a version: a toolchain resolves it later, and refusing here would refuse on every
        // project that uses one.
        assert_eq!(language_level("toolchains"), None);
        assert_eq!(language_level(""), None);
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
                            need: TypeNeed::Optional,
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
