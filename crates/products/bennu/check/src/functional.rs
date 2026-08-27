//! Lambda / functional-interface diagnostics. A lambda (or the arity of a method reference) is only
//! legal against a **functional interface** — an interface with exactly one abstract method (its
//! SAM). We check, for a lambda whose target type is written explicitly:
//!
//!   * the target is a functional interface (exactly one abstract method), and
//!   * the lambda's parameter count matches the SAM's.
//!
//! Conservative (docs: never a false positive):
//!   * only the target contexts where the type is written out — a `T x = …`, a `return …`, a
//!     `(T) …` cast. A lambda passed as a method **argument** infers its target through overload
//!     resolution, which we don't model, so it's skipped;
//!   * only when the target is a **known interface** (`is_interface` from bytecode is reliable) whose
//!     hierarchy is fully resolvable. A project interface carries default flags until the symbol
//!     model grows a type-kind, so it's skipped — a conservative miss, never a false positive;
//!   * `java.lang.Object` methods never count toward the SAM (`Comparator.equals` doesn't make it
//!     non-functional).

use std::collections::HashSet;

use bennu_java::prelude::{extract_symbols, FileSymbols, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::inheritance::{is_abstract_requirement, is_ctor, object_method_names};
use crate::nodes::simple_name;
use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known};

