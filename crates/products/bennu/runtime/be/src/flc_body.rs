//! The body of an accessor, drawn when the caret is on the empty line under its signature.
//!
//! `public String getName() {` with a field `name` of the same type offers `return name;`; `public void setName(String name) {` offers
//! `this.name = name;`, and `return this;` as well for a setter that returns the class. The property comes from the method's name
//! (`getX` / `isX` / `hasX` / `setX` → `x`, or `isX` → the field `isX` when that is what the class has), the style from the class: if
//! the other accessors of the file write `this.x` the new one does too. Nothing is guessed, so nothing here asks the model: no field
//! of that name and type, no suggestion.
//!
//! Measured on public held-out code (`mlbench::m4_body`) against the retrieve-and-adapt generator it replaces for these two kinds of
//! method: see `docs/bennu-ml-bench.md`.

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

/// A block of statements drawn at the caret, and how sure it is.
pub(crate) struct Body {
    /// What goes in at the caret: the first line without its indentation (the caret is already past it), the others with it.
    pub text: String,
    pub confidence: f32,
}

const GETTER: f32 = 0.95;
const SETTER: f32 = 0.9;

fn text<'s>(src: &'s str, n: &Node) -> &'s str {
    &src[n.start_byte()..n.end_byte()]
}

fn property_of(method: &str) -> Option<(&'static str, String)> {
    let (kind, rest) = [("get", "get"), ("is", "get"), ("has", "get"), ("set", "set")].iter().find_map(|(p, k)| method.strip_prefix(p).map(|r| (*k, r)))?;
    let mut c = rest.chars();
    let first = c.next().filter(|f| f.is_uppercase())?;
    Some((kind, format!("{}{}", first.to_lowercase(), c.as_str())))
}

fn simple(t: &str) -> &str {
    t.trim()
}

pub(crate) struct Field {
    pub ty: String,
    pub name: String,
    pub is_static: bool,
}

pub(crate) fn fields_of(src: &str, class_body: &Node) -> Vec<Field> {
    let mut out = Vec::new();
    let mut c = class_body.walk();
    for f in class_body.children(&mut c).filter(|f| f.kind() == "field_declaration") {
        let ty = f.child_by_field_name("type").map(|t| text(src, &t).to_string()).unwrap_or_default();
        let is_static = text(src, &f).split('=').next().unwrap_or("").split_whitespace().any(|w| w == "static");
        let mut c2 = f.walk();
        for d in f.children(&mut c2).filter(|d| d.kind() == "variable_declarator") {
            if let Some(n) = d.child_by_field_name("name") {
                out.push(Field { ty: ty.clone(), name: text(src, &n).to_string(), is_static });
            }
        }
    }
    out
}

/// How the other one-statement accessors write the field: `Some(true)` for `this.x`, `Some(false)` for the bare name, `None` when
/// there is none to follow or they split evenly.
fn tally(src: &str, scope: &Node, skip: usize) -> Option<bool> {
    let (mut with, mut without) = (0, 0);
    let mut stack = vec![*scope];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.kind() == "method_declaration" && ch.start_byte() != skip {
                let (Some(name), Some(body)) = (ch.child_by_field_name("name").map(|n| text(src, &n)), ch.child_by_field_name("body")) else { continue };
                if property_of(name).is_some() && body.named_child_count() == 1 {
                    if text(src, &body).contains("this.") { with += 1 } else { without += 1 }
                }
            } else {
                stack.push(ch);
            }
        }
    }
    (with != without).then_some(with > without)
}

/// The class's own accessors decide; failing them, the other classes of the file; failing those, the bare name.
fn writes_this(src: &str, root: &Node, class_body: &Node, skip: usize) -> bool {
    tally(src, class_body, skip).or_else(|| tally(src, root, skip)).unwrap_or(false)
}

