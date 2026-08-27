//! How good a member-access completion is *here* — the relevance ordering the popup is shown in.
//!
//! ## Why this is not a detail
//!
//! Completion used to come back sorted by `(kind, label)`: alphabetical, within fields-then-methods.
//! Everything that matched was offered, in an order that knew nothing about the question. Typing
//! `list.` put `add` below `clone`, `equals`, `forEach`, `getClass` and `hashCode`; typing `Color.`
//! offered `wait(long, int)` — which is not even legal on a type — in the same breath as the enum's
//! own constants. The list was correct and useless, because being right about *what* is offered is
//! only half of it: what makes completion feel like it read your mind is that the three things you
//! might have meant are the three at the top.
//!
//! ## The signals, and why these
//!
//! Each one is something the engine already knows for certain. Nothing here is a guess, and nothing
//! here needs a model — this is the ordering an IDE gets from its index, and it is most of what
//! "good completion" turns out to be:
//!
//! - **What the receiver IS.** After `Color.` the statics are the answer and the instance methods
//!   are noise (`Color.equals` does not compile). After `color.` it is the other way round, less
//!   sharply — a static reached through an instance compiles, it is just rarely what you meant.
//! - **How far up the hierarchy it was found.** A method the receiver's own class declares beats one
//!   it inherited, and the further up, the weaker.
//! - **`java.lang.Object`.** Its members match every prefix on every receiver and are almost never
//!   what anyone is reaching for. They sink furthest, which is the single biggest change to how the
//!   list reads.
//! - **Deprecated.** Still offered — it exists, and you may be reading old code — but last.
//!   Only visible on project source: a member decoded from bytecode carries no annotations, so this
//!   sharpens as the code you own, which is the code you edit.
//! - **What this file already uses.** A member you have already written in this buffer is very
//!   likely the one you want again, and the buffer is right there. Capped, so a name used forty
//!   times cannot outrank relevance itself.
//!
//! ## What is deliberately NOT here
//!
//! **The expected type.** Knowing that `String s = order.|` wants something `String`-shaped is the
//! strongest signal an IDE has, and it needs the enclosing expression rather than the receiver —
//! the assignment target, the parameter slot, the return type. That is a real addition to the query
//! and it belongs in its own change.
//!
//! **What you picked last time.** Frequency across a session ranks even better than frequency in a
//! file, and it needs somewhere to remember it plus a verb for the editor to say "this one was
//! accepted". Also its own change.
//!
//! Both are additive: they become terms in [`score`], and everything here keeps its meaning.

use std::collections::HashMap;

use bennu_java::prelude::{Member, MemberKind, Visibility};

/// The binary name whose members match everything and are wanted almost never.
const OBJECT: &str = "java/lang/Object";

/// How many uses in the current buffer can still improve an item's standing. Past this the signal
/// has said what it has to say, and letting it keep climbing would let a much-used `getClass()`
/// outrank the method you are actually looking for.
const MAX_COUNTED_USES: usize = 3;

/// What the ranking knows about the question being asked, gathered once per completion.
pub struct Context {
    /// The receiver is a TYPE name (`Color.`), not a value (`color.`) — so statics are the answer
    /// and instance members are not applicable.
    pub receiver_is_type: bool,
    /// Identifier → how many times it appears in the buffer being edited. See the module docs.
    pub uses: HashMap<String, usize>,
}

impl Context {
    /// Read the buffer once, counting every identifier in it.
    ///
    /// Deliberately not a parse: this is a popularity contest, not an analysis, and a token that
    /// happened to be inside a string or a comment is still evidence of what this file is about.
    /// One pass, and the result is asked O(1) per candidate — scanning per candidate would be the
    /// completion path doing quadratic work on every keystroke.
    pub fn new(source: &str, receiver_is_type: bool) -> Self {
        let mut uses: HashMap<String, usize> = HashMap::new();
        let bytes = source.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
                {
                    i += 1;
                }
                if let Some(word) = source.get(start..i) {
                    *uses.entry(word.to_string()).or_insert(0) += 1;
                }
            } else {
                i += 1;
            }
        }
        Self { receiver_is_type, uses }
    }
}

