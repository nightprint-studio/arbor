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
//! - **`private`, pulled up.** Invisible to the class it came from, so where that class reads it
//!   the member is **widened to `protected`** on the way and the row says so — the smallest change
//!   that works whether or not the supertype shares a package. Into an interface it needs nothing:
//!   members there are implicitly public.
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

/// Members the **runtime** reads by name, where no source mention proves anything.
///
/// The serialization ones, which is the whole list that matters here: `serialVersionUID` decides
/// what a class deserializes as, and the three private hooks the ObjectStream calls reflectively.
/// A move of any of them compiles perfectly and changes what the program does.
const READ_BY_THE_RUNTIME: &[&str] =
    &["serialVersionUID", "writeObject", "readObject", "readObjectNoData", "writeReplace", "readResolve"];
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

    // Pull up: **every** supertype the class names, not just the first. `implements` counts — a
    // `default` method pulled into the interface is the commonest pull-up there is — and a class
    // that implements three of them had two of its answers hidden while this took `.next()`.
    // The id stays bare where there is one, so the one-answer case reads and behaves as it did.
    let supertypes = supertypes_written(&owner, source);
    let sole = supertypes.len() == 1;
    for (supertype, at) in &supertypes {
        let id = if sole { PULL_UP.0.to_string() } else { format!("{}:{supertype}", PULL_UP.0) };
        let label = if sole {
            PULL_UP.1.to_string()
        } else {
            format!("Pull member up to `{supertype}`")
        };
        out.push(plan_move(root, source, &member, &owner, supertype, *at, (&id, &label), "up"));
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
    direction: MoveDirection,
    id: &str,
    label: &str,
) -> Outcome {
    let member = member_at(root, source, start, end)?;
    let owner = enclosing_type(member)?;
    // A target the caller located has no written name in this file to point a resolver at — the
    // caller knows the file already, which is how it found the type. `member.start_byte()` keeps
    // the field well-formed and is never asked about.
    let at = supertype_written(&owner, source)
        .filter(|_| direction == MoveDirection::Up)
        .map(|(_, at)| at)
        .unwrap_or_else(|| member.start_byte());
    plan_move(root, source, &member, &owner, target, at, (id, label), direction.as_str())
}

/// Which way a member is moving, which is the whole of what makes the three refactorings differ.
///
/// Not a string at the API edge: `"across"` typed where `"up"` was meant refuses every instance
/// member for a reason that has nothing to do with the move being asked for, and nothing catches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    /// Into a supertype. The member stays reachable under the same name, by inheritance.
    Up,
    /// Into a subtype. Everything that reached it through the supertype loses it.
    Down,
    /// Into a type that is neither. `this` would be a different object, so only `static` goes.
    Across,
}

impl MoveDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Across => "across",
        }
    }

    /// Read a direction off the wire, where it arrives as the refactoring's own id.
    pub fn from_id(id: &str) -> Self {
        match id.split(['@', ':']).next().unwrap_or(id) {
            "pull-up-member" => Self::Up,
            "push-down-member" => Self::Down,
            _ => Self::Across,
        }
    }
}

/// Plan a member move from source text, for a caller that has no parse of its own.
///
/// The entry point a **target picker** needs: the editor found the type, the backend has its file,
/// and neither of them holds a tree. Everything else is [`move_member_to`].
pub fn move_member_plan(
    source: &str,
    start: usize,
    end: usize,
    target: &str,
    direction: MoveDirection,
    id: &str,
    label: &str,
) -> Outcome {
    let tree = bennu_java::prelude::parse_java(source)?;
    move_member_to(tree.root_node(), source, start, end, target, direction, id, label)
}

