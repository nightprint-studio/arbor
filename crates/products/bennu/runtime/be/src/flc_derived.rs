//! Blocks of code that follow from the class alone: nothing is guessed, so nothing here asks the model.
//!
//! Two kinds of block, both drawn as ghost text over several lines once the declaration is started:
//!
//! * the **constructor** — `public Order(` in a class with fields offers the parameters, the closing parenthesis and a body
//!   that assigns each one. It takes the instance fields without a value (a `static` field, or a `final` one that already has
//!   one, is not the constructor's to set), and it is not offered when the class already has a constructor with that many
//!   parameters or when the name typed is not the class's own;
//! * the three methods every class gets from `Object` — `public boolean equals(`, `public int hashCode(`,
//!   `public String toString(` — written from the instance fields: `==` for what is primitive, `Float.compare` and
//!   `Double.compare` for what is floating point, `Arrays.equals` for arrays, `Objects.equals` for the rest. `transient` fields
//!   are left out of `equals` and `hashCode`, not of `toString`. Not offered when the class already declares the method.
//!
//! Typing the **name alone** is enough — `toString`, `equals`, `hashCode`, or the class's own name at the start of a member — and so
//! is `public Order(` with the parenthesis the editor added. The answer is always the **whole declaration** from the first character
//! of the line (`@Override` above the `Object` methods, unless one is there), and the host replaces what was typed on the line and the
//! editor's closers after the caret with it, hiding them meanwhile — see `java-flc.ts`.

use bennu_java::prelude::parse_java;
use tree_sitter::{Node, Tree};

/// A block written out from the class, and how sure it is.
pub(crate) struct Derived {
    pub text: String,
    pub confidence: f32,
}

/// More fields than this are not a constructor or an `equals` anyone writes by hand.
const MAX_FIELDS: usize = 12;
/// A one-line expression longer than this is broken before each operator.
const WRAP_AT: usize = 100;

pub(crate) fn suggest(source: &str, offset: usize) -> Option<Derived> {
    let caret = Caret::at(source, offset)?;
    let tree = parse_java(&caret.clean)?;
    let class = caret.class(&tree)?;
    constructor(&caret, &class).or_else(|| object_method(&caret, &class))
}

/// Where the caret is: at the end of what has been typed of a declaration on its own line.
pub(crate) struct Caret<'s> {
    /// The source with the unfinished line blanked, so the class around it parses as it will once written.
    pub(crate) clean: String,
    pub(crate) source: &'s str,
    pub(crate) line_start: usize,
    pub(crate) indent: &'s str,
    /// The declaration typed so far without its modifiers: `Order(`, `String toString(`, `toStr`.
    pub(crate) head: &'s str,
    /// The access modifier the user typed (`public`, `protected`, `private`), or empty.
    pub(crate) modifier: &'s str,
    /// Whether it ends with the `(`, as opposed to only the name.
    pub(crate) paren: bool,
    /// Whether the line above is an annotation already (`@Override`).
    pub(crate) annotated: bool,
}

/// The class the caret is in.
pub(crate) struct Class<'t> {
    pub(crate) body: Node<'t>,
    pub(crate) node: Node<'t>,
    pub(crate) name: String,
}

impl<'s> Caret<'s> {
    pub(crate) fn at(source: &'s str, offset: usize) -> Option<Self> {
        if !source.is_char_boundary(offset) {
            return None;
        }
        let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
        let line_end = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
        // what the editor left after the caret: nothing, its auto-closed `)`, or `) {}`
        let after: String = source[offset..line_end].chars().filter(|c| !c.is_whitespace()).collect();
        let before = &source[line_start..offset];
        let typed = before.trim_start();
        let indent = &before[..before.len() - typed.len()];
        let paren = typed.ends_with('(');
        if !(if paren { matches!(after.as_str(), "" | ")" | "){" | "){}") } else { after.is_empty() }) {
            return None;
        }
        let (modifier, head) = ["public ", "protected ", "private "]
            .iter()
            .find_map(|m| typed.strip_prefix(m).map(|rest| (m.trim_end(), rest.trim_start())))
            .unwrap_or(("", typed));
        let plain = |c: char| c.is_alphanumeric() || matches!(c, '_' | '$' | ' ');
        if head.is_empty() || !(paren || head.chars().all(plain)) {
            return None;
        }
        let annotated = source[..line_start].lines().rev().find(|l| !l.trim().is_empty()).is_some_and(|l| l.trim_start().starts_with('@'));
        let mut clean = source.to_string();
        clean.replace_range(line_start..line_end, &" ".repeat(line_end - line_start));
        Some(Caret { clean, source, line_start, indent, head, modifier, paren, annotated })
    }

