//! Symbol extraction: [`extract_symbols`]`(source) -> `[`FileSymbols`].
//!
//! Walks the tree-sitter-java CST once and pulls the package, imports, and each
//! top-level (and nested) type declaration with its methods and fields (including
//! declared type texts). No inference here — this is the structural model the
//! type-walk sits on.

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

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

/// Where a declaration sits in the source, in **bytes**.
///
/// Byte offsets throughout, never character ones — tree-sitter counts bytes, and a span that
/// crossed a seam as "characters" would be a bug waiting for the first accented identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// The span of a node.
    pub fn of(node: &Node) -> Self {
        Span { start: node.start_byte(), end: node.end_byte() }
    }
}

/// A single import. `star` marks `import a.b.*;`; `static_` marks `import static`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// Where the `import …;` is written. See [`FieldDecl::span`].
    #[serde(default)]
    pub span: Option<Span>,
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
    /// The annotation's first **positional** argument as raw source text, when it isn't a string
    /// literal — `@Setter(AccessLevel.PACKAGE)` → `AccessLevel.PACKAGE`.
    ///
    /// [`Self::value`] only carries a string literal and [`Self::args`] only carries `name =`
    /// pairs, so this shape fell between them and an `AccessLevel` was simply not visible: every
    /// Lombok accessor read as public, and `AccessLevel.NONE` — which generates nothing at all —
    /// read as "there is a public one".
    #[serde(default)]
    pub positional: Option<String>,
}

/// A field of a type: its name and its declared type (as written in source).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDecl {
    /// Where it is written. **`None` when nobody wrote it** — a Lombok getter: there is no source
    /// to point at, and a `0..0` would point at the package declaration. Anything that navigates to
    /// a member has to be able to tell the two apart.
    ///
    /// A record's field and accessor are *synthesized* but not unwritten: both carry the span of
    /// the component's name in the record header, which is the one place the language lets you
    /// name them — and so the only place a rename can edit or a go-to can land.
    ///
    /// `#[serde(default)]` so a symbol persisted before spans existed still deserializes.
    #[serde(default)]
    pub span: Option<Span>,
    pub name: String,
    /// The declared type text, e.g. `Map<String, Object>` or `HttpServletRequest`.
    pub type_text: String,
    pub is_static: bool,
    /// `true` for a `final` field (Lombok generates no setter for one).
    pub is_final: bool,
    /// The declared access level (`public`/`protected`/`private`, else package-private).
    pub visibility: Visibility,
    /// Whether the declaration assigns a value (`private final int x = 3;`). Lombok's
    /// `@RequiredArgsConstructor` skips an already-initialised `final` field, so the generated
    /// constructor's parameter list — and therefore its arity — depends on this.
    #[serde(default)]
    pub has_initializer: bool,
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
    /// Where it is written, or `None` for a member the language or a framework synthesizes — a
    /// record's accessor and canonical constructor, its `Object` overrides, a Lombok getter. See
    /// [`FieldDecl::span`].
    #[serde(default)]
    pub span: Option<Span>,
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
    /// Where the declaration is written. Always present for a parsed type; `None` only for one
    /// built by hand (a test fixture, a synthesized shape). See [`FieldDecl::span`].
    #[serde(default)]
    pub span: Option<Span>,
    pub name: String,
    /// Fully-qualified name (`package.Outer.Inner` when nested).
    pub fqn: String,
    /// What the declaration is (class / interface / enum / record / annotation). `#[serde(default)]`
    /// = `Class` for a pre-existing persisted symbol. Feeds the project-source class-level flags.
    #[serde(default)]
    pub kind: TypeKind,
    /// The body of a `new X() { … }` (or of an enum constant), which nobody named.
    ///
    /// It is a real type with real members, so it belongs in the index — but its `name` is a
    /// position (`"1"`, `"2"`), not something anyone wrote or could type. Anything that shows type
    /// names to a person — Go-to-Class, a navigator, a picker — should skip these rather than
    /// offer a number. `#[serde(default)]` so a symbol persisted before this field still loads.
    #[serde(default)]
    pub is_anonymous: bool,
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
    /// Where the `package …;` is written. Separate from [`Self::package`] rather than folded into
    /// it, because the *name* is what every consumer wants and the location is what one does.
    #[serde(default)]
    pub package_span: Option<Span>,
    pub imports: Vec<Import>,
    pub types: Vec<TypeDecl>,
}

