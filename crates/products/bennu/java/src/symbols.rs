//! Symbol extraction: [`extract_symbols`]`(source) -> `[`FileSymbols`].
//!
//! Walks the tree-sitter-java CST once and pulls the package, imports, and each
//! top-level (and nested) type declaration with its methods and fields (including
//! declared type texts). No inference here — this is the structural model the
//! type-walk sits on.

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

use crate::seam::Visibility;

/// What kind of type a [`TypeDecl`] declares. Drives the class-level flags the
/// inheritance / implement-abstract checks read for **project-source** supertypes
/// (`interface`/`enum`/`record` legality) — the bytecode side gets the same from
/// [`ClassFlags`](crate::seam::ClassFlags). `#[default]` = `Class` so a pre-existing
/// persisted symbol (before this field) still deserializes as a plain class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TypeKind {
    #[default]
    Class,
    Interface,
    Enum,
    Record,
    /// An `@interface` — an annotation type (an interface at the bytecode level).
    Annotation,
}

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

/// A single annotation on a declaration: its simple name plus the optional single-string
/// argument. `@Service` → `{name:"Service", value:None}`; `@Service("foo")` /
/// `@Service(value="foo")` → `{name:"Service", value:Some("foo")}`. A non-string argument
/// (`@RequestMapping(method=POST)`) leaves `value` `None` — only a plain string literal is
/// captured (the bean-name / stereotype value case).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    /// The annotation's simple name — its type name's last segment (`lombok.Getter` → `Getter`).
    pub name: String,
    /// The unquoted contents of the annotation's first string-literal argument, if any.
    pub value: Option<String>,
}

/// A field of a type: its name and its declared type (as written in source).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDecl {
    pub name: String,
    /// The declared type text, e.g. `Map<String, Object>` or `HttpServletRequest`.
    pub type_text: String,
    pub is_static: bool,
    /// `true` for a `final` field (Lombok generates no setter for one).
    pub is_final: bool,
    /// The declared access level (`public`/`protected`/`private`, else package-private).
    pub visibility: Visibility,
    /// The field's annotations (`@Getter`, `@Autowired`, …) — for Lombok synthesis and
    /// (future) field-injection resolution. Empty for a field with no annotations.
    pub annotations: Vec<Annotation>,
}

impl FieldDecl {
    /// Whether this field carries an annotation with the given simple name.
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a.name == name)
    }
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
    /// The declared access level (`public`/`protected`/`private`, else package-private).
    pub visibility: Visibility,
    /// An abstract method — an explicit `abstract` modifier, or a bodyless interface method
    /// (implicitly abstract). A concrete subclass must implement it. `#[serde(default)]` for
    /// backward-compatible deserialization of a pre-existing persisted symbol.
    #[serde(default)]
    pub is_abstract: bool,
    /// An interface `default` method (a concrete instance method inside an interface) — satisfies
    /// the interface contract, so a subclass need not implement it.
    #[serde(default)]
    pub is_default: bool,
    /// A `final` method — cannot be overridden by a subclass.
    #[serde(default)]
    pub is_final: bool,
}

/// A type declaration (class / interface / enum).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub name: String,
    /// Fully-qualified name (`package.Outer.Inner` when nested).
    pub fqn: String,
    /// What the declaration is (class / interface / enum / record / annotation). `#[serde(default)]`
    /// = `Class` for a pre-existing persisted symbol. Feeds the project-source class-level flags.
    #[serde(default)]
    pub kind: TypeKind,
    /// An `abstract` class (has the `abstract` modifier). Interfaces are abstract by definition —
    /// that's derived from `kind`, not stored here.
    #[serde(default)]
    pub is_abstract: bool,
    /// A `final` class — cannot be extended.
    #[serde(default)]
    pub is_final: bool,
    /// A `sealed` class/interface (a `permits` list restricts its subtypes).
    #[serde(default)]
    pub is_sealed: bool,
    pub methods: Vec<MethodDecl>,
    pub fields: Vec<FieldDecl>,
    /// The `extends` clause type text, if any.
    pub extends: Option<String>,
    /// The `implements` clause type texts (interface `extends` folded in here too).
    pub implements: Vec<String>,
    /// The type's annotations (`@Data`, `@Slf4j`, `@Service("foo")`, …) — the input to Lombok
    /// generated-member synthesis and the Spring stereotype-bean policy. Empty for a type with
    /// no annotations.
    pub annotations: Vec<Annotation>,
}

