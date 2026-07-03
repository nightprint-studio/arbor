//! Category: METHODS — go-to-declaration over method calls in every receiver shape.
//!
//! Covers a bare same-class call, a call via a field receiver, via a local receiver, via a
//! parameter receiver, an inherited method (resolves into the parent's file), an interface
//! default method, a static method call (`Type.method()`), an overloaded method (any overload
//! is acceptable — assert by name), and cross-file receiver calls. Every assertion checks the
//! resolved `.file` plus the `"method <owner>.<name>()"` label.

mod common;
use common::*;

/// A small inter-linked project reused by most method tests.
///
/// * `Base`     — parent with `baseMethod()`.
/// * `Greeter`  — interface with a `default` method + an abstract one.
/// * `Util`     — a `static` helper.
/// * `Service`  — extends `Base`, implements `Greeter`, holds a `Util` field, calls everything.
/// * `Consumer` — cross-file caller through a `Service` receiver.
fn proj() -> Project {
    Project::new(&[
        (
            "Base.java",
            "package app;\n\
             public class Base {\n\
             \x20   public int baseMethod() { return 1; }\n\
             }\n",
        ),
        (
            "Greeter.java",
            "package app;\n\
             public interface Greeter {\n\
             \x20   int name();\n\
             \x20   default int greet() { return 7; }\n\
             }\n",
        ),
        (
            "Util.java",
            "package app;\n\
             public class Util {\n\
             \x20   public static int helper() { return 42; }\n\
             \x20   public int shared() { return 1; }\n\
             \x20   public int shared(int extra) { return extra; }\n\
             }\n",
        ),
        (
            "Service.java",
            "package app;\n\
             public class Service extends Base implements Greeter {\n\
             \x20   private Util util;\n\
             \x20   public int name() { return 0; }\n\
             \x20   public int local() { return 5; }\n\
             \x20   public int run(Service other) {\n\
             \x20       Util loc = new Util();\n\
             \x20       int viaBare = local();\n\
             \x20       int viaField = util.shared();\n\
             \x20       int viaLocal = loc.shared();\n\
             \x20       int viaParam = other.local();\n\
             \x20       int viaInherited = baseMethod();\n\
             \x20       int viaDefault = greet();\n\
             \x20       int viaStatic = Util.helper();\n\
             \x20       int viaOverload = util.shared(3);\n\
             \x20       return viaBare + viaField + viaLocal + viaParam\n\
             \x20            + viaInherited + viaDefault + viaStatic + viaOverload;\n\
             \x20   }\n\
             }\n",
        ),
        (
            "Consumer.java",
            "package app;\n\
             public class Consumer {\n\
             \x20   public int use(Service s) { return s.run(s) + s.baseMethod(); }\n\
             }\n",
        ),
    ])
}

#[test]
fn bare_same_class_call() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let d = p.goto("Service.java", at(&s, "local();")).expect("goto bare same-class method");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "method app.Service.local()");
    assert_eq!(d.line, line_of(&s, "int local()"));
}

#[test]
fn call_via_field_receiver() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // `util.shared()` — the no-arg overload through the `util` field receiver.
    let d = p.goto("Service.java", at(&s, "util.shared();") + "util.".len())
        .expect("goto field-receiver method");
    assert_eq!(d.file, "Util.java");
    assert_eq!(d.label, "method app.Util.shared()");
}

#[test]
fn call_via_local_receiver() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // `loc.shared()` — through a local variable receiver.
    let d = p.goto("Service.java", at(&s, "loc.shared();") + "loc.".len())
        .expect("goto local-receiver method");
    assert_eq!(d.file, "Util.java");
    assert_eq!(d.label, "method app.Util.shared()");
}

#[test]
fn call_via_parameter_receiver() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // `other.local()` — through the `other` parameter receiver → back into Service.
    let d = p.goto("Service.java", at(&s, "other.local();") + "other.".len())
        .expect("goto parameter-receiver method");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "method app.Service.local()");
    assert_eq!(d.line, line_of(&s, "int local()"));
}

#[test]
fn inherited_method_resolves_into_parent_file() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let d = p.goto("Service.java", at(&s, "baseMethod();")).expect("goto inherited method");
    assert_eq!(d.file, "Base.java", "an inherited method resolves into the PARENT's file");
    assert_eq!(d.label, "method app.Base.baseMethod()");
}

#[test]
fn interface_default_method() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    let d = p.goto("Service.java", at(&s, "greet();")).expect("goto interface default method");
    assert_eq!(d.file, "Greeter.java", "a default method resolves into the interface's file");
    assert_eq!(d.label, "method app.Greeter.greet()");
}

#[test]
fn static_method_call() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // `Util.helper()` — the static call through a type name.
    let d = p.goto("Service.java", at(&s, "Util.helper();") + "Util.".len())
        .expect("goto static method");
    assert_eq!(d.file, "Util.java");
    assert_eq!(d.label, "method app.Util.helper()");
}

#[test]
fn overloaded_method_resolves_by_name() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // `util.shared(3)` — the arity-1 overload. Any overload is acceptable; assert by name/file.
    let d = p.goto("Service.java", at(&s, "util.shared(3)") + "util.".len())
        .expect("goto overloaded method");
    assert_eq!(d.file, "Util.java");
    assert_eq!(d.label, "method app.Util.shared()");
}

#[test]
fn cross_file_receiver_call() {
    let p = proj();
    let c = p.source("Consumer.java").to_string();
    // `s.run(s)` — cross-file call through a Service receiver.
    let d = p.goto("Consumer.java", at(&c, "s.run(s)") + "s.".len())
        .expect("goto cross-file receiver method");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "method app.Service.run()");
    assert_eq!(d.line, line_of(p.source("Service.java"), "int run("));
}

