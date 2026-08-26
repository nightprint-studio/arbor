//! The annotation-shaped Java scan every framework extension runs for itself.
//!
//! ## Why a second parser
//!
//! Bennu already has a Java symbol model (`bennu-java`), and this deliberately does not use
//! it. Two reasons, both about the seam rather than about parsing:
//!
//! 1. **Annotations are the whole substance here.** A framework lives in
//!    `@GetMapping("/orders/{id}")` and `@Column(name = "COD_UTENTE")` — in the *arguments*,
//!    with their byte spans, on methods and on parameters. Teaching the shared model to carry
//!    all of that would put framework-shaped data in the core for its consumers' benefit,
//!    which is exactly the coupling the extension seam exists to avoid.
//! 2. **An extension must be self-contained.** The day one becomes a WASM module it takes its
//!    scanner with it. Depending on the host's model would make that a rewrite.
//!
//! The cost is one tree-sitter pass over the files that matter — and only over those: the
//! caller pre-filters by a cheap substring test ([`mentions_any`]) so a thousand-file legacy
//! tree parses the couple of hundred files that mention the framework at all.
//!
//! Everything here is **facts, not policy**: what is written in the source, with spans. What
//! counts as a bean, what an endpoint path joins to, which repository method is well-formed —
//! that is each extension's own business.

use tree_sitter::Node;

/// The workspace's annotation model, re-exported under the name this crate's consumers use.
///
/// This crate used to declare its own, richer than the engine's, and fill it from its own parse of
/// the same file. Two models meant two answers to "what does this annotation say", and the richer
/// one — the one the frameworks actually read — was the one the project index never saw. There is
/// one now: see [`bennu_java::prelude::Annotation`].
pub use bennu_java::prelude::{AnnString, Annotation as AnnFacts};

/// A method parameter, with its annotations (`@PathVariable`, `@Qualifier`, `@Value`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamFacts {
    pub name: String,
    pub type_text: String,
    pub name_offset: usize,
    pub annotations: Vec<AnnFacts>,
}

/// A method or constructor. `is_constructor` rather than a magic `<init>` name, because
/// this model is read by humans writing framework rules, not by an arity checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodFacts {
    pub name: String,
    pub return_type: String,
    pub params: Vec<ParamFacts>,
    pub annotations: Vec<AnnFacts>,
    /// Byte offset of the method's NAME — the go-to target.
    pub name_offset: usize,
    pub is_static: bool,
    pub is_public: bool,
    pub is_constructor: bool,
}

/// A field, with its annotations (`@Autowired`, `@Value`, `@Qualifier`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFacts {
    pub name: String,
    pub type_text: String,
    pub name_offset: usize,
    pub is_static: bool,
    pub is_final: bool,
    pub is_public: bool,
    pub annotations: Vec<AnnFacts>,
}

/// A type declaration, flattened: a nested class is its own entry with a dotted `fqcn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeFacts {
    pub name: String,
    /// Dotted fully-qualified name (`com.acme.Outer.Inner` for a nested type).
    pub fqcn: String,
    /// `"class"` | `"interface"` | `"enum"` | `"record"` | `"annotation"`.
    pub kind: &'static str,
    pub is_abstract: bool,
    /// The `extends` clause as written (simple or qualified), empty when absent.
    pub extends: String,
    /// The `implements` clause entries as written.
    pub implements: Vec<String>,
    pub annotations: Vec<AnnFacts>,
    /// Byte offset of the type's NAME — the go-to target.
    pub name_offset: usize,
    pub methods: Vec<MethodFacts>,
    pub fields: Vec<FieldFacts>,
}

/// Everything one `.java` file contributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaFacts {
    /// Absolute path, forward-slashed.
    pub file: String,
    pub package: String,
    /// Import lines, normalized to the imported name (`com.acme.Foo`, `com.acme.*`).
    pub imports: Vec<String>,
    pub types: Vec<TypeFacts>,
}

/// Whether `source` mentions any of `markers` — the cheap `contains` pre-filter every
/// extension runs before deciding a file is worth a tree-sitter pass.
///
/// The markers are the **caller's**, because relevance is a framework question: what says
/// "this file is worth parsing" for Spring (`@Service`, `springframework`) says nothing for
/// JPA (`@Entity`, `persistence`). What is shared is the shape of the answer, and the
/// discipline behind it — a marker list should be deliberately **over-inclusive**, since a
/// false hit costs one parse and a false miss costs a feature silently not working on a file.
pub fn mentions_any(source: &str, markers: &[&str]) -> bool {
    markers.iter().any(|m| source.contains(m))
}