    pub(crate) fn unit(&self) -> &'static str {
        if self.indent.contains('\t') { "\t" } else { "    " }
    }

    pub(crate) fn class<'t>(&self, tree: &'t Tree) -> Option<Class<'t>> {
        let at = tree.root_node().descendant_for_byte_range(self.line_start, self.line_start)?;
        let body = std::iter::successors(Some(at), |n| n.parent()).find(|n| n.kind() == "class_body")?;
        let node = body.parent().filter(|c| c.kind() == "class_declaration")?;
        let name = text_of(node.child_by_field_name("name")?, &self.clean).to_string();
        Some(Class { body, node, name })
    }

    /// Whether `Type` is written short: the file imports it, or imports its whole package.
    fn uses(&self, qualified: &str) -> bool {
        let pkg = qualified.rsplit_once('.').map_or("", |(p, _)| p);
        self.source.contains(&format!("import {qualified};")) || self.source.contains(&format!("import {pkg}.*;"))
    }

    /// `Objects`, or `java.util.Objects` where the file does not import it.
    fn util(&self, simple: &str) -> String {
        if self.uses(&format!("java.util.{simple}")) { simple.to_string() } else { format!("java.util.{simple}") }
    }
}

pub(crate) fn text_of<'a>(n: Node<'_>, source: &'a str) -> &'a str {
    n.utf8_text(source.as_bytes()).unwrap_or("")
}

// ── the constructor ──────────────────────────────────────────────────────────────────────────────────────────────

fn constructor(caret: &Caret<'_>, class: &Class<'_>) -> Option<Derived> {
    let name = if caret.paren { caret.head.strip_suffix('(')?.trim_end() } else { caret.head };
    if name != class.name {
        return None;
    }
    let fields = fields_of(class.body, &caret.clean, |f| !f.is_static && !f.has_value);
    if fields.is_empty() || fields.len() > MAX_FIELDS || has_constructor_with(class.body, fields.len()) {
        return None;
    }
    let unit = caret.unit();
    let params = fields.iter().map(|f| format!("{} {}", f.ty, f.name)).collect::<Vec<_>>().join(", ");
    let assigns = fields.iter().map(|f| format!("{}{unit}this.{n} = {n};", caret.indent, n = f.name)).collect::<Vec<_>>().join("\n");
    let modifier = if caret.modifier.is_empty() { "public" } else { caret.modifier };
    Some(Derived { text: format!("{modifier} {name}({params}) {{\n{assigns}\n{}}}", caret.indent), confidence: 0.95 })
}

struct Field {
    ty: String,
    name: String,
    is_static: bool,
    is_transient: bool,
    has_value: bool,
}

