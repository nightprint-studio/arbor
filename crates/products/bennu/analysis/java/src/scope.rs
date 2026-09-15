//! What the lexical scope at a caret **binds** — the names a bare identifier there could mean.
//!
//! ## Why this is its own question
//!
//! Everything else in [`crate::infer`] answers *what type does this name have*: the caret names
//! something, and the walk goes looking for it. Completion asks the opposite question — *what
//! names are there at all* — and no amount of resolution answers it, because there is nothing
//! written yet to resolve.
//!
//! Without it, a bare prefix has no semantic answer: `ord|` cannot become the local `order`, and
//! whatever the editor shows there comes from scanning the buffer for words that look similar.
//! Which is what an editor with no index does — and the point of having one is that the three
//! names you might have meant are the three at the top, not that the right answer is somewhere
//! in a list of everything spelled like it.
//!
//! ## What counts as a binding
//!
//! Everything the *scope chain* introduces, and nothing a *type* does: locals, method and
//! constructor parameters, lambda parameters (typed or not), catch parameters, the
//! enhanced-`for` variable, try-with-resources, and pattern variables (`o instanceof Foo f`).
//! Fields and methods are members of the enclosing type and are reached by walking it — a
//! different walk, with inheritance and visibility rules of its own, which the caller does.
//!
//! The walk is [`crate::infer`]'s own scope chain, asked to enumerate instead of to match, so a
//! name is visible here **exactly when** resolving it there would have found it. Two walks would
//! be two opinions about shadowing, and the one nobody tests is the one that is wrong.

use crate::seam::{TypeRef, TypeResolver};
use crate::symbols::FileSymbols;
use tree_sitter::Node;

/// A name bound by the lexical scope at a caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The identifier as written.
    pub name: String,
    /// A method / lambda / catch parameter rather than a local variable. Kept apart because it
    /// ranks differently: a parameter is what the method was handed, and at the top of a body it
    /// is almost always what is being reached for.
    pub is_parameter: bool,
    /// How far out the binding was declared, counted in enclosing nodes. Only the **order** is
    /// meaningful — it is not a scope count, and the absolute number depends on how the grammar
    /// nests the statement the caret is in. The nearer binding wins on both counts: it shadows,
    /// and it is the one you just wrote.
    pub depth: usize,
    /// Where the binding was declared, as a byte offset into the buffer. Two names bound in the
    /// same block are equally deep and not equally near: the one declared on the line above is the
    /// one being reached for, and this is the only thing that says so.
    pub decl: usize,
    /// The binding's type, when Phase-1 can name one. `None` for a `var` whose initializer does
    /// not resolve, for an untyped lambda parameter, and for a multi-catch union — all cases
    /// where the name is real and the type is not something to state.
    pub ty: Option<TypeRef>,
}

/// Every name visible at `byte_offset`, innermost scope first, shadowed names dropped.
///
/// `source` must be a buffer that **parses**: a caret sitting on a half-written identifier is
/// fine, a caret after a bare `.` is not. Callers in the middle of an edit splice a placeholder
/// first — the same repair member completion makes — and pass the offset unchanged, since an
/// insertion at the caret moves nothing before it.
/// Whether a bare name at `byte_offset` is being written in a **static** context — inside a
/// `static` method or a `static { … }` initializer, where an instance member of the enclosing
/// class cannot be named without a receiver (JLS §8.4.3.2).
///
/// Deliberately NOT the same question `bennu-check`'s `static_access` asks. That one is a
/// validator guard and answers `false` for anything it is unsure of — a lambda body, a nested
/// type — because a wrong `true` there is a false positive on working code. Here a wrong `false`
/// only offers a member that will not compile, and a lambda inside a static method IS still a
/// static context, so the two want opposite defaults. Merging them would make one of the two wrong.
pub fn caret_is_static(root: &Node, source: &str, byte_offset: usize) -> bool {
    let at = byte_offset.min(source.len());
    let Some(node) = root.named_descendant_for_byte_range(at, at) else {
        return false;
    };
    let mut cur = Some(node);
    while let Some(p) = cur {
        match p.kind() {
            // The nearest enclosing executable scope decides, and a lambda is not one: it runs
            // with whatever `this` its enclosing method had.
            "method_declaration" => return has_static_modifier(&p),
            "constructor_declaration" => return false,
            "static_initializer" => return true,
            // A field initializer belongs to the field: `static int n = …` is a static context and
            // `int n = …` is not.
            "field_declaration" => return has_static_modifier(&p),
            // An anonymous / local / nested class body starts a new instance context of its own.
            "class_body" | "enum_body" => return false,
            _ => {}
        }
        cur = p.parent();
    }
    false
}

