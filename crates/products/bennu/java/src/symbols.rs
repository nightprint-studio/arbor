//! Symbol extraction: [`extract_symbols`]`(source) -> `[`FileSymbols`].
//!
//! Walks the tree-sitter-java CST once and pulls the package, imports, and each
//! top-level (and nested) type declaration with its methods and fields (including
//! declared type texts). No inference here — this is the structural model the
//! type-walk sits on.

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

/// A single import. `star` marks `import a.b.*;`; `static_` marks `import static`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// The dotted path exactly as written (`java.util.List`, or `java.util` for a
    /// star import).
    pub path: String,
    pub star: bool,
    pub static_: bool,
}

impl Import {
    /// The simple name a non-star import binds (`java.util.List` -> `List`). For a
    /// star import this is `None`.
    pub fn simple_name(&self) -> Option<&str> {
        if self.star {
            None
        } else {
            self.path.rsplit('.').next()
        }
    }
}

/// A field of a type: its name and its declared type (as written in source).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDecl {
    pub name: String,
    /// The declared type text, e.g. `Map<String, Object>` or `HttpServletRequest`.
    pub type_text: String,
    pub is_static: bool,
}

/// A parameter of a method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamDecl {
    pub name: String,
    pub type_text: String,
}

/// A method of a type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodDecl {
    pub name: String,
    /// Return type text (`void`, `HttpServletRequest`, `List<Foo>`).
    pub return_type_text: String,
    pub params: Vec<ParamDecl>,
    pub is_static: bool,
}

/// A type declaration (class / interface / enum).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub name: String,
    /// Fully-qualified name (`package.Outer.Inner` when nested).
    pub fqn: String,
    pub methods: Vec<MethodDecl>,
    pub fields: Vec<FieldDecl>,
    /// The `extends` clause type text, if any.
    pub extends: Option<String>,
    /// The `implements` clause type texts (interface `extends` folded in here too).
    pub implements: Vec<String>,
}

/// The extracted symbols of one `.java` file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSymbols {
    pub package: Option<String>,
    pub imports: Vec<Import>,
    pub types: Vec<TypeDecl>,
}

/// Parse `source` and extract its symbols. Never panics on malformed input —
/// tree-sitter always produces a tree (with ERROR nodes) and we skip what we can't
/// read (a partial/broken buffer is a normal editor state).
pub fn extract_symbols(source: &str) -> FileSymbols {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("load tree-sitter-java grammar");
    let Some(tree) = parser.parse(source, None) else {
        return FileSymbols { package: None, imports: Vec::new(), types: Vec::new() };
    };
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let mut package = None;
    let mut imports = Vec::new();
    let mut types = Vec::new();

    let mut cur = root.walk();
    for child in root.children(&mut cur) {
        match child.kind() {
            "package_declaration" => {
                package = child
                    .named_children(&mut child.walk())
                    .find(|n| n.kind() == "scoped_identifier" || n.kind() == "identifier")
                    .and_then(|n| node_text(&n, bytes));
            }
            "import_declaration" => {
                if let Some(imp) = parse_import(&child, bytes) {
                    imports.push(imp);
                }
            }
            "class_declaration" | "interface_declaration" | "enum_declaration" => {
                collect_type(&child, bytes, package.as_deref(), None, &mut types);
            }
            _ => {}
        }
    }

    FileSymbols { package, imports, types }
}

/// Parse an `import_declaration` node.
fn parse_import(node: &Node, bytes: &[u8]) -> Option<Import> {
    let raw = node_text(node, bytes)?;
    // `import static a.b.C;` — string surgery is robust here since tree-sitter gives
    // us the exact span of the declaration.
    let static_ = raw.contains("import static") || raw.contains("import  static");
    let star = raw.trim_end_matches(';').trim_end().ends_with('*');

    let mut cur = node.walk();
    let path_node = node
        .named_children(&mut cur)
        .find(|n| matches!(n.kind(), "scoped_identifier" | "identifier"));
    let path = path_node.and_then(|n| node_text(&n, bytes))?;
    Some(Import { path, star, static_ })
}

