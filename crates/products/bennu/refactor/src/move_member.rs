//! **Pull up**, **push down**, **move member** — one member changes the type it belongs to.
//!
//! Three names for one transform: a member leaves the type it is written in and lands in another.
//! What differs is only *which* type, and how the answer is found:
//!
//! | | the target | how it is chosen |
//! |---|---|---|
//! | **pull up** | the supertype the class names | read off `extends` / `implements` — there is nothing to ask |
//! | **push down** | a subtype | one offer per subtype, so the menu row *is* the choice |
//! | **move member** | another type in this file | one offer per candidate, same reason |
//!
//! ## The target may not be in this file, and that is the caller's half
//!
//! A superclass usually lives somewhere else, and this crate has no project index on purpose. So a
//! plan whose target is elsewhere carries the removal edit and a [`MemberTransfer`] — the member's
//! text, the names it needs where it lands, the imports it reads — and the caller with the index
//! writes it into the other file. A plan whose target is **here** is complete on its own, and both
//! halves went through the same checks to get there.
//!
//! ## What is checked before anything moves
//!
//! - **What it leaves behind.** A method that reads `count` cannot go to a type that has no
//!   `count`. The names it takes from the type it is leaving are collected and checked against the
//!   target — here when the target is here, by the caller when it is not.
//! - **`super`.** A `super.render()` in a method pulled up means a different method afterwards, and
//!   in a class with no superclass it means nothing at all.
//! - **`private`.** A private member pulled up is invisible to the class it came from — the code
//!   that called it stops compiling, which is not a thing to discover after applying.
//! - **A name the target already has.** Two members with one name, or a silent override.
//! - **Several variables at once** (`int a, b;`), where there is no one member to move.
//!
//! ## What it does not check, and what that costs
//!
//! Whether anything **outside this file** still needs the member where it was. Pushing a method
//! down into one subtype breaks every other caller of it, and seeing them needs the reference index.
//! The caller has one; this is the place to use it if the refactoring is ever measured short.

use tree_sitter::Node;

use crate::body::{
    body_of, has_modifier, imports_of, line_after, line_start, member_indent,
    member_name, members_of, reindent, simple_name, subtypes_of, supertypes_of, type_named,
    types_in, MEMBERS,
};
use crate::plan::{MemberTransfer, Outcome, Plan, RefactorEdit, Refusal};
use crate::selection::{descendants, enclosing, enclosing_type, indent_at, newline, text, TYPE_DECLS};

const PULL_UP: (&str, &str) = ("pull-up-member", "Pull member up");
const PUSH_DOWN: (&str, &str) = ("push-down-member", "Push member down");
const MOVE: (&str, &str) = ("move-member", "Move member to");

/// Every member move on offer at the caret.
///
/// One call rather than three, because they are one transform asked three ways and share every
/// check — and because the *targets* are what makes them different, so a caller that wanted a list
/// per refactoring would get the same work done three times over one member.
pub fn member_moves(
    root: Node<'_>,
    source: &str,
    start: usize,
    end: usize,
) -> Vec<Result<Plan, Refusal>> {
    let Some(member) = member_at(root, source, start, end) else { return Vec::new() };
    let Some(owner) = enclosing_type(member) else { return Vec::new() };
    let mut out = Vec::new();

    // Pull up: the target is the first supertype the class names. `implements` counts — a `default`
    // method pulled into the interface is the commonest pull-up there is.
    if let Some((supertype, at)) = supertype_written(&owner, source) {
        out.push(plan_move(root, source, &member, &owner, &supertype, at, PULL_UP, "up"));
    }
    // Push down: one row per subtype declared here. A subtype in ANOTHER file cannot be seen from
    // this crate, and the caller with the index adds those rows itself.
    let owner_name = owner.child_by_field_name("name").map(|n| text(&n, source)).unwrap_or_default();
    for subtype in subtypes_of(root, source, owner_name) {
        let Some(name) = subtype.child_by_field_name("name").map(|n| text(&n, source)) else {
            continue;
        };
        let id = format!("{}:{name}", PUSH_DOWN.0);
        let label = format!("Push member down to `{name}`");
        out.push(plan_move(root, source, &member, &owner, name, subtype.start_byte(), (&id, &label), "down"));
    }
    // Move: any other type in this file. A target elsewhere needs a picker the editor does not have
    // yet — the caller can plan one with [`move_member_to`] once it does.
    for candidate in types_in(root) {
        let Some(name) = candidate.child_by_field_name("name").map(|n| text(&n, source)) else {
            continue;
        };
        if candidate.id() == owner.id() || name == owner_name {
            continue;
        }
        if supertypes_of(&owner, source).iter().any(|s| simple_name(s) == name)
            || supertypes_of(&candidate, source).iter().any(|s| simple_name(s) == owner_name)
        {
            continue; // already offered as a pull up / a push down
        }
        let id = format!("{}:{name}", MOVE.0);
        let label = format!("Move member to `{name}`");
        out.push(plan_move(root, source, &member, &owner, name, candidate.start_byte(), (&id, &label), "across"));
    }
    out.into_iter().flatten().collect()
}

/// Plan the move of the member at the caret into `target`, which the **caller** located.
///
/// The entry point for a target this crate cannot see: a superclass in another file, a subtype the
/// index knows about, a class the user picked. The checks are the same ones [`member_moves`] runs;
/// what the caller supplies is only the name.
pub fn move_member_to(
    root: Node<'_>,
    source: &str,
    start: usize,
    end: usize,
    target: &str,
    id: &str,
    label: &str,
) -> Outcome {
    let member = member_at(root, source, start, end)?;
    let owner = enclosing_type(member)?;
    plan_move(root, source, &member, &owner, target, member.start_byte(), (id, label), "across")
}