/// Parse `source` and extract its symbols. Never panics on malformed input —
/// tree-sitter always produces a tree (with ERROR nodes) and we skip what we can't
/// read (a partial/broken buffer is a normal editor state).
pub fn extract_symbols(source: &str) -> FileSymbols {
    let Some(tree) = crate::grammar::parse_java(source) else {
        return FileSymbols::default();
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
    let mut package_span = None;
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
                package_span = Some(Span::of(&child));
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

    FileSymbols { package, package_span, imports, types }
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
    Some(Import { span: Some(Span::of(node)), path, star, static_ })
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
            } else if m.kind() == "enum_constant" {
                // A constant IS a member: `public static final E NAME`, exactly how the compiler
                // emits it and how bytecode reports it. Leaving it out made a project enum look like
                // it had no constants at all — `import static E.*` couldn't supply a bare name (the
                // undefined-variable check then called correct code undefined), completion after
                // `E.` offered nothing, and the switch-exhaustiveness check bailed on every project
                // enum because "no visible constants" means "our view is incomplete".
                if let Some(cname) = m.child_by_field_name("name").and_then(|n| node_text(&n, bytes))
                {
                    fields.push(FieldDecl {
                        span: Some(Span::of(&m)),
                        name: cname,
                        type_text: name.clone(),
                        is_static: true,
                        is_final: true,
                        visibility: Visibility::Public,
                        has_initializer: true, // a constant IS its own initialisation
                        annotations: collect_annotations(&m, bytes),
                    });
                }
            } else {
                collect_body_member(&m, bytes, is_interface, package, &fqn, &mut methods, &mut fields, out);
            }
        }
    }

    // A record's components are members the LANGUAGE mandates, not a framework's guess — so they
    // belong here beside the parsed ones, and not in `bennu-intel`'s Lombok synthesis.
    if matches!(kind, TypeKind::Record) {
        synthesize_record_members(node, bytes, &mut methods, &mut fields);
    }

    let annotations = collect_annotations(node, bytes);
    let type_params = type_param_names(node, bytes);
    out.push(TypeDecl {
        span: Some(Span::of(node)),
        name,
        fqn,
        kind,
        is_anonymous: false,
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
/// Collect the types declared inside a method's or constructor's body, if it has one.
fn collect_inner_types_in_body(
    m: &Node,
    bytes: &[u8],
    package: Option<&str>,
    fqn: &str,
    out: &mut Vec<TypeDecl>,
) {
    if let Some(body) = m.child_by_field_name("body") {
        collect_inner_types(&body, bytes, package, fqn, out);
    }
}

/// Collect the types declared INSIDE a member's body — the two kinds a walk that reads
/// *declarations* rather than *statements* will otherwise miss entirely.
///
/// **Local types**: a `class Helper { … }` written in a method. Legal since Java 1.1 (a local
/// interface, enum or record needs Java 16 — JEP 395). Attributed to its ENCLOSING TYPE
/// (`p.Outer.Helper`), the spelling a member type gets. Java's bytecode name disambiguates by
/// position (`Outer$1Helper`), so two local types of the same name in different methods of one
/// class collapse to one entry here — the one thing this cannot express, in exchange for a name
/// that reads like the source.
///
/// **Anonymous classes**: the body of a `new Runnable() { … }`. Named the way javac names them,
/// by position — `p.Outer.1` — because there is no other name to use. Without an identity of their
/// own, everything they declared was attributed to the enclosing named class: an anonymous `run()`
/// counted as a use of `Outer.run()`, and a `this.field` inside the body resolved against `Outer`
/// rather than against the type actually being subclassed.
///
/// Neither kind is descended into here. [`collect_type`] walks the type's own members and reaches
/// anything declared inside THEM through this same function; recursing here as well would collect
/// every nested declaration twice.
fn collect_inner_types(
    node: &Node,
    bytes: &[u8],
    package: Option<&str>,
    fqn: &str,
    out: &mut Vec<TypeDecl>,
) {
    let mut cw = node.walk();
    for c in node.named_children(&mut cw) {
        match c.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => {
                collect_type(&c, bytes, package, Some(fqn), out);
            }
            "class_body" if is_anonymous_body(&c) => {
                collect_anonymous_type(&c, bytes, package, fqn, out);
            }
            _ => collect_inner_types(&c, bytes, package, fqn, out),
        }
    }
}

/// Whether this `class_body` is the body of an anonymous class — i.e. it hangs off a
/// `new X() { … }` or an enum constant with a body, rather than off a type declaration.
pub fn is_anonymous_body(body: &Node) -> bool {
    body.kind() == "class_body"
        && body
            .parent()
            .map(|p| matches!(p.kind(), "object_creation_expression" | "enum_constant"))
            .unwrap_or(false)
}

/// The name javac would give the anonymous class whose body this is — `"1"`, `"2"`, … in source
/// order **within the nearest enclosing named type**.
///
/// Positional rather than counted during a walk, so that the two sides that need this name — the
/// extractor that files the type under it, and the caret query that asks "what type am I inside?"
/// — derive it independently and cannot drift. `None` when `body` is not an anonymous body.
pub fn anonymous_type_name(body: &Node, bytes: &[u8]) -> Option<String> {
    if !is_anonymous_body(body) {
        return None;
    }
    let scope = enclosing_named_type(body)?;
    let mut found: Vec<usize> = Vec::new();
    collect_anonymous_body_starts(&scope, bytes, &scope, &mut found);
    found.sort_unstable();
    let position = found.iter().position(|s| *s == body.start_byte())?;
    Some((position + 1).to_string())
}

/// The nearest type DECLARATION above `node` — the scope anonymous classes are numbered within.
fn enclosing_named_type<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// Start offsets of every anonymous body whose numbering scope is `scope`, in tree order.
fn collect_anonymous_body_starts(node: &Node, bytes: &[u8], scope: &Node, out: &mut Vec<usize>) {
    let mut cw = node.walk();
    for c in node.named_children(&mut cw) {
        if is_anonymous_body(&c) {
            // Only the ones this scope numbers: an anonymous class nested inside a local class is
            // numbered within THAT class, and would otherwise be counted twice.
            if enclosing_named_type(&c).map(|n| n.id()) == Some(scope.id()) {
                out.push(c.start_byte());
            }
        }
        collect_anonymous_body_starts(&c, bytes, scope, out);
    }
}

/// Build the [`TypeDecl`] for an anonymous class body: its members, plus the type being
/// instantiated as its supertype so a member inherited from it still resolves.
fn collect_anonymous_type(
    body: &Node,
    bytes: &[u8],
    package: Option<&str>,
    outer_fqn: &str,
    out: &mut Vec<TypeDecl>,
) {
    let Some(name) = anonymous_type_name(body, bytes) else { return };
    let fqn = format!("{outer_fqn}.{name}");

    // Whether the instantiated type is a class or an interface is a question only the compiler can
    // answer, and the member walk follows both links — so it goes in `implements`, where being
    // wrong costs nothing. Putting it in `extends` would instead feed the "extends a final class"
    // and "must implement abstract" checks a supertype relationship they'd judge on its own terms.
    let implements = body
        .parent()
        .filter(|p| p.kind() == "object_creation_expression")
        .and_then(|p| p.child_by_field_name("type"))
        .and_then(|t| node_text(&t, bytes))
        .map(|t| vec![t])
        .unwrap_or_default();

    let mut methods = Vec::new();
    let mut fields = Vec::new();
    let mut bw = body.walk();
    for m in body.named_children(&mut bw) {
        collect_body_member(&m, bytes, false, package, &fqn, &mut methods, &mut fields, out);
    }

    out.push(TypeDecl {
        span: Some(Span::of(body)),
        name,
        fqn,
        kind: TypeKind::Class,
        is_anonymous: true,
        is_abstract: false,
        is_final: true, // an anonymous class can never be subclassed
        is_sealed: false,
        type_params: Vec::new(),
        methods,
        fields,
        extends: None,
        implements,
        annotations: Vec::new(),
    });
}

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
            collect_inner_types_in_body(m, bytes, package, fqn, out);
        }
        // Constructors are indexed as `<init>` members (like bytecode) so the super-constructor,
        // unhandled-exception-from-`new`, and constructor-arity checks work on project types.
        "constructor_declaration" => {
            if let Some(md) = parse_constructor(m, bytes) {
                methods.push(md);
            }
            collect_inner_types_in_body(m, bytes, package, fqn, out);
        }
        // A `static { … }` or instance `{ … }` initializer is a body like any other, and a local
        // type can be declared in one.
        // A `static { … }` / instance `{ … }` initializer, and a field initializer, are all places an
        // anonymous class is commonly written (`private Runnable r = new Runnable() { … };`).
        "static_initializer" | "block" => collect_inner_types(m, bytes, package, fqn, out),
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
            // `private Runnable r = new Runnable() { … };` — an initializer is one of the commonest
            // places to write an anonymous class, and it is not inside any method body.
            collect_inner_types(m, bytes, package, fqn, out);
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
                    let positional = annotation_positional_value(&a, bytes);
                    out.push(Annotation { name: simple, value, args, positional });
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

