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

pub mod infer;
pub mod log_param;
pub mod prelude;
pub mod seam;
pub mod symbols;
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
                    superclass: Some("java/lang/Object".into()),
                    interfaces: vec![],
                    methods: vec![
                        m("length", tr("int")),
                        m("toUpperCase", tr("java/lang/String")),
                        m("trim", tr("java/lang/String")),
                        m("charAt", tr("char")),
                    ],
                    fields: vec![],
                },
            );

            // java/util/List<E> — get(int) -> E, iterator() -> Iterator<E>
            r.classes.insert(
                "java/util/List".into(),
                ClassMembers {
                    superclass: None,
                    interfaces: vec!["java/util/Collection".into()],
                    methods: vec![
                        m("get", TypeRef::simple("E")),
                        m("iterator", gen("java/util/Iterator", vec![TypeRef::simple("E")])),
                        m("size", tr("int")),
                    ],
                    fields: vec![],
                },
            );

            // java/util/Iterator<E> — next() -> E
            r.classes.insert(
                "java/util/Iterator".into(),
                ClassMembers {
                    superclass: None,
                    interfaces: vec![],
                    methods: vec![m("next", TypeRef::simple("E"))],
                    fields: vec![],
                },
            );

            // java/util/Map<K,V> — get(K) -> V
            r.classes.insert(
                "java/util/Map".into(),
                ClassMembers {
                    superclass: None,
                    interfaces: vec![],
                    methods: vec![m("get", TypeRef::simple("V"))],
                    fields: vec![],
                },
            );

            // A domain type with a getter: Customer.getName() -> String
            r.classes.insert(
                "com/acme/Customer".into(),
                ClassMembers {
                    superclass: Some("java/lang/Object".into()),
                    interfaces: vec![],
                    methods: vec![m("getName", tr("java/lang/String"))],
                    fields: vec![],
                },
            );

            for (s, b) in [
                ("String", "java/lang/String"),
                ("List", "java/util/List"),
                ("Map", "java/util/Map"),
                ("Iterator", "java/util/Iterator"),
                ("Customer", "com/acme/Customer"),
                ("Object", "java/lang/Object"),
            ] {
                r.simple.insert(s.into(), b.into());
            }
            r
        }
    }

    impl TypeResolver for FakeResolver {
        fn members_of(&self, binary_name: &str) -> Option<std::sync::Arc<ClassMembers>> {
            self.classes.get(binary_name).cloned().map(std::sync::Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn tr(bn: &str) -> TypeRef {
        TypeRef::simple(bn)
    }
    fn gen(bn: &str, args: Vec<TypeRef>) -> TypeRef {
        TypeRef { binary_name: bn.into(), type_args: args }
    }
    fn m(name: &str, ret: TypeRef) -> Member {
        Member {
            name: name.into(),
            kind: MemberKind::Method,
            return_type: ret,
            params: vec![],
            is_static: false,
            visibility: Visibility::Public,
            raw_signature: String::new(),
        }
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
    fn this_field_access() {
        let src = r#"package com.acme; class Foo { private Customer bar; void run() { this.bar. } }"#;
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

    /// Sanity over real legacy Java, when a local checkout of PortaleAppalti is
    /// present (it lives outside the repo, so this is a no-op in CI). Asserts we
    /// never panic and recover the expected members of a known file.
    #[test]
    fn extract_over_real_portale_appalti() {
        let root = std::path::Path::new(
            "C:/Sviluppo/Mio/temp/disposable-projects/PortaleAppalti/src/main/java",
        );
        if !root.exists() {
            eprintln!("PortaleAppalti not present, skipping");
            return;
        }

        let rc = root.join("com/agiletec/aps/system/RequestContext.java");
        let src = std::fs::read_to_string(&rc).expect("read RequestContext");
        let fs = extract_symbols(&src);
        assert_eq!(fs.package.as_deref(), Some("com.agiletec.aps.system"));
        let td =
            fs.types.iter().find(|t| t.name == "RequestContext").expect("RequestContext type");
        assert!(td
            .methods
            .iter()
            .any(|m| m.name == "getRequest" && m.return_type_text == "HttpServletRequest"));
        assert!(td.fields.iter().any(|f| f.name == "_request"
            && f.type_text == "HttpServletRequest"));
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
        let Ok(rd) = std::fs::read_dir(dir) else { return };
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