/// Whether the type's first supertype is written with type arguments.
fn supertype_is_generic(owner: &Node<'_>) -> bool {
    let mut cursor = owner.walk();
    let generic = owner
        .named_children(&mut cursor)
        .filter(|c| {
            matches!(c.kind(), "superclass" | "super_interfaces" | "extends_interfaces" | "interfaces")
        })
        .any(|c| !crate::selection::descendants_any(c, &["generic_type"]).is_empty());
    generic
}

/// The first supertype a type names, and where that name is written.
fn supertype_written(owner: &Node<'_>, source: &str) -> Option<(String, usize)> {
    let first = supertypes_of(owner, source).into_iter().next()?;
    // The written span of that same name, found by looking for it in the type's own header — the
    // supertypes come back as text, and the offset has to be the one a resolver can be asked about.
    let header_end =
        owner.child_by_field_name("body").map(|b| b.start_byte()).unwrap_or(owner.end_byte());
    let header = source.get(owner.start_byte()..header_end)?;
    let at = header.find(&first).map(|i| owner.start_byte() + i)?;
    Some((first, at))
}

/// Rewrite a member's own modifiers for the kind of type it is landing in.
///
/// Not cosmetics. A method with a body is legal in an interface **only** as `default` or `static`,
/// and never as `protected`, `final` or `synchronized`; the same method landing back in a class
/// must lose the `default` it was carrying, and take the `public` an interface gave it implicitly,
/// or the move narrows its visibility in silence. Both wrong shapes are compile errors, so this is
/// the difference between a move that works and one that does not.
///
/// Annotations and everything else are left exactly as written: only keyword tokens are added and
/// removed, in place, so a `@Deprecated` on its own line stays on its own line.
///
/// `None` when the member's text does not parse on its own, which is a refusal to touch it rather
/// than a guess at what it meant.
pub fn adapt_modifiers(member: &str, into_interface: bool) -> Option<String> {
    let wrapped = format!("class __Wrap {{\n{member}\n}}");
    let tree = bennu_java::prelude::parse_java(&wrapped)?;
    let offset = "class __Wrap {\n".len();
    let type_decl = types_in(tree.root_node()).into_iter().next()?;
    let body = body_of(&type_decl)?;
    let node = members_of(&body).into_iter().next()?;

    let is_static = has_modifier(&node, &wrapped, "static");
    let has_body = node.child_by_field_name("body").is_some();
    let is_field = node.kind() == "field_declaration";

    // What must go, and what must arrive. An interface's members are implicitly `public`, and its
    // fields implicitly `static final`, so those words are dropped rather than kept — they are not
    // wrong there, but `protected` and `final` on a `default` method are.
    let (drop, add): (&[&str], Option<&str>) = match (into_interface, is_field, is_static, has_body) {
        (true, true, _, _) => (&["public", "protected", "private", "static", "final"], None),
        (true, false, true, _) => (&["public", "final", "synchronized", "native"], None),
        (true, false, false, true) => (
            &["public", "protected", "private", "final", "synchronized", "native", "abstract", "default"],
            Some("default"),
        ),
        (true, false, false, false) => (&["public", "abstract"], None),
        // Landing in a class: `default` is not a modifier there, and dropping it without saying
        // `public` would quietly make a method that everyone could call package-private.
        (false, _, _, _) => (&["default"], None),
    };
    let was_default = has_modifier(&node, &wrapped, "default");
    let add = match (into_interface, was_default) {
        (false, true) => Some("public"),
        _ => add,
    };

    let mut cursor = node.walk();
    let modifiers = node.children(&mut cursor).find(|c| c.kind() == "modifiers");
    let Some(modifiers) = modifiers else {
        // No modifiers at all: whatever has to arrive goes in front of the declaration.
        return Some(match add {
            Some(word) => format!("{word} {member}"),
            None => member.to_string(),
        });
    };

    // Cut the dropped keywords out **in place**, back to front so nothing before them moves.
    let mut out = member.to_string();
    let mut inner = modifiers.walk();
    let mut cuts: Vec<(usize, usize)> = modifiers
        .children(&mut inner)
        .filter(|c| drop.contains(&c.kind()))
        .map(|c| (c.start_byte() - offset, c.end_byte() - offset))
        .collect();
    cuts.sort_by(|a, b| b.0.cmp(&a.0));
    let mut removed = 0usize;
    for (start, end) in cuts {
        // Take the space after the word too, so removing one does not leave a double space.
        let end = if out[end..].starts_with(' ') { end + 1 } else { end };
        removed += end - start;
        out.replace_range(start..end, "");
    }
    if let Some(word) = add {
        // Where the modifiers END — in front of the type, after any annotation — in the text as it
        // is NOW. Measuring it against the original is how the keyword landed after the body once.
        let at = (modifiers.end_byte() - offset).saturating_sub(removed).min(out.len());
        // …then forward over the whitespace, so the word lands at the start of the return type
        // rather than at the end of the line an annotation is on: `@Deprecated\ndefault void f()`,
        // never `@Deprecated default\nvoid f()`.
        let at = at + out[at..].len() - out[at..].trim_start().len();
        out.insert_str(at, &format!("{word} "));
    }
    // A modifiers node emptied of every keyword can leave the line starting with a space.
    Some(out.trim_start_matches(' ').to_string())
}

