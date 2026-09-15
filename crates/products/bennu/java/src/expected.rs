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
//! **A type, not a rule.** This answers what the hole is typed as, generic arguments included —
//! `List<Order>`, not `List`. Whether a candidate fits it (the exact type, its box, a subtype) is
//! the caller's question: see `bennu-query`'s `rank::Fit`, which walks the hierarchy once per
//! produced type. It stays a *ranking* input — a missed match costs a place in the list, never a
//! candidate.
//!
//! A `return` inside a lambda BODY answers to the lambda, not to the method around it: the
//! functional interface's return, when the lambda's target type resolves to one abstract method.

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

/// The shape of the functional interface the expression being written at `byte_offset` is passed
/// to — the one question an argument slot CAN answer without overload resolution.
///
/// [`expected_type`] declines argument slots on purpose, because which parameter is meant depends on
/// which overload binds. A functional parameter is the exception that makes completion inside one
/// worth having: `opt.map(|)` has one `map`, its parameter is a `Function<? super T, ? extends U>`,
/// and after the receiver's `T` is substituted the function **receives** a `ResolvedIdentity`. That
/// is the type the user is about to name — `map(ResolvedIdentity::identifier)`, or a lambda whose
/// parameter is one — and nothing else on the classpath is anywhere near as likely.
///
/// Answers for three carets, all of them "an expression is being written where a function goes":
///
/// * a bare word, or nothing at all, directly in the argument list — `map(Re|)`, `map(|)`;
/// * the member half of a method reference — `map(ResolvedIdentity::|)`, `map(ResolvedIdentity::re|)`;
/// * the initializer of a declared variable and a `return`, which the descriptor reads the same way.
///
/// `None` everywhere else, and wherever [`crate::infer::functional_descriptor`] refuses to guess (an
/// overloaded callee, an interface with more than one abstract method, a hierarchy that does not
/// resolve).
///
/// The buffer is repaired the way [`expected_type`] repairs it — an identifier at the caret — and,
/// when the rest of the line is blank, with the `)` and `;` a half-written call has not been given
/// yet. Each repair is tried in turn, cheapest first, because each one breaks a line the previous
/// one already parsed.
pub fn functional_descriptor_at(
    source: &str,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
) -> Option<crate::infer::FunctionalDescriptor> {
    let mut at = byte_offset.min(source.len());
    while at > 0 && !source.is_char_boundary(at) {
        at -= 1;
    }
    let rest = &source[at..];
    let line_tail_is_blank = rest
        .split('\n')
        .next()
        .is_some_and(|l| l.trim().is_empty());
    let fillers: &[&str] = if line_tail_is_blank { &["x", "x)", "x);"] } else { &["x"] };
    fillers.iter().find_map(|filler| {
        let buf = format!("{}{filler}{}", &source[..at], rest);
        descriptor_of_slot(&buf, at, resolver)
    })
}

