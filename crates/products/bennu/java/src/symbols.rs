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

impl TypeKind {
    /// A stable lowercase slug (`"class"`, `"interface"`, `"enum"`, `"record"`, `"annotation"`) —
    /// the wire form the FE keys its type-kind icons on.
    pub fn slug(&self) -> &'static str {
        match self {
            TypeKind::Class => "class",
            TypeKind::Interface => "interface",
            TypeKind::Enum => "enum",
            TypeKind::Record => "record",
            TypeKind::Annotation => "annotation",
        }
    }
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
    /// The `name = value` pairs of the annotation, with each value kept as raw source text
    /// (`@Accessors(fluent = true, prefix = "m")` → `[("fluent","true"),("prefix","\"m\"")]`).
    /// Lets consumers read non-string flags (Lombok `@Accessors` `fluent`/`chain`/`prefix`) that
    /// the single-string `value` can't carry. Empty for a marker or bare-value annotation.
    #[serde(default)]
    pub args: Vec<(String, String)>,
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
    /// The declared `throws` clause, as written type texts (`IOException`, `java.sql.SQLException`).
    /// Resolved to binary names when the class members are built. Empty when there's no `throws`.
    /// `#[serde(default)]` for backward-compatible deserialization of a pre-existing persisted symbol.
    #[serde(default)]
    pub throws: Vec<String>,
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
    /// The declared generic type-parameter NAMES, in order (`class Pair<L, R>` → `["L","R"]`). Empty
    /// for a non-generic type. Drives exact positional generic substitution downstream (a method
    /// returning `R` maps to the receiver's 2nd type argument). `#[serde(default)]` for old indexes.
    #[serde(default)]
    pub type_params: Vec<String>,
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
            // An ENUM's fields / methods / constructors don't sit directly in the body — they live in
            // an `enum_body_declarations` node (the part after the constants' `;`). Descend into it so
            // an enum's own fields (`this.sqlCriteriaValue`) and methods resolve like any class's.
            if m.kind() == "enum_body_declarations" {
                let mut ew = m.walk();
                for em in m.named_children(&mut ew) {
                    collect_body_member(&em, bytes, is_interface, package, &fqn, &mut methods, &mut fields, out);
                }
            } else {
                collect_body_member(&m, bytes, is_interface, package, &fqn, &mut methods, &mut fields, out);
            }
        }
    }

    let annotations = collect_annotations(node, bytes);
    let type_params = type_param_names(node, bytes);
    out.push(TypeDecl {
        name,
        fqn,
        kind,
        is_abstract,
        is_final,
        is_sealed,
        type_params,
        methods,
        fields,
        extends,
        implements,
        annotations,
    });
}

/// Collect one member node of a type body into `methods` / `fields` (or recurse for a nested type).
/// Shared by the class/interface body loop and the enum's `enum_body_declarations` loop.
#[allow(clippy::too_many_arguments)]
fn collect_body_member(
    m: &Node,
    bytes: &[u8],
    is_interface: bool,
    package: Option<&str>,
    fqn: &str,
    methods: &mut Vec<MethodDecl>,
    fields: &mut Vec<FieldDecl>,
    out: &mut Vec<TypeDecl>,
) {
    match m.kind() {
        "method_declaration" => {
            if let Some(md) = parse_method(m, bytes, is_interface) {
                methods.push(md);
            }
        }
        // Constructors are indexed as `<init>` members (like bytecode) so the super-constructor,
        // unhandled-exception-from-`new`, and constructor-arity checks work on project types.
        "constructor_declaration" => {
            if let Some(md) = parse_constructor(m, bytes) {
                methods.push(md);
            }
        }
        // An `@interface` element (`String value();`, `int count() default 3;`) IS a public abstract
        // no-arg method of the annotation type at the bytecode level. Index it as such so a
        // `myAnno.value()` access resolves its method (otherwise it false-flags "cannot resolve").
        "annotation_type_element_declaration" => {
            if let Some(md) = parse_annotation_element(m, bytes) {
                methods.push(md);
            }
        }
        // `constant_declaration` is an interface's `int MAX = 100;` — same shape as a field (type +
        // declarators), just a different node kind, so index it as a field so a bare / qualified
        // constant reference resolves like any other field.
        "field_declaration" | "constant_declaration" => {
            parse_field(m, bytes, is_interface, fields);
        }
        "class_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "record_declaration"
        | "annotation_type_declaration" => {
            collect_type(m, bytes, package, Some(fqn), out);
        }
        _ => {}
    }
}

