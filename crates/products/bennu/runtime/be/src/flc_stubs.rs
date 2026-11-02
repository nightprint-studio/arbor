//! The methods a class still owes its supertypes, as ghost text: typing the start of a name the class has to implement or may
//! override — `get`, `void ru`, `public int compareTo(` — offers the whole declaration, `@Override` above and a body that compiles.
//!
//! Nothing is guessed: the list comes from the index (the same one behind the "Implement / override methods" dialog), the name typed
//! only picks from it. Abstract methods go first, since the compiler demands them; a method the class already declares with that many
//! parameters is not offered again, and neither is one whose types the file would need an import for — ghost text cannot add one, so
//! that case stays with the dialog. Same shape as the blocks of `flc_derived`, and the host treats them alike.

use bennu_query::prelude::Overridable;

use crate::flc_derived::{text_of, Caret, Class, Derived};

/// A name shorter than this is the start of too many things.
const MIN_TYPED: usize = 3;
/// The `Object` methods have their own layer, which writes a body from the fields.
const OBJECT_METHODS: [&str; 3] = ["equals", "hashCode", "toString"];

pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<Derived> {
    let caret = Caret::at(source, offset)?;
    if caret.modifier == "private" || typed(&caret).is_none() {
        return None;
    }
    let tree = bennu_java::prelude::parse_java(&caret.clean)?;
    let class = caret.class(&tree)?;
    let options = crate::index_service::IndexService::global().overridable_at(file, &caret.clean, caret.line_start);
    render(&caret, &class, &options)
}

/// `(return type typed, name or start of it)` from what was written: `get`, `int comp`, `void run(`.
fn typed<'a>(caret: &Caret<'a>) -> Option<(Option<&'a str>, &'a str)> {
    let head = if caret.paren { caret.head.strip_suffix('(')?.trim_end() } else { caret.head };
    let words: Vec<&str> = head.split_whitespace().collect();
    match words.as_slice() {
        [name] if name.len() >= MIN_TYPED || (caret.paren && !name.is_empty()) => Some((None, name)),
        [ret, name] if !name.is_empty() => Some((Some(*ret), name)),
        _ => None,
    }
}

fn render(caret: &Caret<'_>, class: &Class<'_>, options: &[Overridable]) -> Option<Derived> {
    let (ret, name) = typed(caret)?;
    let lower = name.to_lowercase();
    let mut matching: Vec<&Overridable> = options
        .iter()
        .filter(|o| !OBJECT_METHODS.contains(&o.name.as_str()))
        .filter(|o| if caret.paren { o.name == name } else { o.name.to_lowercase().starts_with(&lower) })
        .filter(|o| ret.is_none_or(|r| r == o.return_type))
        .filter(|o| !declares_with(class, &caret.clean, &o.name, o.params.len()))
        .filter(|o| o.types.iter().all(|t| nameable(&caret.clean, t)))
        .collect();
    // abstract ones are demanded by the compiler; a method that is only overridable must be asked for by its full name
    matching.sort_by_key(|o| !o.is_abstract);
    let pick = *matching.first()?;
    if !pick.is_abstract && pick.name != name {
        return None;
    }
    let unit = caret.unit();
    let params = pick.params.iter().map(|(ty, n)| format!("{ty} {n}")).collect::<Vec<_>>().join(", ");
    let throws = if pick.throws.is_empty() { String::new() } else { format!(" throws {}", pick.throws.join(", ")) };
    let visibility = match pick.visibility.as_str() {
        "public" => "public ",
        "protected" => "protected ",
        _ => "",
    };
    let body = match default_value(&pick.return_type) {
        Some(v) => format!("{}{unit}return {v};\n", caret.indent),
        None => String::new(),
    };
    let annotation = if caret.annotated { String::new() } else { format!("@Override\n{}", caret.indent) };
    let text = format!("{annotation}{visibility}{} {}({params}){throws} {{\n{body}{}}}", pick.return_type, pick.name, caret.indent);
    Some(Derived { text, confidence: if pick.is_abstract { 0.9 } else { 0.8 } })
}

/// The value that makes a body compile: `None` for `void`.
fn default_value(ty: &str) -> Option<&'static str> {
    match ty {
        "void" => None,
        "boolean" => Some("false"),
        "byte" | "short" | "int" => Some("0"),
        "long" => Some("0L"),
        "float" => Some("0f"),
        "double" => Some("0.0"),
        "char" => Some("'\\0'"),
        _ => Some("null"),
    }
}

/// Whether the class already declares `name` with `arity` parameters.
fn declares_with(class: &Class<'_>, source: &str, name: &str, arity: usize) -> bool {
    let mut cursor = class.body.walk();
    let methods: Vec<_> = class.body.named_children(&mut cursor).filter(|c| c.kind() == "method_declaration").collect();
    methods.into_iter().any(|m| {
        let same_name = m.child_by_field_name("name").is_some_and(|n| text_of(n, source) == name);
        let Some(params) = m.child_by_field_name("parameters") else { return false };
        let mut k = params.walk();
        let count = params.named_children(&mut k).filter(|x| matches!(x.kind(), "formal_parameter" | "spread_parameter")).count();
        same_name && count == arity
    })
}