/// Write a [`MemberTransfer`] into the file that holds its target.
///
/// The **other half** of a cross-file move, and deliberately here rather than in whichever caller
/// needed it first: the backend and the measurement harness both do this, and a harness that does
/// it its own way measures a product that does not exist. Pure — the target's source in, edits out
/// — so it stays in this crate and stays testable.
///
/// `Err` is a refusal, and it matters that it is: the plan's other edit **removes** the member, so
/// a caller that applied the removal and swallowed a failure here would delete code and write it
/// nowhere.
pub fn transfer_into(
    plan: &Plan,
    target_source: &str,
    target_file: &str,
) -> Result<Vec<RefactorEdit>, String> {
    let transfer = plan.transfer.as_ref().ok_or("this plan moves nothing between files")?;
    let name = plan.name.clone().unwrap_or_default();
    let tree = bennu_java::prelude::parse_java(target_source)
        .ok_or_else(|| format!("`{}` does not parse", transfer.target))?;
    let root = tree.root_node();
    let target_decl = type_named(root, target_source, &transfer.target)
        .ok_or_else(|| format!("`{}` does not declare `{}`", target_file, transfer.target))?;
    let body = body_of(&target_decl)
        .ok_or_else(|| format!("`{}` has no body to put a member in", transfer.target))?;
    let existing = members_of(&body);
    if existing.iter().any(|m| member_name(m, target_source) == Some(name.as_str())) {
        return Err(format!("`{}` already declares `{name}`", transfer.target));
    }
    if let Some(missing) = transfer.requires.iter().find(|needed| {
        !existing.iter().any(|m| member_name(m, target_source) == Some(needed.as_str()))
    }) {
        return Err(format!("`{name}` reads `{missing}`, which stays behind"));
    }

    let (at, terminator) = crate::body::append_into(&body, target_source)
        .ok_or_else(|| format!("`{}` has no place to put a member", transfer.target))?;
    let indent = member_indent(target_source, &body);
    let nl = newline(target_source);
    // The member's own modifiers, rewritten for what it is landing in — which is knowable only
    // here, where the target's source is. See [`adapt_modifiers`].
    let into_interface =
        matches!(target_decl.kind(), "interface_declaration" | "annotation_type_declaration");
    let adapted = adapt_modifiers(&transfer.member, into_interface)
        .ok_or("the member's text does not stand on its own")?;
    let mut edits = vec![RefactorEdit::new(
        at,
        at,
        format!("{terminator}{nl}{}{nl}", reindent(&adapted, "", &indent)),
        "member",
    )
    .in_file(target_file)];

    // The imports the member reads, minus the ones already there. A simple name the target already
    // imports from somewhere ELSE is a refusal and not a second import: two imports of one simple
    // name do not compile, and picking which one wins is not a decision a refactoring may make.
    let theirs = imports_of(root, target_source);
    let import_at = import_point(root, target_source);
    let mut added = String::new();
    for line in &transfer.imports {
        let what = line.trim_start_matches("import").trim().trim_start_matches("static").trim().trim_end_matches(';').trim();
        let simple = what.rsplit('.').next().unwrap_or(what);
        if theirs.iter().any(|(t, _)| t == what) {
            continue;
        }
        if simple != "*" && theirs.iter().any(|(t, _)| t.rsplit('.').next() == Some(simple)) {
            return Err(format!(
                "`{}` already imports a different `{simple}`, and the member needs `{what}`",
                transfer.target
            ));
        }
        added.push_str(line);
        added.push_str(nl);
    }
    if !added.is_empty() {
        edits.push(RefactorEdit::new(import_at, import_at, added, "import").in_file(target_file));
    }
    Ok(edits)
}

/// Where an `import` goes in a file: after the last one, else after the `package` line, else at the
/// very top.
fn import_point(root: Node<'_>, source: &str) -> usize {
    let mut cursor = root.walk();
    let mut after = None;
    for child in root.named_children(&mut cursor) {
        if matches!(child.kind(), "package_declaration" | "import_declaration") {
            after = Some(child.end_byte());
        }
    }
    match after {
        Some(end) => line_after(source, end),
        None => 0,
    }
}

