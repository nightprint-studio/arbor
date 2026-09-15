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

pub mod annotation_site;
pub mod ast;
// The type a position wants — the constraint on the hole rather than on the candidate.
pub mod expected;
// The four questions every framework rule asks of a declaration — annotations, modifiers,
// literals — read as nodes rather than as text.
pub mod decl;
// The name a declaration is about to be given — `private final OrderRepository ` → `orderRepository`.
// Read from tokens, not the parse: the declaration is unfinished by definition.
pub mod declaration_name;
pub mod grammar;
pub mod hierarchy;
pub mod import_hint;
pub mod infer;
// The `/** … */` block above a declaration, for one offset or for a whole file.
pub mod javadoc;
// The name a variable of a written type reads as — `List<Order>` is `orders`. Built on the postfix
// templates' word rules, so both features propose the same names.
pub mod names;
// Postfix templates — `orders.for`, `found.ifpe` — decided by the expression's type and the module's
// language level. Pure: the provider infers the type and turns expansions into edits.
pub mod postfix;
pub mod prelude;
// What the lexical scope at a caret binds — the names a bare identifier there could mean.
pub mod scope;
pub mod scaffold;
pub mod seam;
pub mod spans;
pub mod static_import;
pub mod symbols;
// The abbreviations a Java file expands — `psf`, `sout`, `psvm`. A vocabulary, not a feature.
pub mod templates;
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

            // `Object` itself, so a hierarchy that ends in it is a COMPLETE one. The functional
            // interface reads (a lambda parameter's type, a method reference's descriptor) refuse to
            // answer over a hierarchy with a hole in it, and every class here names `Object` as its
            // superclass.
            r.classes.insert("java/lang/Object".into(), class(vec![]));

            // `final class Optional<T>` — `<U> Optional<U> map(Function<? super T, ? extends U>)`,
            // `T orElse(T)`, `boolean isPresent()`. The wildcards are what the bytecode decoder
            // collapses onto their bounds, so the fake spells the bounds.
            let mut optional = class(vec![
                Member::method(
                    "map",
                    gen("java/util/Optional", vec![TypeRef::simple("U")]),
                    vec![gen(
                        "java/util/function/Function",
                        vec![TypeRef::simple("T"), TypeRef::simple("U")],
                    )],
                ),
                Member::method("orElse", TypeRef::simple("T"), vec![TypeRef::simple("T")]),
                m("isPresent", tr("boolean")),
            ]);
            optional.type_params = vec!["T".into()];
            r.classes.insert("java/util/Optional".into(), optional);

            // `interface Function<T, R> { R apply(T t); }` — the one abstract method is the SAM.
            r.classes.insert(
                "java/util/function/Function".into(),
                ClassMembers {
                    type_params: vec!["T".into(), "R".into()],
                    superclass: None,
                    interfaces: vec![],
                    methods: vec![Member::method(
                        "apply",
                        TypeRef::simple("R"),
                        vec![TypeRef::simple("T")],
                    )
                    .abstract_()],
                    fields: vec![],
                    flags: crate::seam::ClassFlags { is_interface: true, is_abstract: true, ..Default::default() },
                },
            );

            // The user's project: a snake_case method on an INTERFACE, returning an `Optional`.
            r.classes.insert(
                "com/acme/IdentityResolver".into(),
                ClassMembers {
                    type_params: Vec::new(),
                    superclass: None,
                    interfaces: vec![],
                    methods: vec![Member::method(
                        "resolve_identity",
                        gen("java/util/Optional", vec![tr("com/acme/ResolvedIdentity")]),
                        vec![],
                    )
                    .abstract_()],
                    fields: vec![],
                    flags: crate::seam::ClassFlags { is_interface: true, is_abstract: true, ..Default::default() },
                },
            );
            r.classes.insert(
                "com/acme/ResolvedIdentity".into(),
                class(vec![m("identifier", tr("java/lang/String")), m("roles", tr("int"))]),
            );
            r.classes.insert("com/acme/Token".into(), class(vec![]));
            r.classes.insert(
                "com/acme/Util".into(),
                class(vec![Member::method(
                    "convert",
                    tr("com/acme/Token"),
                    vec![tr("com/acme/ResolvedIdentity")],
                )
                .stat()]),
            );

            for (s, b) in [
                ("String", "java/lang/String"),
                ("List", "java/util/List"),
                ("Map", "java/util/Map"),
                ("Iterator", "java/util/Iterator"),
                ("Customer", "com/acme/Customer"),
                ("Pair", "com/acme/Pair"),
                ("Object", "java/lang/Object"),
                ("Optional", "java/util/Optional"),
                ("Function", "java/util/function/Function"),
                ("IdentityResolver", "com/acme/IdentityResolver"),
                ("ResolvedIdentity", "com/acme/ResolvedIdentity"),
                ("Token", "com/acme/Token"),
                ("Util", "com/acme/Util"),
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
            wildcard: false,
        }
    }
    fn m(name: &str, ret: TypeRef) -> Member {
        Member::method(name, ret, vec![]).sig(String::new())
    }
    /// A plain class extending `Object`, with `methods`.
    fn class(methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: Some(TypeRef::simple("java/lang/Object")),
            interfaces: vec![],
            methods,
            fields: vec![],
            flags: Default::default(),
        }
    }

    // ---- the reported Spring/Lombok class, exactly as written ----
    //
    // A snake_case method on a field typed by a project INTERFACE, returning `Optional<…>`, inside a
    // class whose header carries two annotations and an `implements` — and a statement with no `;`
    // yet, because the caret is at the end of the line being written.

    /// The reported class with `body` as the one statement of `check_delegate`.
    fn check_assigned_user(body: &str) -> String {
        format!(
            "package com.acme;\n\
             \n\
             import lombok.RequiredArgsConstructor;\n\
             import lombok.val;\n\
             import org.springframework.stereotype.Component;\n\
             \n\
             @RequiredArgsConstructor @Component\n\
             public class CheckAssignedUser implements AttributeValidator {{\n\
             \x20   private final IdentityResolver identity_resolver;\n\
             \n\
             \x20   private void check_delegate(final String username) {{\n\
             \x20       {body}\n\
             \x20   }}\n\
             }}\n"
        )
    }

    /// The type of the local `name` at its DECLARATION — what hover asks of a local's name. Found as
    /// ` name =`, because the bare word also occurs inside `check_delegate`.
    fn type_of(src: &str, name: &str) -> Option<TypeRef> {
        let start = src.find(&format!(" {name} =")).expect("declaration present") + 1;
        infer_expression_type(src, start, start + name.len(), &FakeResolver::jdk())
    }

    /// Bug 1's receiver, with the caret right after the dot (member completion excises whatever
    /// prefix was typed before it asks, so `.ma|` arrives here as `.|`).
    #[test]
    fn a_snake_case_call_on_a_field_typed_by_a_project_interface_is_its_optional() {
        let src = check_assigned_user("identity_resolver.resolve_identity().");
        let ty = infer(&src);
        assert_eq!(ty.binary_name, "java/util/Optional");
        assert_eq!(
            ty.type_args.first().map(|a| a.binary_name.as_str()),
            Some("com/acme/ResolvedIdentity")
        );
    }

    /// Bug 4: `val delegate = ….map(ResolvedIdentity::identifier)` hovers as `Optional<String>`. The
    /// hover of a local asks exactly this — the type of its NAME at the declaration.
    #[test]
    fn a_lombok_val_bound_to_a_map_over_a_method_reference_is_an_optional_of_its_result() {
        let src = check_assigned_user(
            "val delegate = identity_resolver.resolve_identity().map(ResolvedIdentity::identifier);",
        );
        let ty = type_of(&src, "delegate").expect("the local is typed");
        assert_eq!(ty.binary_name, "java/util/Optional");
        assert_eq!(ty.type_args.first().map(|a| a.binary_name.as_str()), Some("java/lang/String"));
    }

    #[test]
    fn a_final_val_is_typed_the_same_way() {
        let src = check_assigned_user(
            "final val delegate = identity_resolver.resolve_identity().map(ResolvedIdentity::identifier);",
        );
        let ty = type_of(&src, "delegate").expect("the local is typed");
        assert_eq!(ty.type_args.first().map(|a| a.binary_name.as_str()), Some("java/lang/String"));
    }

    /// `Foo::new` produces a `Foo`.
    #[test]
    fn a_constructor_reference_binds_the_type_it_constructs() {
        let src = check_assigned_user(
            "val delegate = identity_resolver.resolve_identity().map(ResolvedIdentity::new);",
        );
        let ty = type_of(&src, "delegate").expect("the local is typed");
        assert_eq!(
            ty.type_args.first().map(|a| a.binary_name.as_str()),
            Some("com/acme/ResolvedIdentity")
        );
    }

    /// A STATIC method taking the element: `Util::convert(ResolvedIdentity) -> Token`.
    #[test]
    fn a_static_method_reference_binds_what_the_static_returns() {
        let src = check_assigned_user(
            "val delegate = identity_resolver.resolve_identity().map(Util::convert);",
        );
        let ty = type_of(&src, "delegate").expect("the local is typed");
        assert_eq!(ty.type_args.first().map(|a| a.binary_name.as_str()), Some("com/acme/Token"));
    }

    /// The lambda spelling of the same thing: its parameter is typed from `Optional<T>`, its body
    /// from that parameter.
    #[test]
    fn an_expression_lambda_binds_what_its_body_returns() {
        let src = check_assigned_user(
            "val delegate = identity_resolver.resolve_identity().map(r -> r.identifier());",
        );
        let ty = type_of(&src, "delegate").expect("the local is typed");
        assert_eq!(ty.type_args.first().map(|a| a.binary_name.as_str()), Some("java/lang/String"));
    }

    /// The type flows on: `….map(ResolvedIdentity::identifier).orElse(null).` completes a `String`.
    #[test]
    fn the_bound_element_flows_into_the_next_call_in_the_chain() {
        let src = check_assigned_user(
            "identity_resolver.resolve_identity().map(ResolvedIdentity::identifier).orElse(null).",
        );
        assert_eq!(infer(&src).binary_name, "java/lang/String");
    }

    // ---- what a function slot receives (bug 2, bug 3) ----

    /// The descriptor at the caret marked `|` in `body`.
    fn descriptor(body: &str) -> Option<crate::infer::FunctionalDescriptor> {
        let marked = check_assigned_user(body);
        let at = marked.find('|').expect("a caret marker");
        let src = marked.replacen('|', "", 1);
        functional_descriptor_at(&src, at, &FakeResolver::jdk())
    }

    fn params(d: Option<crate::infer::FunctionalDescriptor>) -> Vec<String> {
        d.map(|d| d.params.into_iter().map(|p| p.binary_name).collect()).unwrap_or_default()
    }

    /// `map(Re|)`: the function receives the `Optional`'s element.
    #[test]
    fn a_word_in_optional_map_is_where_the_element_goes() {
        assert_eq!(
            params(descriptor("identity_resolver.resolve_identity().map(Re|)")),
            vec!["com/acme/ResolvedIdentity".to_string()]
        );
    }

    /// Nothing typed yet, and no `)` written either — the explicit Ctrl+Space case.
    #[test]
    fn an_empty_argument_still_has_a_descriptor() {
        assert_eq!(
            params(descriptor("identity_resolver.resolve_identity().map(|)")),
            vec!["com/acme/ResolvedIdentity".to_string()]
        );
        assert_eq!(
            params(descriptor("identity_resolver.resolve_identity().map(|")),
            vec!["com/acme/ResolvedIdentity".to_string()]
        );
    }

    /// The member half of a method reference is the same slot.
    #[test]
    fn the_member_half_of_a_method_reference_has_the_slots_descriptor() {
        for body in [
            "identity_resolver.resolve_identity().map(ResolvedIdentity::|)",
            "identity_resolver.resolve_identity().map(ResolvedIdentity::ide|)",
        ] {
            let d = descriptor(body);
            assert_eq!(params(d.clone()), vec!["com/acme/ResolvedIdentity".to_string()], "{body}");
            // `U` is bound by the reference being written, not by anything around it.
            assert_eq!(d.map(|d| d.returns.binary_name).as_deref(), Some("java/lang/Object"), "{body}");
        }
    }

    /// `orElse(T)` takes a value, not a function — there is no shape to describe.
    #[test]
    fn a_value_argument_has_no_descriptor() {
        assert!(descriptor("identity_resolver.resolve_identity().orElse(Re|)").is_none());
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