/// Collect a type declaration (recursing into nested types).
fn collect_type(
    node: &Node,
    bytes: &[u8],
    package: Option<&str>,
    outer_fqn: Option<&str>,
    out: &mut Vec<TypeDecl>,
) {
    let Some(name) = node.child_by_field_name("name").and_then(|n| node_text(&n, bytes)) else {
        return;
    };
    let fqn = match (outer_fqn, package) {
        (Some(o), _) => format!("{o}.{name}"),
        (None, Some(p)) => format!("{p}.{name}"),
        (None, None) => name.clone(),
    };

    let mut extends = None;
    let mut implements = Vec::new();
    if let Some(sc) = node.child_by_field_name("superclass") {
        extends = first_type_text(&sc, bytes);
    }
    if let Some(intf) = node.child_by_field_name("interfaces") {
        implements = all_type_texts(&intf, bytes);
    }
    // Interface `extends` list has no `interfaces` field; fold its supertypes into
    // `implements` for the member-walk (they behave identically for lookup).
    let mut cw = node.walk();
    for c in node.children(&mut cw) {
        if c.kind() == "extends_interfaces" {
            implements.extend(all_type_texts(&c, bytes));
        }
    }

    let mut methods = Vec::new();
    let mut fields = Vec::new();

    if let Some(body) = node.child_by_field_name("body") {
        let mut bw = body.walk();
        for m in body.named_children(&mut bw) {
            match m.kind() {
                "method_declaration" => {
                    if let Some(md) = parse_method(&m, bytes) {
                        methods.push(md);
                    }
                }
                "field_declaration" => {
                    parse_field(&m, bytes, &mut fields);
                }
                "class_declaration" | "interface_declaration" | "enum_declaration" => {
                    collect_type(&m, bytes, package, Some(&fqn), out);
                }
                _ => {}
            }
        }
    }

    out.push(TypeDecl { name, fqn, methods, fields, extends, implements });
}

/// Extract a method_declaration.
fn parse_method(node: &Node, bytes: &[u8]) -> Option<MethodDecl> {
    let name = node.child_by_field_name("name").and_then(|n| node_text(&n, bytes))?;
    let return_type_text = node
        .child_by_field_name("type")
        .and_then(|n| node_text(&n, bytes))
        .unwrap_or_else(|| "void".to_string());
    let is_static = has_modifier(node, bytes, "static");

    let mut params = Vec::new();
    if let Some(pl) = node.child_by_field_name("parameters") {
        let mut pw = pl.walk();
        for p in pl.named_children(&mut pw) {
            if p.kind() == "formal_parameter" || p.kind() == "spread_parameter" {
                let pname = p
                    .child_by_field_name("name")
                    .and_then(|n| node_text(&n, bytes))
                    .unwrap_or_default();
                let ptype = p
                    .child_by_field_name("type")
                    .and_then(|n| node_text(&n, bytes))
                    .unwrap_or_default();
                params.push(ParamDecl { name: pname, type_text: ptype });
            }
        }
    }

    Some(MethodDecl { name, return_type_text, params, is_static })
}

/// Extract the (possibly multiple) fields of a field_declaration (`int a, b, c;`).
fn parse_field(node: &Node, bytes: &[u8], out: &mut Vec<FieldDecl>) {
    let Some(type_text) = node.child_by_field_name("type").and_then(|n| node_text(&n, bytes))
    else {
        return;
    };
    let is_static = has_modifier(node, bytes, "static");
    let mut cw = node.walk();
    for c in node.named_children(&mut cw) {
        if c.kind() == "variable_declarator" {
            if let Some(name) = c.child_by_field_name("name").and_then(|n| node_text(&n, bytes)) {
                out.push(FieldDecl { name, type_text: type_text.clone(), is_static });
            }
        }
    }
}

/// Whether a declaration node has a given modifier keyword.
fn has_modifier(node: &Node, bytes: &[u8], keyword: &str) -> bool {
    let mut cw = node.walk();
    for c in node.children(&mut cw) {
        if c.kind() == "modifiers" {
            if let Some(t) = node_text(&c, bytes) {
                return t.split_whitespace().any(|w| w == keyword);
            }
        }
    }
    false
}

/// First type text under a `superclass` wrapper node.
fn first_type_text(node: &Node, bytes: &[u8]) -> Option<String> {
    let mut cw = node.walk();
    let found = node.named_children(&mut cw).find(|n| is_type_node(n));
    found.and_then(|n| node_text(&n, bytes))
}

/// All type texts under an `interfaces` / `type_list` wrapper.
fn all_type_texts(node: &Node, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    collect_type_texts(node, bytes, &mut out);
    out
}

fn collect_type_texts(node: &Node, bytes: &[u8], out: &mut Vec<String>) {
    let mut cw = node.walk();
    for c in node.named_children(&mut cw) {
        if is_type_node(&c) {
            if let Some(t) = node_text(&c, bytes) {
                out.push(t);
            }
        } else if c.kind() == "type_list" || c.kind() == "interface_type_list" {
            collect_type_texts(&c, bytes, out);
        }
    }
}

/// Whether a node is a type expression we want the text of.
fn is_type_node(n: &Node) -> bool {
    matches!(
        n.kind(),
        "type_identifier"
            | "generic_type"
            | "scoped_type_identifier"
            | "array_type"
            | "integral_type"
            | "floating_point_type"
            | "boolean_type"
            | "void_type"
    )
}

/// Exact source text of a node.
pub(crate) fn node_text(node: &Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(|s| s.to_string())
}
