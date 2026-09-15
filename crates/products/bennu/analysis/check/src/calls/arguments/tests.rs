use super::*;
use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member};
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
    fn resolve_simple_name(&self, name: &str, _i: &[Import]) -> Option<String> {
        self.simple.get(name).cloned()
    }
}

fn method(name: &str, params: &[&str]) -> Member {
    let params = params.iter().map(|p| TypeRef::simple(p.to_string())).collect();
    Member::method(name, TypeRef::simple("void"), params)
}

fn returning(name: &str, params: &[&str], ret: TypeRef) -> Member {
    let mut m = method(name, params);
    m.return_type = ret;
    m
}

fn generic(binary: &str, args: &[&str]) -> TypeRef {
    TypeRef {
        type_args: args.iter().map(|a| TypeRef::simple(a.to_string())).collect(),
        ..TypeRef::simple(binary)
    }
}

fn cls(methods: Vec<Member>) -> ClassMembers {
    ClassMembers {
        type_params: Vec::new(),
        superclass: Some(TypeRef::simple("java/lang/Object")),
        interfaces: Vec::new(),
        methods,
        fields: Vec::new(),
        flags: ClassFlags::default(),
    }
}

fn extending(superclass: &str, interfaces: &[&str]) -> ClassMembers {
    let mut c = cls(vec![]);
    c.superclass = Some(TypeRef::simple(superclass));
    c.interfaces = interfaces.iter().map(|i| TypeRef::simple(i.to_string())).collect();
    c
}

fn interface(methods: Vec<Member>) -> ClassMembers {
    let mut c = cls(methods);
    c.flags.is_interface = true;
    c.flags.is_abstract = true;
    c
}