/// The one transform, once.
#[allow(clippy::too_many_arguments)]
fn plan_move(
    root: Node<'_>,
    source: &str,
    member: &Node<'_>,
    owner: &Node<'_>,
    target: &str,
    target_at: usize,
    (id, label): (&str, &str),
    direction: &str,
) -> Outcome {
    let name = member_name(member, source).unwrap_or("this member");
    if let Some(reason) = unfit(root, member, source, direction) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    // The type's OWN name, written in the member: a `new Thing(…)`, a factory returning `Thing`.
    // Wherever it lands, that name means the same class — so the member either builds something it
    // is no longer part of (and its constructor may be private to it), or promises to return a
    // subtype of whatever now declares it.
    if let Some(owner_name) = owner.child_by_field_name("name").map(|n| text(&n, source)) {
        let mentions_itself = crate::selection::descendants_any(
            *member,
            &["type_identifier", "identifier"],
        )
        .iter()
        .any(|n| text(n, source) == owner_name);
        if mentions_itself {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("`{name}` names `{owner_name}` itself, which means the same class wherever it lands"),
            )));
        }
    }
    // A supertype written WITH type arguments — `class LongRange extends Range<Long>` — substitutes
    // them for its own parameters. A member moved up sees `N` where it was written for `Long`, and
    // seeing that needs the substitution, which is a resolver's job and not this crate's.
    if direction == "up" && supertype_is_generic(owner) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!(
                "`{target}` is generic and this class fixes its type arguments, so a member moved \
                 up would see the type variable where it was written for the concrete type"
            ),
        )));
    }
    // A type parameter of the type it is leaving. `class Box<T> { void put(T t) }` moved anywhere
    // that never declared `T` does not compile — and unlike a missing member, no target can supply
    // it, because a type argument is not something a move may invent.
    let borrowed = crate::body::type_parameters(owner, source);
    if let Some(taken) = crate::body::mentions_type(member, source, &borrowed) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("`{name}` is written in terms of `{taken}`, a type parameter of the class it is in"),
        )));
    }
    // Something in this file OVERRIDES it. Moved anywhere but up, the member stops being the one
    // those subclasses override — their `@Override` has nothing left to point at, and the calls
    // that went through it stop being dispatched. `Strategy.isNumber()` with nine subclasses
    // implementing it is the shape, and it produced eight identical failures before this existed.
    if direction != "up" {
        let owner_name =
            owner.child_by_field_name("name").map(|n| text(&n, source)).unwrap_or_default();
        let overridden = subtypes_of(root, source, owner_name).into_iter().any(|sub| {
            body_of(&sub).is_some_and(|b| {
                members_of(&b).iter().any(|m| member_name(m, source) == Some(name))
            })
        });
        if overridden {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("a subclass of `{owner_name}` overrides `{name}`, and moving it leaves that override with nothing to override"),
            )));
        }
    }
    // What the member reads from the type it is leaving. `Vec` and not a bool: the sentence names
    // the member that keeps it here, which is the thing the user has to deal with.
    let requires = left_behind(member, owner, source);
    // Sideways, a name the member reads is **not** made safe by the target happening to declare
    // one too — that is a collision, not a match. `Functions.accept(consumer, o1, o2)` moved into
    // `FailableBiConsumer` rebinds to that interface's own one-argument `accept`, and javac says
    // "actual and formal argument lists differ in length". Up and down keep the same hierarchy, so
    // there the name really is the same member; across, it is a different one.
    if direction == "across" {
        if let Some(needed) = requires.first() {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("`{name}` reads `{needed}` from the class it is in, and in an unrelated class that name would mean something else"),
            )));
        }
    }

    // The text that moves, javadoc included — a doc comment that stayed behind would document a
    // member that is no longer there.
    let (from_start, from_end) = span_of(member, source);
    let own_indent = indent_at(source, member.start_byte());
    let moved = source.get(from_start..from_end)?.trim_end_matches(['\n', '\r']).to_string();
    let removal = RefactorEdit::new(from_start, from_end, String::new(), "removal");

    // A target in this file is planned whole; one elsewhere is described and handed over.
    let Some(target_decl) = type_named(root, source, target) else {
        let plan = Plan::new(id, label, vec![removal]).named(name);
        return Some(Ok(plan.transferring(MemberTransfer {
            target: target.to_string(),
            target_at,
            member: reindent(&moved, &own_indent, ""),
            requires,
            imports: imports_the_member_reads(root, member, source),
        })));
    };
    if let Some(reason) = target_refuses(&target_decl, source, member, name, &requires, target) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    let target_body = body_of(&target_decl)?;
    let (at, terminator) = crate::body::append_into(&target_body, source)?;
    let indent = member_indent(source, &target_body);
    let nl = newline(source);
    let into_interface =
        matches!(target_decl.kind(), "interface_declaration" | "annotation_type_declaration");
    let adapted = adapt_modifiers(&reindent(&moved, &own_indent, ""), into_interface)?;
    let insertion = format!("{terminator}{nl}{}{nl}", reindent(&adapted, "", &indent));

    let plan = Plan::new(
        id,
        label,
        vec![removal, RefactorEdit::new(at, at, insertion, "member")],
    )
    .named(name);
    Some(Ok(plan))
}

/// Why this member cannot move at all — whatever the target is.
fn unfit(root: Node<'_>, member: &Node<'_>, source: &str, direction: &str) -> Option<String> {
    let name = member_name(member, source).unwrap_or("this member");
    match member.kind() {
        "constructor_declaration" | "compact_constructor_declaration" => {
            return Some("a constructor belongs to the type that declares it".to_string())
        }
        "static_initializer" | "block" => {
            return Some("an initialiser block is not a member that can be named and moved".to_string())
        }
        "field_declaration" => {
            if descendants(*member, "variable_declarator").len() > 1 {
                return Some(
                    "this declares several fields at once, and a move has to be about one of them"
                        .to_string(),
                );
            }
        }
        _ => {}
    }
    // `super.x()` means the type above whatever declares the method, so it means something else
    // afterwards — and in a type with nothing above it, nothing at all.
    if !descendants(*member, "super").is_empty() {
        return Some(format!("`{name}` calls `super`, which means a different method once it moves"));
    }
    // `@Override` is a promise about the type the member is declared in. Moved, it is a promise
    // about a different one — and javac checks it: `method does not override or implement a method
    // from a supertype`, which is what most of these produced before the rule existed.
    if crate::selection::descendants_any(*member, &["marker_annotation", "annotation"])
        .iter()
        .any(|a| text(a, source).trim_start_matches('@').trim_start() == "Override")
    {
        return Some(format!(
            "`{name}` is annotated `@Override`, which is a promise about the type it is declared in"
        ));
    }
    // A member with no body is a **contract**, not code: moving one changes what both types
    // promise, and moving one into a `@FunctionalInterface` gives it a second abstract method and
    // stops it being one at all.
    if member.kind() == "method_declaration" && member.child_by_field_name("body").is_none() {
        return Some(format!(
            "`{name}` has no body — it is a contract of the type that declares it rather than code \
             that can live anywhere"
        ));
    }
    // A private member pulled up is invisible from the class it came from: every call to it stops
    // compiling. Pushed down it is invisible to everything ABOVE, which is the same fact seen from
    // the other side and cannot be checked from this file alone.
    if direction == "up" && has_modifier(member, source, "private") {
        return Some(format!(
            "`{name}` is `private`, so pulled up it would be invisible to the class it came from"
        ));
    }
    // Moving an INSTANCE member to an unrelated class is a different refactoring: `this` would mean
    // an object of that class, and deciding which of the method's parameters should become the new
    // receiver is a question only a person can answer. IntelliJ splits the two for the same reason.
    // Up and down keep the same object, so this is only about moving across.
    if direction == "across" && !has_modifier(member, source, "static") {
        return Some(format!(
            "`{name}` is an instance member, and in an unrelated class `this` would be a different \
             object — only a `static` member moves sideways on its own"
        ));
    }
    // A member the type's own code still uses breaks the moment it leaves — unless it goes UP,
    // where inheritance keeps it reachable under the same name. What is outside this file is the
    // caller's to check; see the module docs.
    if direction != "up" {
        if let Some(user) = still_used_by(root, member, source, name) {
            return Some(format!(
                "`{name}` is still used by `{user}`, which would not see it once it moves"
            ));
        }
    }
    None
}

