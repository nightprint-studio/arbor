//! The type a position **wants** — what an IDE means by "smart" completion.
//!
//! ## Why this outranks everything else
//!
//! Every other completion signal is about the candidate: what declares it, how deep it was
//! inherited from, whether this file already mentions it. This one is about the *hole*. In
//!
//! ```java
//! String name = order.
//! ```
//!
//! there are forty members on `order` and exactly the handful returning a `String` can be written
//! there at all. Nothing about `order` says which — the constraint is on the left of the `=`, in a
//! part of the line completion was not looking at.
//!
//! It is also what makes a *second*, stricter gesture meaningful: with the expected type known,
//! "show me only what fits here" is a filter rather than a guess.
//!
//! ## What it answers, and what it does not
//!
//! Four positions, chosen because each is unambiguous from the syntax alone: a declaration with a
//! written type, an assignment to something already typed, a `return`, and a condition (which is
//! a `boolean` and needs no resolving at all).
//!
//! **Not an argument slot.** `f(order.|)` looks like the same question and is not: which parameter
//! type is expected depends on which overload of `f` binds, and that depends on the argument being
//! completed. Answering it needs overload resolution over a half-written call, and a wrong answer
//! there would rank confidently against the truth. Silence is the honest result until then.
//!
//! **Not assignability.** The caller compares by name (see `bennu-query`'s ranking), so a method
//! returning `ArrayList` does not match an expected `List`. It is a *ranking* input — a missed
//! match costs a place in the list, never a candidate — and computing the real rule means walking
//! every candidate's hierarchy on every keystroke.

use crate::seam::{TypeRef, TypeResolver};
use crate::symbols::FileSymbols;
use tree_sitter::Node;

/// The type expected at `byte_offset`, or `None` when the position constrains nothing (or when
/// the constraint is one of the kinds this deliberately does not answer — see the module docs).
///
/// `source` must be a buffer that parses; callers mid-edit splice a placeholder first, exactly as
/// they do for [`crate::scope::visible_bindings`].
pub fn expected_type_at(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
) -> Option<TypeRef> {
    crate::infer::expected_at(root, source, symbols, byte_offset, resolver)
}

/// [`expected_type_at`] for a caller holding the **buffer being edited** — it makes the buffer
/// parse first, and that repair is most of what makes this work at all.
///
/// A half-written statement has no `;` yet, and without one tree-sitter recovers
/// `String s = order.` as a single ERROR node: the declared type, the name and the initializer
/// are all still in the text, and none of them is a `variable_declarator` any more. So the walk
/// finds nothing, on precisely the caret this exists for.
///
/// Two insertions, both at the caret and neither moving anything before it:
///
/// * an identifier, so `order.` is an expression rather than a dangling dot — the same repair
///   member completion makes, for the same reason;
/// * a `;`, but **only when the rest of the line is blank**. That is the "still typing at the end
///   of a line" case, which is the one that fails. Mid-expression — `f(order.|)` — the line
///   already parses and a `;` would be the thing that breaks it.
pub fn expected_type(
    source: &str,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
) -> Option<TypeRef> {
    let mut at = byte_offset.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let rest = &source[at..];
    let line_tail_is_blank = rest
        .split('\n')
        .next()
        .is_some_and(|l| l.trim().is_empty());
    let filler = if line_tail_is_blank { "x;" } else { "x" };
    let buf = format!("{}{filler}{}", &source[..at], rest);

    let tree = crate::grammar::parse_java(&buf)?;
    let symbols = crate::symbols::extract_symbols(&buf);
    expected_type_at(&tree.root_node(), &buf, &symbols, at, resolver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::ClassMembers;
    use crate::symbols::Import;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct Names(HashMap<&'static str, &'static str>);
    impl TypeResolver for Names {
        fn members_of(&self, _b: &str) -> Option<Arc<ClassMembers>> {
            None
        }
        fn resolve_simple_name(&self, n: &str, _i: &[Import]) -> Option<String> {
            self.0.get(n).map(|b| b.to_string())
        }
    }

    fn resolver() -> Names {
        Names(HashMap::from([
            ("String", "java/lang/String"),
            ("Order", "shop/Order"),
        ]))
    }

    fn expected(src: &str, marker: &str) -> Option<String> {
        let offset = src.find(marker).expect("marker");
        expected_type(src, offset, &resolver()).map(|t| t.binary_name)
    }

    #[test]
    fn a_declaration_wants_its_declared_type() {
        let src = "class A { void m() { String s = ZZZ; } }";
        assert_eq!(expected(src, "ZZZ").as_deref(), Some("java/lang/String"));
    }

    /// `var` wants whatever it is given, which is not a constraint — and treating it as one would
    /// rank against a type nobody wrote.
    #[test]
    fn a_var_declaration_wants_nothing() {
        let src = "class A { void m() { var s = ZZZ; } }";
        assert_eq!(expected(src, "ZZZ"), None);
    }

    #[test]
    fn a_return_wants_the_methods_return_type() {
        let src = "class A { String m() { return ZZZ; } }";
        assert_eq!(expected(src, "ZZZ").as_deref(), Some("java/lang/String"));
    }

    /// A lambda declares no return type to read, and reaching past it to the enclosing method's
    /// would constrain the hole with the wrong signature entirely.
    #[test]
    fn a_return_inside_a_lambda_wants_nothing() {
        let src = r#"class A { String m(java.util.List<String> xs) { xs.forEach(x -> { return ZZZ; }); return ""; } }"#;
        assert_eq!(expected(src, "ZZZ"), None);
    }

    #[test]
    fn a_condition_wants_a_boolean() {
        assert_eq!(expected("class A { void m() { if (ZZZ) { } } }", "ZZZ").as_deref(), Some("boolean"));
        assert_eq!(expected("class A { void m() { while (ZZZ) { } } }", "ZZZ").as_deref(), Some("boolean"));
    }

    /// The *body* of an `if` is not its condition. Walking up without asking which half the caret
    /// is in would have every statement inside a loop expect a `boolean`.
    #[test]
    fn the_body_of_a_condition_wants_nothing() {
        let src = "class A { void m(boolean b) { if (b) { ZZZ } } }";
        assert_eq!(expected(src, "ZZZ"), None);
    }

    /// An argument slot needs overload resolution, which this does not do — and a confident wrong
    /// answer would rank against the truth. See the module docs.
    #[test]
    fn an_argument_slot_is_deliberately_unanswered() {
        let src = "class A { void m(java.util.List<String> xs) { xs.add(ZZZ); } }";
        assert_eq!(expected(src, "ZZZ"), None);
    }
}