/// The first **positional** (non-`name =`) argument of an annotation, as raw source text, skipping
/// a string literal (which [`annotation_string_value`] already carries): `@Setter(AccessLevel.PACKAGE)`
/// → `AccessLevel.PACKAGE`, `@Getter(onMethod_ = @X)` → the annotation text. `None` for a marker, an
/// empty list, or an all-pairs argument list.
fn annotation_positional_value(annotation: &Node, bytes: &[u8]) -> Option<String> {
    let args = annotation.child_by_field_name("arguments")?;
    let mut aw = args.walk();
    for arg in args.named_children(&mut aw) {
        if matches!(arg.kind(), "element_value_pair" | "string_literal") {
            continue;
        }
        return node_text(&arg, bytes).map(|t| t.trim().to_string());
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
        span: Some(Span::of(node)),
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
/// Add the members the JLS says a `record` has, on top of the ones written in its body.
///
/// A record's header (`record Point(int x, int y)`) declares, per JLS §8.10, a `private final`
/// field and a `public` accessor **per component**, a canonical constructor taking them all in
/// order, and `toString` / `equals` / `hashCode`. None of that is in the source tree, so without
/// this every `point.x()` on a project record resolved to nothing — and "cannot resolve method"
/// on correct code is the worst answer an editor can give.
///
/// This belongs in the pure extractor, not beside `bennu-intel`'s Lombok synthesis, and the
/// distinction is the point: Lombok's members depend on an annotation, an import and a dependency
/// being present, so they are a *guess about the build* that has to be gated. A record's members
/// depend on nothing but the language, so they are as certain as the ones the parser read.
///
/// **A declared member always wins.** A record may write its own accessor, override `toString`, or
/// declare the canonical constructor explicitly (or a compact one) — in each case the synthetic
/// version must not be added, or the type would carry the same member twice and overload
/// resolution would see an ambiguity that doesn't exist. Matching is by name **and arity**, which
/// is what keeps a record that declares an extra `x(int)` overload from suppressing its own
/// zero-arg accessor.
/// Each record component's name, mapped to the span of that name in the header.
fn component_name_spans(
    record: &Node,
    bytes: &[u8],
) -> std::collections::HashMap<String, Span> {
    let mut out = std::collections::HashMap::new();
    let Some(params) = record.child_by_field_name("parameters") else { return out };
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        let Some(name_node) = param.child_by_field_name("name") else { continue };
        let Some(name) = node_text(&name_node, bytes) else { continue };
        out.insert(name, Span::of(&name_node));
    }
    out
}

fn synthesize_record_members(
    node: &Node,
    bytes: &[u8],
    methods: &mut Vec<MethodDecl>,
    fields: &mut Vec<FieldDecl>,
) {
    // `record R(int x)` puts its components in the `parameters` field, exactly like a method's —
    // so the existing parser handles them, varargs component included. A `record Empty()` yields
    // none, and still gets its constructor and the `Object` overrides below.
    let components = parse_params(node, bytes);
    // Where each component's NAME is written. A record's field and accessor are synthesized, but
    // they are not *unwritten*: the header is the one place the language lets you name them, and it
    // is where renaming one has to edit and where go-to has to land. Left as `None`, both fell back
    // to the generated-source view — "go to the accessor" opened a stub of the record instead of
    // the record — and a rename could find no declaration to edit.
    let component_spans = component_name_spans(node, bytes);

    for c in &components {
        let span = component_spans.get(&c.name).copied();
        // The backing field. `private final`, and it carries the component's own name — which is
        // also why a record can't declare instance fields of its own (checked in `bennu-check`).
        if !fields.iter().any(|f| f.name == c.name) {
            fields.push(FieldDecl {
                span,
                name: c.name.clone(),
                type_text: c.type_text.clone(),
                is_static: false,
                is_final: true,
                visibility: Visibility::Private,
                has_initializer: false, // assigned by the canonical constructor
                annotations: Vec::new(),
            });
        }
        // The accessor — named after the component, NOT `getX()`. Half the point of the bug report:
        // people reach for `p.x()`, and Java is on their side.
        if !is_declared(methods, &c.name, 0) {
            methods.push(MethodDecl {
                span,
                name: c.name.clone(),
                return_type_text: c.type_text.clone(),
                params: Vec::new(),
                is_static: false,
                visibility: Visibility::Public,
                is_abstract: false,
                is_default: false,
                is_final: false,
                throws: Vec::new(),
            });
        }
    }

    // The canonical constructor. Same `<init>` convention `parse_constructor` uses, so arity and
    // argument-type checks see it like any other. Suppressed when the body declares a constructor
    // of the same arity — the canonical or compact form the user wrote themselves.
    if !is_declared(methods, "<init>", components.len()) {
        methods.push(MethodDecl {
            span: None,
            name: "<init>".to_string(),
            return_type_text: "void".to_string(),
            params: components.clone(),
            is_static: false,
            visibility: Visibility::Public,
            is_abstract: false,
            is_default: false,
            is_final: false,
            throws: Vec::new(),
        });
    }

    // The `Object` overrides a record implements for you. Declared ON the record rather than found
    // by an inherited-from-Object lookup, which is what lets hover say the type provides them — and
    // what makes `hashCode()` on a record resolve at all when `java.lang.Object` isn't indexed.
    //
    // NOT `is_final`: JLS §8.10.3 lets a record declare its own `equals`/`hashCode`/`toString`, and
    // when it does the branch above suppresses the synthetic one anyway.
    for (name, ret, params) in [
        ("toString", "String", Vec::new()),
        ("hashCode", "int", Vec::new()),
        ("equals", "boolean", vec![ParamDecl { name: "o".into(), type_text: "Object".into() }]),
    ] {
        if is_declared(methods, name, params.len()) {
            continue;
        }
        methods.push(MethodDecl {
            span: None,
            name: name.to_string(),
            return_type_text: ret.to_string(),
            params,
            is_static: false,
            visibility: Visibility::Public,
            is_abstract: false,
            is_default: false,
            is_final: false,
            throws: Vec::new(),
        });
    }
}

/// Whether `methods` already holds one of that name and arity — the "a declared member wins" test.
/// Arity as well as name, so a record declaring an extra `x(int)` overload doesn't suppress the
/// zero-arg accessor the language owes it.
fn is_declared(methods: &[MethodDecl], name: &str, arity: usize) -> bool {
    methods.iter().any(|m| m.name == name && m.params.len() == arity)
}

fn parse_constructor(node: &Node, bytes: &[u8]) -> Option<MethodDecl> {
    node.child_by_field_name("name")?; // a real declaration names its class
    Some(MethodDecl {
        span: Some(Span::of(node)),
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
        span: Some(Span::of(node)),
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
                    // The DECLARATOR, not the whole `int a, b, c;` — two fields on one line are
                    // two rows, and each has to select its own.
                    span: Some(Span::of(&c)),
                    name,
                    type_text: type_text.clone(),
                    is_static,
                    is_final,
                    visibility,
                    has_initializer: c.child_by_field_name("value").is_some(),
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

    // ── spans ────────────────────────────────────────────────────────────────────

    /// A span has to point at the thing it belongs to, or every consumer of it (navigation, the
    /// model panel) lands somewhere plausible and wrong.
    #[test]
    fn a_declarations_span_covers_its_own_text() {
        let src = "package p;\nclass C {\n  private int total;\n  void go() {}\n}";
        let t = one_type(src);
        let span = t.span.expect("a parsed type has a span");
        assert!(src[span.start..span.end].starts_with("class C"));

        let field = t.fields.iter().find(|f| f.name == "total").expect("total");
        assert_eq!(&src[field.span.expect("a span").start..field.span.unwrap().end], "total");

        let method = t.methods.iter().find(|m| m.name == "go").expect("go");
        let span = method.span.expect("a span");
        assert!(src[span.start..span.end].starts_with("void go()"));
    }

    /// Two fields on one line are two rows, and each has to select its own — which is why the
    /// span is the declarator and not the whole `int a, b;`.
    #[test]
    fn each_declarator_on_a_shared_line_gets_its_own_span() {
        let src = "class C { int a, b; }";
        let t = one_type(src);
        let spans: Vec<&str> = t
            .fields
            .iter()
            .map(|f| &src[f.span.expect("a span").start..f.span.unwrap().end])
            .collect();
        assert_eq!(spans, ["a", "b"]);
    }

    /// **Nobody wrote these.** A record's accessor has no source, and a `0..0` would point at the
    /// package declaration — so it says "nowhere" instead.
    #[test]
    fn a_record_component_points_at_where_it_is_written() {
        // Synthesized is not the same as unwritten. A record's accessor and backing field are both
        // generated, but the component's NAME is in the header — and that is where a rename has to
        // edit and where go-to has to land. Left spanless, go-to on `p.x()` opened a generated stub
        // of the record instead of the record, and a rename could find no declaration to change.
        let src = "record P(int x) {}";
        let t = one_type(src);
        let component = src.find("int x").unwrap() + "int ".len();

        let accessor = t.methods.iter().find(|m| m.name == "x").expect("the accessor");
        assert_eq!(accessor.span.map(|s| s.start), Some(component));
        let field = t.fields.iter().find(|f| f.name == "x").expect("the backing field");
        assert_eq!(field.span.map(|s| s.start), Some(component));
        assert_eq!(field.span.map(|s| &src[s.start..s.end]), Some("x"));

        // A member with genuinely nothing written for it still has none — the canonical
        // constructor is not spelled anywhere in `record P(int x) {}`.
        let ctor = t.methods.iter().find(|m| m.name == "<init>").expect("the canonical ctor");
        assert!(ctor.span.is_none());

        // ...while the record itself is very much written down.
        assert!(t.span.is_some());
    }

    // ── record components (JLS §8.10) ────────────────────────────────────────────

    /// The reported bug: a record's generated members didn't exist, so `p.x()` / `p.toString()`
    /// resolved to nothing and were flagged "cannot resolve method" on correct code.
    #[test]
    fn a_record_gets_its_accessors_fields_constructor_and_object_overrides() {
        let t = one_type("record Point(int x, String label) {}");
        assert_eq!(t.kind, TypeKind::Record);

        // An accessor per component, named AFTER the component (not `getX`), returning its type.
        let x = t.methods.iter().find(|m| m.name == "x").expect("x() accessor");
        assert_eq!(x.return_type_text, "int");
        assert!(x.params.is_empty());
        assert_eq!(x.visibility, Visibility::Public);
        let label = t.methods.iter().find(|m| m.name == "label").expect("label() accessor");
        assert_eq!(label.return_type_text, "String");

        // A private final backing field per component.
        let fx = t.fields.iter().find(|f| f.name == "x").expect("x field");
        assert_eq!(fx.type_text, "int");
        assert!(fx.is_final);
        assert_eq!(fx.visibility, Visibility::Private);

        // The canonical constructor, components in order.
        let ctor = t.methods.iter().find(|m| m.name == "<init>").expect("canonical constructor");
        assert_eq!(
            ctor.params.iter().map(|p| p.type_text.as_str()).collect::<Vec<_>>(),
            vec!["int", "String"],
        );

        // The Object overrides a record implements for you.
        for (name, ret) in [("toString", "String"), ("hashCode", "int"), ("equals", "boolean")] {
            let m = t.methods.iter().find(|m| m.name == name).unwrap_or_else(|| panic!("{name}()"));
            assert_eq!(m.return_type_text, ret);
        }
        assert_eq!(
            t.methods.iter().find(|m| m.name == "equals").unwrap().params.len(),
            1,
            "equals takes one Object",
        );
    }

    /// A member the record writes itself wins — synthesizing it too would give the type the same
    /// method twice and invent an overload ambiguity.
    #[test]
    fn a_declared_record_member_is_not_synthesized_twice() {
        let t = one_type(
            "record Point(int x) {\n\
               public int x() { return x * 2; }\n\
               @Override public String toString() { return \"p\"; }\n\
             }",
        );
        assert_eq!(t.methods.iter().filter(|m| m.name == "x").count(), 1, "{:?}", t.methods);
        assert_eq!(t.methods.iter().filter(|m| m.name == "toString").count(), 1);
        // The user's accessor is the one kept — the body is theirs.
        assert_eq!(t.methods.iter().find(|m| m.name == "x").unwrap().return_type_text, "int");
    }

    /// An explicitly declared canonical constructor suppresses the synthetic one; an extra
    /// overload of a different arity does not.
    #[test]
    fn a_declared_constructor_of_the_same_arity_wins() {
        let one = one_type("record R(int x) { R(int x) { this.x = x; } }");
        assert_eq!(one.methods.iter().filter(|m| m.name == "<init>").count(), 1);

        // A convenience constructor of another arity leaves the canonical one owed.
        let two = one_type("record R(int x) { R() { this(0); } }");
        assert_eq!(two.methods.iter().filter(|m| m.name == "<init>").count(), 2, "{:?}", two.methods);
        assert!(two.methods.iter().any(|m| m.name == "<init>" && m.params.len() == 1));
    }

    /// A component-less record still gets its constructor and the Object overrides.
    #[test]
    fn an_empty_record_still_gets_its_overrides() {
        let t = one_type("record Unit() {}");
        assert!(t.fields.is_empty());
        assert!(t.methods.iter().any(|m| m.name == "<init>" && m.params.is_empty()));
        assert!(t.methods.iter().any(|m| m.name == "hashCode"));
    }

    /// A non-record is untouched — no phantom accessors on an ordinary class.
    #[test]
    fn a_class_gains_nothing() {
        let t = one_type("class C { private int x; }");
        assert!(t.methods.is_empty(), "{:?}", t.methods);
        assert_eq!(t.fields.len(), 1);
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

    /// The constants themselves are members too — `public static final E NAME`, as the compiler
    /// emits them. Without them a project enum reads as having no constants at all: `import static
    /// E.*` supplies no name, `E.` completes to nothing, and switch exhaustiveness gives up.
    #[test]
    fn enum_constants_are_indexed_as_static_fields_of_the_enum() {
        let t = one_type("enum Color { RED, GREEN(\"g\"), BLUE { void x() {} }; }");
        for c in ["RED", "GREEN", "BLUE"] {
            let f = t
                .fields
                .iter()
                .find(|f| f.name == c)
                .unwrap_or_else(|| panic!("constant {c} indexed: {:?}", t.fields));
            assert_eq!(f.type_text, "Color", "a constant's type is its own enum");
            assert!(f.is_static && f.is_final, "constants are static final");
        }
    }

    /// A constant with a body declares a subclass, and its members belong to THAT, not to the enum —
    /// indexing them on the enum would invent members the type does not have.
    #[test]
    fn a_constant_body_does_not_leak_members_onto_the_enum() {
        let t = one_type("enum E { A { void hidden() {} }; void real() {} }");
        assert!(t.methods.iter().any(|m| m.name == "real"));
        assert!(!t.methods.iter().any(|m| m.name == "hidden"), "{:?}", t.methods);
    }

    #[test]
    fn class_abstract_method_is_abstract() {
        let t = one_type("abstract class C { abstract void run(); void done() {} }");
        assert!(t.methods.iter().find(|m| m.name == "run").unwrap().is_abstract);
        // A concrete class method (with a body) is never marked abstract.
        assert!(!t.methods.iter().find(|m| m.name == "done").unwrap().is_abstract);
    }

    /// A class declared inside a method body — a **local class**, legal since Java 1.1. The walk
    /// never descended into method bodies, so it was invisible: absent from the index, its members
    /// unresolvable, and every use of it reported as an unknown type.
    #[test]
    fn a_class_declared_inside_a_method_is_extracted() {
        let src = "package p;\npublic class Outer {\n    void run() {\n        class Helper {\n            int count;\n            int twice() { return count * 2; }\n        }\n        new Helper().twice();\n    }\n}\n";
        let fs = extract_symbols(src);
        let helper = fs
            .types
            .iter()
            .find(|t| t.name == "Helper")
            .expect("the local class was not extracted");
        assert_eq!(helper.fqn, "p.Outer.Helper");
        assert!(helper.methods.iter().any(|m| m.name == "twice"));
        assert!(helper.fields.iter().any(|f| f.name == "count"));
    }

    /// Local interfaces, enums and records arrived in Java 16 (JEP 395); before that only a local
    /// *class* was legal. The extractor reads what is written rather than gating on a version.
    #[test]
    fn a_record_declared_inside_a_method_is_extracted() {
        let src = "package p;\npublic class Outer {\n    void run() {\n        record Point(int x, int y) {}\n        new Point(1, 2);\n    }\n}\n";
        let fs = extract_symbols(src);
        let point = fs.types.iter().find(|t| t.name == "Point").expect("the local record");
        assert_eq!(point.kind, TypeKind::Record);
        assert!(point.fields.iter().any(|f| f.name == "x"));
    }

    /// A local class inside a CONSTRUCTOR body, and one nested inside a local class's own method —
    /// the walk has to recurse, not just look one level down.
    #[test]
    fn local_types_nest_and_appear_in_constructors_too() {
        let src = "package p;\npublic class Outer {\n    Outer() {\n        class A {\n            void go() {\n                class B { }\n            }\n        }\n    }\n}\n";
        let fs = extract_symbols(src);
        assert!(fs.types.iter().any(|t| t.fqn == "p.Outer.A"), "local class in a constructor");
        assert!(fs.types.iter().any(|t| t.fqn == "p.Outer.A.B"), "local class inside a local class");
    }

    /// A static initializer block is a body too.
    #[test]
    fn a_local_class_in_a_static_initializer_is_extracted() {
        let src = "package p;\npublic class Outer {\n    static {\n        class Boot { }\n    }\n}\n";
        let fs = extract_symbols(src);
        assert!(fs.types.iter().any(|t| t.name == "Boot"));
    }

}