/// What still needs this member where it is — anything in this file that names it.
///
/// The **whole file**, and the two shapes that forced it are both places a narrower scan missed.
/// A sibling subtype inherits the member, so pushing it into one leaves the other calling a method
/// that now lives in a class it is unrelated to. And the class that *contains* the hierarchy calls
/// it through the supertype — `stateStrategy(current).isCheckIntervalFinished(this, …)` — which is
/// neither the owner nor a subtype of it.
///
/// Conservative on purpose: an unrelated class's method of the same name reads as a use, and the
/// move is refused where it need not be. Telling those apart is a resolver's job, and the cost of
/// the wrong answer is not symmetric — a refusal costs an offer, a false clean costs a build.
fn still_used_by(
    root: Node<'_>,
    member: &Node<'_>,
    source: &str,
    name: &str,
) -> Option<String> {
    let user = crate::selection::identifiers(root).into_iter().find(|identifier| {
        text(identifier, source) == name
            && !(identifier.start_byte() >= member.start_byte()
                && identifier.end_byte() <= member.end_byte())
            && !names_a_declaration(identifier)
    })?;
    enclosing_type(user)
        .and_then(|t| t.child_by_field_name("name").map(|n| text(&n, source).to_string()))
        .or_else(|| Some("this file".to_string()))
}

/// Why the target cannot take it.
fn target_refuses(
    target_decl: &Node<'_>,
    source: &str,
    member: &Node<'_>,
    name: &str,
    requires: &[String],
    target: &str,
) -> Option<String> {
    let body = body_of(target_decl)?;
    if members_of(&body).iter().any(|m| member_name(m, source) == Some(name)) {
        return Some(format!("`{target}` already declares `{name}`"));
    }
    // An interface may not hold an instance field, and a method with a body has to say `default`.
    if matches!(target_decl.kind(), "interface_declaration" | "annotation_type_declaration") {
        // A field of an interface is implicitly `public static final`, so it must be initialised
        // where it is declared (JLS §9.3) — an uninitialised one has nowhere to be assigned.
        if member.kind() == "field_declaration" {
            let initialised = descendants(*member, "variable_declarator")
                .iter()
                .all(|d| d.child_by_field_name("value").is_some());
            // Implicitly `public static final` there (JLS §9.3): it must be initialised where it is
            // declared, and it must already BE static and final, or the code that assigns it — a
            // constructor, a setter — stops compiling the moment it arrives.
            if !initialised
                || !has_modifier(member, source, "static")
                || !has_modifier(member, source, "final")
            {
                return Some(format!(
                    "`{target}` is an interface, where a field is implicitly `public static final` \
                     and initialised where it is declared"
                ));
            }
        }
    }
    if let Some(missing) = requires.iter().find(|needed| {
        !members_of(&body).iter().any(|m| member_name(m, source) == Some(needed.as_str()))
    }) {
        return Some(format!("`{name}` reads `{missing}`, which stays behind"));
    }
    None
}

/// The names this member takes from the type it is leaving.
///
/// Only the type's **own** members, and only the ones the member mentions bare or through `this` —
/// a local, a parameter and an import all travel with it and are not the question.
fn left_behind(member: &Node<'_>, owner: &Node<'_>, source: &str) -> Vec<String> {
    let Some(body) = body_of(owner) else { return Vec::new() };
    let mine = member_name(member, source);
    let all: Vec<String> = members_of(&body)
        .iter()
        .filter(|m| m.id() != member.id())
        .flat_map(|m| match m.kind() {
            "field_declaration" => descendants(*m, "variable_declarator")
                .iter()
                .filter_map(|d| d.child_by_field_name("name").map(|n| text(&n, source).to_string()))
                .collect::<Vec<_>>(),
            _ => member_name(m, source).map(str::to_string).into_iter().collect(),
        })
        .collect();
    // The member's OWN name is normally not a dependency — a recursive call travels with it. It is
    // one the moment the class declares an **overload**: `containsAny(cs, searches)` calling
    // `containsAny(this::contains, cs, searches)` reads as recursion and is not, and moved alone it
    // meets a class where only the two-argument one exists. javac then tries to make a method
    // reference into a `CharSequence` and says the interface is not functional, which names
    // everything except what happened.
    let overloaded = mine.is_some_and(|name| all.iter().any(|n| n == name));
    let siblings: Vec<String> =
        all.into_iter().filter(|n| overloaded || Some(n.as_str()) != mine).collect();
    let mut out: Vec<String> = Vec::new();
    // Names AND written types. A **nested type** of the owner is the case a scan of identifiers
    // alone misses: `Config` written bare resolves while the member is inside the class that
    // declares it, and resolves to nothing anywhere else — exactly the way a sibling nested type
    // breaks a `move class`.
    for node in crate::selection::descendants_any(*member, &["identifier", "type_identifier"]) {
        let word = text(&node, source);
        if siblings.iter().any(|s| s == word) && !out.iter().any(|o| o == word) {
            out.push(word.to_string());
        }
    }
    out
}

/// Whether this identifier IS a declaration's name rather than a use of one.
///
/// A subtype that overrides the member mentions its name once, in its own signature — and calling
/// that "still uses it" points at the wrong fact: what breaks there is the override, which has its
/// own refusal and its own sentence.
fn names_a_declaration(identifier: &Node<'_>) -> bool {
    identifier
        .parent()
        .and_then(|p| p.child_by_field_name("name").map(|n| (p, n)))
        .is_some_and(|(parent, name)| {
            name.id() == identifier.id()
                && (MEMBERS.contains(&parent.kind()) || parent.kind() == "variable_declarator")
        })
}