impl TypeDecl {
    /// Whether this type carries an annotation with the given simple name.
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a.name == name)
    }
}

/// The extracted symbols of one `.java` file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    extract_symbols_from_root(&tree.root_node(), source)
}

/// [`extract_symbols`] over an ALREADY-parsed tree — for callers that also need the CST
/// (e.g. the reference-index walk), so the file is parsed once and both the symbols and the
/// walk reuse the same tree instead of re-parsing per concern. `root` must be the root node
/// of a tree parsed over `source`.
pub fn extract_symbols_from_root(root: &Node, source: &str) -> FileSymbols {
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
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => {
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

    let kind = match node.kind() {
        "interface_declaration" => TypeKind::Interface,
        "enum_declaration" => TypeKind::Enum,
        "record_declaration" => TypeKind::Record,
        "annotation_type_declaration" => TypeKind::Annotation,
        _ => TypeKind::Class,
    };
    let is_interface = matches!(kind, TypeKind::Interface | TypeKind::Annotation);
    let is_abstract = has_modifier(node, bytes, "abstract");
    let is_final = has_modifier(node, bytes, "final");
    let is_sealed = has_modifier(node, bytes, "sealed");

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
                    if let Some(md) = parse_method(&m, bytes, is_interface) {
                        methods.push(md);
                    }
                }
                // `constant_declaration` is an interface's `int MAX = 100;` — same shape as a
                // field (type + declarators), just a different node kind, so index it as a field
                // so a bare / qualified constant reference resolves like any other field.
                "field_declaration" | "constant_declaration" => {
                    parse_field(&m, bytes, &mut fields);
                }
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration" => {
                    collect_type(&m, bytes, package, Some(&fqn), out);
                }
                _ => {}
            }
        }
    }

    let annotations = collect_annotations(node, bytes);
    out.push(TypeDecl {
        name,
        fqn,
        kind,
        is_abstract,
        is_final,
        is_sealed,
        methods,
        fields,
        extends,
        implements,
        annotations,
    });
}

/// Collect a declaration's annotations from its `modifiers` node. A `marker_annotation`
/// (`@Getter`) or `annotation` (`@Getter(...)`) contributes its name's LAST segment
/// (`lombok.Getter` → `Getter`) as [`Annotation::name`], plus the unquoted contents of its
/// first string-literal argument (`@Service("foo")` / `@Service(value="foo")` → `foo`) as
/// [`Annotation::value`] when present. Empty when the node has no annotations.
fn collect_annotations(node: &Node, bytes: &[u8]) -> Vec<Annotation> {
    let mut out = Vec::new();
    let mut cw = node.walk();
    for c in node.children(&mut cw) {
        if c.kind() != "modifiers" {
            continue;
        }
        let mut mw = c.walk();
        for a in c.children(&mut mw) {
            if matches!(a.kind(), "marker_annotation" | "annotation") {
                if let Some(name) = a.child_by_field_name("name").and_then(|n| node_text(&n, bytes)) {
                    let simple = name.rsplit('.').next().unwrap_or(&name).to_string();
                    let value = annotation_string_value(&a, bytes);
                    out.push(Annotation { name: simple, value });
                }
            }
        }
    }
    out
}