/// `Svc` and the types around it. Animal / Dog (Dog extends Animal) / Widget (unrelated).
fn resolver() -> MapResolver {
    let mut members = HashMap::new();
    let object = ClassMembers { superclass: None, ..cls(vec![]) };
    members.insert("java/lang/Object".to_string(), object);
    members.insert("java/lang/String".to_string(), cls(vec![]));
    // The box of `int`: the loose phase asks whether an `Integer` can be passed, and an unreadable
    // one would (rightly) abstain — which is not what a real classpath does.
    members.insert("java/lang/Integer".to_string(), cls(vec![]));
    members.insert("java/lang/Enum".to_string(), cls(vec![]));
    members.insert("com/acme/Animal".to_string(), cls(vec![]));
    members.insert("com/acme/Dog".to_string(), extending("com/acme/Animal", &[]));
    members.insert("com/acme/Widget".to_string(), cls(vec![]));
    // A DIFFERENT type sharing the simple name `Widget` (another package) — for the same-name skip.
    members.insert("com/other/Widget".to_string(), cls(vec![]));
    members.insert("com/acme/Color".to_string(), extending("java/lang/Enum", &[]));
    members.insert("com/acme/Task".to_string(), interface(vec![]));
    members.insert("com/acme/Job".to_string(), extending("java/lang/Object", &["com/acme/Task"]));
    // Hierarchies with a hole: above the CLASS chain, and above an interface only.
    members.insert("com/acme/Orphan".to_string(), extending("com/missing/Base", &[]));
    members.insert(
        "com/acme/HalfKnown".to_string(),
        extending("java/lang/Object", &["com/missing/Marker"]),
    );
    let mut sealed = cls(vec![]);
    sealed.flags.is_final = true;
    members.insert("com/acme/Sealed".to_string(), sealed);
    members.insert(
        "com/acme/Svc".to_string(),
        cls(vec![
            method("label", &["java/lang/String", "java/lang/String"]),
            method("take", &["com/acme/Animal"]),
            method("overloaded", &["int"]),
            method("overloaded", &["java/lang/String"]),
            // Two arity-2 overloads, ONE with an array param (non-checkable): a call must not be
            // judged against the lone checkable `(String, String)` — mirrors the reported
            // `setRecipients(String, Addresses[])` + `setRecipients(String, String)` case.
            method("recip", &["java/lang/String", "com/acme/Widget[]"]),
            method("recip", &["java/lang/String", "java/lang/String"]),
            // A VARARGS overload of a DIFFERENT arity than a fixed sibling — SLF4J's
            // `debug(String, Object...)` vs `debug(Marker, String, Object, Object)`. A 3-arg call
            // could bind the varargs, so the fixed arity-3 must not be judged alone.
            method("emit", &["java/lang/String", "java/lang/Object[]"]),
            method("emit", &["com/acme/Widget", "java/lang/String", "java/lang/String"]),
            method("logv", &["java/lang/String", "java/lang/String[]"]),
            // A parameter typed as a SAME-SIMPLE-NAME type in another package (`com/other/Widget`
            // vs the argument's `com/acme/Widget`) — the same-name-collision case.
            method("dup", &["com/other/Widget"]),
            // Boxing both ways, and widening.
            method("count", &["java/lang/Integer"]),
            method("prim", &["int"]),
            method("wide", &["long"]),
            // Three parameters, the THIRD the one gone wrong.
            method("triple", &["java/lang/String", "int", "com/acme/Animal"]),
            method("named", &["java/lang/String", "int", "java/lang/String"]),
            // A generic method whose third parameter is concrete.
            method("gen", &["T", "T", "com/acme/Animal"]),
            // Two same-arity overloads that refuse the same position, and two that refuse different
            // ones.
            method("pair", &["java/lang/String", "int", "com/acme/Animal"]),
            method("pair", &["java/lang/String", "int", "com/acme/Dog"]),
            method("swap", &["java/lang/String", "int"]),
            method("swap", &["int", "java/lang/String"]),
            method("run", &["com/acme/Task"]),
            method("seal", &["com/acme/Sealed"]),
            method("flag", &["boolean"]),
            returning("dog", &[], TypeRef::simple("com/acme/Dog")),
            returning("widget", &[], TypeRef::simple("com/acme/Widget")),
            returning("boxed", &[], TypeRef::simple("java/lang/Integer")),
            returning("job", &[], TypeRef::simple("com/acme/Job")),
            returning("orphan", &[], TypeRef::simple("com/acme/Orphan")),
            returning("half", &[], TypeRef::simple("com/acme/HalfKnown")),
            returning("me", &[], TypeRef::simple("com/acme/Svc")),
        ]),
    );
    members.insert(
        "com/acme/Util".to_string(),
        cls(vec![method("convert", &["java/lang/String", "int", "com/acme/Animal"]).stat()]),
    );
    members.insert(
        "lib/Mailer".to_string(),
        cls(vec![method("send", &["java/lang/String", "int", "java/lang/String"])]),
    );
    // The class the test sources are written in, so a BARE call has a fully-known `this`, plus a
    // constructor to judge `new Ctor("x")` against.
    members.insert(
        "C".to_string(),
        cls(vec![
            method("own", &["int"]),
            method("put", &["java/lang/String[]", "int"]),
            method("keep", &["T", "int"]),
        ]),
    );
    members.insert("com/acme/Ctor".to_string(), cls(vec![method("<init>", &["int"])]));
    add_reported_project(&mut members);
    let simple = [
        ("C", "C"),
        ("Ctor", "com/acme/Ctor"),
        ("Svc", "com/acme/Svc"),
        ("Animal", "com/acme/Animal"),
        ("Dog", "com/acme/Dog"),
        ("Widget", "com/acme/Widget"),
        ("Color", "com/acme/Color"),
        ("Util", "com/acme/Util"),
        ("Mailer", "lib/Mailer"),
        ("String", "java/lang/String"),
        ("Object", "java/lang/Object"),
        ("Optional", "java/util/Optional"),
        ("IdentityResolver", "com/acme/IdentityResolver"),
        ("ResolvedIdentity", "com/acme/ResolvedIdentity"),
        ("DelegateClient", "com/acme/DelegateClient"),
        ("Token", "com/acme/Token"),
    ]
    .into_iter()
    .map(|(s, b)| (s.to_string(), b.to_string()))
    .collect();
    MapResolver { members, simple }
}

/// The reported Spring/Lombok project: `Optional<T>`, an interface returning one, and a client
/// whose third parameter is a `Token`.
fn add_reported_project(members: &mut HashMap<String, ClassMembers>) {
    let mut optional = cls(vec![
        returning("get", &[], TypeRef::simple("T")),
        returning("isEmpty", &[], TypeRef::simple("boolean")),
    ]);
    optional.type_params = vec!["T".to_string()];
    optional.flags.is_final = true;
    members.insert("java/util/Optional".to_string(), optional);
    members.insert(
        "com/acme/IdentityResolver".to_string(),
        interface(vec![returning(
            "resolve_identity",
            &[],
            generic("java/util/Optional", &["com/acme/ResolvedIdentity"]),
        )]),
    );
    members.insert(
        "com/acme/ResolvedIdentity".to_string(),
        cls(vec![returning("identifier", &[], TypeRef::simple("java/lang/String"))]),
    );
    members.insert("com/acme/Token".to_string(), cls(vec![]));
    members.insert(
        "com/acme/DelegateClient".to_string(),
        interface(vec![returning(
            "delegate_for_user",
            &["java/lang/String", "java/lang/String", "com/acme/Token"],
            generic("java/util/Optional", &["com/acme/Token"]),
        )]),
    );
}