/// The fields of the class that `keep` accepts, in declaration order.
fn fields_of(body: Node<'_>, source: &str, keep: impl Fn(&Field) -> bool) -> Vec<Field> {
    let mut out = Vec::new();
    let mut cursor = body.walk();
    for field in body.named_children(&mut cursor).filter(|n| n.kind() == "field_declaration") {
        let mut c = field.walk();
        let modifiers = field.named_children(&mut c).find(|n| n.kind() == "modifiers").map(|m| text_of(m, source)).unwrap_or("");
        let words: Vec<&str> = modifiers.split_whitespace().collect();
        let Some(ty) = field.child_by_field_name("type").map(|t| text_of(t, source).to_string()) else { continue };
        let mut c = field.walk();
        for decl in field.named_children(&mut c).filter(|n| n.kind() == "variable_declarator") {
            let Some(name) = decl.child_by_field_name("name") else { continue };
            let f = Field {
                ty: ty.clone(),
                name: text_of(name, source).to_string(),
                is_static: words.contains(&"static"),
                is_transient: words.contains(&"transient"),
                has_value: decl.child_by_field_name("value").is_some(),
            };
            if keep(&f) {
                out.push(f);
            }
        }
    }
    out
}

/// Whether the class already has a constructor with `n` parameters.
fn has_constructor_with(body: Node<'_>, n: usize) -> bool {
    let mut cursor = body.walk();
    let ctors: Vec<Node<'_>> = body.named_children(&mut cursor).filter(|c| c.kind() == "constructor_declaration").collect();
    ctors.into_iter().any(|c| {
        let Some(params) = c.child_by_field_name("parameters") else { return false };
        let mut k = params.walk();
        let count = params.named_children(&mut k).filter(|x| matches!(x.kind(), "formal_parameter" | "spread_parameter")).count();
        count == n
    })
}

// ── equals, hashCode, toString ───────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Equals,
    HashCode,
    ToString,
}

/// `(return type, name)` of the `Object` method that what was typed starts: `boolean equals(`, `toStr`, `String toString`.
fn object_signature(caret: &Caret<'_>) -> Option<(&'static str, &'static str)> {
    const SIGNATURES: [(&str, &str); 3] = [("boolean", "equals"), ("int", "hashCode"), ("String", "toString")];
    if caret.paren {
        let sig = caret.head.strip_suffix('(')?.trim_end();
        return SIGNATURES.into_iter().find(|(ret, name)| sig == format!("{ret} {name}"));
    }
    let words: Vec<&str> = caret.head.split_whitespace().collect();
    match words.as_slice() {
        // the name alone, or the start of it: four letters, so `has` on the way to `hasItems` does not offer a `hashCode`
        [w] if w.len() >= 4 => SIGNATURES.into_iter().find(|(_, name)| name.starts_with(w)),
        [ret, w] if w.len() >= 3 => SIGNATURES.into_iter().find(|(r, name)| r == ret && name.starts_with(w)),
        _ => None,
    }
}

fn object_method(caret: &Caret<'_>, class: &Class<'_>) -> Option<Derived> {
    let (ret, name) = object_signature(caret)?;
    let method = match name {
        "equals" => Method::Equals,
        "hashCode" => Method::HashCode,
        _ => Method::ToString,
    };
    if declares(class.body, &caret.clean, name) {
        return None;
    }
    let fields = fields_of(class.body, &caret.clean, |f| !f.is_static && (method == Method::ToString || !f.is_transient));
    if fields.is_empty() || fields.len() > MAX_FIELDS {
        return None;
    }
    let unit = caret.unit();
    let ind = format!("{}{unit}", caret.indent);
    let (params, body) = match method {
        Method::Equals => ("Object o", equals_body(caret, class, &fields, &ind, unit)),
        Method::HashCode => ("", hash_body(caret, &fields, &ind)),
        Method::ToString => ("", to_string_body(caret, class, &fields, &ind, unit)),
    };
    // `@Override` goes above unless one is there already: the block starts at the line's first character, so it carries its own
    let annotation = if caret.annotated { String::new() } else { format!("@Override\n{}", caret.indent) };
    Some(Derived { text: format!("{annotation}public {ret} {name}({params}) {{\n{body}\n{}}}", caret.indent), confidence: 0.95 })
}

pub(crate) fn declares(body: Node<'_>, source: &str, name: &str) -> bool {
    let mut cursor = body.walk();
    let methods: Vec<Node<'_>> = body.named_children(&mut cursor).filter(|c| c.kind() == "method_declaration").collect();
    methods.into_iter().any(|m| m.child_by_field_name("name").is_some_and(|n| text_of(n, source) == name))
}

