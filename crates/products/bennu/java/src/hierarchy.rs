//! Walking a type's supertypes **with its type arguments still attached**.
//!
//! Every consumer that asks "where does this member come from" — inference, find-usages, the
//! Structure panel's inherited bucket, member completion, half the validation checks — walks the
//! same chain, and until this module they each walked it themselves. Seven copies of
//! `stack.push(superclass); stack.extend(interfaces)`, which is seven places to fix a cycle guard
//! and seven chances to disagree about what a type inherits.
//!
//! ## What the copies could not do
//!
//! They pushed **names**. A name arrives at the supertype having forgotten what the subtype passed
//! it: the walk reaches `Range` knowing it declares a `T` and not knowing that this `T` is a
//! `Double`. Every inherited generic member therefore came back as a bare variable — unusable to
//! anything that writes a type down, and the reason `DoubleRange.fit` could not be typed.
//!
//! Now [`ClassMembers::superclass`](crate::seam::ClassMembers::superclass) is a [`TypeRef`], so the
//! arguments written on the `extends` clause survive, and each step of the walk substitutes them
//! into the next: `DoubleRange` → `NumberRange<Double>` → `Range<Double>`, where `T` is finally a
//! name for something.
//!
//! ## The substitution is exact, not a convention
//!
//! [`substitute`] decides that a name is a type variable by finding it in the declaring class's own
//! parameter list — not by its shape. A variable is an identifier, and `record Edit<Source, Param>`
//! is ordinary Java; judging by "one uppercase letter" declines to substitute half the multi-letter
//! variables real code declares, and mistakes a one-letter *class* for a variable.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use crate::seam::{ClassMembers, TypeRef, TypeResolver};

/// One type reached by a hierarchy walk, with the arguments that actually reach it.
pub struct Ancestor {
    /// The type as seen from where the walk started — `Range<Double>`, not `Range<T>`.
    pub ty: TypeRef,
    /// Its resolved member surface.
    pub members: Arc<ClassMembers>,
    /// How far above the starting type this is: `0` for the start itself, `1` for its direct
    /// superclass and interfaces. Breadth-first order means a smaller depth always arrives first,
    /// which is what "the nearest declaration wins" means to a member lookup or a completion rank.
    pub depth: usize,
}

/// `ty` with each type variable declared by `params` replaced by the matching entry of `args`.
///
/// Recursive, because a variable hides inside an argument as readily as at the top: `List<T>` under
/// `T = String` is `List<String>`. An array's depth is kept and added to whatever it stands for, so
/// `T[]` under `T = String` is `String[]` rather than `String`.
///
/// A variable with no matching argument — a raw supertype, or a mismatched arity — is left as it
/// is. That is the honest answer: nothing here knows what it should be, and inventing `Object`
/// would be indistinguishable, to every reader downstream, from a type that really is `Object`.
pub fn substitute(ty: &TypeRef, params: &[String], args: &[TypeRef]) -> TypeRef {
    if let Some(i) = params.iter().position(|p| *p == ty.binary_name) {
        if let Some(arg) = args.get(i) {
            let mut out = arg.clone();
            out.dims += ty.dims;
            return out;
        }
    }
    TypeRef {
        binary_name: ty.binary_name.clone(),
        type_args: ty.type_args.iter().map(|t| substitute(t, params, args)).collect(),
        dims: ty.dims,
    }
}

/// A real class hierarchy is shallow. Past this many visited types the walk gives up **and says so**
/// rather than looping on a pathological or cyclic graph.
pub const MAX_HIER_NODES: usize = 256;

/// What a walk came back with.
pub struct Walk<T> {
    /// Whatever `visit` answered, if anything.
    pub found: Option<T>,
    /// Whether the WHOLE hierarchy was seen.
    ///
    /// `false` when a supertype would not resolve, or the node budget ran out. The distinction is
    /// load-bearing and is why the walk reports it instead of each caller guessing: a check that
    /// concludes "this method does not exist" from an incomplete hierarchy is a false positive on
    /// every project whose classpath is not fully indexed, which early on is all of them.
    pub complete: bool,
}

/// [`walk`], for the callers that only want the answer.
pub fn walk_up<T>(
    resolver: &dyn TypeResolver,
    start: &TypeRef,
    visit: impl FnMut(&Ancestor) -> Option<T>,
) -> Option<T> {
    walk(resolver, start, visit).found
}