/// Parse one Java source into [`JavaFacts`]. `None` when the grammar can't be loaded.
/// A file with syntax errors still yields whatever parsed — tree-sitter recovers, and a
/// half-written file mid-keystroke must not blank the panel.
pub fn scan_java(file: &str, source: &str) -> Option<JavaFacts> {
    // The workspace's one Java parse — see `bennu_java::grammar`. Not a private parser, for three
    // reasons that all bit at once: the grammar pin belongs in one place, the parse cache means a
    // file the engine already read is not read again, and a construct the grammar cannot handle is
    // recovered there (`Object @Nullable ... args`) — so a Spring `@Value` in such a file was
    // invisible to this scan while being perfectly visible to everything else.
    let tree = bennu_java::prelude::parse_java(source)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut facts = JavaFacts {
        file: file.replace('\\', "/"),
        package: String::new(),
        imports: Vec::new(),
        types: Vec::new(),
    };

    let mut cw = root.walk();
    for child in root.named_children(&mut cw) {
        match child.kind() {
            "package_declaration" => {
                let mut pw = child.walk();
                facts.package = child
                    .named_children(&mut pw)
                    .find(|n| matches!(n.kind(), "scoped_identifier" | "identifier"))
                    .and_then(|n| text(&n, bytes))
                    .unwrap_or_default();
            }
            "import_declaration" => {
                if let Some(t) = text(&child, bytes) {
                    let t = t
                        .trim()
                        .trim_start_matches("import")
                        .trim()
                        .trim_start_matches("static")
                        .trim()
                        .trim_end_matches(';')
                        .trim();
                    if !t.is_empty() {
                        facts.imports.push(t.to_string());
                    }
                }
            }
            k if is_type_decl(k) => {
                collect_type(&child, bytes, &facts.package, &mut facts.types);
            }
            _ => {}
        }
    }
    Some(facts)
}

fn is_type_decl(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

fn type_kind(kind: &str) -> &'static str {
    match kind {
        "interface_declaration" => "interface",
        "enum_declaration" => "enum",
        "record_declaration" => "record",
        "annotation_type_declaration" => "annotation",
        _ => "class",
    }
}

/// Extract a type declaration (and, recursively, its nested types) into `out`.
/// `owner` is the dotted prefix — the package for a top-level type, the enclosing type's
/// fqcn for a nested one.
fn collect_type(node: &Node, bytes: &[u8], owner: &str, out: &mut Vec<TypeFacts>) {
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Some(name) = text(&name_node, bytes) else { return };
    let fqcn = if owner.is_empty() { name.clone() } else { format!("{owner}.{name}") };

    let mut facts = TypeFacts {
        name,
        fqcn: fqcn.clone(),
        kind: type_kind(node.kind()),
        is_abstract: has_modifier(node, "abstract"),
        extends: clause_text(node, bytes, "superclass", "extends"),
        implements: interfaces_of(node, bytes),
        annotations: bennu_java::prelude::collect_annotations(node, bytes),
        name_offset: name_node.start_byte(),
        methods: Vec::new(),
        fields: Vec::new(),
    };

    // A **record** keeps its state in the header, not in the body: `record Foo(String a, int b)`
    // has no `field_declaration` anywhere, so a body-only walk sees a type with no state at all.
    // That is not a curiosity — a record is the modern way to write a `@ConfigurationProperties`
    // class (Boot binds it through the constructor), and treating its components as anything
    // other than fields loses every key such a class binds.
    if node.kind() == "record_declaration" {
        record_components(node, bytes, &mut facts.fields);
    }
    if let Some(body) = node.child_by_field_name("body") {
        collect_members(&body, bytes, &fqcn, &mut facts, out);
    }
    out.push(facts);
}

/// A record's components, as fields — which is what they are to every consumer here: named,
/// typed, annotatable, and readable through a generated accessor. They are `final` and their
/// accessor is public, so they are recorded that way.
fn record_components(node: &Node, bytes: &[u8], out: &mut Vec<FieldFacts>) {
    let Some(list) = node.child_by_field_name("parameters") else { return };
    let mut w = list.walk();
    for p in list.named_children(&mut w) {
        if p.kind() != "formal_parameter" {
            continue;
        }
        let Some(name_node) = p.child_by_field_name("name") else { continue };
        let Some(name) = text(&name_node, bytes) else { continue };
        out.push(FieldFacts {
            name,
            type_text: p
                .child_by_field_name("type")
                .and_then(|n| text(&n, bytes))
                .unwrap_or_default(),
            name_offset: name_node.start_byte(),
            is_static: false,
            is_final: true,
            is_public: true,
            annotations: bennu_java::prelude::collect_annotations(&p, bytes),
        });
    }
}