fn is_primitive(ty: &str) -> bool {
    matches!(ty, "byte" | "short" | "int" | "long" | "char" | "boolean")
}

fn is_array(ty: &str) -> bool {
    ty.ends_with(']')
}

fn equals_body(caret: &Caret<'_>, class: &Class<'_>, fields: &[Field], ind: &str, unit: &str) -> String {
    let (objects, arrays) = (caret.util("Objects"), caret.util("Arrays"));
    // the other object's name: the class's own, lower-cased, unless that is taken
    let mut other = class.name.chars().next().map(|c| c.to_lowercase().collect::<String>() + &class.name[c.len_utf8()..]).unwrap_or_else(|| "that".into());
    if other == "o" || fields.iter().any(|f| f.name == other) || is_keyword(&other) {
        other = "that".into();
    }
    // a generic class is cast to its wildcard type: `Box<?> box = (Box<?>) o;`
    let ty = if class.node.child_by_field_name("type_parameters").is_some() { format!("{}<?>", class.name) } else { class.name.clone() };
    let tests: Vec<String> = fields
        .iter()
        .map(|f| {
            let n = &f.name;
            match f.ty.as_str() {
                t if is_primitive(t) => format!("{n} == {other}.{n}"),
                "float" => format!("Float.compare({other}.{n}, {n}) == 0"),
                "double" => format!("Double.compare({other}.{n}, {n}) == 0"),
                t if t.matches('[').count() > 1 => format!("{arrays}.deepEquals({n}, {other}.{n})"),
                t if is_array(t) => format!("{arrays}.equals({n}, {other}.{n})"),
                _ => format!("{objects}.equals({n}, {other}.{n})"),
            }
        })
        .collect();
    let ret = wrapped(&format!("{ind}return "), &tests, " && ", &format!("{ind}{unit}{unit}"), "&& ");
    format!(
        "{ind}if (this == o) {{\n{ind}{unit}return true;\n{ind}}}\n{ind}if (o == null || getClass() != o.getClass()) {{\n{ind}{unit}return false;\n{ind}}}\n{ind}{ty} {other} = ({ty}) o;\n{ret};"
    )
}

fn hash_body(caret: &Caret<'_>, fields: &[Field], ind: &str) -> String {
    let (objects, arrays) = (caret.util("Objects"), caret.util("Arrays"));
    let plain: Vec<&str> = fields.iter().filter(|f| !is_array(&f.ty)).map(|f| f.name.as_str()).collect();
    let arrs: Vec<&Field> = fields.iter().filter(|f| is_array(&f.ty)).collect();
    if arrs.is_empty() {
        return format!("{ind}return {objects}.hash({});", plain.join(", "));
    }
    let start = if plain.is_empty() { "1".to_string() } else { format!("{objects}.hash({})", plain.join(", ")) };
    let mut out = format!("{ind}int result = {start};");
    for f in arrs {
        let call = if f.ty.matches('[').count() > 1 { "deepHashCode" } else { "hashCode" };
        out.push_str(&format!("\n{ind}result = 31 * result + {arrays}.{call}({});", f.name));
    }
    out.push_str(&format!("\n{ind}return result;"));
    out
}

fn to_string_body(caret: &Caret<'_>, class: &Class<'_>, fields: &[Field], ind: &str, unit: &str) -> String {
    let arrays = caret.util("Arrays");
    // one piece per field — `", items=" + items` — so a long line breaks between fields and never inside one
    let mut pieces: Vec<String> = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        let label = if i == 0 { format!("\"{}{{\" + \"{}=\"", class.name, f.name) } else { format!("\", {}=\"", f.name) };
        let value = if is_array(&f.ty) { format!("{arrays}.{}({})", if f.ty.matches('[').count() > 1 { "deepToString" } else { "toString" }, f.name) } else { f.name.clone() };
        pieces.push(format!("{label} + {value}"));
    }
    pieces.push("'}'".into());
    format!("{};", wrapped(&format!("{ind}return "), &pieces, " + ", &format!("{ind}{unit}{unit}"), "+ "))
}