/// The `import` lines of this file whose type the member's text actually mentions.
///
/// By simple name, which is how an import is used and therefore how it is found. An import that
/// turns out to be unnecessary in the new file is harmless; a missing one is a file that does not
/// compile, so the direction of the guess is the safe one.
fn imports_the_member_reads(root: Node<'_>, member: &Node<'_>, source: &str) -> Vec<String> {
    let words: Vec<&str> =
        crate::selection::identifiers(*member).iter().map(|n| text(n, source)).collect();
    let types: Vec<&str> = crate::selection::descendants_any(*member, &["type_identifier"])
        .iter()
        .map(|n| text(n, source))
        .collect();
    imports_of(root, source)
        .into_iter()
        .filter(|(what, _)| {
            let simple = what.rsplit('.').next().unwrap_or(what);
            simple == "*" || words.contains(&simple) || types.contains(&simple)
        })
        .map(|(_, line)| line)
        .collect()
}

/// The span a member occupies, its javadoc included.
///
/// A doc comment is a sibling node rather than part of the declaration, so a move that took only the
/// declaration would leave the documentation behind describing nothing. Only a comment that
/// **touches** it counts: a blank line between them means it was about the section, not the member.
fn span_of(member: &Node<'_>, source: &str) -> (usize, usize) {
    let mut start = line_start(source, member.start_byte());
    let mut previous = member.prev_sibling();
    while let Some(node) = previous {
        if !matches!(node.kind(), "block_comment" | "line_comment") {
            break;
        }
        let between = source.get(node.end_byte()..start).unwrap_or_default();
        if between.matches('\n').count() > 1 {
            break;
        }
        start = line_start(source, node.start_byte());
        previous = node.prev_sibling();
    }
    (start, line_after(source, member.end_byte()))
}