/// Walk a type body, filling `facts` with its own members and `out` with its nested types.
fn collect_members(
    body: &Node,
    bytes: &[u8],
    fqcn: &str,
    facts: &mut TypeFacts,
    out: &mut Vec<TypeFacts>,
) {
    let mut bw = body.walk();
    for m in body.named_children(&mut bw) {
        match m.kind() {
            "method_declaration" => {
                if let Some(f) = method_facts(&m, bytes, false) {
                    facts.methods.push(f);
                }
            }
            "compact_constructor_declaration" | "constructor_declaration" => {
                if let Some(f) = method_facts(&m, bytes, true) {
                    facts.methods.push(f);
                }
            }
            "field_declaration" => field_facts(&m, bytes, &mut facts.fields),
            // An enum's members live one level deeper, inside `enum_body_declarations`.
            "enum_body_declarations" => collect_members(&m, bytes, fqcn, facts, out),
            k if is_type_decl(k) => collect_type(&m, bytes, fqcn, out),
            _ => {}
        }
    }
}

fn method_facts(node: &Node, bytes: &[u8], is_constructor: bool) -> Option<MethodFacts> {
    let name_node = node.child_by_field_name("name")?;
    Some(MethodFacts {
        name: text(&name_node, bytes)?,
        return_type: node
            .child_by_field_name("type")
            .and_then(|n| text(&n, bytes))
            .unwrap_or_else(|| "void".to_string()),
        params: params_of(node, bytes),
        annotations: bennu_java::prelude::collect_annotations(node, bytes),
        name_offset: name_node.start_byte(),
        is_static: has_modifier(node, "static"),
        is_public: has_modifier(node, "public"),
        is_constructor,
    })
}

fn params_of(node: &Node, bytes: &[u8]) -> Vec<ParamFacts> {
    let mut out = Vec::new();
    let Some(list) = node.child_by_field_name("parameters") else { return out };
    let mut pw = list.walk();
    for p in list.named_children(&mut pw) {
        if !matches!(p.kind(), "formal_parameter" | "spread_parameter") {
            continue;
        }
        // A spread parameter keeps its name inside a `variable_declarator` and its type as
        // an unnamed child, so both are looked up leniently.
        let mut dw = p.walk();
        let name_node = p.child_by_field_name("name").or_else(|| {
            p.named_children(&mut dw)
                .find(|c| c.kind() == "variable_declarator")
                .and_then(|d| d.child_by_field_name("name"))
        });
        let Some(name_node) = name_node else { continue };
        out.push(ParamFacts {
            name: text(&name_node, bytes).unwrap_or_default(),
            type_text: p
                .child_by_field_name("type")
                .and_then(|n| text(&n, bytes))
                .unwrap_or_default(),
            name_offset: name_node.start_byte(),
            annotations: bennu_java::prelude::collect_annotations(&p, bytes),
        });
    }
    out
}

fn field_facts(node: &Node, bytes: &[u8], out: &mut Vec<FieldFacts>) {
    let type_text =
        node.child_by_field_name("type").and_then(|n| text(&n, bytes)).unwrap_or_default();
    let annotations = bennu_java::prelude::collect_annotations(node, bytes);
    let is_static = has_modifier(node, "static");
    let is_final = has_modifier(node, "final");
    let is_public = has_modifier(node, "public");
    let mut w = node.walk();
    for d in node.named_children(&mut w) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = d.child_by_field_name("name") else { continue };
        let Some(name) = text(&name_node, bytes) else { continue };
        out.push(FieldFacts {
            name,
            type_text: type_text.clone(),
            name_offset: name_node.start_byte(),
            is_static,
            is_final,
            is_public,
            annotations: annotations.clone(),
        });
    }
}

/// The text of a clause field (`superclass` / `permits`), with its keyword stripped.
fn clause_text(node: &Node, bytes: &[u8], field: &str, keyword: &str) -> String {
    node.child_by_field_name(field)
        .and_then(|n| text(&n, bytes))
        .map(|t| t.trim().trim_start_matches(keyword).trim().to_string())
        .unwrap_or_default()
}

