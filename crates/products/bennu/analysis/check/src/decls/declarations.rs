//! Declaration & modifier legality — pure-AST rules that don't need a resolver.
//!
//! These are structural Java rules the compiler enforces regardless of types, so they're
//! false-positive-free from the syntax tree alone:
//!   * an `abstract` method only in an `abstract` class or an interface (never a concrete class);
//!   * `abstract` methods carry no body; a `default` method only in an interface;
//!   * illegal modifier combinations (two visibility modifiers, `abstract` + `private`/`static`/
//!     `final`/`native`, a class that's `abstract` *and* `final`, `final` + `volatile` field);
//!   * a `record` can't be `abstract` and can't declare instance fields;
//!   * an `enum` constant with arguments needs a constructor;
//!   * a `native` method carries no body;
//!   * an interface member is never `protected`, and a bodyless interface method is never `private`;
//!   * an `enum` constructor takes no access modifier (it is implicitly private).

use bennu_lombok::prelude::{generates_constructor, ParsedImport};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::support::nodes::{modifier_keywords};

const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// All declaration/modifier legality errors in `root`.
pub fn declaration_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    declaration_errors_nodes(&crate::engine::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn declaration_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // Parsed once for the file: only `check_enum` consults it, but it used to walk out to the
    // compilation unit and rescan the imports for every enum in the file.
    let imports = crate::support::lombok::imports_from_nodes(nodes, bytes);
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "method_declaration" => check_method(n, bytes, &mut out),
            "class_declaration" => check_class(n, bytes, &mut out),
            "interface_declaration" => check_interface(n, bytes, &mut out),
            "record_declaration" => check_record(n, bytes, &mut out),
            "enum_declaration" => check_enum(n, bytes, &imports, &mut out),
            "field_declaration" => check_field(n, bytes, &mut out),
            "constructor_declaration" => check_constructor(n, bytes, &mut out),
            _ => {}
        }
    }
    out
}

/// The nearest enclosing type declaration of `node`, if any.
///
/// An anonymous class body (`new I() { … }`, an enum constant's body) is a type too, just one with
/// no declaration node — reaching one answers `None`, because the declaration further out is not the
/// type the member belongs to: a method of an anonymous class written inside an interface is a class
/// method, and holding it to the interface's rules reported legal code.
fn enclosing_type(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if TYPE_DECLS.contains(&n.kind()) {
            return Some(n);
        }
        if n.kind() == "class_body"
            && n.parent().is_some_and(|p| matches!(p.kind(), "object_creation_expression" | "enum_constant"))
        {
            return None;
        }
        cur = n.parent();
    }
    None
}

/// Whether a type declaration sits directly in the compilation unit (not nested in another type).
fn is_top_level(n: Node) -> bool {
    n.parent().is_some_and(|p| p.kind() == "program")
}

/// Modifiers JLS §8.1.1 / §9.1.1 do not allow on a TOP-LEVEL type: they only make sense for a member.
fn check_top_level_modifiers(n: Node, mods: &[&str], out: &mut Vec<Diagnostic>) {
    if !is_top_level(n) {
        return;
    }
    for bad in ["private", "protected", "static"] {
        if mods.contains(&bad) {
            out.push(err(name_span(n), format!("Modifier `{bad}` is not allowed on a top-level type")));
        }
    }
}

/// `sealed`, `non-sealed` and `final` each say something different about who may extend the type,
/// so any two of them contradict each other (JLS §8.1.1.2).
fn check_sealing_modifiers(n: Node, mods: &[&str], out: &mut Vec<Diagnostic>) {
    let present: Vec<&str> =
        ["sealed", "non-sealed", "final"].into_iter().filter(|m| mods.contains(m)).collect();
    if present.len() > 1 {
        out.push(err(
            name_span(n),
            format!("Illegal combination of modifiers: {}", present.join(" and ")),
        ));
    }
}

/// The member modifiers a kind of declaration can never carry — `(modifier, what it is)` pairs for
/// the message. These are `compiler.err.mod.not.allowed.here` in every Java version.
fn check_disallowed(anchor: Node, mods: &[&str], disallowed: &[&str], what: &str, out: &mut Vec<Diagnostic>) {
    for bad in disallowed {
        if mods.contains(bad) {
            out.push(err(anchor, format!("Modifier `{bad}` is not allowed on {what}")));
        }
    }
}