fn file_diags(src: &str) -> Vec<String> {
    argument_type_errors(src, &resolver()).into_iter().map(|d| d.message).collect()
}

fn diags(body: &str) -> Vec<String> {
    file_diags(&format!("class C {{ Svc s; void m() {{ {body} }} }}"))
}

#[track_caller]
fn one(d: &[String], needles: &[&str]) {
    assert_eq!(d.len(), 1, "{d:?}");
    for n in needles {
        assert!(d[0].contains(n), "`{n}` missing: {d:?}");
    }
}

// ── the shapes that were already judged ─────────────────────────────────────────────────────────

#[test]
fn bare_call_with_a_bad_argument_is_flagged() {
    one(&diags("own(\"x\");"), &["own"]);
}

#[test]
fn bare_call_with_a_good_argument_is_ok() {
    assert!(diags("own(1);").is_empty());
}

#[test]
fn bare_call_the_buffer_overloads_is_not_judged() {
    // The source adds `own(String)`, which the index has not seen. The stale overload set would
    // otherwise stand alone and call a perfectly legal call wrong.
    assert!(file_diags("class C { void own(String t) {} void m() { own(\"x\"); } }").is_empty());
}

#[test]
fn constructor_with_a_bad_argument_is_flagged() {
    one(&diags("Ctor c = new Ctor(\"x\");"), &["Argument 1 of `Ctor`"]);
}

#[test]
fn constructor_with_a_good_argument_is_ok() {
    assert!(diags("Ctor c = new Ctor(1);").is_empty());
}

#[test]
fn int_for_string_param_is_flagged() {
    one(&diags("s.label(1, \"b\");"), &["Argument 1", "String"]);
}

#[test]
fn correct_string_args_are_ok() {
    assert!(diags("s.label(\"a\", \"b\");").is_empty());
}

#[test]
fn subtype_argument_is_ok() {
    assert!(diags("s.take(s.dog());").is_empty());
}

#[test]
fn unrelated_class_argument_is_flagged() {
    one(&diags("s.take(s.widget());"), &["Widget", "Animal"]);
}

#[test]
fn ambiguous_overload_is_skipped() {
    // `overloaded` has two distinct 1-arg signatures, and each call is applicable to one of them.
    assert!(diags("s.overloaded(1);").is_empty());
    assert!(diags("s.overloaded(\"x\");").is_empty());
}

#[test]
fn overload_with_array_param_sibling_is_skipped() {
    // `recip(String, Widget[])` can take a lone `Widget` as its varargs element.
    assert!(diags("s.recip(\"a\", s.widget());").is_empty());
    assert!(diags("s.recip(\"a\", \"b\");").is_empty());
}

#[test]
fn unknown_receiver_is_skipped() {
    assert!(diags("Unknown u = null; u.whatever(1);").is_empty());
}

#[test]
fn same_simple_name_argument_is_not_flagged() {
    assert!(diags("s.dup(s.widget());").is_empty());
}

#[test]
fn varargs_overload_of_other_arity_is_skipped() {
    // `emit(String, Object...)` binds the 3-argument call — the SLF4J `LOG.debug("fmt", a, b, c)`
    // shape, which must never be judged against the fixed arity-3 sibling.
    assert!(diags("s.emit(\"a\", s.widget(), \"c\");").is_empty());
}

#[test]
fn a_lambda_argument_is_never_reported() {
    assert!(diags("s.take(x -> x);").is_empty());
    assert!(diags("s.label(\"a\", x -> x);").is_empty());
}

#[test]
fn varargs_elements_are_not_judged() {
    // The varargs phase takes `1` and `2` as `Object` elements.
    assert!(diags("s.emit(\"a\", 1, 2);").is_empty());
    // Nothing applies to `logv("a", widget)`, but the refused position is a varargs element.
    assert!(diags("s.logv(\"a\", s.widget());").is_empty());
}

#[test]
fn a_fixed_parameter_before_varargs_is_judged() {
    // `emit(String, Object...)` is the only overload `emit(1)` could be, and its first parameter
    // is fixed whether or not the array is varargs.
    one(&diags("s.emit(1);"), &["Argument 1 of `emit`", "String"]);
}