/// The unquoted contents of an annotation's FIRST string-literal argument, whether written as
/// `@X("v")` (a bare value) or `@X(value="v")` (an `element_value_pair`). `None` for a marker
/// annotation, an empty arg list, or a non-string argument (`@RequestMapping(method=POST)`).
fn annotation_string_value(annotation: &Node, bytes: &[u8]) -> Option<String> {
    let args = annotation.child_by_field_name("arguments")?;
    let mut aw = args.walk();
    for arg in args.named_children(&mut aw) {
        match arg.kind() {
            "string_literal" => return string_literal_text(&arg, bytes),
            // `@X(value="v")` — take the string on the RHS of the pair (any pair, since a
            // single-element annotation's only pair IS `value`).
            "element_value_pair" => {
                if let Some(v) = arg.child_by_field_name("value") {
                    if v.kind() == "string_literal" {
                        return string_literal_text(&v, bytes);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// The contents of a `string_literal` node with the surrounding quotes stripped. Robust across
/// grammar shapes: prefer the concatenated `string_fragment` children, falling back to trimming
/// the literal's own `"` delimiters.
fn string_literal_text(literal: &Node, bytes: &[u8]) -> Option<String> {
    let mut fragments = String::new();
    let mut lw = literal.walk();
    for part in literal.children(&mut lw) {
        if part.kind() == "string_fragment" {
            if let Some(t) = node_text(&part, bytes) {
                fragments.push_str(&t);
            }
        }
    }
    if !fragments.is_empty() {
        return Some(fragments);
    }
    // Empty string literal (`""`) or a grammar without `string_fragment` nodes: trim the quotes.
    let raw = node_text(literal, bytes)?;
    Some(raw.trim_matches('"').to_string())
}

/// Extract a method_declaration. `enclosing_is_interface` lets a bodyless method be recognised as
/// implicitly abstract (an interface method with no `default`/`static` body).
fn parse_method(node: &Node, bytes: &[u8], enclosing_is_interface: bool) -> Option<MethodDecl> {
    let name = node.child_by_field_name("name").and_then(|n| node_text(&n, bytes))?;
    let return_type_text = node
        .child_by_field_name("type")
        .and_then(|n| node_text(&n, bytes))
        .unwrap_or_else(|| "void".to_string());
    let is_static = has_modifier(node, bytes, "static");
    let visibility = parse_visibility(node, bytes);
    let is_default = has_modifier(node, bytes, "default");
    // Abstract = an explicit `abstract` modifier, OR an interface method with no body that isn't
    // `static`/`default`/`native` (implicitly abstract, JLS §9.4). Requiring "no body" keeps a class
    // concrete method — which always has a body — from ever being mis-marked abstract (never a false
    // positive for the implement-abstract check that reads this).
    let has_body = node.child_by_field_name("body").is_some();
    let is_abstract = has_modifier(node, bytes, "abstract")
        || (enclosing_is_interface
            && !has_body
            && !is_static
            && !is_default
            && !has_modifier(node, bytes, "native"));

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

    let is_final = has_modifier(node, bytes, "final");
    Some(MethodDecl {
        name,
        return_type_text,
        params,
        is_static,
        visibility,
        is_abstract,
        is_default,
        is_final,
    })
}

/// Extract the (possibly multiple) fields of a field_declaration (`int a, b, c;`).
fn parse_field(node: &Node, bytes: &[u8], out: &mut Vec<FieldDecl>) {
    let Some(type_text) = node.child_by_field_name("type").and_then(|n| node_text(&n, bytes))
    else {
        return;
    };
    let is_static = has_modifier(node, bytes, "static");
    let is_final = has_modifier(node, bytes, "final");
    let visibility = parse_visibility(node, bytes);
    let annotations = collect_annotations(node, bytes);
    let mut cw = node.walk();
    for c in node.named_children(&mut cw) {
        if c.kind() == "variable_declarator" {
            if let Some(name) = c.child_by_field_name("name").and_then(|n| node_text(&n, bytes)) {
                out.push(FieldDecl {
                    name,
                    type_text: type_text.clone(),
                    is_static,
                    is_final,
                    visibility,
                    annotations: annotations.clone(),
                });
            }
        }
    }
}

/// The declared access level of a member from its `modifiers` node. Java's default (no explicit
/// `public`/`protected`/`private`) is package-private. (Interface members are implicitly public;
/// we do not special-case that here — the enclosing-kind context isn't threaded to this helper.)
fn parse_visibility(node: &Node, bytes: &[u8]) -> Visibility {
    if has_modifier(node, bytes, "public") {
        Visibility::Public
    } else if has_modifier(node, bytes, "protected") {
        Visibility::Protected
    } else if has_modifier(node, bytes, "private") {
        Visibility::Private
    } else {
        Visibility::Package
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The annotations of the single top-level type in `src`.
    fn type_annotations(src: &str) -> Vec<Annotation> {
        let fs = extract_symbols(src);
        fs.types.into_iter().next().expect("one type").annotations
    }

    #[test]
    fn captures_bare_string_annotation_value() {
        let ann = type_annotations("@Service(\"fooService\") class FooService {}");
        assert_eq!(ann, vec![Annotation { name: "Service".into(), value: Some("fooService".into()) }]);
    }

    #[test]
    fn captures_value_named_argument() {
        let ann = type_annotations("@Service(value=\"bar\") class FooService {}");
        assert_eq!(ann, vec![Annotation { name: "Service".into(), value: Some("bar".into()) }]);
    }

    #[test]
    fn marker_annotation_has_no_value() {
        let ann = type_annotations("@Service class FooService {}");
        assert_eq!(ann, vec![Annotation { name: "Service".into(), value: None }]);
    }

    #[test]
    fn non_string_argument_yields_no_value() {
        // `@RequestMapping(method=POST)` — the argument isn't a plain string literal.
        let ann = type_annotations("@RequestMapping(method=POST) class C {}");
        assert_eq!(ann, vec![Annotation { name: "RequestMapping".into(), value: None }]);
    }

    #[test]
    fn qualified_annotation_name_is_simple() {
        let ann = type_annotations("@lombok.Getter class C {}");
        assert_eq!(ann, vec![Annotation { name: "Getter".into(), value: None }]);
    }

    #[test]
    fn field_annotation_value_is_captured() {
        let fs = extract_symbols("class C { @Qualifier(\"db\") private DataSource ds; }");
        let f = &fs.types[0].fields[0];
        assert!(f.has_annotation("Qualifier"));
        assert_eq!(f.annotations[0].value.as_deref(), Some("db"));
    }

    fn one_type(src: &str) -> TypeDecl {
        extract_symbols(src).types.into_iter().next().expect("one type")
    }

    #[test]
    fn type_kind_is_detected() {
        assert_eq!(one_type("class C {}").kind, TypeKind::Class);
        assert_eq!(one_type("interface I {}").kind, TypeKind::Interface);
        assert_eq!(one_type("enum E { A }").kind, TypeKind::Enum);
        assert_eq!(one_type("record R(int x) {}").kind, TypeKind::Record);
        assert_eq!(one_type("@interface A {}").kind, TypeKind::Annotation);
    }

    #[test]
    fn class_modifiers_are_captured() {
        let t = one_type("abstract class C {}");
        assert!(t.is_abstract && !t.is_final);
        let f = one_type("final class C {}");
        assert!(f.is_final && !f.is_abstract);
    }

    #[test]
    fn interface_method_is_implicitly_abstract() {
        // A bodyless interface method is abstract; a `default` one is not; a `static` one is not.
        let t = one_type("interface I { void run(); default void ok() {} static void s() {} }");
        let run = t.methods.iter().find(|m| m.name == "run").unwrap();
        assert!(run.is_abstract && !run.is_default, "bodyless interface method is abstract");
        let ok = t.methods.iter().find(|m| m.name == "ok").unwrap();
        assert!(!ok.is_abstract && ok.is_default, "default method is not abstract");
        let s = t.methods.iter().find(|m| m.name == "s").unwrap();
        assert!(!s.is_abstract && !s.is_default, "static interface method is neither");
    }

    #[test]
    fn class_abstract_method_is_abstract() {
        let t = one_type("abstract class C { abstract void run(); void done() {} }");
        assert!(t.methods.iter().find(|m| m.name == "run").unwrap().is_abstract);
        // A concrete class method (with a body) is never marked abstract.
        assert!(!t.methods.iter().find(|m| m.name == "done").unwrap().is_abstract);
    }
}