/// Collect a declaration's annotations from its `modifiers` node. A `marker_annotation`
/// (`@Getter`) or `annotation` (`@Getter(...)`) contributes its name's LAST segment
/// (`lombok.Getter` → `Getter`) as [`Annotation::name`], plus the unquoted contents of its
/// first string-literal argument (`@Service("foo")` / `@Service(value="foo")` → `foo`) as
/// [`Annotation::value`] when present. Empty when the node has no annotations.
/// The declared generic type-parameter names of a type declaration, in order (`class Pair<L, R>` →
/// `["L", "R"]`). Reads the node's `type_parameters` field; each `type_parameter`'s name is its first
/// `type_identifier` child. Empty for a non-generic type.
fn type_param_names(node: &Node, bytes: &[u8]) -> Vec<String> {
    let Some(tps) = node.child_by_field_name("type_parameters") else { return Vec::new() };
    let mut out = Vec::new();
    let mut c = tps.walk();
    for tp in tps.named_children(&mut c) {
        if tp.kind() != "type_parameter" {
            continue;
        }
        let mut tc = tp.walk();
        for ch in tp.named_children(&mut tc) {
            if ch.kind() == "type_identifier" {
                if let Some(t) = node_text(&ch, bytes) {
                    out.push(t);
                }
                break;
            }
        }
    }
    out
}

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
                    let args = annotation_arg_pairs(&a, bytes);
                    out.push(Annotation { name: simple, value, args });
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