/// What a caret on a member's own header is standing on — the gesture all three moves answer to.
///
/// The **offer list's** question, asked once: a caller that wants to add a row per family needs to
/// know whether there is a member here at all, and which supertypes it could be pulled into. It
/// gets that without planning anything, because planning needs a target and finding one is the
/// caller's half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveSite {
    /// The member's name, for the row.
    pub member: String,
    /// The type it is declared in.
    pub owner: String,
    /// The supertypes that type names, as the source writes them — every one a pull up could
    /// target, not just the first.
    pub supertypes: Vec<String>,
    /// Whether the member is `static`, which is the only kind that moves sideways on its own.
    pub is_static: bool,
    /// Whether pulling it up would have to **widen** it to `protected` — it is `private` and the
    /// class still reads it. A picker says so on the row rather than surprising anyone afterwards.
    pub widens_private: bool,
}

/// The move site at the caret, or `None` when the caret is not on a member's header.
pub fn move_site(source: &str, start: usize, end: usize) -> Option<MoveSite> {
    let tree = bennu_java::prelude::parse_java(source)?;
    let root = tree.root_node();
    let member = member_at(root, source, start, end)?;
    let owner = enclosing_type(member)?;
    Some(MoveSite {
        member: member_name(&member, source)?.to_string(),
        owner: owner.child_by_field_name("name").map(|n| text(&n, source))?.to_string(),
        supertypes: supertypes_of(&owner, source),
        is_static: has_modifier(&member, source, "static"),
        widens_private: has_modifier(&member, source, "private")
            && member_name(&member, source)
                .is_some_and(|name| still_used_by(root, &member, source, name).is_some()),
    })
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

/// Every supertype a type names, and where each name is written.
///
/// The offset matters as much as the name: it is what a caller with a resolver asks
/// go-to-declaration about, so `Base` resolves through **this** file's imports and package rather
/// than through a guess at which `Base` was meant.
fn supertypes_written(owner: &Node<'_>, source: &str) -> Vec<(String, usize)> {
    let header_end =
        owner.child_by_field_name("body").map(|b| b.start_byte()).unwrap_or(owner.end_byte());
    let Some(header) = source.get(owner.start_byte()..header_end) else { return Vec::new() };
    supertypes_of(owner, source)
        .into_iter()
        .filter_map(|name| {
            let at = header.find(&name).map(|i| owner.start_byte() + i)?;
            Some((name, at))
        })
        .collect()
}

/// The first supertype a type names, and where that name is written.
fn supertype_written(owner: &Node<'_>, source: &str) -> Option<(String, usize)> {
    supertypes_written(owner, source).into_iter().next()
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
/// `widen_private` is the third thing it does and the one that turns a refusal into a move: a
/// `private` member pulled up is invisible to the class it came from, so where that class actually
/// reads it, `private` becomes **`protected`** — the smallest widening that works whether or not the
/// supertype shares a package. Only where it is needed: a private member nothing reads goes up
/// exactly as written, because the smallest edit is the one that changes least.
///
/// `None` when the member's text does not parse on its own, which is a refusal to touch it rather
/// than a guess at what it meant.
pub fn adapt_modifiers(member: &str, into_interface: bool, widen_private: bool) -> Option<String> {
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
    // Widening comes first, and only outside an interface: there, `private` is dropped along with
    // every other access keyword and the member is implicitly public, which is wider still.
    if widen_private && !into_interface && has_modifier(&node, &wrapped, "private") {
        let swapped = replace_keyword(member, &node, offset, "private", "protected")?;
        return adapt_modifiers(&swapped, into_interface, false);
    }
    let (drop, add): (&[&str], Option<&str>) = match (into_interface, is_field, is_static, has_body) {
        (true, true, _, _) => (&["public", "protected", "private", "static", "final"], None),
        // `private` is in the list for a reason that is easy to miss: a `private static` method in
        // an interface is legal from Java 9, and it is private **to the interface** — the class that
        // was calling it would still not see it. Dropped, the member is implicitly public.
        (true, false, true, _) => (
            &["public", "protected", "private", "final", "synchronized", "native"],
            None,
        ),
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
    // The same rule the in-file path applies, read off the member's own text. The two used to check
    // different things, and this is the half that checked less.
    if into_interface {
        let wrapped = format!("class __Wrap {{\n{}\n}}", transfer.member);
        let parsed = bennu_java::prelude::parse_java(&wrapped);
        let node = parsed.as_ref().and_then(|tree| {
            let ty = types_in(tree.root_node()).into_iter().next()?;
            members_of(&body_of(&ty)?).into_iter().next()
        });
        if let Some(node) = node {
            if let Some(reason) = interface_refuses(&node, &wrapped, &transfer.target) {
                return Err(reason);
            }
        }
    }
    let adapted = adapt_modifiers(&transfer.member, into_interface, transfer.widen_private)
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

/// Swap one modifier keyword for another, in place.
fn replace_keyword(
    member: &str,
    node: &Node<'_>,
    offset: usize,
    from: &str,
    to: &str,
) -> Option<String> {
    let mut cursor = node.walk();
    let modifiers = node.children(&mut cursor).find(|c| c.kind() == "modifiers")?;
    let mut inner = modifiers.walk();
    let word = modifiers.children(&mut inner).find(|c| c.kind() == from)?;
    let mut out = member.to_string();
    out.replace_range(word.start_byte() - offset..word.end_byte() - offset, to);
    Some(out)
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

    // A `private` member the class still reads has to be widened on the way up, or the read it
    // leaves behind stops compiling. Worked out here, where both halves are known, and carried into
    // the label so the row states what it will do rather than a dialog asking whether it may.
    let widening = direction == "up"
        && has_modifier(member, source, "private")
        && still_used_by(root, member, source, name).is_some();
    let (id, label) = if widening {
        (id, &*format!("{label} (widening it to `protected`)"))
    } else {
        (id, label)
    };

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
            widen_private: widening,
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
    let adapted = adapt_modifiers(&reindent(&moved, &own_indent, ""), into_interface, widening)?;
    let insertion = format!("{terminator}{nl}{}{nl}", reindent(&adapted, "", &indent));

    let plan = Plan::new(
        id,
        label,
        vec![removal, RefactorEdit::new(at, at, insertion, "member")],
    )
    .named(name);
    Some(Ok(level_for(plan, into_interface, member)))
}

/// The Java floor a member landing in an interface puts under the file.
///
/// A method that keeps its body there is a `default` one and a `static` one is a static interface
/// method: **both are Java 8**, and this editor exists for codebases that are not. Read here, where
/// what was written is known; checked by the caller, which is the only side that knows what the
/// project targets.
fn level_for(plan: Plan, into_interface: bool, member: &Node<'_>) -> Plan {
    if !into_interface || member.kind() != "method_declaration" {
        return plan;
    }
    let name = plan.name.clone().unwrap_or_default();
    plan.needing_level(
        8,
        format!(
            "`{name}` keeps its body, and a method with a body in an interface is a `default` or \
             `static` one — which Java added in 8"
        ),
    )
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
    // A `final` field with no initialiser is assigned by a **constructor**, and a final field may
    // only be assigned by the class that declares it (JLS §8.3.1.2). Moved up, that constructor
    // stays behind and stops compiling — `cannot assign a value to final variable`, thirty times on
    // Apache Commons Lang the day the `private` rule was relaxed. No visibility fixes it, which is
    // why this is not the widening case below.
    if direction == "up"
        && member.kind() == "field_declaration"
        && has_modifier(member, source, "final")
        && !descendants(*member, "variable_declarator")
            .iter()
            .all(|d| d.child_by_field_name("value").is_some())
    {
        return Some(format!(
            "`{name}` is `final` with no value here, so a constructor assigns it — and a `final` \
             field may only be assigned by the class that declares it"
        ));
    }
    // A private member pulled up is invisible from the class it came from — which is a reason to
    // **widen** it, not to refuse. Refusing on `private` alone was sound and useless: 252 of the
    // refusals on Apache Commons Lang were `serialVersionUID`, and each of the rest was a pull up
    // that only needed `protected`. See `widening` below and `adapt_modifiers`.
    //
    // `serialVersionUID` itself still stays, and for the reason safe delete already refuses on: the
    // serialization runtime reads it **by name**, so no source mentioning it proves nothing, and
    // moving it up silently changes what the subclass serializes as.
    if direction == "up" && has_modifier(member, source, "private") {
        if READ_BY_THE_RUNTIME.contains(&name) {
            return Some(format!(
                "`{name}` is read by name at run time rather than by any code, so moving it changes \
                 what this class does without anything here saying so"
            ));
        }
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
    } else if let Some(user) = used_outside_the_hierarchy(root, member, source, name) {
        // Going up, inheritance keeps the member reachable — but only for the class it left and
        // the classes under it. A **sibling nested class** calling it unqualified, which works
        // only because they share a top-level body, has no inheritance to fall back on:
        // `cannot find symbol: method appendDigits(…)`, twice on Apache Commons Lang.
        return Some(format!(
            "`{name}` is used by `{user}`, which is not under `{}` — inheritance would not carry it \
             there",
            enclosing_type(*member)
                .and_then(|t| t.child_by_field_name("name"))
                .map(|n| text(&n, source))
                .unwrap_or("this type")
        ));
    }
    None
}

/// A type in this file that uses the member and is **neither its owner nor under it**.
///
/// The distinction a pull up turns on: everything inside the owner, and everything that extends it,
/// keeps reaching the member by inheritance once it is one level up. Anything else was reaching it
/// some other way — sharing a top-level body is the one that looks like inheritance and is not.
fn used_outside_the_hierarchy(
    root: Node<'_>,
    member: &Node<'_>,
    source: &str,
    name: &str,
) -> Option<String> {
    let owner = enclosing_type(*member)?;
    let owner_name = owner.child_by_field_name("name").map(|n| text(&n, source))?;
    let under: Vec<usize> = std::iter::once(owner)
        .chain(subtypes_of(root, source, owner_name))
        .map(|t| t.id())
        .collect();
    crate::selection::identifiers(root)
        .into_iter()
        .filter(|identifier| {
            text(identifier, source) == name
                && !(identifier.start_byte() >= member.start_byte()
                    && identifier.end_byte() <= member.end_byte())
                && !names_a_declaration(identifier)
        })
        .find_map(|identifier| {
            let scope = enclosing_type(identifier)?;
            (!under.contains(&scope.id()))
                .then(|| scope.child_by_field_name("name").map(|n| text(&n, source).to_string()))
                .flatten()
        })
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
    if matches!(target_decl.kind(), "interface_declaration" | "annotation_type_declaration") {
        if let Some(reason) = interface_refuses(member, source, target) {
            return Some(reason);
        }
    }
    if let Some(missing) = requires.iter().find(|needed| {
        !members_of(&body).iter().any(|m| member_name(m, source) == Some(needed.as_str()))
    }) {
        return Some(format!("`{name}` reads `{missing}`, which stays behind"));
    }
    None
}

/// Why an **interface** cannot take this member.
///
/// Shared by both paths on purpose. The in-file plan and the cross-file [`transfer_into`] used to
/// check different things, and the half that checked less is the half that produced `= expected`:
/// a `private final` field with no value, written into an interface where it is implicitly
/// `public static final` and must be initialised where it is declared (JLS §9.3).
fn interface_refuses(member: &Node<'_>, source: &str, target: &str) -> Option<String> {
    if member.kind() != "field_declaration" {
        return None;
    }
    let initialised = descendants(*member, "variable_declarator")
        .iter()
        .all(|d| d.child_by_field_name("value").is_some());
    (!initialised || !has_modifier(member, source, "static") || !has_modifier(member, source, "final"))
        .then(|| {
            format!(
                "`{target}` is an interface, where a field is implicitly `public static final` and \
                 initialised where it is declared"
            )
        })
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

    /// Apply, and insist the result is still Java — the assertion that catches the worst thing a
    /// move can produce, which is a file that does not parse.
    fn parses(plan: &Plan, source: &str) -> String {
        let out = plan.apply(source);
        assert!(
            bennu_java::prelude::parse_java(&out).is_some_and(|t| !t.root_node().has_error()),
            "does not parse:\n{out}"
        );
        out
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

    /// A private member the class still reads is widened on the way, and the row says so.
    #[test]
    fn a_private_member_the_class_reads_goes_up_as_protected() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    private void moved() {\n    }\n\n    void caller() {\n        moved();\n    }\n}\n";
        let Ok(plan) = by_id(src, "private void moved", "pull-up-member") else {
            panic!("expected a plan")
        };
        assert!(plan.label.contains("protected"), "{}", plan.label);
        let out = plan.apply(src);
        assert!(out.contains("protected void moved()"), "{out}");
        assert!(!out.contains("private void moved"), "{out}");
    }

    /// Into an interface it needs no widening at all: members there are implicitly public, and
    /// `protected` is not a modifier one may carry.
    #[test]
    fn a_private_method_pulled_into_an_interface_is_not_made_protected() {
        let src = "interface Base {\n}\n\nclass Impl implements Base {\n    private void moved() {\n    }\n\n    void caller() {\n        moved();\n    }\n}\n";
        let Ok(plan) = by_id(src, "private void moved", "pull-up-member") else {
            panic!("expected a plan")
        };
        let out = plan.apply(src);
        assert!(out.contains("default void moved()"), "{out}");
        assert!(!out.contains("protected"), "{out}");
        assert!(!out.contains("private void moved"), "{out}");
    }

    /// A `private static` method in an interface is legal from Java 9 and private **to the
    /// interface** — the class that called it would still not see it.
    #[test]
    fn a_private_static_method_does_not_stay_private_in_an_interface() {
        let adapted = adapt_modifiers("private static int two() {\n    return 2;\n}", true, false)
            .unwrap();
        assert!(adapted.starts_with("static int two()"), "{adapted}");
    }

    /// …and one nothing reads goes up **unchanged**: the smallest edit is the one that changes
    /// least, and nothing forces a widening here.
    #[test]
    fn a_private_member_nothing_reads_goes_up() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    private void moved() {\n    }\n}\n";
        let Ok(plan) = by_id(src, "private void moved", "pull-up-member") else {
            panic!("expected a plan")
        };
        assert!(plan.apply(src).contains("class Base {\n\n    private void moved()"), "{}", plan.apply(src));
    }

    /// `serialVersionUID` is the exception, and for the reason safe delete already refuses on: the
    /// serialization runtime reads it by name, so no source mentioning it proves nothing.
    #[test]
    fn a_member_the_runtime_reads_by_name_stays() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    private static final long serialVersionUID = 1L;\n}\n";
        let Err(refusal) = by_id(src, "serialVersionUID", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("by name at run time"), "{}", refusal.reason);
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
            adapt_modifiers("protected final synchronized void work() {\n}", true, false).unwrap();
        assert_eq!(adapted, "default void work() {\n}");
    }

    /// An annotation is left exactly where it was written.
    #[test]
    fn an_annotation_keeps_its_own_line() {
        let adapted = adapt_modifiers("@Deprecated\npublic void work() {\n}", true, false).unwrap();
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

    /// A class implementing three interfaces has three answers, and only one used to be offered.
    #[test]
    fn every_supertype_is_its_own_row() {
        let src = "interface A {\n}\n\ninterface B {\n}\n\nclass Impl implements A, B {\n    static int two() {\n        return 2;\n    }\n}\n";
        let ids: Vec<String> = moves(src, "static int two")
            .into_iter()
            .map(|o| match o {
                Ok(p) => p.id,
                Err(r) => r.id,
            })
            .collect();
        assert!(ids.contains(&"pull-up-member:A".to_string()), "{ids:?}");
        assert!(ids.contains(&"pull-up-member:B".to_string()), "{ids:?}");
    }

    /// …and with a single supertype the id stays bare, so the common case reads as it did.
    #[test]
    fn one_supertype_keeps_the_plain_id() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    static int two() {\n        return 2;\n    }\n}\n";
        assert!(matches!(by_id(src, "static int two", "pull-up-member"), Ok(_)));
    }

    /// The site a target picker asks about: a member's header, and every type it could go up into.
    #[test]
    fn the_move_site_reports_the_member_and_its_supertypes() {
        let src = "interface A {\n}\n\nclass Impl implements A {\n    static int two() {\n        return 2;\n    }\n}\n";
        let at = src.find("static int two").unwrap();
        let site = move_site(src, at, at).expect("a site");
        assert_eq!(site.member, "two");
        assert_eq!(site.owner, "Impl");
        assert_eq!(site.supertypes, vec!["A"]);
        assert!(site.is_static);
        // …and a caret in the body is not a site.
        let inside = src.find("return 2").unwrap();
        assert!(move_site(src, inside, inside).is_none());
    }

    /// The picker's entry point: a target the caller located, planned from text alone.
    #[test]
    fn a_picked_target_is_planned_from_source_text() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    static int two() {\n        return 2;\n    }\n}\n";
        let at = src.find("static int two").unwrap();
        let Some(Ok(plan)) = move_member_plan(
            src,
            at,
            at,
            "Base",
            MoveDirection::Up,
            "pull-up-member",
            "Pull member up",
        ) else {
            panic!("expected a plan")
        };
        assert!(plan.apply(src).contains("class Base {\n\n    static int two()"), "{}", plan.apply(src));
    }

    /// A direction typed as a string is the mistake this enum exists to make impossible.
    #[test]
    fn the_direction_is_read_off_the_refactorings_own_id() {
        assert_eq!(MoveDirection::from_id("pull-up-member"), MoveDirection::Up);
        assert_eq!(MoveDirection::from_id("pull-up-member:Base"), MoveDirection::Up);
        assert_eq!(MoveDirection::from_id("push-down-member@pick"), MoveDirection::Down);
        assert_eq!(MoveDirection::from_id("move-member:B"), MoveDirection::Across);
    }

    /// A method keeping its body inside an interface is a `default` one, which is Java 8.
    #[test]
    fn a_method_pulled_into_an_interface_puts_a_java_floor_on_the_file() {
        let src = "interface Base {\n}\n\nclass Impl implements Base {\n    public void work() {\n        run();\n    }\n}\n";
        let Ok(plan) = by_id(src, "public void work", "pull-up-member") else {
            panic!("expected a plan")
        };
        let level = plan.needs_level.expect("a floor");
        assert_eq!(level.at_least, 8);
        assert!(level.because.contains("`default`"), "{}", level.because);
    }

    /// …and a class target puts none: nothing there is newer than Java 1.
    #[test]
    fn a_move_between_classes_needs_no_particular_java() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    public void work() {\n    }\n}\n";
        let Ok(plan) = by_id(src, "public void work", "pull-up-member") else {
            panic!("expected a plan")
        };
        assert!(plan.needs_level.is_none());
    }

    /// The positive push down: the member lands in the subtype and leaves the supertype.
    #[test]
    fn a_member_pushed_down_lands_in_the_subtype() {
        let src = "class Base {\n    /** Doc. */\n    void helper() {\n        System.out.println(1);\n    }\n}\n\nclass Impl extends Base {\n}\n";
        let Ok(plan) = by_id(src, "void helper", "push-down-member:Impl") else {
            panic!("expected a plan")
        };
        let out = parses(&plan, src);
        assert!(out.contains("class Impl extends Base {\n\n    /** Doc. */\n    void helper()"), "{out}");
        assert!(out.starts_with("class Base {\n}"), "{out}");
    }

    /// A field moves the same way a method does, javadoc and initialiser included.
    #[test]
    fn a_field_moves_with_its_value() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    static final int LIMIT = 10;\n}\n";
        let Ok(plan) = by_id(src, "static final int LIMIT", "pull-up-member") else {
            panic!("expected a plan")
        };
        let out = parses(&plan, src);
        assert!(out.contains("class Base {\n\n    static final int LIMIT = 10;\n}"), "{out}");
    }

    /// An interface will not take a field that is not already a constant: there it would become
    /// `public static final`, and whatever assigned it would stop compiling.
    #[test]
    fn an_interface_refuses_a_field_that_is_not_a_constant() {
        let src = "interface Base {\n}\n\nclass Impl implements Base {\n    int count;\n}\n";
        let Err(refusal) = by_id(src, "int count;", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("public static final"), "{}", refusal.reason);
    }

    /// …and takes one that is.
    #[test]
    fn an_interface_takes_a_constant() {
        let src = "interface Base {\n}\n\nclass Impl implements Base {\n    static final int LIMIT = 10;\n}\n";
        let Ok(plan) = by_id(src, "static final int LIMIT", "pull-up-member") else {
            panic!("expected a plan")
        };
        let out = parses(&plan, src);
        // Implicitly `public static final` there, so the words come off.
        assert!(out.contains("interface Base {\n\n    int LIMIT = 10;\n}"), "{out}");
    }

    /// The body a member came from keeps its own indentation step, and the one it goes to gets its.
    #[test]
    fn a_member_is_re_indented_for_the_body_it_lands_in() {
        let src = "class Host {\n  static class Base {\n  }\n\n  static class Impl extends Base {\n    static int two() {\n      return 2;\n    }\n  }\n}\n";
        let Ok(plan) = by_id(src, "static int two", "pull-up-member") else {
            panic!("expected a plan")
        };
        let out = parses(&plan, src);
        assert!(out.contains("    static int two() {\n      return 2;\n    }"), "{out}");
    }

    /// A member with no javadoc and a blank line above it does not drag the blank line along.
    #[test]
    fn a_comment_a_blank_line_away_stays_behind() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    // A section heading.\n\n    static int two() {\n        return 2;\n    }\n}\n";
        let Ok(plan) = by_id(src, "static int two", "pull-up-member") else {
            panic!("expected a plan")
        };
        let out = parses(&plan, src);
        assert!(out.contains("// A section heading."), "{out}");
        assert!(!out.contains("class Base {\n\n    // A section heading."), "{out}");
    }

    /// A `final` field with no value is assigned by a constructor that stays behind, and a final
    /// field may only be assigned by the class that declares it — no visibility fixes that.
    #[test]
    fn a_final_field_a_constructor_assigns_cannot_be_pulled_up() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    private final int limit;\n\n    Impl(int limit) {\n        this.limit = limit;\n    }\n}\n";
        let Err(refusal) = by_id(src, "private final int limit", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("may only be assigned by the class"), "{}", refusal.reason);
    }

    /// …and one that carries its own value goes up, because nothing else assigns it.
    #[test]
    fn a_final_field_with_a_value_goes_up() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    private final int limit = 10;\n}\n";
        assert!(matches!(by_id(src, "private final int limit", "pull-up-member"), Ok(_)));
    }

    /// The cross-file half checks what the in-file half checks — that divergence wrote a field
    /// with no value into an interface, where it is implicitly `final`.
    #[test]
    fn a_transfer_into_an_interface_refuses_a_field_that_is_not_a_constant() {
        let src = "class Impl implements Base {\n    static int count;\n}\n";
        let Ok(plan) = by_id(src, "static int count", "pull-up-member") else {
            panic!("expected a plan")
        };
        let err = transfer_into(&plan, "interface Base {\n}\n", "Base.java").unwrap_err();
        assert!(err.contains("public static final"), "{err}");
    }

    /// A sibling nested class reaches a member by sharing a top-level body, not by inheritance —
    /// so a pull up takes it out of reach.
    #[test]
    fn a_member_a_sibling_nested_class_calls_cannot_be_pulled_up() {
        let src = "class Host {\n    interface Rule {\n    }\n\n    static class A implements Rule {\n        static int two() {\n            return 2;\n        }\n    }\n\n    static class B {\n        int f() {\n            return two();\n        }\n    }\n}\n";
        let Err(refusal) = by_id(src, "static int two", "pull-up-member") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("not under `A`"), "{}", refusal.reason);
    }

    /// …while a subclass keeps reaching it, which is what a pull up is for.
    #[test]
    fn a_subclass_that_calls_it_does_not_block_a_pull_up() {
        let src = "class Base {\n}\n\nclass Impl extends Base {\n    static int two() {\n        return 2;\n    }\n}\n\nclass Deeper extends Impl {\n    int f() {\n        return two();\n    }\n}\n";
        assert!(matches!(by_id(src, "static int two", "pull-up-member"), Ok(_)));
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
