//! `bennu-java` — the Java source model.
//!
//! Role (docs §2): parse `.java` with **tree-sitter-java**, extract symbols, and — the
//! hard, homegrown piece (docs §10) — do **local type-inference** good enough for
//! member-access completion. Spike B said GO for homegrown: nominal type-walks over
//! the bytecode member index (`bennu-classpath`) rather than compiler-grade inference.
//!
//! Two entry points, both re-exported from the [`prelude`]:
//!   * [`extract_symbols`](symbols::extract_symbols) → the structural model
//!     ([`FileSymbols`](symbols::FileSymbols): package, imports, type decls).
//!   * [`infer_receiver_type`](infer::infer_receiver_type) → the static type of the
//!     expression left of the `.` at a caret, resolved against a [`TypeResolver`].
//!
//! The [`TypeResolver`](seam::TypeResolver) trait + the [`TypeRef`](seam::TypeRef) /
//! [`ClassMembers`](seam::ClassMembers) shapes are the shared Bennu seam (docs §10);
//! `bennu-intel` unifies them with `bennu-classpath`'s member index at the boundary.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_java::prelude::...`.

pub mod ast;
pub mod grammar;
pub mod hierarchy;
pub mod import_hint;
pub mod infer;
pub mod prelude;
pub mod scaffold;
pub mod seam;
pub mod spans;
pub mod static_import;
pub mod symbols;
pub mod typename;
pub mod typeparse;

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use std::collections::HashMap;

    /// A hard-coded resolver for a couple of JDK types, to drive inference tests.
    #[derive(Default)]
    struct FakeResolver {
        classes: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl FakeResolver {
        fn jdk() -> Self {
            let mut r = FakeResolver::default();

            r.classes.insert(
                "java/lang/String".into(),
                ClassMembers {
                    type_params: Vec::new(),
                    superclass: Some(crate::seam::TypeRef::simple("java/lang/Object")),
                    interfaces: vec![],
                    methods: vec![
                        m("length", tr("int")),
                        m("toUpperCase", tr("java/lang/String")),
                        m("trim", tr("java/lang/String")),
                        m("charAt", tr("char")),
                    ],
                    fields: vec![],
                    flags: Default::default(),
                },
            );

            // java/util/List<E> — get(int) -> E, iterator() -> Iterator<E>
            r.classes.insert(
                "java/util/List".into(),
                ClassMembers {
                    type_params: vec!["E".into()],
                    superclass: None,
                    interfaces: vec![crate::seam::TypeRef::simple("java/util/Collection")],
                    methods: vec![
                        m("get", TypeRef::simple("E")),
                        m(
                            "iterator",
                            gen("java/util/Iterator", vec![TypeRef::simple("E")]),
                        ),
                        m("size", tr("int")),
                    ],
                    fields: vec![],
                    flags: Default::default(),
                },
            );

            // java/util/Iterator<E> — next() -> E
            r.classes.insert(
                "java/util/Iterator".into(),
                ClassMembers {
                    type_params: vec!["E".into()],
                    superclass: None,
                    interfaces: vec![],
                    methods: vec![m("next", TypeRef::simple("E"))],
                    fields: vec![],
                    flags: Default::default(),
                },
            );

            // java/util/Map<K,V> — get(K) -> V
            r.classes.insert(
                "java/util/Map".into(),
                ClassMembers {
                    type_params: vec!["K".into(), "V".into()],
                    superclass: None,
                    interfaces: vec![],
                    methods: vec![m("get", TypeRef::simple("V"))],
                    fields: vec![],
                    flags: Default::default(),
                },
            );

            // A generic pair with NON-conventional parameter names: Pair<X, Y> — left() -> X,
            // right() -> Y. Proves the exact positional substitution (from the declared type-param
            // list), which the naming-convention heuristic alone can't do for `X`/`Y`.
            r.classes.insert(
                "com/acme/Pair".into(),
                ClassMembers {
                    type_params: vec!["X".into(), "Y".into()],
                    superclass: Some(crate::seam::TypeRef::simple("java/lang/Object")),
                    interfaces: vec![],
                    methods: vec![
                        m("left", TypeRef::simple("X")),
                        m("right", TypeRef::simple("Y")),
                    ],
                    fields: vec![],
                    flags: Default::default(),
                },
            );

            // A domain type with a getter: Customer.getName() -> String
            r.classes.insert(
                "com/acme/Customer".into(),
                ClassMembers {
                    type_params: Vec::new(),
                    superclass: Some(crate::seam::TypeRef::simple("java/lang/Object")),
                    interfaces: vec![],
                    methods: vec![m("getName", tr("java/lang/String"))],
                    fields: vec![],
                    flags: Default::default(),
                },
            );

            for (s, b) in [
                ("String", "java/lang/String"),
                ("List", "java/util/List"),
                ("Map", "java/util/Map"),
                ("Iterator", "java/util/Iterator"),
                ("Customer", "com/acme/Customer"),
                ("Pair", "com/acme/Pair"),
                ("Object", "java/lang/Object"),
            ] {
                r.simple.insert(s.into(), b.into());
            }
            r
        }
    }

    impl TypeResolver for FakeResolver {
        fn members_of(&self, binary_name: &str) -> Option<std::sync::Arc<ClassMembers>> {
            self.classes
                .get(binary_name)
                .cloned()
                .map(std::sync::Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn tr(bn: &str) -> TypeRef {
        TypeRef::simple(bn)
    }
    fn gen(bn: &str, args: Vec<TypeRef>) -> TypeRef {
        TypeRef {
            binary_name: bn.into(),
            type_args: args,
            dims: 0,
        }
    }
    fn m(name: &str, ret: TypeRef) -> Member {
        Member::method(name, ret, vec![]).sig(String::new())
    }

    /// Byte offset just after the LAST `.` in `src`.
    fn caret_after_last_dot(src: &str) -> usize {
        src.rfind('.').map(|i| i + 1).expect("no dot")
    }

    fn infer(src: &str) -> TypeRef {
        let off = caret_after_last_dot(src);
        infer_receiver_type(src, off, &FakeResolver::jdk()).expect("inference should resolve")
    }

    #[test]
    fn typed_local_then_dot() {
        let src = r#"package com.acme; class Foo { void run() { String s = "hi"; s. } }"#;
        assert_eq!(infer(src).binary_name, "java/lang/String");
    }

    #[test]
    fn method_param_then_dot() {
        let src = r#"package com.acme; class Foo { void run(String name) { name. } }"#;
        assert_eq!(infer(src).binary_name, "java/lang/String");
    }

    #[test]
    fn two_hop_getter_chain() {
        let src = r#"package com.acme; class Foo { void run() { String s = "x"; s.trim(). } }"#;
        assert_eq!(infer(src).binary_name, "java/lang/String");
    }

    #[test]
    fn domain_getter_chain() {
        let src = r#"package com.acme; class Foo { void run(Customer customer) { customer.getName(). } }"#;
        assert_eq!(infer(src).binary_name, "java/lang/String");
    }

    #[test]
    fn list_generic_get_element() {
        let src = r#"package com.acme; import java.util.List; class Foo { void run() { List<Customer> list = null; list.get(0). } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn list_iterator_next_element() {
        let src = r#"package com.acme; import java.util.List; class Foo { void run() { List<Customer> list = null; list.iterator().next(). } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn map_generic_get_value() {
        let src = r#"package com.acme; import java.util.Map; class Foo { void run() { Map<String, Customer> m = null; m.get("k"). } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn pair_second_type_param_by_declared_position() {
        // `Pair<X, Y>.right() -> Y` with NON-conventional param names: only the declared type-param
        // list (`["X","Y"]`) resolves `right()` to the 2nd argument. The naming heuristic can't.
        let src = r#"package com.acme; class Foo { void run() { Pair<Customer, String> p = null; p.right(). } }"#;
        assert_eq!(infer(src).binary_name, "java/lang/String");
    }

    #[test]
    fn pair_first_type_param_by_declared_position() {
        // `Pair<X, Y>.left() -> X` → the 1st argument (Customer).
        let src = r#"package com.acme; class Foo { void run() { Pair<Customer, String> p = null; p.left(). } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn this_field_access() {
        let src =
            r#"package com.acme; class Foo { private Customer bar; void run() { this.bar. } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn try_with_resources_var_infers_from_initializer() {
        // `try (var c = seed)` — a `var` resource is a local visible in the try body; its type is
        // inferred from the initializer, so `c.` resolves. (Regression: resources weren't scanned.)
        let src = r#"package com.acme; class Foo { void run(Customer seed) { try (var c = seed) { c. } } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn try_with_resources_typed_resource_resolves() {
        // A conventionally-typed resource is visible too.
        let src =
            r#"package com.acme; class Foo { void run() { try (Customer c = null) { c. } } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn bare_field_access() {
        let src = r#"package com.acme; class Foo { private Customer bar; void run() { bar. } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Customer");
    }

    #[test]
    fn local_of_same_file_type_without_resolver_hint() {
        // `Order` is declared in THIS file but the resolver has NO simple→binary hint for
        // it (only `Customer`/JDK types are seeded). The same-file `symbols.types` fallback
        // must bind `Order` -> its package-qualified binary name so a local of it resolves.
        let src = r#"package com.acme;
            class Order { int total; }
            class Repo { void run() { Order o = new Order(); o. } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Order");
    }

    #[test]
    fn nested_type_local_resolves_to_qualified_fqn() {
        // A nested type's FQN (`com.acme.Outer.Inner`) comes off the extracted symbols; a
        // local of it must bind to the fully-qualified binary name, not a bare `Inner`.
        let src = r#"package com.acme;
            class Outer { class Inner { } void run() { Inner x = null; x. } }"#;
        assert_eq!(infer(src).binary_name, "com/acme/Outer/Inner");
    }

    // ---- extract_symbols structural tests ----

    #[test]
    fn extract_basic_class() {
        let src = r#"
            package com.acme;
            import java.util.Map;
            import java.util.HashMap;
            public class Widget {
                private Map<String, Object> params;
                private int count;
                public Map<String, Object> getParams() { return params; }
                public void setCount(int c) { this.count = c; }
                public static final String KEY = "k";
            }
        "#;
        let fs = extract_symbols(src);
        assert_eq!(fs.package.as_deref(), Some("com.acme"));
        assert_eq!(fs.imports.len(), 2);
        assert_eq!(fs.types.len(), 1);
        let td = &fs.types[0];
        assert_eq!(td.name, "Widget");
        assert_eq!(td.fqn, "com.acme.Widget");
        assert!(td
            .methods
            .iter()
            .any(|m| m.name == "getParams" && m.return_type_text == "Map<String, Object>"));
        assert!(td.fields.iter().any(|f| f.name == "params"));
        assert!(td.fields.iter().any(|f| f.name == "KEY" && f.is_static));
    }

    #[test]
    fn extract_extends_implements() {
        let src = r#"
            package com.acme;
            public class Sub extends Base implements Iface, Other {
                void go() {}
            }
        "#;
        let fs = extract_symbols(src);
        let td = &fs.types[0];
        assert_eq!(td.extends.as_deref(), Some("Base"));
        assert!(td.implements.contains(&"Iface".to_string()));
        assert!(td.implements.contains(&"Other".to_string()));
    }

    /// Sanity over real legacy Java: point `BENNU_TEST_JAVA_ROOT` at a source root
    /// of a checked-out Entando-era application and this asserts we never panic and
    /// recover the expected members of a known framework class.
    ///
    /// Opt-in by environment rather than by a path in the source: the checkout
    /// lives outside the repository, so a hard-coded one is one machine's — and a
    /// test that quietly skipped on every other machine is a test that passes
    /// everywhere and checks nothing anywhere.
    #[test]
    fn extract_over_a_real_legacy_checkout() {
        let Ok(root) = std::env::var("BENNU_TEST_JAVA_ROOT") else {
            eprintln!("BENNU_TEST_JAVA_ROOT not set, skipping");
            return;
        };
        let root = std::path::PathBuf::from(root);
        let root = root.as_path();
        if !root.exists() {
            eprintln!("BENNU_TEST_JAVA_ROOT does not exist, skipping");
            return;
        }

        let rc = root.join("com/agiletec/aps/system/RequestContext.java");
        let src = std::fs::read_to_string(&rc).expect("read RequestContext");
        let fs = extract_symbols(&src);
        assert_eq!(fs.package.as_deref(), Some("com.agiletec.aps.system"));
        let td = fs
            .types
            .iter()
            .find(|t| t.name == "RequestContext")
            .expect("RequestContext type");
        assert!(td
            .methods
            .iter()
            .any(|m| m.name == "getRequest" && m.return_type_text == "HttpServletRequest"));
        assert!(td
            .fields
            .iter()
            .any(|f| f.name == "_request" && f.type_text == "HttpServletRequest"));
        assert!(td
            .fields
            .iter()
            .any(|f| f.name == "_extraParams" && f.type_text == "Map<String, Object>"));

        let mut scanned = 0usize;
        let mut with_types = 0usize;
        walk_java(root, &mut |path| {
            if scanned >= 400 {
                return;
            }
            scanned += 1;
            if let Ok(s) = std::fs::read_to_string(path) {
                let fs = extract_symbols(&s); // must not panic
                if !fs.types.is_empty() {
                    with_types += 1;
                }
            }
        });
        eprintln!("scanned {scanned} real files, {with_types} yielded >=1 type");
        assert!(scanned > 50);
        assert!(with_types * 100 / scanned >= 90);
    }

    #[cfg(test)]
    fn walk_java(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path)) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk_java(&p, f);
            } else if p.extension().and_then(|x| x.to_str()) == Some("java") {
                f(&p);
            }
        }
    }
}