/// The `name = value` pairs of an annotation, each value kept as raw source text (so a boolean,
/// enum-constant or string reads back verbatim). Positional / string-only arguments are ignored —
/// only `element_value_pair`s are captured. Used for Lombok `@Accessors(fluent = true, …)`.
fn annotation_arg_pairs(annotation: &Node, bytes: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(args) = annotation.child_by_field_name("arguments") else { return out };
    let mut aw = args.walk();
    for arg in args.named_children(&mut aw) {
        if arg.kind() != "element_value_pair" {
            continue;
        }
        // `key` is the LHS identifier; fall back to the first identifier child if the grammar names
        // the field differently, so the capture never silently yields nothing.
        let key = arg
            .child_by_field_name("key")
            .or_else(|| arg.named_child(0).filter(|n| n.kind() == "identifier"))
            .and_then(|k| node_text(&k, bytes));
        let val = arg.child_by_field_name("value").and_then(|v| node_text(&v, bytes));
        if let (Some(k), Some(v)) = (key, val) {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
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
    let visibility = parse_visibility(node, bytes, enclosing_is_interface);
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

    let params = parse_params(node, bytes);
    let is_final = has_modifier(node, bytes, "final");
    let throws = parse_throws(node, bytes);
    Some(MethodDecl {
        name,
        return_type_text,
        params,
        is_static,
        visibility,
        is_abstract,
        is_default,
        is_final,
        throws,
    })
}

/// The `(name, type)` parameters of a method/constructor `parameters` list. Shared by
/// [`parse_method`] and [`parse_constructor`].
fn parse_params(node: &Node, bytes: &[u8]) -> Vec<ParamDecl> {
    let mut params = Vec::new();
    if let Some(pl) = node.child_by_field_name("parameters") {
        let mut pw = pl.walk();
        for p in pl.named_children(&mut pw) {
            match p.kind() {
                "formal_parameter" => {
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
                // A varargs parameter `T... xs` — which IS a `T[]` array. Unlike `formal_parameter`,
                // tree-sitter gives its type as an UNNAMED child (before `...`) and its name inside a
                // `variable_declarator`, so neither is a direct field. Extract both and record the type
                // WITH a trailing `[]`, so downstream (the arity check) sees the array/varargs it is and
                // accepts a call with zero trailing arguments.
                "spread_parameter" => {
                    params.push(parse_spread_parameter(&p, bytes));
                }
                _ => {}
            }
        }
    }
    params
}

/// Parse a `spread_parameter` (`T... xs`) into a [`ParamDecl`] whose `type_text` is the varargs
/// element type with a `[]` suffix (varargs erase to an array). The element type is the first
/// non-modifier / non-annotation / non-declarator child; the name lives in the `variable_declarator`.
fn parse_spread_parameter(p: &Node, bytes: &[u8]) -> ParamDecl {
    let mut type_text = String::new();
    let mut name = String::new();
    let mut c = p.walk();
    for ch in p.named_children(&mut c) {
        match ch.kind() {
            "variable_declarator" => {
                name = ch
                    .child_by_field_name("name")
                    .and_then(|n| node_text(&n, bytes))
                    .unwrap_or_default();
            }
            "modifiers" | "annotation" | "marker_annotation" => {}
            // The first remaining named child is the element type (`String`, `List<X>`, `int`, …).
            _ if type_text.is_empty() => {
                type_text = node_text(&ch, bytes).unwrap_or_default();
            }
            _ => {}
        }
    }
    if !type_text.is_empty() {
        // Record the ERASED array type (`List<X>` → `List[]`, `Object` → `Object[]`), matching how
        // bytecode erases a varargs parameter. Erasing before the `[]` keeps the array suffix on the
        // resolved binary name (generics would otherwise strip it), so the arity check sees the array.
        let erased = type_text.split('<').next().unwrap_or("").trim().to_string();
        type_text = format!("{erased}[]");
    }
    ParamDecl { name, type_text }
}

/// The `throws` clause exception type names (written text; resolved to binary names when the class
/// members are built). Shared by [`parse_method`] and [`parse_constructor`].
fn parse_throws(node: &Node, bytes: &[u8]) -> Vec<String> {
    let mut throws = Vec::new();
    let mut mw = node.walk();
    for ch in node.children(&mut mw) {
        if ch.kind() == "throws" {
            let mut tw = ch.walk();
            for t in ch.named_children(&mut tw) {
                if let Some(txt) = node_text(&t, bytes) {
                    throws.push(txt);
                }
            }
        }
    }
    throws
}

/// Extract a `constructor_declaration` as an `<init>` pseudo-method — mirroring the `<init>` members
/// decoded from bytecode for library types — so the resolver-backed checks that key off constructors
/// (super-constructor chaining, unhandled checked exception from `new T(...)`, constructor arity) see
/// a PROJECT type's constructors too. The name is the JVM `<init>`; the return type is unused. A
/// constructor is never static/abstract/default/final for our purposes.
fn parse_constructor(node: &Node, bytes: &[u8]) -> Option<MethodDecl> {
    node.child_by_field_name("name")?; // a real declaration names its class
    Some(MethodDecl {
        name: "<init>".to_string(),
        return_type_text: "void".to_string(),
        params: parse_params(node, bytes),
        is_static: false,
        visibility: parse_visibility(node, bytes, false),
        is_abstract: false,
        is_default: false,
        is_final: false,
        throws: parse_throws(node, bytes),
    })
}

/// Extract an `annotation_type_element_declaration` — an `@interface` element such as
/// `String value();` or `int count() default 3;`. These ARE public abstract no-arg methods of the
/// annotation type at the bytecode level, so index each as a [`MethodDecl`] (no params, its declared
/// return type, implicitly public + abstract) — mirroring how a library annotation's elements decode
/// from bytecode. Without this, accessing an element on a project annotation (`ann.value()`) can't
/// resolve its method and would be wrongly flagged "cannot resolve method".
fn parse_annotation_element(node: &Node, bytes: &[u8]) -> Option<MethodDecl> {
    let name = node.child_by_field_name("name").and_then(|n| node_text(&n, bytes))?;
    let return_type_text = node
        .child_by_field_name("type")
        .and_then(|n| node_text(&n, bytes))
        .unwrap_or_else(|| "void".to_string());
    Some(MethodDecl {
        name,
        return_type_text,
        params: Vec::new(),
        is_static: false,
        visibility: Visibility::Public,
        is_abstract: true,
        is_default: false,
        is_final: false,
        throws: Vec::new(),
    })
}

/// Extract the (possibly multiple) fields of a field_declaration (`int a, b, c;`). `in_interface`
/// marks an interface's `constant_declaration` (`int MAX = 100;`) — implicitly `public static final`,
/// so its visibility is public, not the class-default package-private.
fn parse_field(node: &Node, bytes: &[u8], in_interface: bool, out: &mut Vec<FieldDecl>) {
    let Some(type_text) = node.child_by_field_name("type").and_then(|n| node_text(&n, bytes))
    else {
        return;
    };
    let is_static = has_modifier(node, bytes, "static");
    let is_final = has_modifier(node, bytes, "final");
    let visibility = parse_visibility(node, bytes, in_interface);
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
/// `public`/`protected`/`private`) is package-private for a CLASS/ENUM member — but **implicitly
/// public** for an INTERFACE/`@interface` member (a method or a constant), JLS §9.3/§9.4. Passing
/// `in_interface` is what keeps a cross-package call to a project interface method from being a false
/// "not public" error (before, a modifier-less interface method read as package-private). An explicit
/// `private` (a Java 9+ private interface method) is still honoured — it's checked first.
fn parse_visibility(node: &Node, bytes: &[u8], in_interface: bool) -> Visibility {
    if has_modifier(node, bytes, "public") {
        Visibility::Public
    } else if has_modifier(node, bytes, "protected") {
        Visibility::Protected
    } else if has_modifier(node, bytes, "private") {
        Visibility::Private
    } else if in_interface {
        Visibility::Public
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
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].name, "Service");
        assert_eq!(ann[0].value.as_deref(), Some("fooService"));
        assert!(ann[0].args.is_empty(), "positional arg isn't a pair");
    }

    #[test]
    fn captures_value_named_argument() {
        let ann = type_annotations("@Service(value=\"bar\") class FooService {}");
        assert_eq!(ann[0].value.as_deref(), Some("bar"));
    }

    #[test]
    fn marker_annotation_has_no_value() {
        let ann = type_annotations("@Service class FooService {}");
        assert_eq!(ann[0].value, None);
        assert!(ann[0].args.is_empty());
    }

    #[test]
    fn non_string_argument_yields_no_value() {
        // `@RequestMapping(method=POST)` — the argument isn't a plain string literal.
        let ann = type_annotations("@RequestMapping(method=POST) class C {}");
        assert_eq!(ann[0].name, "RequestMapping");
        assert_eq!(ann[0].value, None);
    }

    #[test]
    fn captures_named_arg_pairs_as_raw_text() {
        // `@Accessors(fluent = true, prefix = "m")` — non-string flags land in `args` verbatim.
        let ann = type_annotations("@Accessors(fluent = true, prefix = \"m\") class C {}");
        assert_eq!(ann[0].name, "Accessors");
        let fluent = ann[0].args.iter().find(|(k, _)| k == "fluent").map(|(_, v)| v.as_str());
        assert_eq!(fluent, Some("true"), "got {:?}", ann[0].args);
    }

    #[test]
    fn qualified_annotation_name_is_simple() {
        let ann = type_annotations("@lombok.Getter class C {}");
        assert_eq!(ann[0].name, "Getter");
        assert_eq!(ann[0].value, None);
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
    fn extends_and_implements_across_newlines_are_captured() {
        // A class whose `extends` / `implements` clauses (and the opening brace) sit on their own
        // lines must still resolve its supertypes and index its members — the tree parse is
        // whitespace-insensitive, so nothing about the layout may change the extracted symbols.
        let src = "package com.acme;\n\
                   public class Foo\n\
                       extends AbstractBar\n\
                       implements Baz, Qux {\n\
                       void run() {}\n\
                   }\n";
        let t = one_type(src);
        assert_eq!(t.name, "Foo");
        assert_eq!(t.extends.as_deref(), Some("AbstractBar"), "{:?}", t.extends);
        assert_eq!(t.implements, vec!["Baz", "Qux"], "{:?}", t.implements);
        assert!(t.methods.iter().any(|m| m.name == "run"), "member indexed: {:?}", t.methods);
    }

    #[test]
    fn interface_extends_across_newlines_is_captured() {
        // An interface `extends` list on its own lines folds into `implements` for the member walk.
        let src = "interface I\n    extends A,\n            B {\n}\n";
        let t = one_type(src);
        assert_eq!(t.implements, vec!["A", "B"], "{:?}", t.implements);
    }

    #[test]
    fn varargs_parameter_type_is_recorded_as_an_array() {
        // `String... args` is a varargs parameter — it erases to `String[]`. Recording it WITH the
        // `[]` (and its name) is what lets the arity check accept a call with zero trailing args.
        let t = one_type("class C { void fmt(String f, Object... rest) {} }");
        let m = t.methods.iter().find(|m| m.name == "fmt").expect("fmt");
        assert_eq!(m.params.len(), 2, "{:?}", m.params);
        assert_eq!(m.params[0].type_text, "String");
        assert_eq!(m.params[1].name, "rest", "varargs name captured: {:?}", m.params[1]);
        assert_eq!(m.params[1].type_text, "Object[]", "varargs recorded as array: {:?}", m.params[1]);

        // A GENERIC varargs erases to a bare array (`List<T>` → `List[]`), so the array suffix
        // survives binary-name resolution and the arity check still recognises the varargs.
        let g = one_type("class C { <T> void addAll(java.util.List<T>... lists) {} }");
        let gm = g.methods.iter().find(|m| m.name == "addAll").expect("addAll");
        assert_eq!(gm.params.len(), 1, "{:?}", gm.params);
        assert_eq!(gm.params[0].type_text, "java.util.List[]", "generic varargs erased: {:?}", gm.params[0]);
    }

    #[test]
    fn generic_type_parameters_are_captured_in_order() {
        assert_eq!(one_type("class Pair<L, R> {}").type_params, vec!["L", "R"]);
        assert_eq!(one_type("interface Repo<T, ID> {}").type_params, vec!["T", "ID"]);
        // A bounded parameter keeps just the name.
        assert_eq!(one_type("class Box<T extends Number> {}").type_params, vec!["T"]);
        assert!(one_type("class Plain {}").type_params.is_empty());
    }

    #[test]
    fn interface_method_without_modifier_is_public() {
        // Interface members are implicitly public (JLS §9.4) — NOT package-private.
        let t = one_type("interface I { String get(); }");
        assert_eq!(t.methods[0].visibility, Visibility::Public, "{:?}", t.methods[0]);
    }

    #[test]
    fn interface_constant_is_public() {
        let t = one_type("interface I { int MAX = 100; }");
        assert_eq!(t.fields[0].visibility, Visibility::Public, "{:?}", t.fields[0]);
    }

    #[test]
    fn private_interface_method_stays_private() {
        // A Java 9+ `private` interface helper keeps its explicit visibility.
        let t = one_type("interface I { private void helper() {} }");
        assert_eq!(t.methods[0].visibility, Visibility::Private, "{:?}", t.methods[0]);
    }

    #[test]
    fn class_method_without_modifier_is_package() {
        // The class default is unchanged: no modifier → package-private.
        let t = one_type("class C { void m() {} }");
        assert_eq!(t.methods[0].visibility, Visibility::Package, "{:?}", t.methods[0]);
    }

    #[test]
    fn constructor_is_indexed_as_init() {
        // A `constructor_declaration` is captured as an `<init>` member (with its param arity) so the
        // super-constructor / new-arity / new-exception checks can reason about project constructors.
        let t = one_type("class C { C(int x) {} void m() {} }");
        let inits: Vec<_> = t.methods.iter().filter(|m| m.name == "<init>").collect();
        assert_eq!(inits.len(), 1, "{:?}", t.methods);
        assert_eq!(inits[0].params.len(), 1, "{:?}", inits[0]);
        assert!(t.methods.iter().any(|m| m.name == "m"), "the normal method is still there");
    }

    #[test]
    fn constructor_throws_are_captured() {
        let t = one_type("class C { C() throws java.io.IOException {} }");
        let init = t.methods.iter().find(|m| m.name == "<init>").expect("<init>");
        assert!(init.throws.iter().any(|x| x.contains("IOException")), "{:?}", init.throws);
    }

    #[test]
    fn annotation_elements_are_indexed_as_methods() {
        // `@interface` elements are public abstract no-arg methods — a `value()` access must resolve.
        let t = one_type("@interface Route { String value(); int count() default 3; }");
        let value = t.methods.iter().find(|m| m.name == "value").expect("value()");
        assert!(value.params.is_empty(), "annotation element takes no args");
        assert_eq!(value.visibility, Visibility::Public, "{value:?}");
        assert!(value.return_type_text.contains("String"), "{value:?}");
        assert!(t.methods.iter().any(|m| m.name == "count"), "element with a default is indexed too");
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
    fn enum_fields_methods_and_constructor_are_indexed() {
        // Enum members live under `enum_body_declarations` (after the constants' `;`) — they must be
        // extracted, so an enum's own field (`this.sqlCriteriaValue`) and methods resolve.
        let src = "enum OrderCriteria {\n  ASC(\"asc\"), DESC(\"desc\");\n  private final String sqlCriteriaValue;\n  OrderCriteria(String v) { this.sqlCriteriaValue = v; }\n  public String getSql() { return sqlCriteriaValue; }\n}";
        let t = one_type(src);
        assert_eq!(t.kind, TypeKind::Enum);
        assert!(t.fields.iter().any(|f| f.name == "sqlCriteriaValue"), "enum field indexed: {:?}", t.fields);
        assert!(t.methods.iter().any(|m| m.name == "getSql"), "enum method indexed: {:?}", t.methods);
        assert!(t.methods.iter().any(|m| m.name == "<init>"), "enum constructor indexed: {:?}", t.methods);
    }

    #[test]
    fn class_abstract_method_is_abstract() {
        let t = one_type("abstract class C { abstract void run(); void done() {} }");
        assert!(t.methods.iter().find(|m| m.name == "run").unwrap().is_abstract);
        // A concrete class method (with a body) is never marked abstract.
        assert!(!t.methods.iter().find(|m| m.name == "done").unwrap().is_abstract);
    }
}