fn err(node: Node, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        message: message.into(),
        severity: crate::engine::check_id::CheckId::IllegalDeclaration.severity().to_string(),
        code: crate::engine::check_id::CheckId::IllegalDeclaration.code().to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

/// The span to anchor a declaration error on: its NAME token (tighter than the whole decl).
fn name_span(node: Node) -> Node {
    node.child_by_field_name("name").unwrap_or(node)
}

fn visibility_count(mods: &[&str]) -> usize {
    mods.iter().filter(|m| matches!(**m, "public" | "private" | "protected")).count()
}

fn check_method(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mods = modifier_keywords(n, bytes);
    let has = |m: &str| mods.contains(&m);
    let is_abstract = has("abstract");
    let has_body = n.child_by_field_name("body").map(|b| b.kind() == "block").unwrap_or(false);
    let anchor = name_span(n);

    if visibility_count(&mods) > 1 {
        out.push(err(anchor, "Illegal combination of modifiers: only one of public/protected/private"));
    }
    if is_abstract {
        for bad in ["private", "static", "final", "native", "synchronized"] {
            if has(bad) {
                out.push(err(anchor, format!("Illegal combination of modifiers: abstract and {bad}")));
            }
        }
        if has_body {
            out.push(err(anchor, "Abstract method cannot have a body"));
        }
        // Placement: only concrete CLASSES are wrong (interfaces allow abstract; enums/records are
        // left to avoid a false positive on an enum's constant-bodied abstract method).
        if let Some(ty) = enclosing_type(n) {
            if ty.kind() == "class_declaration" && !modifier_keywords(ty, bytes).contains(&"abstract") {
                // On the class, where javac puts it: the class is what has to change (declare it
                // `abstract`), or the method does — and the class header names the obligation.
                let class_name = name_span(ty);
                let method = n.child_by_field_name("name").and_then(|m| m.utf8_text(bytes).ok()).unwrap_or("?");
                out.push(err(class_name, format!("Class is not abstract but declares the abstract method `{method}`")));
            }
        }
    }
    if has("default") {
        let in_interface = enclosing_type(n).map(|t| t.kind() == "interface_declaration").unwrap_or(false);
        if !in_interface {
            out.push(err(anchor, "Default methods are only allowed in interfaces"));
        }
    }
    // `native` says the body lives in another language; writing one here is a contradiction, and
    // javac says so (`compiler.err.native.meth.cant.have.body`).
    if has("native") && has_body {
        out.push(err(anchor, "Native method cannot have a body"));
    }
    // Interface members. `protected` is not a modifier an interface member can carry at all, in any
    // Java version. `private` IS legal since Java 9 — but only for a method with a body, because a
    // private abstract method could never be implemented.
    if enclosing_type(n).is_some_and(|t| t.kind() == "interface_declaration") {
        if has("protected") {
            out.push(err(anchor, "Modifier `protected` is not allowed on an interface member"));
        } else if has("private") && !has_body {
            out.push(err(anchor, "A `private` interface method must have a body"));
        }
        // An interface method is overridable by definition, and has no instance to lock or state
        // to keep: `final`, `synchronized` and `native` contradict what it is.
        check_disallowed(anchor, &mods, &["final", "synchronized", "native", "transient", "volatile"], "an interface method", out);
        // Only `default`, `static` and `private` methods carry a body; any other is abstract.
        if has_body && !has("default") && !has("static") && !has("private") {
            out.push(err(anchor, "Interface abstract methods cannot have a body"));
        }
    } else {
        check_disallowed(anchor, &mods, &["transient", "volatile"], "a method", out);
    }
}

/// An `enum` constructor is implicitly private, and JLS §8.9.2 forbids writing an access modifier on
/// it — `public E() {}` inside an enum is `compiler.err.mod.not.allowed.here`.
fn check_constructor(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    if !enclosing_type(n).is_some_and(|t| t.kind() == "enum_declaration") {
        return;
    }
    let mods = modifier_keywords(n, bytes);
    for bad in ["public", "protected"] {
        if mods.contains(&bad) {
            out.push(err(
                name_span(n),
                format!("Modifier `{bad}` is not allowed on an enum constructor"),
            ));
        }
    }
}

fn check_class(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mods = modifier_keywords(n, bytes);
    if mods.contains(&"abstract") && mods.contains(&"final") {
        out.push(err(name_span(n), "Illegal combination of modifiers: abstract and final"));
    }
    check_sealing_modifiers(n, &mods, out);
    check_top_level_modifiers(n, &mods, out);
    check_disallowed(name_span(n), &mods, &["transient", "volatile", "synchronized", "native", "default"], "a class", out);
}

fn check_interface(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mods = modifier_keywords(n, bytes);
    // An interface exists to be implemented; `final` would forbid exactly that.
    if mods.contains(&"final") {
        out.push(err(name_span(n), "Illegal combination of modifiers: interface and final"));
    }
    if mods.contains(&"sealed") && mods.contains(&"non-sealed") {
        out.push(err(name_span(n), "Illegal combination of modifiers: sealed and non-sealed"));
    }
    check_top_level_modifiers(n, &mods, out);
    check_disallowed(name_span(n), &mods, &["transient", "volatile", "synchronized", "native", "default"], "an interface", out);
}

fn check_record(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mods = modifier_keywords(n, bytes);
    if mods.contains(&"abstract") {
        out.push(err(name_span(n), "A record cannot be abstract"));
    }
    check_top_level_modifiers(n, &mods, out);
    check_disallowed(name_span(n), &mods, &["sealed", "non-sealed", "transient", "volatile", "synchronized", "native"], "a record", out);
    // Instance fields: a record's state is its components — only static fields are allowed in the body.
    if let Some(body) = n.child_by_field_name("body") {
        let mut c = body.walk();
        for member in body.named_children(&mut c) {
            if member.kind() == "field_declaration"
                && !modifier_keywords(member, bytes).contains(&"static")
            {
                out.push(err(member, "Records cannot declare instance fields"));
            }
            // A bare `{ … }` in a type body is an instance initializer; `static { … }` is a
            // `static_initializer` node, and that one a record may have.
            if member.kind() == "block" {
                out.push(err(member, "Records cannot declare instance initializers"));
            }
        }
    }
}

fn check_enum(n: Node, bytes: &[u8], imports: &[ParsedImport], out: &mut Vec<Diagnostic>) {
    // Whether an enum is abstract or final is decided by its constants, never written (JLS §8.9).
    let mods = modifier_keywords(n, bytes);
    check_top_level_modifiers(n, &mods, out);
    check_disallowed(name_span(n), &mods, &["abstract", "final", "sealed", "non-sealed", "transient", "volatile", "synchronized", "native"], "an enum", out);
    let Some(body) = n.child_by_field_name("body") else { return };
    // Lombok writes the constructor the constants call, so an annotated enum has one even though
    // the tree shows none. Without this, every `@AllArgsConstructor` enum with valued constants —
    // the standard way to write one — was reported as missing its constructor. Which annotations
    // carry a constructor is `bennu-lombok`'s to say, and it applies the "only if Lombok is really
    // in use" gate: a project's own `@AllArgsConstructor` in another package generates nothing.
    let mut has_ctor = crate::support::lombok::has_lombok_annotation(n, bytes, imports, |a| {
        generates_constructor(a.simple)
    });
    let mut arg_constant: Option<Node> = None;
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        match member.kind() {
            "enum_constant" => {
                if member.child_by_field_name("arguments").is_some() && arg_constant.is_none() {
                    arg_constant = Some(member);
                }
            }
            "enum_body_declarations" => {
                let mut dc = member.walk();
                for d in member.named_children(&mut dc) {
                    if d.kind() == "constructor_declaration" {
                        has_ctor = true;
                    }
                }
            }
            "constructor_declaration" => has_ctor = true,
            _ => {}
        }
    }
    if let (Some(constant), false) = (arg_constant, has_ctor) {
        out.push(err(
            name_span(constant),
            "Enum constant has arguments but the enum declares no matching constructor",
        ));
    }
}

