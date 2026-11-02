//! A test skeleton for a method of the class under test, as ghost text: in `OrderTest`, starting `total`, `testTotal` or `void total`
//! draws a `@Test` method that builds an `Order` and calls `total()`, ready to be given its assertions.
//!
//! The class under test is found from the name and place of the test file — `src/test/java/.../OrderTest.java` tests
//! `src/main/java/.../Order.java` — and read from disk, so a project that keeps them elsewhere simply gets nothing. Nothing is guessed
//! beyond that: the arguments are the plain default of each parameter's type, and the assertions are left to the person who knows what
//! the method is for. Same block shape as `flc_derived` and `flc_stubs`.

use std::path::Path;

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use crate::flc_derived::{declares, text_of, Caret, Class, Derived};

const MIN_TYPED: usize = 3;
/// Words a test name starts with before the method's own.
const LEADS: [&str; 2] = ["test", "should"];

pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<Derived> {
    let subject = subject_path(file)?;
    let subject_source = std::fs::read_to_string(subject).ok()?;
    let caret = Caret::at(source, offset)?;
    let tree = parse_java(&caret.clean)?;
    let class = caret.class(&tree)?;
    render(&caret, &class, &subject_source)
}

/// `…/src/test/java/p/OrderTest.java` → `…/src/main/java/p/Order.java`.
fn subject_path(file: &str) -> Option<String> {
    let stem = Path::new(file).file_stem()?.to_str()?;
    let name = stem.strip_suffix("Tests").or_else(|| stem.strip_suffix("Test"))?;
    let marker = ["/src/test/java/", "\\src\\test\\java\\"].into_iter().find(|m| file.contains(m))?;
    let (root, rest) = file.split_once(marker)?;
    let dir = Path::new(rest).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let main = marker.replace("test", "main");
    let sep = &main[..1];
    let tail = if dir.is_empty() { format!("{name}.java") } else { format!("{dir}{sep}{name}.java") };
    Some(format!("{root}{main}{tail}"))
}

struct Candidate {
    name: String,
    returns_value: bool,
    is_static: bool,
    args: String,
}

fn render(caret: &Caret<'_>, class: &Class<'_>, subject_source: &str) -> Option<Derived> {
    if !class.name.ends_with("Test") && !class.name.ends_with("Tests") {
        return None;
    }
    let typed = typed_name(caret)?;
    let subject_name = class.name.strip_suffix("Tests").or_else(|| class.name.strip_suffix("Test"))?;
    let tree = parse_java(subject_source)?;
    let (candidates, constructor_args) = subject_members(tree.root_node(), subject_source, subject_name)?;
    let pick = candidates.iter().find(|c| matches(&c.name, &typed))?;
    let test_name = pick.name.clone();
    if declares(class.body, &caret.clean, &test_name) {
        return None;
    }
    let unit = caret.unit();
    let ind = format!("{}{unit}", caret.indent);
    let var = lower_first(subject_name);
    let target = if pick.is_static { subject_name.to_string() } else { var.clone() };
    let setup = if pick.is_static { String::new() } else { format!("{ind}{subject_name} {var} = new {subject_name}({constructor_args});\n") };
    let result = if pick.returns_value { "var result = " } else { "" };
    let junit4 = caret.source.contains("import org.junit.Test;");
    let visibility = if junit4 { "public " } else { "" };
    let annotation = if caret.annotated { String::new() } else { format!("@Test\n{}", caret.indent) };
    let text = format!("{annotation}{visibility}void {test_name}() {{\n{setup}{ind}{result}{target}.{}({});\n{}}}", pick.name, pick.args, caret.indent);
    Some(Derived { text, confidence: 0.8 })
}

/// What was typed of the test's name, without a `void`, a `test` or a `should` before it: `total`, `void testTotal`, `testTot`.
fn typed_name(caret: &Caret<'_>) -> Option<String> {
    let head = if caret.paren { caret.head.strip_suffix('(')?.trim_end() } else { caret.head };
    let word = match head.split_whitespace().collect::<Vec<_>>().as_slice() {
        [w] => *w,
        ["void", w] => *w,
        _ => return None,
    };
    let lower = word.to_lowercase();
    let bare = LEADS.iter().find_map(|l| lower.strip_prefix(l)).unwrap_or(&lower);
    (bare.len() >= MIN_TYPED || (caret.paren && !bare.is_empty())).then(|| bare.to_string())
}

fn matches(method: &str, typed: &str) -> bool {
    method.to_lowercase().starts_with(typed)
}