/// Whether a declaration node carries the `static` keyword. The keyword is an ANONYMOUS token
/// inside `modifiers`, so a named-children scan never sees it.
fn has_static_modifier(decl: &Node) -> bool {
    let mut c = decl.walk();
    for ch in decl.children(&mut c) {
        if ch.kind() != "modifiers" {
            continue;
        }
        let mut mc = ch.walk();
        for m in ch.children(&mut mc) {
            if !m.is_named() && m.kind() == "static" {
                return true;
            }
        }
    }
    false
}

pub fn visible_bindings(
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    byte_offset: usize,
    resolver: &dyn TypeResolver,
) -> Vec<Binding> {
    crate::infer::bindings_at(root, source, symbols, byte_offset, resolver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::ClassMembers;
    use crate::symbols::Import;
    use std::sync::Arc;

    /// A resolver that resolves nothing. Every assertion here is about which NAMES are visible,
    /// which is decided by the parse and the scope chain alone — a resolver would only decide
    /// whether `ty` comes back filled in.
    struct NoTypes;
    impl TypeResolver for NoTypes {
        fn members_of(&self, _b: &str) -> Option<Arc<ClassMembers>> {
            None
        }
        fn resolve_simple_name(&self, _n: &str, _i: &[Import]) -> Option<String> {
            None
        }
    }

    /// The names visible where `marker` appears in `src` (the marker itself is not removed — it
    /// stands in for the half-written identifier a real caret sits on).
    fn names_at(src: &str, marker: &str) -> Vec<String> {
        let offset = src.find(marker).expect("marker not in source");
        let tree = crate::grammar::parse_java(src).expect("parse");
        let symbols = crate::symbols::extract_symbols(src);
        let mut names: Vec<String> =
            visible_bindings(&tree.root_node(), src, &symbols, offset, &NoTypes)
                .into_iter()
                .map(|b| b.name)
                .collect();
        names.sort();
        names
    }

    #[test]
    fn parameters_and_locals_are_visible() {
        let src = r#"
class A {
  void go(String name, int n) {
    var total = n * 2;
    ZZZ
  }
}
"#;
        assert_eq!(names_at(src, "ZZZ"), vec!["n", "name", "total"]);
    }

    /// The whole reason this shares `Ctx::scope_locals`: a local declared after the caret is not
    /// in scope at it, and offering it means offering the name being typed as its own completion.
    #[test]
    fn a_local_declared_below_the_caret_is_not_visible_yet() {
        let src = r#"
class A {
  void go() {
    int before = 1;
    ZZZ
    int after = 2;
  }
}
"#;
        assert_eq!(names_at(src, "ZZZ"), vec!["before"]);
    }

    /// A lambda binds names with nothing written to resolve. They are still bindings — omitting
    /// them omits exactly the names the lambda introduced.
    #[test]
    fn untyped_lambda_parameters_are_bindings() {
        let src = r#"
class A {
  void go(java.util.List<String> xs) {
    xs.forEach(item -> { ZZZ });
  }
}
"#;
        assert!(names_at(src, "ZZZ").contains(&"item".to_string()));
    }

    #[test]
    fn a_two_parameter_lambda_binds_both() {
        let src = r#"
class A {
  void go(java.util.Map<String,String> m) {
    m.forEach((k, v) -> { ZZZ });
  }
}
"#;
        let names = names_at(src, "ZZZ");
        assert!(names.contains(&"k".to_string()) && names.contains(&"v".to_string()), "{names:?}");
    }

    #[test]
    fn the_for_each_variable_the_catch_parameter_and_the_resource_are_all_bindings() {
        let src = r#"
class A {
  void go(java.util.List<String> xs) {
    for (String row : xs) {
      try (java.io.InputStream in = open()) {
        ZZZ
      } catch (java.io.IOException e) {
      }
    }
  }
}
"#;
        let names = names_at(src, "ZZZ");
        assert!(names.contains(&"row".to_string()), "{names:?}");
        assert!(names.contains(&"in".to_string()), "{names:?}");
    }

    #[test]
    fn a_pattern_variable_is_visible_in_the_branch_it_governs() {
        let src = r#"
class A {
  void go(Object o) {
    if (o instanceof String s) {
      ZZZ
    }
  }
}
"#;
        assert!(names_at(src, "ZZZ").contains(&"s".to_string()));
    }

    /// Shadowing is not de-duplication: the inner binding is the one that is visible, and the
    /// outer one is not offered at all. Same rule the resolver applies when it matches a name.
    #[test]
    fn an_inner_binding_shadows_the_outer_one_and_is_offered_once() {
        let src = r#"
class A {
  void go(String value) {
    java.util.List<String> xs = null;
    xs.forEach(value -> { ZZZ });
  }
}
"#;
        let all = {
            let offset = src.find("ZZZ").unwrap();
            let tree = crate::grammar::parse_java(src).unwrap();
            let symbols = crate::symbols::extract_symbols(src);
            visible_bindings(&tree.root_node(), src, &symbols, offset, &NoTypes)
        };
        let values: Vec<&Binding> = all.iter().filter(|b| b.name == "value").collect();
        assert_eq!(values.len(), 1, "{all:?}");
        assert!(values[0].is_parameter, "the lambda's parameter is the one in scope, not the outer one");
    }

    /// Depth is what makes the nearest binding rank first — a completion that ordered a field of
    /// the outermost method above the variable declared on the line above reads as random.
    #[test]
    fn depth_grows_outwards_from_the_caret() {
        let src = r#"
class A {
  void go(String outer) {
    java.util.List<String> xs = null;
    xs.forEach(inner -> { ZZZ });
  }
}
"#;
        let offset = src.find("ZZZ").unwrap();
        let tree = crate::grammar::parse_java(src).unwrap();
        let symbols = crate::symbols::extract_symbols(src);
        let bs = visible_bindings(&tree.root_node(), src, &symbols, offset, &NoTypes);
        let d = |n: &str| bs.iter().find(|b| b.name == n).map(|b| b.depth);
        assert!(d("inner") < d("outer"), "{bs:?}");
    }

    /// A caret outside any method has nothing bound, and must not panic reaching for it.
    #[test]
    fn a_caret_in_a_class_body_binds_nothing() {
        let src = "class A {\n  ZZZ\n}\n";
        assert!(names_at(src, "ZZZ").is_empty());
    }

    fn is_static_at(src: &str, marker: &str) -> bool {
        let offset = src.find(marker).expect("marker not in source");
        let tree = crate::grammar::parse_java(src).expect("parse");
        caret_is_static(&tree.root_node(), src, offset)
    }

    #[test]
    fn a_static_method_body_is_a_static_context() {
        assert!(is_static_at("class A { static void m() { ZZZ } }", "ZZZ"));
        assert!(!is_static_at("class A { void m() { ZZZ } }", "ZZZ"));
    }

    /// A lambda runs with the `this` of the method around it — which in a `static` method is none.
    /// This is the case `bennu-check` deliberately answers the other way; see `caret_is_static`.
    #[test]
    fn a_lambda_inside_a_static_method_is_still_static() {
        let src = "class A { static void m(java.util.List<String> xs) { xs.forEach(x -> { ZZZ }); } }";
        assert!(is_static_at(src, "ZZZ"));
    }

    #[test]
    fn a_constructor_and_an_instance_initializer_are_not() {
        assert!(!is_static_at("class A { A() { ZZZ } }", "ZZZ"));
        assert!(is_static_at("class A { static { ZZZ } }", "ZZZ"));
    }

    #[test]
    fn an_offset_past_the_end_does_not_panic() {
        let src = "class A {}";
        let tree = crate::grammar::parse_java(src).unwrap();
        let symbols = crate::symbols::extract_symbols(src);
        let _ = visible_bindings(&tree.root_node(), src, &symbols, 9_999, &NoTypes);
    }
}