#[test]
fn boxing_unboxing_and_widening_are_not_reported() {
    assert!(diags("s.count(1);").is_empty());
    assert!(diags("s.prim(s.boxed());").is_empty());
    assert!(diags("s.wide(1);").is_empty());
}

#[test]
fn an_untyped_argument_is_not_reported() {
    assert!(diags("s.take(mystery());").is_empty());
    assert!(diags("s.label(mystery(), \"b\");").is_empty());
}

// ── the third argument, in every shape ──────────────────────────────────────────────────────────

#[test]
fn a_wrong_third_argument_is_flagged_whatever_its_expression() {
    one(&diags("s.triple(\"a\", 1, \"c\");"), &["Argument 3 of `triple`", "`String`", "`Animal`"]);
    one(&diags("s.triple(\"a\", 1, s.widget());"), &["Argument 3", "`Widget`"]);
    one(&diags("s.triple(\"a\", 1, new Widget());"), &["Argument 3", "`Widget`"]);
    one(&diags("Widget w = null; s.triple(\"a\", 1, w);"), &["Argument 3", "`Widget`"]);
    one(&diags("Color c = null; s.named(\"a\", 1, c);"), &["Argument 3", "`Color`", "`String`"]);
    one(&diags("s.triple(\"a\", \"b\", s.dog());"), &["Argument 2", "`int`"]);
    assert!(diags("s.triple(\"a\", 1, s.dog());").is_empty());
}

#[test]
fn every_wrong_position_is_reported() {
    let d = diags("s.triple(1, \"b\", s.widget());");
    assert_eq!(d.len(), 3, "{d:?}");
}

#[test]
fn the_receiver_may_be_a_local_a_chain_or_this() {
    one(&diags("Svc local = s; local.triple(\"a\", 1, s.widget());"), &["Argument 3"]);
    one(&diags("s.me().triple(\"a\", 1, s.widget());"), &["Argument 3"]);
    one(&diags("this.own(\"x\");"), &["own"]);
}

#[test]
fn a_call_on_this_the_buffer_overloads_is_not_judged() {
    assert!(file_diags("class C { void own(String t) {} void m() { this.own(\"x\"); } }").is_empty());
}

#[test]
fn a_classpath_method_is_judged() {
    one(&diags("Mailer mailer = null; mailer.send(\"a\", 1, 2);"), &["Argument 3 of `send`"]);
}

#[test]
fn a_static_call_is_judged() {
    one(&diags("Util.convert(\"a\", 1, s.widget());"), &["Argument 3 of `convert`"]);
    assert!(diags("Util.convert(\"a\", 1, s.dog());").is_empty());
}

#[test]
fn a_variable_named_like_a_type_is_not_read_as_the_type() {
    // `Util` is a local whose type nothing can read: it obscures the class `Util`.
    assert!(diags("Mystery Util = null; Util.convert(\"a\", 1, s.widget());").is_empty());
}

#[test]
fn a_generic_methods_concrete_parameter_is_judged() {
    one(&diags("s.gen(1, \"x\", s.widget());"), &["Argument 3 of `gen`"]);
    // The type-variable positions take anything.
    assert!(diags("s.gen(s.widget(), s.dog(), s.dog());").is_empty());
}

#[test]
fn same_arity_overloads_refusing_the_same_position_name_every_expected_type() {
    one(&diags("s.pair(\"a\", 1, s.widget());"), &["Argument 3 of `pair`", "`Animal` or `Dog`"]);
    assert!(diags("s.pair(\"a\", 1, s.dog());").is_empty());
}

#[test]
fn overloads_refusing_different_positions_report_the_call() {
    one(&diags("s.swap(\"a\", \"b\");"), &["No overload of `swap` accepts these arguments"]);
}

#[test]
fn an_interface_parameter_is_judged_over_a_known_hierarchy() {
    one(&diags("s.run(s.widget());"), &["`Widget`", "`Task`"]);
    assert!(diags("s.run(s.job());").is_empty());
    // An unreadable interface above the argument could be `Task` itself.
    assert!(diags("s.run(s.half());").is_empty());
}