/// The methods a test can call (not `private`), and the arguments for `new Subject(…)`.
fn subject_members(root: Node<'_>, source: &str, subject: &str) -> Option<(Vec<Candidate>, String)> {
    let mut cursor = root.walk();
    let class = root.named_children(&mut cursor).find(|n| {
        n.kind() == "class_declaration" && n.child_by_field_name("name").is_some_and(|x| text_of(x, source) == subject)
    })?;
    let body = class.child_by_field_name("body")?;
    let mut out = Vec::new();
    let mut ctor_args = None::<String>;
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        match member.kind() {
            "constructor_declaration" => {
                let args = defaults(member.child_by_field_name("parameters"), source);
                // the smallest constructor is the one to call
                if ctor_args.as_ref().is_none_or(|a| args.len() < a.len()) && !modifiers(member, source).contains("private") {
                    ctor_args = Some(args);
                }
            }
            "method_declaration" => {
                let mods = modifiers(member, source);
                if mods.contains("private") {
                    continue;
                }
                let name = text_of(member.child_by_field_name("name")?, source).to_string();
                let returns_value = member.child_by_field_name("type").is_some_and(|t| text_of(t, source) != "void");
                out.push(Candidate { name, returns_value, is_static: mods.contains("static"), args: defaults(member.child_by_field_name("parameters"), source) });
            }
            _ => {}
        }
    }
    Some((out, ctor_args.unwrap_or_default()))
}

fn modifiers(node: Node<'_>, source: &str) -> String {
    let mut c = node.walk();
    let m = node.named_children(&mut c).find(|n| n.kind() == "modifiers").map(|m| text_of(m, source).to_string());
    m.unwrap_or_default()
}

/// The plain default of each parameter's type, in order: `0, "", null`.
fn defaults(params: Option<Node<'_>>, source: &str) -> String {
    let Some(params) = params else { return String::new() };
    let mut c = params.walk();
    let values: Vec<&str> = params
        .named_children(&mut c)
        .filter(|p| p.kind() == "formal_parameter")
        .map(|p| default_of(p.child_by_field_name("type").map(|t| text_of(t, source)).unwrap_or("")))
        .collect();
    values.join(", ")
}

fn default_of(ty: &str) -> &'static str {
    match ty {
        "byte" | "short" | "int" => "0",
        "long" => "0L",
        "float" => "0f",
        "double" => "0.0",
        "boolean" => "false",
        "char" => "'\\0'",
        "String" => "\"\"",
        _ => "null",
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_lowercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: &str = "package shop;\npublic class Order {\n    public Order(String id) {}\n    public int total(int tax, String code) { return 0; }\n    public void cancel() {}\n    static Order empty() { return null; }\n    private void hidden() {}\n}\n";

    fn at(head: &str) -> Option<Derived> {
        let source = format!("package shop;\n\nimport org.junit.jupiter.api.Test;\n\nclass OrderTest {{\n    {head}\n}}\n");
        let offset = source.rfind(head)? + head.len();
        let caret = Caret::at(&source, offset)?;
        let tree = parse_java(&caret.clean)?;
        let class = caret.class(&tree)?;
        render(&caret, &class, ORDER)
    }

    #[test]
    fn the_method_is_called_with_the_defaults_of_its_parameter_types() {
        let d = at("total").expect("offered");
        assert_eq!(d.text, "@Test\n    void total() {\n        Order order = new Order(\"\");\n        var result = order.total(0, \"\");\n    }");
    }

    #[test]
    fn test_and_void_before_the_name_are_understood_and_void_methods_keep_no_result() {
        assert!(at("testTot").is_some());
        assert!(at("void should_cancel").is_none(), "an underscore is not part of the method's name");
        let d = at("void cancel").expect("offered");
        assert!(d.text.contains("order.cancel();") && !d.text.contains("var result"));
    }

    #[test]
    fn a_static_method_is_called_on_the_class_and_private_ones_are_left_out() {
        let d = at("empty").expect("offered");
        assert!(d.text.contains("var result = Order.empty();") && !d.text.contains("new Order"));
        assert!(at("hidden").is_none());
        assert!(at("to").is_none(), "two letters is the start of too many things");
    }

    #[test]
    fn only_a_test_class_gets_a_skeleton_and_a_test_already_there_is_not_offered_again() {
        let source = "class Order {\n    total\n}\n";
        let caret = Caret::at(source, source.find("total").unwrap() + 5).unwrap();
        let tree = parse_java(&caret.clean).unwrap();
        assert!(render(&caret, &caret.class(&tree).unwrap(), ORDER).is_none());
        let source = "class OrderTest {\n    void total() {}\n\n    total\n}\n";
        let caret = Caret::at(source, source.rfind("total").unwrap() + 5).unwrap();
        let tree = parse_java(&caret.clean).unwrap();
        assert!(render(&caret, &caret.class(&tree).unwrap(), ORDER).is_none());
    }

    #[test]
    fn the_test_file_names_its_subject() {
        assert_eq!(subject_path("/p/src/test/java/shop/OrderTest.java").as_deref(), Some("/p/src/main/java/shop/Order.java"));
        assert_eq!(subject_path("/p/src/test/java/OrderTests.java").as_deref(), Some("/p/src/main/java/Order.java"));
        assert_eq!(subject_path("/p/src/main/java/shop/Order.java"), None);
    }
}