/// The `implements` (or interface `extends`) list, one entry per interface.
///
/// Found by field **and** by node kind, because the grammar only names one of the two. A
/// `class_declaration` carries its list under the `interfaces` field, but an
/// `interface_declaration`'s `extends_interfaces` is an ordinary child with no field name at
/// all — so a field-only lookup reports that `interface OrderRepository extends
/// JpaRepository<Order, Long>` extends nothing.
///
/// That is not a corner case: a Spring Data repository **is** an interface whose entire
/// declaration is its extends clause, so missing it made the whole JPA extension see a project
/// with no repositories in it.
fn interfaces_of(node: &Node, bytes: &[u8]) -> Vec<String> {
    let mut w = node.walk();
    let by_kind = node
        .named_children(&mut w)
        .find(|c| matches!(c.kind(), "extends_interfaces" | "super_interfaces"));
    let Some(clause) = node.child_by_field_name("interfaces").or(by_kind) else {
        return Vec::new();
    };
    let Some(raw) = text(&clause, bytes) else { return Vec::new() };
    split_types(raw.trim().trim_start_matches("implements").trim_start_matches("extends"))
}

/// Split a comma-separated type list at **depth-0 commas only**.
///
/// The bug this replaced was a plain `split(',')`, and it was expensive out of all proportion to
/// its size: `extends JpaRepository<Order, Long>` came back as `JpaRepository<Order` and `Long>`.
/// The first still *looked* right to anything checking the name — `strip_generics` gives
/// `JpaRepository`, so the type was recognised — but its type arguments were unreadable, so every
/// consumer asking "which entity does this repository manage" got nothing and silently carried on
/// with an empty answer. A generated method came out as `List<> findById(Object id)`.
///
/// That is the shape of failure worth writing a comment about: not a crash, not an error, but a
/// correct-looking name with its arguments quietly amputated.
fn split_types(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in list.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth <= 0 => {
                push_type(&list[start..i], &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    push_type(&list[start..], &mut out);
    out
}

fn push_type(text: &str, out: &mut Vec<String>) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
}

/// Whether a declaration carries a modifier keyword.
///
/// Read off the `modifiers` node's own **keyword tokens**, never off its text. An annotation is a
/// modifier too, so `@Value("abstract static")` puts both of those words inside that node — and a
/// text match finds them there however careful it is about word boundaries, which is how a class
/// came back abstract because of a string it was annotated with.
fn has_modifier(node: &Node, keyword: &str) -> bool {
    let mut w = node.walk();
    // Bound to a local rather than left as the tail expression: the iterator borrows the
    // cursor, and a tail expression outlives the block's own locals.
    let found = node.children(&mut w).any(|c| {
        if c.kind() != "modifiers" {
            return false;
        }
        let mut mw = c.walk();
        // An anonymous token's `kind()` IS its text, so this compares the keyword itself and
        // cannot reach inside an annotation child.
        let hit = c.children(&mut mw).any(|m| m.kind() == keyword);
        hit
    });
    found
}





fn text(node: &Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(src: &str) -> JavaFacts {
        scan_java("/p/src/main/java/com/acme/T.java", src).expect("grammar loads")
    }

    #[test]
    fn package_imports_and_type_identity() {
        let f = scan(
            "package com.acme.order;\n\
             import java.util.List;\n\
             import static com.acme.Util.help;\n\
             @Service(\"orderSvc\")\n\
             public class OrderService implements Ordering, Auditable {}\n",
        );
        assert_eq!(f.package, "com.acme.order");
        assert_eq!(f.imports, ["java.util.List", "com.acme.Util.help"]);
        let t = &f.types[0];
        assert_eq!(t.fqcn, "com.acme.order.OrderService");
        assert_eq!(t.kind, "class");
        assert_eq!(t.implements, ["Ordering", "Auditable"]);
        assert_eq!(t.annotations[0].name, "Service");
        assert_eq!(t.annotations[0].value().unwrap().value, "orderSvc");
    }

    #[test]
    fn annotation_string_span_points_inside_the_quotes() {
        let src = "class C { @Value(\"${app.timeout:30}\") int t; }";
        let f = scan(src);
        let s = f.types[0].fields[0].annotations[0].value().unwrap();
        assert_eq!(s.value, "${app.timeout:30}");
        assert_eq!(&src[s.start..s.end], "${app.timeout:30}", "span excludes the quotes");
    }

    #[test]
    fn method_and_parameter_annotations_are_captured() {
        let f = scan(
            "class C {\n\
               @GetMapping(\"/orders/{id}\")\n\
               public String get(@PathVariable(\"id\") Long id, @RequestParam String q) { return null; }\n\
             }",
        );
        let m = &f.types[0].methods[0];
        assert_eq!(m.annotations[0].name, "GetMapping");
        assert_eq!(m.annotations[0].value().unwrap().value, "/orders/{id}");
        assert!(m.is_public);
        assert_eq!(m.params[0].annotations[0].name, "PathVariable");
        assert_eq!(m.params[0].annotations[0].value().unwrap().value, "id");
        assert_eq!(m.params[0].type_text, "Long");
        assert_eq!(m.params[1].annotations[0].name, "RequestParam");
    }

    #[test]
    fn array_valued_element_yields_every_string() {
        let f = scan("@RequestMapping(path = {\"/a\", \"/b\"}, method = RequestMethod.POST) class C {}");
        let a = &f.types[0].annotations[0];
        assert_eq!(
            a.strings_for("path").map(|s| s.value.as_str()).collect::<Vec<_>>(),
            ["/a", "/b"]
        );
        assert_eq!(a.pair("method"), Some("RequestMethod.POST"));
    }

    #[test]
    fn constructor_params_carry_their_qualifiers() {
        let f = scan(
            "class C { C(@Qualifier(\"primaryDs\") DataSource ds) { } void m() {} }",
        );
        let ctor = f.types[0].methods.iter().find(|m| m.is_constructor).expect("constructor");
        assert_eq!(ctor.params[0].annotations[0].value().unwrap().value, "primaryDs");
        assert!(f.types[0].methods.iter().any(|m| !m.is_constructor && m.name == "m"));
    }

    #[test]
    fn several_declarators_on_one_field_line_each_get_the_annotations() {
        let f = scan("class C { @Autowired private Foo a, b; }");
        let names: Vec<_> = f.types[0].fields.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert!(f.types[0].fields.iter().all(|x| x.annotations[0].name == "Autowired"));
    }

    #[test]
    fn nested_types_are_flattened_with_dotted_names() {
        let f = scan("package p; class Outer { @Configuration static class Inner {} }");
        let inner = f.types.iter().find(|t| t.name == "Inner").expect("nested type");
        assert_eq!(inner.fqcn, "p.Outer.Inner");
        assert_eq!(inner.annotations[0].name, "Configuration");
    }

    #[test]
    fn fqcn_uses_the_simple_name_when_there_is_no_package() {
        assert_eq!(scan("class Loose {}").types[0].fqcn, "Loose");
    }

    #[test]
    fn abstract_and_interface_are_visible() {
        let f = scan("abstract class A extends B {} interface I {} enum E { X; void m() {} }");
        let a = f.types.iter().find(|t| t.name == "A").unwrap();
        assert!(a.is_abstract);
        assert_eq!(a.extends, "B");
        assert_eq!(f.types.iter().find(|t| t.name == "I").unwrap().kind, "interface");
        let e = f.types.iter().find(|t| t.name == "E").unwrap();
        assert_eq!(e.kind, "enum");
        assert!(e.methods.iter().any(|m| m.name == "m"), "enum body members are reached");
    }

    #[test]
    fn a_string_argument_never_reads_as_a_modifier() {
        // An annotation IS a modifier, so both words sit inside the `modifiers` node. Reading its
        // TEXT finds them there however careful the word boundaries are — which is why the
        // keywords are read as tokens instead.
        let f = scan("@Value(\"abstract static\") class C {}");
        assert!(!f.types[0].is_abstract);
    }

    #[test]
    fn fully_qualified_annotation_reduces_to_its_simple_name() {
        let f = scan("@org.springframework.stereotype.Service class C {}");
        assert_eq!(f.types[0].annotations[0].name, "Service");
    }

    /// A text block is a string literal with different delimiters, and it is how anyone writes a
    /// multi-line one — which is to say, how anyone writes a real JPQL query.
    #[test]
    fn a_text_block_is_read_as_the_string_it_is() {
        let src = "class C { @Query(\"\"\"\n    select o\n    from Order o\n    \"\"\") Object m(); }";
        let f = scan(src);
        let s = f.types[0].methods[0].annotations[0].value().unwrap();
        assert!(s.value.contains("select o"));
        assert!(s.value.contains("from Order o"));
        // The span must index the file as written — a highlight is drawn over it.
        assert_eq!(&src[s.start..s.end], s.value);
        assert!(!s.value.contains('"'), "the delimiters are not part of the value");
    }

    /// Incidental indentation is NOT stripped, even though the JLS strips it: shortening the
    /// text would move every span computed against it.
    #[test]
    fn a_text_blocks_indentation_is_kept_so_the_spans_stay_true() {
        let src = "class C { @Query(\"\"\"\n        select 1\n        \"\"\") Object m(); }";
        let f = scan(src);
        let s = f.types[0].methods[0].annotations[0].value().unwrap();
        assert!(s.value.starts_with("        select"), "got: {:?}", s.value);
        assert_eq!(&src[s.start..s.end], s.value);
    }

    #[test]
    fn a_truncated_text_block_yields_nothing_rather_than_a_bad_span() {
        // Mid-keystroke: the closing delimiter is not there yet, so the opener runs to the end of
        // the file and the class body never closes — which usually means there is no type to
        // report at all. That IS the graceful answer, and it is why every level here is
        // tolerated: what must never happen is a span that indexes backwards.
        let f = scan("class C { @Query(\"\"\") Object m(); }");
        assert!(f.types.first().is_none_or(|t| t.methods.first().is_none_or(|m| m
            .annotations
            .first()
            .is_none_or(|a| a.value().is_none_or(|s| s.end >= s.start)))));
    }

    /// An interface's `extends` list has no field name in the grammar, so a field-only lookup
    /// reported that a Spring Data repository extends nothing — which made the whole JPA
    /// extension see a project with no repositories in it.
    #[test]
    fn an_interfaces_extends_list_is_read() {
        // ONE base with two type arguments — not two bases. Splitting on every comma produced
        // `JpaRepository<Order` and `Long>`, which still passed a name check (`strip_generics`
        // gives `JpaRepository`) while making the type arguments unreadable, so every consumer
        // asking which entity the repository manages got an empty answer and carried on.
        let f = scan("interface OrderRepository extends JpaRepository<Order, Long> {}");
        assert_eq!(f.types[0].implements, ["JpaRepository<Order, Long>"]);

        // Several, and a class's `implements` still works the same way.
        let f = scan("interface R extends A<X>, B {}");
        assert_eq!(f.types[0].implements, ["A<X>", "B"]);
        // Nesting, and a bound that carries its own comma.
        let f = scan("interface R extends A<Map<K, V>, X>, B<Y> {}");
        assert_eq!(f.types[0].implements, ["A<Map<K, V>, X>", "B<Y>"]);
        let f = scan("class C extends Base implements A, B {}");
        assert_eq!(f.types[0].extends, "Base");
        assert_eq!(f.types[0].implements, ["A", "B"]);
    }

    /// A record keeps its state in the header. Reading only the body sees a type with no
    /// state — and a record is the modern way to write a `@ConfigurationProperties` class, so
    /// missing this loses every key such a class binds.
    #[test]
    fn record_components_are_read_as_fields() {
        let f = scan("record Endpoint(String url, @Name(\"ms\") int readTimeout) {}");
        let fields = &f.types[0].fields;
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "url");
        assert_eq!(fields[0].type_text, "String");
        assert!(fields[0].is_final, "a component is final");
        assert_eq!(fields[1].name, "readTimeout");
        assert_eq!(fields[1].annotations[0].name, "Name", "an annotated component keeps it");
        assert_eq!(f.types[0].kind, "record");
    }

    /// A record can carry a body too; the two sources of state must both be read, not one.
    #[test]
    fn a_record_with_a_body_keeps_both_its_components_and_its_static_fields() {
        let f = scan("record R(String a) { static final int MAX = 3; }");
        let names: Vec<&str> = f.types[0].fields.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["a", "MAX"]);
    }

    #[test]
    fn the_prefilter_is_a_plain_substring_test_over_the_callers_markers() {
        const MARKERS: &[&str] = &["@Service", "@Entity", "springframework"];
        assert!(mentions_any("@Service public class A {}", MARKERS));
        assert!(mentions_any("import org.springframework.stereotype.Service;", MARKERS));
        assert!(!mentions_any("public class PlainOldJava { int x; }", MARKERS));
        assert!(!mentions_any("@Service class A {}", &[]), "no markers, no interest");
    }

    #[test]
    fn a_file_with_a_syntax_error_still_yields_what_parsed() {
        let f = scan("@Service class Broken { void m( { }");
        assert_eq!(f.types[0].name, "Broken");
        assert_eq!(f.types[0].annotations[0].name, "Service");
    }
}