#[test]
fn an_unreadable_part_of_the_arguments_hierarchy_abstains_only_where_it_could_matter() {
    // `Orphan extends com.missing.Base`: the missing base could extend `Animal`.
    assert!(diags("s.take(s.orphan());").is_empty());
    // A `final` class has no subtype to be reached through it.
    one(&diags("s.seal(s.orphan());"), &["`Orphan`", "`Sealed`"]);
    // A missing INTERFACE cannot lead to a class.
    one(&diags("s.take(s.half());"), &["`HalfKnown`", "`Animal`"]);
}

#[test]
fn primitives_against_classes_and_booleans_are_judged() {
    one(&diags("s.take(1);"), &["`int`", "`Animal`"]);
    one(&diags("s.prim(s.widget());"), &["`Widget`", "`int`"]);
    one(&diags("s.flag(1);"), &["`int`", "`boolean`"]);
}

#[test]
fn an_object_argument_is_left_alone() {
    // What an unbound type variable reads as, not a type anybody wrote.
    assert!(diags("Object o = null; s.take(o);").is_empty());
}

// ── shapes that used to be refused wholesale ─────────────────────────────────────────────────────

#[test]
fn a_bare_call_inside_a_lambda_is_judged() {
    one(&diags("Runnable r = () -> own(\"x\");"), &["own"]);
    one(&diags("Runnable r = () -> s.take(s.widget());"), &["Animal"]);
}

#[test]
fn a_bare_call_inside_an_anonymous_class_is_still_skipped() {
    // The anonymous body could declare its own `own(String)`.
    assert!(diags("Object o = new Object() { void go() { own(\"x\"); } };").is_empty());
}

#[test]
fn an_anonymous_class_creation_is_judged_against_its_supertypes_constructors() {
    one(&diags("Ctor c = new Ctor(\"x\") { };"), &["Argument 1 of `Ctor`"]);
    assert!(diags("Ctor c = new Ctor(1) { };").is_empty());
}

#[test]
fn a_lombok_class_still_judges_calls_to_its_own_methods() {
    one(&file_diags("@Data class C { void m() { own(\"x\"); } }"), &["own"]);
}

#[test]
fn an_unreadable_static_wildcard_import_does_not_hide_own_methods() {
    // A member `own` shadows every statically imported `own` (JLS §6.4.1).
    let src = "import static com.missing.Util.*;\nclass C { void m() { own(\"x\"); } }";
    one(&file_diags(src), &["own"]);
}

#[test]
fn own_methods_with_array_or_type_variable_parameters_are_judged() {
    let src = "class C { void put(String[] xs, int n) {} <T> void keep(T v, int n) {}\n\
               void m() { put(null, \"x\"); keep(1, \"x\"); keep(\"any\", 1); } }";
    let d = file_diags(src);
    assert_eq!(d.len(), 2, "{d:?}");
    assert!(d.iter().all(|m| m.contains("Argument 2")), "{d:?}");
}

// ── the reported method, as written ──────────────────────────────────────────────────────────────

fn check_delegate_src() -> String {
    "package com.acme;\n\
     \n\
     import lombok.RequiredArgsConstructor;\n\
     import lombok.val;\n\
     \n\
     @RequiredArgsConstructor\n\
     public class CheckAssignedUser {\n\
     \x20   private final IdentityResolver identity_resolver;\n\
     \x20   private final DelegateClient client;\n\
     \n\
     \x20   private ValidationOutcome check_delegate(final String username) {\n\
     \x20       val delegate_opt = identity_resolver.resolve_identity();\n\
     \n\
     \x20       if (delegate_opt.isEmpty())\n\
     \x20           return STD_DENIED;\n\
     \n\
     \x20       val delegate = delegate_opt.get();\n\
     \n\
     \x20       val delegate_db_opt =\n\
     \x20           client.delegate_for_user(\n\
     \x20               username\n\
     \x20               , delegate.identifier()\n\
     \x20               , ARG3\n\
     \x20           );\n\
     \n\
     \x20       if (delegate_db_opt.isEmpty())\n\
     \x20           return STD_DENIED;\n\
     \n\
     \x20       return ValidationOutcome.allow();\n\
     \x20   }\n\
     }\n"
        .to_string()
}

#[test]
fn the_reported_third_argument_is_flagged() {
    let src = check_delegate_src().replace("ARG3", "delegate_opt.get()");
    one(
        &file_diags(&src),
        &["Argument 3 of `delegate_for_user`", "`ResolvedIdentity`", "`Token`"],
    );
}

#[test]
fn the_reported_call_with_the_right_third_argument_is_ok() {
    let src = check_delegate_src().replace("ARG3", "new Token()");
    assert!(file_diags(&src).is_empty(), "{:?}", file_diags(&src));
}