fn check_field(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // Only class/interface members — a record's instance fields are handled in `check_record`, and a
    // local variable declaration is a different node (`local_variable_declaration`).
    let mods = modifier_keywords(n, bytes);
    if visibility_count(&mods) > 1 {
        out.push(err(n, "Illegal combination of modifiers: only one of public/protected/private"));
    }
    if mods.contains(&"final") && mods.contains(&"volatile") {
        out.push(err(n, "Illegal combination of modifiers: final and volatile"));
    }
    // A field is state, not behaviour: the method-only modifiers never apply to it.
    check_disallowed(n, &mods, &["abstract", "synchronized", "native", "strictfp", "default", "sealed", "non-sealed"], "a field", out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errs(src: &str) -> Vec<String> {
        let tree = parse(src);
        declaration_errors(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn native_method_with_a_body_is_flagged() {
        let e = errs("class C { native void m() { } }");
        assert!(e.iter().any(|m| m.contains("Native method")), "{e:?}");
    }

    #[test]
    fn native_method_without_a_body_is_ok() {
        assert!(errs("class C { native void m(); }").is_empty());
    }

    #[test]
    fn protected_interface_member_is_flagged() {
        let e = errs("interface I { protected void m(); }");
        assert!(e.iter().any(|m| m.contains("protected")), "{e:?}");
    }

    #[test]
    fn private_interface_method_without_a_body_is_flagged() {
        let e = errs("interface I { private void m(); }");
        assert!(e.iter().any(|m| m.contains("must have a body")), "{e:?}");
    }

    #[test]
    fn private_interface_method_with_a_body_is_ok() {
        // Legal since Java 9 — a private helper an interface's default methods can share.
        assert!(errs("interface I { private void m() { } }").is_empty());
    }

    #[test]
    fn public_enum_constructor_is_flagged() {
        let e = errs("enum E { X; public E() {} }");
        assert!(e.iter().any(|m| m.contains("enum constructor")), "{e:?}");
    }

    #[test]
    fn package_private_enum_constructor_is_ok() {
        assert!(errs("enum E { X; E() {} }").is_empty());
    }

    #[test]
    fn class_constructor_may_be_public() {
        assert!(errs("class C { public C() {} }").is_empty());
    }

    #[test]
    fn abstract_method_in_concrete_class_is_flagged() {
        let e = errs("class C { abstract void m(); }");
        assert!(e.iter().any(|m| m.contains("not abstract but declares the abstract method `m`")), "{e:?}");
    }

    #[test]
    fn abstract_method_in_abstract_class_is_ok() {
        assert!(errs("abstract class C { abstract void m(); }").is_empty());
    }

    #[test]
    fn abstract_method_in_interface_is_ok() {
        assert!(errs("interface I { void m(); abstract void n(); }").is_empty());
    }

    #[test]
    fn abstract_method_with_body_is_flagged() {
        let e = errs("abstract class C { abstract void m() {} }");
        assert!(e.iter().any(|m| m.contains("cannot have a body")), "{e:?}");
    }

    #[test]
    fn default_method_in_class_is_flagged() {
        let e = errs("class C { default void m() {} }");
        assert!(e.iter().any(|m| m.contains("only allowed in interfaces")), "{e:?}");
    }

    #[test]
    fn default_method_in_interface_is_ok() {
        assert!(errs("interface I { default void m() {} }").is_empty());
    }

    #[test]
    fn abstract_final_class_is_flagged() {
        let e = errs("abstract final class C {}");
        assert!(e.iter().any(|m| m.contains("abstract and final")), "{e:?}");
    }

    #[test]
    fn abstract_private_method_is_flagged() {
        let e = errs("abstract class C { abstract private void m(); }");
        assert!(e.iter().any(|m| m.contains("abstract and private")), "{e:?}");
    }

    #[test]
    fn abstract_final_method_is_flagged() {
        // `abstract final void m();` — the two modifiers are mutually exclusive on a method.
        let e = errs("abstract class C { abstract final void m(); }");
        assert!(e.iter().any(|m| m.contains("abstract and final")), "{e:?}");
    }

    #[test]
    fn two_visibility_modifiers_flagged() {
        let e = errs("class C { public private void m() {} }");
        assert!(e.iter().any(|m| m.contains("only one of")), "{e:?}");
    }

    #[test]
    fn record_cannot_be_abstract() {
        let e = errs("abstract record R(int x) {}");
        assert!(e.iter().any(|m| m.contains("record cannot be abstract")), "{e:?}");
    }

    #[test]
    fn record_instance_field_flagged_static_ok() {
        let e = errs("record R(int x) { int y; static int Z = 1; }");
        assert_eq!(
            e.iter().filter(|m| m.contains("instance fields")).count(),
            1,
            "only the non-static field y is flagged: {e:?}",
        );
    }

    #[test]
    fn enum_constant_with_args_needs_constructor() {
        let e = errs("enum E { RED(255), GREEN(0); }");
        assert!(e.iter().any(|m| m.contains("no matching constructor")), "{e:?}");
    }

    #[test]
    fn enum_constant_with_args_and_constructor_is_ok() {
        let src = "enum E { RED(255); private final int v; E(int v) { this.v = v; } }";
        assert!(errs(src).iter().all(|m| !m.contains("constructor")), "{:?}", errs(src));
    }

    #[test]
    fn plain_enum_without_args_is_ok() {
        assert!(errs("enum E { A, B, C }").is_empty());
    }

    /// The reported bug: the idiomatic Lombok enum — valued constants, a field, and the
    /// constructor generated by `@AllArgsConstructor` — was flagged as missing its constructor.
    #[test]
    fn lombok_generated_enum_constructor_counts() {
        let src = "import lombok.AllArgsConstructor;\n\
                   @AllArgsConstructor\n\
                   enum ProfiloEnum { OWNER(\"owner\"), SOLA_LETTURA(\"ro\");\n\
                     private final String label;\n }";
        assert!(
            errs(src).iter().all(|m| !m.contains("constructor")),
            "a Lombok-generated constructor exists at compile time: {:?}",
            errs(src),
        );
    }

    /// `@RequiredArgsConstructor` — the other common spelling for the same enum shape.
    #[test]
    fn lombok_required_args_constructor_counts_too() {
        let src = "import lombok.RequiredArgsConstructor;\n\
                   @RequiredArgsConstructor\n\
                   enum E { A(1); private final int v; }";
        assert!(errs(src).iter().all(|m| !m.contains("constructor")), "{:?}", errs(src));
    }

    /// The gate is the import, not the bare name: somebody's own `@AllArgsConstructor` generates
    /// nothing, so the missing constructor is still a real error.
    #[test]
    fn an_unimported_annotation_of_the_same_name_does_not_silence_the_check() {
        let src = "@AllArgsConstructor\nenum E { A(1); private final int v; }";
        assert!(
            errs(src).iter().any(|m| m.contains("no matching constructor")),
            "without a lombok import the annotation is somebody else's: {:?}",
            errs(src),
        );
    }

    /// And a plain enum with valued constants and no annotation at all still reports.
    #[test]
    fn lombok_import_alone_does_not_silence_the_check() {
        let src = "import lombok.Data;\nenum E { A(1); private final int v; }";
        assert!(
            errs(src).iter().any(|m| m.contains("no matching constructor")),
            "the annotation has to be ON the enum: {:?}",
            errs(src),
        );
    }

    #[test]
    fn final_volatile_field_flagged() {
        let e = errs("class C { final volatile int x = 0; }");
        assert!(e.iter().any(|m| m.contains("final and volatile")), "{e:?}");
    }

    #[test]
    fn final_interface_is_flagged() {
        let e = errs("final interface I {}");
        assert!(e.iter().any(|m| m.contains("interface and final")), "{e:?}");
    }

    #[test]
    fn abstract_enum_and_abstract_field_are_flagged() {
        assert!(errs("class C { abstract enum E { A } }").iter().any(|m| m.contains("`abstract`")));
        assert!(errs("class C { abstract int x; }").iter().any(|m| m.contains("on a field")));
    }

    #[test]
    fn transient_method_is_flagged() {
        let e = errs("class C { transient void m() {} }");
        assert!(e.iter().any(|m| m.contains("`transient`")), "{e:?}");
    }

    #[test]
    fn interface_method_rules() {
        let e = errs("interface I { final void a(); void b() {} }");
        assert!(e.iter().any(|m| m.contains("`final`")), "{e:?}");
        assert!(e.iter().any(|m| m.contains("cannot have a body")), "{e:?}");
        // default / static / private bodies are the legal ones.
        let ok = "interface I { default void a() {} static void b() {} private void c() {} void d(); }";
        assert!(errs(ok).is_empty(), "{:?}", errs(ok));
    }

    #[test]
    fn anonymous_class_inside_an_interface_follows_class_rules() {
        let src = "interface I { default Runnable r() { return new Runnable() { public void run() {} }; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn member_only_modifiers_on_a_top_level_type_are_flagged() {
        assert!(errs("private class C {}").iter().any(|m| m.contains("top-level")));
        assert!(errs("static class C {}").iter().any(|m| m.contains("top-level")));
        // Nested, the same modifiers are legal.
        assert!(errs("class O { private static class C {} }").is_empty());
    }

    #[test]
    fn sealed_and_final_contradict() {
        let e = errs("class O { sealed final static class C {} }");
        assert!(e.iter().any(|m| m.contains("sealed and final")), "{e:?}");
    }

    #[test]
    fn ordinary_class_is_clean() {
        let src = "public class C { private int x; public int get() { return x; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }
}