/// Walk `start` and everything above it — superclass then interfaces, nearest first — until `visit`
/// answers.
///
/// Breadth-first, which is what a member lookup wants: an override in a nearer type must win over
/// the declaration it hides. A type is visited **once**, keyed by name: a hierarchy may reach the
/// same interface by two routes, and Java forbids reaching it with two different sets of arguments,
/// so the first arrival is the only one. That set is also the cycle guard — a source file is free
/// to write `class A extends B` and `class B extends A`, and the walk has to end anyway.
///
/// A supertype the resolver cannot answer for ends that branch rather than the walk: an unresolved
/// interface should not hide a member that a resolvable superclass declares.
pub fn walk<T>(
    resolver: &dyn TypeResolver,
    start: &TypeRef,
    mut visit: impl FnMut(&Ancestor) -> Option<T>,
) -> Walk<T> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(TypeRef, usize)> = VecDeque::new();
    let mut complete = true;
    queue.push_back((start.clone(), 0));
    while let Some((ty, depth)) = queue.pop_front() {
        if seen.len() > MAX_HIER_NODES {
            return Walk { found: None, complete: false };
        }
        if !seen.insert(ty.binary_name.clone()) {
            continue;
        }
        // An unresolvable supertype ends its branch, not the walk — a missing interface must not
        // hide a member the superclass declares — but the answer is no longer a complete one.
        let Some(members) = resolver.members_of(&ty.binary_name) else {
            complete = false;
            continue;
        };
        let ancestor = Ancestor { ty, members: Arc::clone(&members), depth };
        if let Some(found) = visit(&ancestor) {
            return Walk { found: Some(found), complete };
        }
        let (params, args) = (&ancestor.members.type_params, &ancestor.ty.type_args);
        if let Some(sc) = ancestor.members.superclass.as_ref() {
            queue.push_back((substitute(sc, params, args), depth + 1));
        }
        for itf in &ancestor.members.interfaces {
            queue.push_back((substitute(itf, params, args), depth + 1));
        }
    }
    Walk { found: None, complete }
}

/// The binary names of `start` and everything above it, nearest first.
///
/// For the callers that only ask *which types are in this hierarchy* — a cycle check, an
/// assignability test, "is this an exception type". They get the shared walk's cycle guard and
/// ordering without having to care about the arguments.
pub fn supertype_names(resolver: &dyn TypeResolver, start: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk_up::<()>(resolver, &TypeRef::simple(start), |a| {
        out.push(a.ty.binary_name.clone());
        None
    });
    out
}

/// `start` seen **as** `binary`: that supertype's name carrying the arguments that reach it.
///
/// The question a member lookup has to answer before it can substitute anything. A method declared
/// on `Range<T>` and called on a `DoubleRange` has to be read against `Range<Double>` — the
/// receiver's own name and arguments say nothing about `T`, because `DoubleRange` has none.
///
/// `None` when `binary` is not in the hierarchy at all, which is a different fact from "it is there
/// and raw" and worth keeping apart.
pub fn seen_as(resolver: &dyn TypeResolver, start: &TypeRef, binary: &str) -> Option<TypeRef> {
    walk_up(resolver, start, |a| (a.ty.binary_name == binary).then(|| a.ty.clone()))
}

/// The nearest type in `start`'s hierarchy that declares a member `pick` accepts, and that type
/// seen with its real arguments.
///
/// The shape find-usages needs (which type OWNS this member) and the shape inference needs (what
/// are its arguments there) are the same walk asked for two halves of one answer, so they are one
/// call — two walks that could disagree about what declares a member is the bug that never gets
/// reported, because each looks right on its own.
pub fn declaring(
    resolver: &dyn TypeResolver,
    start: &TypeRef,
    mut pick: impl FnMut(&ClassMembers) -> bool,
) -> Option<TypeRef> {
    walk_up(resolver, start, |a| pick(&a.members).then(|| a.ty.clone()))
}

/// The nearest declaration of a **method** by name, with the type that declares it.
///
/// The single most-asked question in the engine, and the one every copy of the walk was really
/// asking. Kept here so "which type declares `fit`" has one answer, whether the asker is inference
/// wanting to substitute its return type or find-usages wanting to key a reference on its owner.
pub fn declaring_method(
    resolver: &dyn TypeResolver,
    start: &TypeRef,
    name: &str,
) -> Option<TypeRef> {
    declaring(resolver, start, |m| m.methods.iter().any(|x| x.name == name))
}

/// The nearest declaration of a **field** by name, with the type that declares it.
pub fn declaring_field(resolver: &dyn TypeResolver, start: &TypeRef, name: &str) -> Option<TypeRef> {
    declaring(resolver, start, |m| m.fields.iter().any(|x| x.name == name))
}