#[test]
fn cross_file_inherited_receiver_call() {
    let p = proj();
    let c = p.source("Consumer.java").to_string();
    // `s.baseMethod()` — cross-file call resolving into the parent file.
    let d = p.goto("Consumer.java", at(&c, "s.baseMethod()") + "s.".len())
        .expect("goto cross-file inherited receiver method");
    assert_eq!(d.file, "Base.java");
    assert_eq!(d.label, "method app.Base.baseMethod()");
}

#[test]
fn caret_on_method_declaration_name_resolves_to_itself() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // Clicking the method's own declaration name resolves to that method decl.
    let d = p.goto("Service.java", at(&s, "int run(Service") + "int ".len())
        .expect("goto on method decl name");
    assert_eq!(d.file, "Service.java");
    assert_eq!(d.label, "method app.Service.run()");
    assert_eq!(d.line, line_of(&s, "int run("));
}

#[test]
fn find_usages_of_bare_method() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // caret on the `local()` DECLARATION → its use sites: viaBare + other.local() = 2.
    let n = p.usage_count("Service.java", at(&s, "int local()") + "int ".len());
    assert_eq!(n, 2, "local() is called bare and via the parameter receiver");
}

#[test]
fn find_usages_of_cross_file_method() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // caret on the `run` DECLARATION → its single cross-file use in Consumer.
    let n = p.usage_count("Service.java", at(&s, "int run(") + "int ".len());
    assert_eq!(n, 1, "run() is called once from Consumer");
}

#[test]
fn find_usages_of_inherited_method() {
    let p = proj();
    let b = p.source("Base.java").to_string();
    // caret on the `baseMethod` DECLARATION → its uses: Service.run() + Consumer = 2.
    let n = p.usage_count("Base.java", at(&b, "int baseMethod()") + "int ".len());
    assert_eq!(n, 2, "baseMethod() is called from Service and Consumer");
}

#[test]
fn call_on_keyword_or_literal_is_none() {
    let p = proj();
    let s = p.source("Service.java").to_string();
    // caret on the `return` keyword and on a numeric literal — never a panic, never a target.
    assert!(p.goto("Service.java", at(&s, "return viaBare")).is_none());
    assert!(p.goto("Service.java", at(&s, "return 5;") + "return ".len()).is_none());
}

#[test]
fn interface_default_via_cross_file_receiver() {
    // A dedicated project: a consumer calling the interface default through the concrete type.
    let p = Project::new(&[
        (
            "Greeter.java",
            "package app;\n\
             public interface Greeter {\n\
             \x20   default int greet() { return 7; }\n\
             }\n",
        ),
        (
            "Impl.java",
            "package app;\n\
             public class Impl implements Greeter {\n\
             }\n",
        ),
        (
            "Caller.java",
            "package app;\n\
             public class Caller {\n\
             \x20   public int use(Impl i) { return i.greet(); }\n\
             }\n",
        ),
    ]);
    let c = p.source("Caller.java").to_string();
    let d = p.goto("Caller.java", at(&c, "i.greet()") + "i.".len())
        .expect("goto default method via cross-file receiver");
    assert_eq!(d.file, "Greeter.java", "resolves into the interface that declares the default");
    assert_eq!(d.label, "method app.Greeter.greet()");
}

#[test]
fn static_method_bare_call_within_declaring_type() {
    // A static method called bare (no `Type.` prefix) from inside its own class.
    let p = Project::new(&[(
        "Tools.java",
        "package app;\n\
         public class Tools {\n\
         \x20   static int base() { return 3; }\n\
         \x20   int derive() { return base() + 1; }\n\
         }\n",
    )]);
    let s = p.source("Tools.java").to_string();
    let d = p.goto("Tools.java", at(&s, "base() + 1")).expect("goto bare static call");
    assert_eq!(d.file, "Tools.java");
    assert_eq!(d.label, "method app.Tools.base()");
    assert_eq!(d.line, line_of(&s, "int base()"));
}

#[test]
fn overload_selection_does_not_panic_either_arity() {
    // Both overloads are valid targets; assert each resolves by name into the Util file.
    let p = proj();
    let s = p.source("Service.java").to_string();
    let no_arg = p.goto("Service.java", at(&s, "util.shared();") + "util.".len())
        .expect("goto no-arg overload");
    let with_arg = p.goto("Service.java", at(&s, "util.shared(3)") + "util.".len())
        .expect("goto arity-1 overload");
    assert_eq!(no_arg.file, "Util.java");
    assert_eq!(with_arg.file, "Util.java");
    assert_eq!(no_arg.label, "method app.Util.shared()");
    assert_eq!(with_arg.label, "method app.Util.shared()");
}

#[test]
fn cross_file_static_method_call() {
    // A fresh consumer of the static helper in its own isolated project.
    let p2 = Project::new(&[
        (
            "Util.java",
            "package app;\n\
             public class Util {\n\
             \x20   public static int helper() { return 42; }\n\
             }\n",
        ),
        (
            "Client.java",
            "package app;\n\
             public class Client {\n\
             \x20   public int go() { return Util.helper(); }\n\
             }\n",
        ),
    ]);
    let c = p2.source("Client.java").to_string();
    let d = p2.goto("Client.java", at(&c, "Util.helper()") + "Util.".len())
        .expect("goto cross-file static method");
    assert_eq!(d.file, "Util.java");
    assert_eq!(d.label, "method app.Util.helper()");
}