pub(crate) fn suggest(source: &str, offset: usize) -> Option<Body> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let present = &source[line_start..offset];
    // the caret is on an empty line, with at most the editor's own `}` on a later one
    let line_end = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
    if !present.trim().is_empty() || !source[offset..line_end].trim().is_empty() {
        return None;
    }
    let tree = parse_java(source)?;
    let at = source[..line_start].trim_end().len().saturating_sub(1);
    let mut node = tree.root_node().descendant_for_byte_range(at, at)?;
    // up to the block whose opening brace ends the previous line
    let body = loop {
        if node.kind() == "block" && node.parent().is_some_and(|p| p.kind() == "method_declaration") && node.start_byte() == at {
            break node;
        }
        node = node.parent()?;
    };
    if body.named_child_count() != 0 {
        return None;
    }
    let method = body.parent()?;
    let name = text(source, &method.child_by_field_name("name")?);
    let ret = simple(text(source, &method.child_by_field_name("type")?)).to_string();
    let (kind, property) = property_of(name)?;
    let mut params = Vec::new();
    if let Some(list) = method.child_by_field_name("parameters") {
        let mut pc = list.walk();
        for p in list.children(&mut pc).filter(|p| p.kind() == "formal_parameter") {
            if let (Some(t), Some(n)) = (p.child_by_field_name("type"), p.child_by_field_name("name")) {
                params.push((simple(text(source, &t)).to_string(), text(source, &n).to_string()));
            }
        }
    }
    let class_body = method.parent().filter(|p| p.kind() == "class_body" || p.kind() == "enum_body")?;
    let class_name = class_body.parent().and_then(|c| c.child_by_field_name("name")).map(|n| text(source, &n).to_string()).unwrap_or_default();
    let fields = fields_of(source, &class_body);
    let this = writes_this(source, &tree.root_node(), &class_body, method.start_byte());
    let statements: Vec<String>;
    let confidence;
    match kind {
        "get" if params.is_empty() && ret != "void" => {
            // `isActive` on a field `active`, or on a field called `isActive`
            let field = fields.iter().filter(|f| !f.is_static).find(|f| f.name == property && simple(&f.ty) == ret).or_else(|| fields.iter().filter(|f| !f.is_static).find(|f| f.name == name && simple(&f.ty) == ret))?;
            // an array is handed out as a copy or as it is, depending on the class: that is not this layer's to guess
            if field.ty.ends_with("]") {
                return None;
            }
            statements = vec![if this { format!("return this.{};", field.name) } else { format!("return {};", field.name) }];
            confidence = GETTER;
        }
        "set" if params.len() == 1 => {
            let field = fields.iter().filter(|f| !f.is_static).find(|f| f.name == property && simple(&f.ty) == params[0].0)?;
            let target = if this || params[0].1 == field.name { format!("this.{}", field.name) } else { field.name.clone() };
            let mut s = vec![format!("{target} = {};", params[0].1)];
            match ret.as_str() {
                "void" => {}
                r if r == class_name => s.push("return this;".to_string()),
                _ => return None,
            }
            statements = s;
            confidence = SETTER;
        }
        _ => return None,
    }
    // one indent deeper than the signature, in the file's own unit
    let sig_line_start = source[..at].rfind('\n').map_or(0, |p| p + 1);
    let sig_indent: String = source[sig_line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let unit = if sig_indent.contains('\t') { "\t" } else { "    " };
    let inner = format!("{sig_indent}{unit}");
    let lead = inner.strip_prefix(present).unwrap_or("");
    let joined = statements.join(&format!("\n{inner}"));
    Some(Body { text: format!("{lead}{joined}"), confidence })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A class with `body` as the members; the caret goes on the empty line the `§` stands for.
    fn at(members: &str) -> (String, usize) {
        let src = format!("public class Order {{\n{members}\n}}\n");
        let offset = src.find('§').unwrap();
        (src.replacen('§', "", 1), offset)
    }

    fn drawn(members: &str) -> Option<String> {
        let (src, offset) = at(members);
        suggest(&src, offset).map(|b| b.text)
    }

    #[test]
    fn a_getter_returns_the_field_its_name_derives() {
        let got = drawn("    private String name;\n\n    public String getName() {\n        §\n    }");
        assert_eq!(got.as_deref(), Some("return name;"));
        let flag = drawn("    private boolean active;\n    public boolean isActive() {\n        §\n    }");
        assert_eq!(flag.as_deref(), Some("return active;"));
    }

    #[test]
    fn the_class_decides_between_this_and_the_bare_name() {
        let members = "    private int a;\n    private int b;\n    public int getA() {\n        return this.a;\n    }\n    public int getB() {\n        §\n    }";
        assert_eq!(drawn(members).as_deref(), Some("return this.b;"));
    }

    #[test]
    fn a_setter_assigns_and_a_fluent_one_returns_the_object() {
        assert_eq!(drawn("    private String name;\n    public void setName(String value) {\n        §\n    }").as_deref(), Some("name = value;"));
        assert_eq!(drawn("    private String name;\n    public void setName(String name) {\n        §\n    }").as_deref(), Some("this.name = name;"));
        assert_eq!(drawn("    private String name;\n    public Order setName(String name) {\n        §\n    }").as_deref(), Some("this.name = name;\n        return this;"));
    }

    #[test]
    fn nothing_is_drawn_without_a_field_of_that_name_and_type() {
        assert_eq!(drawn("    private String label;\n    public String getName() {\n        §\n    }"), None);
        assert_eq!(drawn("    private int name;\n    public String getName() {\n        §\n    }"), None);
        assert_eq!(drawn("    private static String name;\n    public String getName() {\n        §\n    }"), None);
        assert_eq!(drawn("    private String name;\n    public Order setName(String x, int y) {\n        §\n    }"), None);
    }

    #[test]
    fn it_only_speaks_on_an_empty_line_under_an_empty_body() {
        assert_eq!(drawn("    private String name;\n    public String getName() {\n        return §name;\n    }"), None);
        assert_eq!(drawn("    private String name;\n    public String getName() {\n        int x = 1;\n        §\n    }"), None);
        assert_eq!(drawn("    private String name;\n    public String size() {\n        §\n    }"), None);
        let (src, _) = at("    private String name;\n    public String getName() {\n        §\n    }");
        assert!(suggest(&src, src.len() + 5).is_none());
    }

    #[test]
    fn the_first_line_leaves_out_the_indentation_already_typed() {
        let members = "    private String name;\n    public String getName() {\n        §\n    }";
        let (src, offset) = at(members);
        // the user has typed the indentation: the caret is past it
        let typed = format!("{}{}", &src[..offset], "");
        assert_eq!(suggest(&typed, offset).unwrap().text, "return name;");
        // nothing typed on the line: the indentation comes with the text
        let (src2, off2) = at("    private String name;\n    public String getName() {\n§\n    }");
        assert_eq!(suggest(&src2, off2).unwrap().text, "        return name;");
    }
}