/// Parse `source` and flag lambda / functional-interface mismatches.
pub fn functional_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    let nodes = crate::check::collect_nodes(tree.root_node());
    functional_errors_in(&nodes, source, &symbols, resolver)
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`.
pub fn functional_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let objects = object_method_names(resolver);
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "lambda_expression" {
            check_lambda(n, bytes, symbols, resolver, &objects, &mut out);
        }
    }
    out
}

fn check_lambda(
    lambda: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    objects: &HashSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    let Some((target_text, anchor)) = target_type(lambda, bytes) else { return };
    let Some(binary) = type_binary(&target_text, symbols, resolver) else { return };
    let Some(cm) = resolver.members_of(&binary) else { return };
    // Only assert against a genuine, fully-known interface (project types carry default flags).
    if !cm.flags.is_interface || !hierarchy_fully_known(resolver, &binary) {
        return;
    }

    let sam = single_abstract_method(resolver, &binary, objects);
    let name = simple_name(&binary);
    match sam {
        Sam::One { arity } => {
            let got = lambda_param_count(lambda, bytes);
            if got != arity {
                out.push(err(
                    format!(
                        "Lambda has {got} parameter{}, but `{name}`'s abstract method takes {arity}",
                        plural(got)
                    ),
                    anchor,
                ));
            }
        }
        Sam::NotFunctional => {
            out.push(err(format!("`{name}` is not a functional interface"), anchor));
        }
    }
}

enum Sam {
    /// Exactly one abstract method, with this parameter count.
    One { arity: usize },
    /// Zero or more-than-one abstract methods → not a lambda target.
    NotFunctional,
}

/// Determine the SAM of an interface: gather its abstract methods across the hierarchy (excluding
/// `Object` methods), dedup by name. Exactly one → functional with that arity.
fn single_abstract_method(
    resolver: &dyn TypeResolver,
    binary: &str,
    objects: &HashSet<String>,
) -> Sam {
    // name → arity, deduped (a functional interface has no overloaded abstract method).
    let mut abstracts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    // Names some type in the hierarchy IMPLEMENTS. A `default` that overrides an inherited abstract
    // method discharges it — `org.apache.commons.collections4.Predicate` extends
    // `java.util.function.Predicate` and gives `test` a default body, leaving `evaluate` as its only
    // requirement. Counting the inherited `test` as well made it "not a functional interface", and
    // every lambda written against it was reported on code that compiles.
    let mut implemented: HashSet<String> = HashSet::new();
    for_each_supertype(resolver, binary, &mut |_bn, cm| {
        for m in &cm.methods {
            if m.kind != MemberKind::Method || is_ctor(&m.name) || objects.contains(&m.name) {
                continue;
            }
            if is_abstract_requirement(cm, m) {
                abstracts.entry(m.name.clone()).or_insert(m.params.len());
            } else {
                implemented.insert(m.name.clone());
            }
        }
    });
    // Subtract the discharged names — but never down to nothing. An interface reached through this
    // check HAS a lambda written against it, so "zero abstract methods" is a conclusion about our
    // member model, not about the code; keeping the unsubtracted set there leaves the check where it
    // was rather than turning a silent case into a false positive.
    let before = abstracts.clone();
    abstracts.retain(|name, _| !implemented.contains(name));
    if abstracts.is_empty() {
        abstracts = before;
    }
    match abstracts.len() {
        1 => Sam::One { arity: *abstracts.values().next().unwrap() },
        _ => Sam::NotFunctional,
    }
}

/// The written target type of a lambda + the node to anchor a diagnostic on, for the contexts where
/// the target type is explicit: `T x = <lambda>`, `return <lambda>`, `(T) <lambda>`.
fn target_type<'t>(lambda: Node<'t>, bytes: &[u8]) -> Option<(String, Node<'t>)> {
    let parent = lambda.parent()?;
    match parent.kind() {
        // `(T) <lambda>`
        "cast_expression" => {
            let ty = parent.child_by_field_name("type")?;
            Some((ty.utf8_text(bytes).ok()?.to_string(), lambda))
        }
        // `T x = <lambda>` (local var or field) — parent is the declarator, grandparent the decl.
        "variable_declarator" => {
            let decl = parent.parent()?;
            if !matches!(decl.kind(), "local_variable_declaration" | "field_declaration") {
                return None;
            }
            let ty = decl.child_by_field_name("type")?;
            Some((ty.utf8_text(bytes).ok()?.to_string(), lambda))
        }
        // `return <lambda>`
        "return_statement" => {
            let method = enclosing_method(lambda)?;
            let ty = method.child_by_field_name("type")?;
            Some((ty.utf8_text(bytes).ok()?.to_string(), lambda))
        }
        _ => None,
    }
}

/// The nearest enclosing `method_declaration` (stopping at another lambda — a `return` there targets
/// that lambda, not the method).
fn enclosing_method(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        match p.kind() {
            "method_declaration" => return Some(p),
            // The lambda itself is the start; a *different* enclosing lambda means the return isn't ours.
            "lambda_expression" if p.id() != n.id() => return None,
            _ => cur = p.parent(),
        }
    }
    None
}

/// The number of parameters a lambda declares: `a -> …` = 1, `() -> …` = 0, `(a, b) -> …` = 2,
/// `(int a) -> …` = 1.
fn lambda_param_count(lambda: Node, _bytes: &[u8]) -> usize {
    let Some(params) = lambda.child_by_field_name("parameters") else { return 0 };
    match params.kind() {
        "identifier" => 1,
        "inferred_parameters" | "formal_parameters" => {
            let mut c = params.walk();
            let mut n = 0;
            for ch in params.named_children(&mut c) {
                if matches!(ch.kind(), "identifier" | "formal_parameter" | "spread_parameter") {
                    n += 1;
                }
            }
            n
        }
        _ => 0,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn err(message: String, node: Node) -> Diagnostic {
    crate::check_id::CheckId::LambdaArity.at(node, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn m(name: &str, params: usize, is_abstract: bool) -> Member {
        let params = (0..params).map(|_| TypeRef::simple("java/lang/Object")).collect();
        let m = Member::method(name, TypeRef::simple("void"), params);
        if is_abstract {
            m.abstract_()
        } else {
            m
        }
    }

    fn iface(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: None,
            interfaces: Vec::new(),
            methods,
            fields: Vec::new(),
            flags: {
                let mut f = ClassFlags::default();
                f.is_interface = true;
                f
            },
        }
    }

    /// Runnable (run/0), Function-like Fn (apply/1), BiFn (apply/2), NotFn (two abstract), Empty (none).
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), {
            let mut c = iface(vec![m("equals", 1, true), m("hashCode", 0, true)]);
            c.flags.is_interface = false;
            c
        });
        members.insert("com/acme/Run".to_string(), iface(vec![m("run", 0, true)]));
        members.insert("com/acme/Fn".to_string(), iface(vec![m("apply", 1, true)]));
        members.insert("com/acme/BiFn".to_string(), iface(vec![m("apply", 2, true)]));
        members.insert(
            "com/acme/NotFn".to_string(),
            iface(vec![m("a", 0, true), m("b", 1, true)]),
        );
        // A functional interface that also re-declares an Object method (must be ignored).
        members.insert(
            "com/acme/Cmp".to_string(),
            iface(vec![m("compare", 2, true), m("equals", 1, true)]),
        );
        let simple = [
            ("Run", "com/acme/Run"),
            ("Fn", "com/acme/Fn"),
            ("BiFn", "com/acme/BiFn"),
            ("NotFn", "com/acme/NotFn"),
            ("Cmp", "com/acme/Cmp"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ void m() {{ {body} }} }}");
        functional_errors(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn matching_arity_is_ok() {
        assert!(diags("Run r = () -> {};").is_empty());
        assert!(diags("Fn f = x -> x;").is_empty());
        assert!(diags("BiFn f = (a, b) -> a;").is_empty());
    }

    #[test]
    fn wrong_arity_is_flagged() {
        let d = diags("Fn f = (a, b) -> a;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("2 parameters") && d[0].contains("takes 1"), "{d:?}");
    }

    #[test]
    fn zero_vs_one_is_flagged() {
        let d = diags("Fn f = () -> null;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("0 parameters"), "{d:?}");
    }

    #[test]
    fn non_functional_interface_is_flagged() {
        let d = diags("NotFn f = x -> x;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("not a functional interface"), "{d:?}");
    }

    #[test]
    fn object_method_does_not_break_sam() {
        // Cmp has compare/2 + equals (Object) → SAM is compare/2.
        assert!(diags("Cmp c = (a, b) -> 0;").is_empty());
        assert_eq!(diags("Cmp c = a -> 0;").len(), 1);
    }

    #[test]
    fn return_context_is_checked() {
        let src = "class C { Fn make() { return (a, b) -> a; } }";
        let d: Vec<String> = functional_errors(src, &resolver()).into_iter().map(|x| x.message).collect();
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("takes 1"), "{d:?}");
    }

    #[test]
    fn cast_context_is_checked() {
        assert!(diags("Object o = (Run) () -> {};").is_empty());
        assert_eq!(diags("Object o = (Fn) (a, b) -> a;").len(), 1);
    }

    #[test]
    fn unknown_target_is_not_flagged() {
        assert!(diags("Mystery mp = x -> x;").is_empty());
    }
}

#[cfg(test)]
mod default_override_tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct R(HashMap<String, ClassMembers>);
    impl TypeResolver for R {
        fn members_of(&self, b: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(b).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, n: &str, _i: &[Import]) -> Option<String> {
            (n == "Pred").then(|| "p/Pred".to_string())
        }
        fn is_project_type(&self, b: &str) -> bool {
            self.0.contains_key(b)
        }
    }

    /// `p.Pred extends java.util.function.Predicate` and gives `test` a DEFAULT body, so its only
    /// requirement is `evaluate` — one abstract method, a functional interface. Counting the
    /// inherited-and-overridden `test` as a second one is how Apache Commons' `Predicate` came to be
    /// reported as not functional at every lambda written against it.
    #[test]
    fn a_default_override_discharges_the_inherited_abstract_method() {
        let mut m = HashMap::new();
        let mut pred = ClassMembers {
            type_params: vec!["T".to_string()],
            superclass: None,
            interfaces: vec!["java/util/function/Predicate".to_string()],
            methods: vec![
                Member::method("evaluate", TypeRef::simple("boolean"), vec![TypeRef::simple("T")])
                    .abstract_(),
                Member::method("test", TypeRef::simple("boolean"), vec![TypeRef::simple("T")])
                    .default_(),
            ],
            fields: Vec::new(),
            flags: Default::default(),
        };
        pred.flags.is_interface = true;
        m.insert("p/Pred".to_string(), pred);
        let mut jdk = ClassMembers {
            type_params: vec!["T".to_string()],
            superclass: None,
            interfaces: Vec::new(),
            methods: vec![Member::method(
                "test",
                TypeRef::simple("boolean"),
                vec![TypeRef::simple("T")],
            )
            .abstract_()],
            fields: Vec::new(),
            flags: Default::default(),
        };
        jdk.flags.is_interface = true;
        m.insert("java/util/function/Predicate".to_string(), jdk);
        let r = R(m);
        let src = "class A { void f() { Pred<String> p = s -> s.isEmpty(); } }";
        let out: Vec<String> =
            functional_errors(src, &r).into_iter().map(|d| d.message).collect();
        assert!(out.is_empty(), "{out:?}");
    }
}