/// The member the caret is standing on — through its own header, never from inside its body.
///
/// Standing in a method's body is standing in the code, not on the declaration, and offering to
/// move the whole method from there puts three rows in every menu for something the user is not
/// looking at.
fn member_at<'t>(root: Node<'t>, source: &str, start: usize, end: usize) -> Option<Node<'t>> {
    let at = crate::selection::node_covering(root, start, end)?;
    let member = enclosing(at, MEMBERS)?;
    // `block` is in `MEMBERS` for the instance initialiser, and a method's BODY is a `block` too —
    // so without this every caret anywhere inside any method reads as a caret on a member, and the
    // menu grows three rows about moving an initialiser that is not there.
    if !member.parent().is_some_and(|p| crate::body::TYPE_BODIES.contains(&p.kind())) {
        return None;
    }
    // A nested type is a member too, but moving one is `move-class`'s job — it has a file to write.
    if TYPE_DECLS.contains(&member.kind()) {
        return None;
    }
    let head_end = match member.child_by_field_name("body") {
        Some(body) => body.start_byte(),
        None => member.end_byte(),
    };
    let _ = source;
    (start >= member.start_byte() && start <= head_end).then_some(member)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn moves(source: &str, needle: &str) -> Vec<Result<Plan, Refusal>> {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        member_moves(tree.root_node(), source, at, at)
    }

    fn by_id(source: &str, needle: &str, id: &str) -> Result<Plan, Refusal> {
        moves(source, needle)
            .into_iter()
            .find(|o| match o {
                Ok(p) => p.id == id,
                Err(r) => r.id == id,
            })
            .unwrap_or_else(|| panic!("no `{id}` among the offers"))
    }

    const PAIR: &str = "class Base {\n    void common() {\n    }\n}\n\nclass Impl extends Base {\n    /** Doc. */\n    void moved() {\n        work();\n    }\n\n    void work() {\n    }\n}\n";

    #[test]
    fn a_member_moves_into_a_supertype_in_the_same_file() {
        // `moved` reads `work`, which stays behind — so the fixture that MOVES is one that does not.
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    /** Doc. */\n    void moved() {\n        System.out.println(1);\n    }\n}\n";
        let Ok(plan) = by_id(src, "void moved", "pull-up-member") else { panic!("expected a plan") };
        let out = plan.apply(src);
        assert!(out.starts_with("class Base {\n\n    /** Doc. */\n    void moved()"), "{out}");
        assert!(!out.contains("class Impl extends Base {\n    /** Doc. */"), "{out}");
    }

    /// What the member reads from the type it leaves is the check that keeps this honest.
    #[test]
    fn a_member_that_reads_a_sibling_is_refused() {
        let Err(refusal) = by_id(PAIR, "void moved", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("reads `work`"), "{}", refusal.reason);
    }

    #[test]
    fn a_private_member_cannot_be_pulled_up() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    private void moved() {\n    }\n}\n";
        let Err(refusal) = by_id(src, "private void moved", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("`private`"), "{}", refusal.reason);
    }

    #[test]
    fn a_name_the_target_already_has_is_refused() {
        let src = "class Base {\n    void moved() {\n    }\n}\n\nclass Impl extends Base {\n    void moved() {\n    }\n}\n";
        let second = src.rfind("void moved").unwrap();
        let tree = parse_java(src).unwrap();
        let Some(Err(refusal)) = member_moves(tree.root_node(), src, second, second)
            .into_iter()
            .find(|o| matches!(o, Err(r) if r.id == "pull-up-member"))
        else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("already declares"), "{}", refusal.reason);
    }

    #[test]
    fn a_super_call_will_not_travel() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    void moved() {\n        super.toString();\n    }\n}\n";
        let Err(refusal) = by_id(src, "void moved", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("super"), "{}", refusal.reason);
    }

    /// Each subtype in the file is its own row, so the menu is the choice.
    #[test]
    fn push_down_offers_one_row_per_subtype() {
        let src = "class Base {\n    void moved() {\n    }\n}\n\nclass A extends Base {\n}\n\nclass B extends Base {\n}\n";
        let ids: Vec<String> = moves(src, "void moved")
            .into_iter()
            .map(|o| match o {
                Ok(p) => p.id,
                Err(r) => r.id,
            })
            .collect();
        assert!(ids.contains(&"push-down-member:A".to_string()), "{ids:?}");
        assert!(ids.contains(&"push-down-member:B".to_string()), "{ids:?}");
    }

    #[test]
    fn a_member_the_type_still_uses_will_not_go_down() {
        let src = "class Base {\n    void moved() {\n    }\n    void caller() {\n        moved();\n    }\n}\n\nclass A extends Base {\n}\n";
        let Err(refusal) = by_id(src, "void moved", "push-down-member:A") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("still used"), "{}", refusal.reason);
    }

    /// A target this file does not declare is described rather than planned — the caller with the
    /// index writes the other half.
    #[test]
    fn a_target_elsewhere_comes_back_as_a_transfer() {
        let src = "import java.util.List;\n\nclass Impl extends Base {\n    void moved() {\n        List<String> l = null;\n    }\n}\n";
        let Ok(plan) = by_id(src, "void moved", "pull-up-member") else { panic!("expected a plan") };
        let transfer = plan.transfer.clone().expect("a transfer");
        assert_eq!(transfer.target, "Base");
        assert!(transfer.member.contains("void moved()"), "{}", transfer.member);
        assert!(transfer.imports.iter().any(|i| i.contains("java.util.List")), "{:?}", transfer.imports);
        // The removal still applies here and now.
        assert!(!plan.apply(src).contains("void moved"), "{}", plan.apply(src));
    }

    /// The other half of a cross-file move: the same checks, against the target's own source.
    #[test]
    fn a_transfer_is_written_into_the_target_file() {
        let src = "import java.util.List;\n\nclass Impl extends Base {\n    void moved() {\n        List<String> l = null;\n    }\n}\n";
        let Ok(plan) = by_id(src, "void moved", "pull-up-member") else { panic!("expected a plan") };
        let target = "package p;\n\nclass Base {\n    int n;\n}\n";
        let edits = transfer_into(&plan, target, "Base.java").expect("edits");
        let mut whole = Plan::new("t", "t", edits);
        whole.edits.iter_mut().for_each(|e| e.file = String::new());
        whole.reorder();
        let out = whole.apply(target);
        assert!(out.contains("import java.util.List;"), "{out}");
        assert!(out.contains("    void moved() {"), "{out}");
    }

    /// A target that does not declare what the member reads is a refusal, not a half-applied move.
    #[test]
    fn a_target_missing_what_the_member_reads_refuses() {
        let src = "class Impl extends Base {\n    void moved() {\n        helper();\n    }\n\n    void helper() {\n    }\n}\n";
        let Ok(plan) = by_id(src, "void moved", "pull-up-member") else { panic!("expected a plan") };
        let err = transfer_into(&plan, "class Base {\n}\n", "Base.java").unwrap_err();
        assert!(err.contains("helper"), "{err}");
    }

    /// A method with a body is legal in an interface only as `default`.
    #[test]
    fn a_method_pulled_into_an_interface_becomes_default() {
        let src = "interface Base {\n}\n\nclass Impl implements Base {\n    public void work() {\n        run();\n    }\n}\n";
        let Ok(plan) = by_id(src, "public void work", "pull-up-member") else {
            panic!("expected a plan")
        };
        let out = plan.apply(src);
        assert!(out.contains("default void work()"), "{out}");
        assert!(!out.contains("public default"), "{out}");
    }

    /// …and `static` stays `static`, which needs no `default`.
    #[test]
    fn a_static_method_keeps_its_staticness_in_an_interface() {
        let src = "interface Base {\n}\n\nclass Impl implements Base {\n    public static int two() {\n        return 2;\n    }\n}\n";
        let Ok(plan) = by_id(src, "public static int two", "pull-up-member") else {
            panic!("expected a plan")
        };
        let out = plan.apply(src);
        assert!(out.contains("static int two()"), "{out}");
        assert!(!out.contains("default"), "{out}");
    }

    /// Going the other way, `default` is not a modifier a class method may carry — and dropping it
    /// without saying `public` would quietly narrow what everybody could call.
    #[test]
    fn a_default_method_pushed_down_becomes_public() {
        let src = "interface Base {\n    default void work() {\n    }\n}\n\nclass Impl implements Base {\n}\n";
        let Ok(plan) = by_id(src, "default void work", "push-down-member:Impl") else {
            panic!("expected a plan")
        };
        let out = plan.apply(src);
        assert!(out.contains("public void work()"), "{out}");
        assert!(!out.contains("default void work"), "{out}");
    }

    /// A `protected final synchronized` method cannot be a `default` one.
    #[test]
    fn the_modifiers_an_interface_forbids_are_dropped() {
        let adapted =
            adapt_modifiers("protected final synchronized void work() {\n}", true).unwrap();
        assert_eq!(adapted, "default void work() {\n}");
    }

    /// An annotation is left exactly where it was written.
    #[test]
    fn an_annotation_keeps_its_own_line() {
        let adapted = adapt_modifiers("@Deprecated\npublic void work() {\n}", true).unwrap();
        assert!(adapted.starts_with("@Deprecated\n"), "{adapted}");
        assert!(adapted.contains("default void work()"), "{adapted}");
    }

    /// `@Override` is a promise about the type the member is declared in.
    #[test]
    fn an_override_does_not_travel() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    @Override\n    public String toString() {\n        return \"x\";\n    }\n}\n";
        let Err(refusal) = by_id(src, "@Override", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("@Override"), "{}", refusal.reason);
    }

    /// A method with no body is a contract, and moving one changes what both types promise.
    #[test]
    fn an_abstract_method_is_a_contract_and_stays() {
        let src = "interface Base {\n}\n\ninterface Impl extends Base {\n    void work();\n}\n";
        let Err(refusal) = by_id(src, "void work();", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("no body"), "{}", refusal.reason);
    }

    /// A factory returning its own type means the same class wherever it lands.
    #[test]
    fn a_member_that_names_its_own_class_stays() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    static Impl of() {\n        return null;\n    }\n}\n";
        let Err(refusal) = by_id(src, "static Impl of", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("names `Impl` itself"), "{}", refusal.reason);
    }

    /// `class LongRange extends Range<Long>` substitutes `Long` for `N`; moved up, the member sees
    /// `N` where it was written for `Long`.
    #[test]
    fn a_generic_supertype_with_fixed_arguments_refuses_a_pull_up() {
        let src = "class Range<N> {\n}\n\nclass LongRange extends Range<Long> {\n    void work() {\n    }\n}\n";
        let Err(refusal) = by_id(src, "void work", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("fixes its type arguments"), "{}", refusal.reason);
    }

    /// Sideways, `this` would be a different object — so only a `static` member goes on its own.
    #[test]
    fn an_instance_member_does_not_move_sideways() {
        let src = "class A {\n    void work() {\n    }\n}\n\nclass B {\n}\n";
        let Err(refusal) = by_id(src, "void work", "move-member:B") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("instance member"), "{}", refusal.reason);
    }

    /// A nested type of the owner is written bare, and that name resolves nowhere else.
    #[test]
    fn a_member_typed_with_a_nested_type_of_its_own_class_stays() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    static class Config {\n    }\n\n    Config make() {\n        return null;\n    }\n}\n";
        let Err(refusal) = by_id(src, "Config make", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("Config"), "{}", refusal.reason);
    }

    /// An enum with no `;` after its constants has no member section — a method written straight
    /// after them is read as another constant, which is a syntax error.
    #[test]
    fn a_member_moved_into_an_enum_gets_the_semicolon_it_needs() {
        let src = "class Host {\n    enum Kind {\n        BIG, SMALL\n    }\n\n    static int two() {\n        return 2;\n    }\n}\n";
        let Ok(plan) = by_id(src, "static int two", "move-member:Kind") else {
            panic!("expected a plan")
        };
        let out = plan.apply(src);
        assert!(out.contains("BIG, SMALL;"), "{out}");
        assert!(bennu_java::prelude::parse_java(&out).is_some(), "{out}");
    }

    /// Something in the file overrides it, and the override would be left pointing at nothing.
    #[test]
    fn a_member_a_subclass_overrides_stays() {
        let src = "class Strategy {\n    boolean isNumber() {\n        return false;\n    }\n}\n\nclass Digits extends Strategy {\n    @Override\n    boolean isNumber() {\n        return true;\n    }\n}\n\nclass Other extends Strategy {\n}\n";
        let Err(refusal) = by_id(src, "boolean isNumber", "push-down-member:Other") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("overrides"), "{}", refusal.reason);
    }

    /// Sideways, a name the target happens to declare too is a collision and not a match.
    #[test]
    fn a_static_member_moved_sideways_has_to_be_self_contained() {
        let src = "class A {\n    static int helper() {\n        return 1;\n    }\n\n    static int work() {\n        return helper();\n    }\n}\n\nclass B {\n    static int helper() {\n        return 2;\n    }\n}\n";
        let Err(refusal) = by_id(src, "static int work", "move-member:B") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("would mean something else"), "{}", refusal.reason);
    }

    /// A call that reads as recursion and is not: the class declares an overload that stays.
    #[test]
    fn an_overload_left_behind_keeps_the_member_where_it_is() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    boolean has(String cs) {\n        return has(cs, null);\n    }\n\n    boolean has(String cs, String other) {\n        return false;\n    }\n}\n";
        let Err(refusal) = by_id(src, "boolean has(String cs) {", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("reads `has`"), "{}", refusal.reason);
    }

    /// The class that CONTAINS a hierarchy calls through the supertype, and it is neither the
    /// owner nor a subtype of it.
    #[test]
    fn the_enclosing_class_counts_as_a_user() {
        let src = "class Breaker {\n    static class State {\n        boolean finished() {\n            return true;\n        }\n    }\n\n    static class Open extends State {\n    }\n\n    boolean check(State s) {\n        return s.finished();\n    }\n}\n";
        let Err(refusal) = by_id(src, "boolean finished", "push-down-member:Open") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("still used by `Breaker`"), "{}", refusal.reason);
    }

    /// A type argument is not something a move may invent.
    #[test]
    fn a_member_written_in_the_class_type_parameter_stays() {
        let src = "class Base {\n}\n\nclass Box<T> extends Base {\n    void put(T item) {\n    }\n}\n";
        let Err(refusal) = by_id(src, "void put", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("type parameter"), "{}", refusal.reason);
    }

    /// Moving sideways breaks the owner just as pushing down does — only a pull UP keeps the
    /// member reachable under the same name.
    #[test]
    fn a_member_the_type_still_uses_will_not_move_across_either() {
        let src = "class A {\n    static void moved() {\n    }\n    void caller() {\n        moved();\n    }\n}\n\nclass B {\n}\n";
        let Err(refusal) = by_id(src, "static void moved", "move-member:B") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("still used"), "{}", refusal.reason);
    }

    /// A caret in the body is standing in the code, not on the declaration.
    #[test]
    fn a_caret_inside_the_body_offers_nothing() {
        let tree = parse_java(PAIR).unwrap();
        let at = PAIR.find("work();").unwrap();
        assert!(member_moves(tree.root_node(), PAIR, at, at).is_empty());
    }

    #[test]
    fn a_constructor_belongs_where_it_is() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    Impl() {\n    }\n}\n";
        let Err(refusal) = by_id(src, "Impl() {", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("constructor"), "{}", refusal.reason);
    }
}