/// [`functional_descriptor_at`] over a buffer already repaired so that `at` sits inside an
/// identifier.
fn descriptor_of_slot(
    buf: &str,
    at: usize,
    resolver: &dyn TypeResolver,
) -> Option<crate::infer::FunctionalDescriptor> {
    let tree = crate::grammar::parse_java(buf)?;
    let root = tree.root_node();
    // `[at, at + 1)` is the spliced identifier character, so the leaf found is the word being
    // written — never the `(` or `::` in front of it, which an empty range at `at` could return.
    let mut node = root.named_descendant_for_byte_range(at, at + 1)?;
    loop {
        let parent = node.parent()?;
        let is_slot = match parent.kind() {
            "argument_list" | "return_statement" => true,
            // The initializer, not the NAME: `Function<A, B> f|` is a declaration being written, and
            // offering `A` there would be answering a question about a different hole.
            "variable_declarator" => {
                parent.child_by_field_name("value").map(|v| v.id()) == Some(node.id())
            }
            _ => false,
        };
        if is_slot {
            let symbols = crate::symbols::extract_symbols(buf);
            let cache = crate::infer::InferCache::new();
            return crate::infer::functional_descriptor(&root, buf, &symbols, &node, resolver, &cache);
        }
        // Only the two shapes that ARE the expression being written. Climbing further — through a
        // field access, a call, a cast — would describe the slot some enclosing expression sits in.
        if !matches!(node.kind(), "identifier" | "method_reference") {
            return None;
        }
        node = parent;
    }
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

    /// Reaching past a lambda to the enclosing method would constrain the hole with the wrong
    /// signature entirely — here `String`, where the `forEach` consumer returns nothing. When the
    /// lambda's interface cannot be read, the answer is nothing rather than the method's type.
    #[test]
    fn a_return_inside_an_unresolved_lambda_wants_nothing() {
        let src = r#"class A { String m(java.util.List<String> xs) { xs.forEach(x -> { return ZZZ; }); return ""; } }"#;
        assert_eq!(expected(src, "ZZZ"), None);
    }

    /// A resolver that also knows one functional interface: `interface Maker { Order make(); }`.
    struct WithMaker(Names);
    impl TypeResolver for WithMaker {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            if binary != "shop/Maker" {
                return None;
            }
            let mut make = crate::seam::Member::method("make", TypeRef::simple("shop/Order"), Vec::new());
            make.is_abstract = true;
            Some(Arc::new(ClassMembers {
                superclass: None,
                interfaces: Vec::new(),
                methods: vec![make],
                fields: Vec::new(),
                flags: crate::seam::ClassFlags { is_interface: true, ..Default::default() },
                type_params: Vec::new(),
            }))
        }
        fn resolve_simple_name(&self, n: &str, i: &[Import]) -> Option<String> {
            if n == "Maker" {
                return Some("shop/Maker".to_string());
            }
            self.0.resolve_simple_name(n, i)
        }
    }

    fn expected_with_maker(src: &str, offset: usize) -> Option<String> {
        expected_type(src, offset, &WithMaker(resolver())).map(|t| t.binary_name)
    }

    /// `Maker m = () -> { return | };` — the lambda returns to `Maker.make`, which returns `Order`,
    /// not to the `String` method it is written in.
    #[test]
    fn a_return_inside_a_lambda_wants_what_its_interface_returns() {
        let src = "class A { String m() { Maker k = () -> { return ZZZ; }; return \"\"; } }";
        let offset = src.find("ZZZ").expect("marker");
        assert_eq!(expected_with_maker(src, offset).as_deref(), Some("shop/Order"));
    }

    /// The user's caret: `return builder.` with nothing after the dot and no `;` yet.
    #[test]
    fn a_half_written_member_access_after_return_wants_the_return_type() {
        let src = "class A {\n    Order create(Order builder) {\n        return builder.\n    }\n}\n";
        let offset = src.find("builder.\n").expect("marker") + "builder.".len();
        assert_eq!(expected_type(src, offset, &resolver()).map(|t| t.binary_name).as_deref(), Some("shop/Order"));
    }

    /// `return Ra|` — a class name being started, the line not finished.
    #[test]
    fn a_half_written_type_name_after_return_wants_the_return_type() {
        let src = "class A {\n    Order create() {\n        return Ra\n    }\n}\n";
        let offset = src.find("return Ra").expect("marker") + "return Ra".len();
        assert_eq!(expected_type(src, offset, &resolver()).map(|t| t.binary_name).as_deref(), Some("shop/Order"));
    }

    /// The generic arguments are part of the answer: a `List<Order>` is not any `List`.
    #[test]
    fn a_generic_return_type_keeps_its_arguments() {
        let names = Names(HashMap::from([("List", "java/util/List"), ("Order", "shop/Order")]));
        let src = "class A { List<Order> m() { return ZZZ; } }";
        let t = expected_type(src, src.find("ZZZ").unwrap(), &names).expect("an expected type");
        assert_eq!(t.binary_name, "java/util/List");
        assert_eq!(t.type_args.first().map(|a| a.binary_name.as_str()), Some("shop/Order"));
    }

    #[test]
    fn a_primitive_return_wants_the_primitive() {
        let src = "class A { int m() { return ZZZ; } }";
        assert_eq!(expected(src, "ZZZ").as_deref(), Some("int"));
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