/// How relevant `m` is, where `declaring` is the binary that declares it and `depth` is how many
/// levels up the hierarchy it was found (`0` = the receiver's own type). Higher is better; the
/// caller sorts descending and breaks ties on the stable `(kind, label)` order.
///
/// The weights are chosen so the classes of signal cannot swap places by accident: being wrong for
/// the receiver dominates hierarchy distance, which dominates familiarity. Within a class the
/// numbers are only ordering, not measurement.
pub fn score(m: &Member, declaring: &str, depth: usize, ctx: &Context) -> i32 {
    let mut s = 0i32;

    // Object's members match every prefix on every receiver. This is the term that changes how the
    // list reads more than any other.
    if declaring == OBJECT {
        s -= 60;
    }

    // Inherited is weaker than declared, and keeps weakening — bounded, so a deep framework
    // hierarchy does not end up ranked purely by shape.
    s -= (depth as i32 * 3).min(30);

    if is_deprecated(m) {
        s -= 40;
    }

    if ctx.receiver_is_type {
        // `Color.RED` and `Color.valueOf(..)` are the question. `Color.equals(..)` is not a program.
        if m.is_static {
            s += 25;
            // A constant is what a type receiver is reached for most often of all.
            if m.kind == MemberKind::Field && m.is_final {
                s += 5;
            }
        } else {
            s -= 35;
        }
    } else if m.is_static {
        // Legal through an instance, and almost always a mistake in the reading — worth demoting,
        // not worth hiding.
        s -= 8;
    }

    // Something this file already says. Weak on its own, decisive between equals.
    if let Some(n) = ctx.uses.get(&m.name) {
        s += 6 * (*n).min(MAX_COUNTED_USES) as i32;
    }

    // A member you cannot see from anywhere else is, where it IS offered, usually your own.
    if m.visibility == Visibility::Private {
        s += 2;
    }

    s
}

/// Whether the member is marked `@Deprecated`.
///
/// Source-only, and knowingly: a member decoded from a class file carries no annotations through
/// the seam, so this never fires for a JDK or dependency member. The effect is that the signal is
/// sharpest on the code you own — which is the code you are editing.
///
/// Public because the completion item is flagged with the same answer that demoted it — the popup
/// draws it struck through, and the two must never disagree about which member that is.
pub fn is_deprecated(m: &Member) -> bool {
    m.annotations.iter().any(|a| a.name == "Deprecated")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::TypeRef;

    fn method(name: &str) -> Member {
        Member::method(name, TypeRef::simple("void"), Vec::new())
    }

    fn ctx(receiver_is_type: bool) -> Context {
        Context { receiver_is_type, uses: HashMap::new() }
    }

    /// The headline case: `list.` should not open on `clone`, `equals` and `getClass`.
    #[test]
    fn an_objects_own_method_beats_an_inherited_object_one() {
        let own = score(&method("add"), "java/util/ArrayList", 0, &ctx(false));
        let inherited = score(&method("getClass"), OBJECT, 3, &ctx(false));
        assert!(own > inherited, "{own} should beat {inherited}");
    }

    /// `Color.wait(..)` is not a program, and it used to be offered beside the constants.
    #[test]
    fn a_type_receiver_puts_statics_above_instance_members() {
        let c = ctx(true);
        let constant = score(&Member::field("RED", TypeRef::simple("Color")).stat().final_(), "Color", 0, &c);
        let value_of = score(&method("valueOf").stat(), "Color", 0, &c);
        let name = score(&method("name"), "java/lang/Enum", 1, &c);
        assert!(constant > value_of, "a constant is what a type receiver is reached for");
        assert!(value_of > name, "{value_of} (static) should beat {name} (instance)");
    }

    /// The other way round: through a value, a static is unusual rather than wrong.
    #[test]
    fn a_value_receiver_demotes_statics_only_mildly() {
        let c = ctx(false);
        let instance = score(&method("size"), "java/util/ArrayList", 0, &c);
        let static_one = score(&method("copyOf").stat(), "java/util/ArrayList", 0, &c);
        assert!(instance > static_one);
        // Mildly: still far above the inherited Object noise.
        assert!(static_one > score(&method("hashCode"), OBJECT, 3, &c));
    }

    #[test]
    fn deprecated_sinks_but_is_still_offered() {
        let mut old = method("legacyName");
        old.annotations.push(bennu_java::prelude::Annotation {
            name: "Deprecated".into(),
            qualified: "Deprecated".into(),
            start: 0,
            end: 0,
            strings: Vec::new(),
            args: Vec::new(),
            positional: Vec::new(),
        });
        let c = ctx(false);
        assert!(score(&old, "com/acme/Order", 0, &c) < score(&method("name"), "com/acme/Order", 0, &c));
    }

    /// Familiarity decides between members that are otherwise equal, and cannot do more than that.
    #[test]
    fn use_in_this_file_breaks_a_tie_without_overturning_relevance() {
        let mut c = ctx(false);
        c.uses.insert("getStatus".to_string(), 9);
        let used = score(&method("getStatus"), "com/acme/Order", 0, &c);
        let unused = score(&method("getState"), "com/acme/Order", 0, &c);
        assert!(used > unused, "a name this file already uses comes first");
        // But not enough to lift an Object method over a declared one.
        c.uses.insert("toString".to_string(), 9);
        let familiar_noise = score(&method("toString"), OBJECT, 2, &c);
        assert!(unused > familiar_noise, "familiarity must not outrank relevance");
    }

    /// The counter is a scan, not a parse — but it must at least agree with itself.
    #[test]
    fn identifiers_are_counted_once_each_occurrence() {
        let c = Context::new("order.getStatus(); if (getStatus() == 1) { order.x; }", false);
        assert_eq!(c.uses.get("getStatus"), Some(&2));
        assert_eq!(c.uses.get("order"), Some(&2));
        assert_eq!(c.uses.get("nothing"), None);
    }
}