/// Whether the file can write the JVM binary name `t` short: it is a primitive, in `java.lang`, imported (alone or by its package),
/// or in the file's own package.
fn nameable(source: &str, t: &str) -> bool {
    let outer = t.split('$').next().unwrap_or(t);
    let Some((pkg, _)) = outer.rsplit_once('.') else { return true };
    pkg == "java.lang"
        || source.contains(&format!("import {outer};"))
        || source.contains(&format!("import {pkg}.*;"))
        || source.lines().any(|l| l.trim() == format!("package {pkg};"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn option(name: &str, ret: &str, params: &[(&str, &str)], is_abstract: bool, types: &[&str]) -> Overridable {
        Overridable {
            name: name.into(),
            params: params.iter().map(|(t, n)| (t.to_string(), n.to_string())).collect(),
            return_type: ret.into(),
            visibility: "public".into(),
            is_abstract,
            throws: Vec::new(),
            declaring_type: "x.Y".into(),
            signature: String::new(),
            types: types.iter().map(|t| t.to_string()).collect(),
        }
    }

    /// `head` is what is typed on the member's line; the class is `Order implements Comparable<Order>`.
    fn at(head: &str, options: &[Overridable]) -> Option<Derived> {
        let source = format!("package shop;\n\nimport java.util.List;\n\nclass Order {{\n    int total;\n\n    {head}\n}}\n");
        let offset = source.find(head)? + head.len();
        let caret = Caret::at(&source, offset)?;
        let tree = parse_java(&caret.clean)?;
        let class = caret.class(&tree)?;
        render(&caret, &class, options)
    }

    #[test]
    fn an_abstract_method_is_written_out_with_a_body_that_compiles() {
        let o = [option("compareTo", "int", &[("Order", "other")], true, &["shop.Order"])];
        let d = at("compa", &o).expect("offered");
        assert_eq!(d.text, "@Override\n    public int compareTo(Order other) {\n        return 0;\n    }");
        let d = at("public int compareTo(", &o).expect("offered with the parenthesis");
        assert!(d.text.starts_with("@Override\n    public int compareTo(Order other)"));
    }

    #[test]
    fn void_gets_an_empty_body_and_checked_exceptions_are_declared() {
        let mut run = option("close", "void", &[], true, &[]);
        run.throws = vec!["IOException".into()];
        let d = at("clo", &[run]).expect("offered");
        assert_eq!(d.text, "@Override\n    public void close() throws IOException {\n    }");
    }

    #[test]
    fn short_names_the_object_methods_and_declared_ones_get_nothing() {
        let o = [option("compareTo", "int", &[("Order", "other")], true, &["shop.Order"]), option("toString", "String", &[], false, &[])];
        assert!(at("co", &o).is_none(), "two letters is the start of too many things");
        assert!(at("toStr", &o).is_none(), "toString has its own layer");
        let source = "class Order {\n    public int compareTo(Order other) { return 0; }\n\n    compa\n}\n";
        let caret = Caret::at(source, source.rfind("compa").unwrap() + 5).unwrap();
        let tree = parse_java(&caret.clean).unwrap();
        let class = caret.class(&tree).unwrap();
        assert!(render(&caret, &class, &o).is_none(), "already declared with that many parameters");
    }

    #[test]
    fn a_type_the_file_would_need_an_import_for_is_left_to_the_dialog() {
        let needs = [option("items", "java.util.Map", &[], true, &["java.util.Map"])];
        assert!(at("ite", &needs).is_none());
        let has = [option("items", "List<String>", &[], true, &["java.util.List"])];
        assert!(at("ite", &has).is_some(), "java.util.List is imported");
    }

    #[test]
    fn a_method_that_is_only_overridable_needs_its_full_name_and_abstract_ones_go_first() {
        let o = [option("handle", "void", &[], false, &[]), option("handleAll", "void", &[], true, &[])];
        assert!(at("hand", &[o[0].clone()]).is_none());
        assert!(at("handle", &[o[0].clone()]).is_some());
        assert!(at("hand", &o).expect("offered").text.contains("handleAll"));
    }

    #[test]
    fn the_return_type_typed_must_match() {
        let o = [option("size", "int", &[], true, &[])];
        assert!(at("int siz", &o).is_some());
        assert!(at("long siz", &o).is_none());
    }

    #[test]
    fn defaults_follow_the_return_type() {
        for (ty, v) in [("boolean", "false"), ("long", "0L"), ("double", "0.0"), ("String", "null")] {
            assert_eq!(default_value(ty), Some(v));
        }
        assert_eq!(default_value("void"), None);
    }
}