/// `lead` + the parts joined by `glue` on one line, or, when that is too long, one part per line each led by `glue`'s operator.
fn wrapped(lead: &str, parts: &[String], glue: &str, continuation: &str, operator: &str) -> String {
    let one = format!("{lead}{}", parts.join(glue));
    if one.len() <= WRAP_AT || parts.len() < 2 {
        return one;
    }
    let rest = parts[1..].iter().map(|p| format!("{continuation}{operator}{p}")).collect::<Vec<_>>().join("\n");
    format!("{lead}{}\n{rest}", parts[0])
}

fn is_keyword(w: &str) -> bool {
    matches!(w, "class" | "new" | "this" | "int" | "long" | "if" | "for" | "do" | "case" | "enum" | "default" | "package" | "import" | "return")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: &str = "package shop;\n\npublic class Order {\n    private final Customer customer;\n    private final List<Item> items;\n    private int count;\n    private static final int MAX = 3;\n    private final String code = \"x\";\n\n    ";

    fn at(head: &str, after: &str) -> Option<Derived> {
        let src = format!("{ORDER}{head}{after}\n}}\n");
        suggest(&src, ORDER.len() + head.len())
    }

    #[test]
    fn the_constructor_comes_from_the_instance_fields_without_a_value() {
        let d = at("public Order(", ")").expect("a constructor");
        assert_eq!(d.text, "public Order(Customer customer, List<Item> items, int count) {\n        this.customer = customer;\n        this.items = items;\n        this.count = count;\n    }");
        assert!(d.confidence > 0.9);
    }

    #[test]
    fn the_editors_closers_and_a_missing_modifier_are_handled() {
        assert!(at("public Order(", "").is_some());
        assert!(at("public Order(", ") {}").is_some());
        assert!(at("Order(", ")").is_some());
        assert!(at("protected Order(", ")").is_some());
    }

    #[test]
    fn other_names_other_shapes_and_text_after_the_caret_get_nothing() {
        assert!(at("public Customer(", ")").is_none(), "not the class's own name");
        assert!(at("public Order(Customer c", ")").is_none(), "parameters already started");
        assert!(at("public Order(", ") throws X {").is_none(), "a signature that is already written");
        assert!(suggest("public interface Order {\n    Order(", 31).is_none());
    }

    #[test]
    fn a_class_that_has_the_constructor_is_left_alone() {
        let src = "public class P {\n    private final int a;\n    private final int b;\n\n    public P(int a, int b) {\n        this.a = a;\n        this.b = b;\n    }\n\n    P(";
        let full = format!("{src})\n}}\n");
        assert!(suggest(&full, src.len()).is_none());
    }

    #[test]
    fn tabs_indent_the_body_with_tabs() {
        let src = "public class P {\n\tprivate int a;\n\n\tpublic P()\n}\n";
        let at = src.find("P()").unwrap() + 2;
        let d = suggest(src, at).expect("a constructor");
        assert_eq!(d.text, "public P(int a) {\n\t\tthis.a = a;\n\t}");
    }

    #[test]
    fn no_fields_no_constructor() {
        assert!(suggest("public class P {\n    public P()\n}\n", "public class P {\n    public P(".len()).is_none());
    }

    #[test]
    fn to_string_names_the_class_and_every_field_including_static_ones_left_out() {
        let d = at("public String toString(", ")").expect("toString");
        assert_eq!(
            d.text,
            "@Override\n    public String toString() {\n        return \"Order{\" + \"customer=\" + customer\n                + \", items=\" + items\n                + \", count=\" + count\n                + \", code=\" + code\n                + '}';\n    }"
        );
    }

    #[test]
    fn hash_code_hashes_the_fields_and_equals_compares_them() {
        let h = at("public int hashCode(", ")").expect("hashCode");
        assert_eq!(h.text, "@Override\n    public int hashCode() {\n        return java.util.Objects.hash(customer, items, count, code);\n    }");
        let e = at("public boolean equals(", ")").expect("equals").text;
        assert!(e.starts_with("@Override\n    public boolean equals(Object o) {\n        if (this == o) {\n            return true;\n        }\n"));
        assert!(e.contains("Order order = (Order) o;"));
        assert!(e.contains("java.util.Objects.equals(customer, order.customer)"));
        assert!(e.contains("count == order.count"));
        assert!(e.ends_with(";\n    }"));
    }

    #[test]
    fn what_the_file_imports_is_written_short_and_floating_point_and_arrays_have_their_own_forms() {
        let src = "import java.util.Objects;\nimport java.util.Arrays;\n\nclass Box<T> {\n    double w;\n    int[] ids;\n    transient int cache;\n    T value;\n\n    boolean equals(\n}\n";
        let at = src.find("equals(").unwrap() + 7;
        let e = suggest(src, at).expect("equals").text;
        assert!(e.contains("Box<?> box = (Box<?>) o;"));
        assert!(e.contains("Double.compare(box.w, w) == 0"));
        assert!(e.contains("Arrays.equals(ids, box.ids)"));
        assert!(e.contains("Objects.equals(value, box.value)"));
        assert!(!e.contains("cache"), "a transient field is not compared");
        let at = src.replace("boolean equals(", "int hashCode(");
        let h = suggest(&at, at.find("hashCode(").unwrap() + 9).expect("hashCode").text;
        assert_eq!(h, "@Override\n    public int hashCode() {\n        int result = Objects.hash(w, value);\n        result = 31 * result + Arrays.hashCode(ids);\n        return result;\n    }");
    }

    #[test]
    fn the_name_alone_is_enough_and_an_override_already_there_is_not_repeated() {
        // the class's own name at the start of a member
        let c = at("Order", "").expect("a constructor from the name");
        assert!(c.text.starts_with("public Order(Customer customer, List<Item> items, int count) {"));
        // the method's name, whole or started, with or without its return type
        for typed in ["toString", "toStr", "String toString", "String toSt", "public toString"] {
            let d = at(typed, "").unwrap_or_else(|| panic!("{typed}"));
            assert!(d.text.starts_with("@Override\n    public String toString() {"), "{typed}: {}", d.text);
        }
        assert!(at("hashCode", "").unwrap().text.contains("public int hashCode()"));
        // fewer than four letters, a return type that does not fit, other words
        assert!(at("has", "").is_none() && at("int equals", "").is_none() && at("hashes", "").is_none() && at("Customer", "").is_none());
        // text after the caret is not ours in name mode
        assert!(at("toString", "x").is_none());
        // `@Override` on the line above: not written twice
        let src = format!("{ORDER}@Override\n    toString\n}}\n");
        let d = suggest(&src, ORDER.len() + "@Override\n    toString".len()).expect("toString");
        assert!(d.text.starts_with("public String toString() {"), "{}", d.text);
    }

    #[test]
    fn a_method_the_class_already_has_is_not_offered_again() {
        let src = "class P {\n    int a;\n\n    @Override\n    public String toString() {\n        return \"P\";\n    }\n\n    public int hashCode(\n}\n";
        assert!(suggest(src, src.find("hashCode(").unwrap() + 9).is_some());
        let again = src.replace("public int hashCode(\n", "public String toString(\n");
        assert!(suggest(&again, again.rfind("toString(").unwrap() + 9).is_none());
    }

    #[test]
    fn a_long_expression_breaks_before_each_operator() {
        let fields = (0..8).map(|i| format!("    private String longFieldName{i};\n")).collect::<String>();
        let src = format!("class P {{\n{fields}\n    public String toString(\n}}\n");
        let d = suggest(&src, src.find("toString(").unwrap() + 9).expect("toString").text;
        assert!(d.lines().filter(|l| l.trim_start().starts_with("+ ")).count() > 4, "{d}");
    }
}
