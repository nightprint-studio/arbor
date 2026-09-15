//! Reading a class as the JSON it produces.
//!
//! The model is deliberately small: the **properties** a class exposes, where each one came from,
//! and what it will be called. Every check here is a statement about that list rather than about
//! the syntax that produced it, which is why they read as things a person would say — *two members
//! claim the same name*, *this field has no way out*.

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

/// One member that takes part in serialisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// The JSON name — `@JsonProperty`'s value when there is one, else the member's own.
    pub name: String,
    /// The Java member it came from.
    pub member: String,
    pub kind: MemberKind,
    /// Byte span of the member's name — what a diagnostic underlines.
    pub start: usize,
    pub end: usize,
    /// It carries `@JsonIgnore`.
    pub ignored: bool,
    /// It carries an explicit `@JsonProperty`.
    pub explicit: bool,
    /// Written type, for a field.
    pub type_text: String,
    /// The member is `public`.
    pub public: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Field,
    Getter,
    Setter,
}

/// A class read as a DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dto {
    pub name: String,
    /// Byte offset of the class name.
    pub offset: usize,
    pub properties: Vec<Property>,
    /// The class carries at least one Jackson annotation somewhere — the gate that says it is
    /// meant to be serialised rather than merely being a class in a project that uses Jackson.
    pub jackson: bool,
    /// The class widens or narrows Jackson's visibility itself, so nothing here may reason about
    /// what is reachable.
    pub auto_detect: bool,
    /// Lombok generates accessors for it. Nothing here may reason about missing ones.
    pub lombok_accessors: bool,
}

/// The Lombok annotations that generate a getter, at class level.
const LOMBOK_ACCESSORS: [&str; 5] = ["Data", "Getter", "Value", "Setter", "Builder"];

/// Read every class in a source as a DTO. Empty when the file mentions no Jackson at all.
pub fn dtos_in(source: &str) -> Vec<Dto> {
    if !source.contains("Json") && !source.contains("jackson") {
        return Vec::new();
    }
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    collect(tree.root_node(), source, &mut out);
    out
}

fn collect(node: Node<'_>, source: &str, out: &mut Vec<Dto>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(child.kind(), "class_declaration" | "record_declaration") {
            if let Some(dto) = read_class(child, source) {
                out.push(dto);
            }
        }
        collect(child, source, out);
    }
}

fn read_class(decl: Node<'_>, source: &str) -> Option<Dto> {
    let name_node = decl.child_by_field_name("name")?;
    let body = decl.child_by_field_name("body")?;
    let class_annotations = annotations_of(decl, source);

    let mut dto = Dto {
        name: text(&name_node, source).to_string(),
        offset: name_node.start_byte(),
        properties: Vec::new(),
        jackson: class_annotations.iter().any(|(n, _)| is_jackson(n)),
        auto_detect: class_annotations.iter().any(|(n, _)| simple(n) == "JsonAutoDetect"),
        lombok_accessors: class_annotations
            .iter()
            .any(|(n, _)| LOMBOK_ACCESSORS.contains(&simple(n))),
    };

    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        match member.kind() {
            "field_declaration" => {
                if let Some(p) = read_field(member, source, &mut dto) {
                    dto.properties.push(p);
                }
            }
            "method_declaration" => {
                if let Some(p) = read_method(member, source, &mut dto) {
                    dto.properties.push(p);
                }
            }
            _ => {}
        }
    }
    Some(dto)
}

fn read_field(member: Node<'_>, source: &str, dto: &mut Dto) -> Option<Property> {
    let words = modifier_words(member, source);
    // A `static` field is not a property, and a `transient` one is explicitly not serialised.
    if words.iter().any(|w| w == "static" || w == "transient") {
        return None;
    }
    let declarator = member.child_by_field_name("declarator")?;
    let name_node = declarator.child_by_field_name("name")?;
    let annotations = annotations_of(member, source);
    note(dto, &annotations);
    if annotations.iter().any(|(n, _)| LOMBOK_ACCESSORS.contains(&simple(n))) {
        dto.lombok_accessors = true;
    }
    let member_name = text(&name_node, source).to_string();
    Some(Property {
        name: json_name(&annotations, source).unwrap_or_else(|| member_name.clone()),
        member: member_name,
        kind: MemberKind::Field,
        start: name_node.start_byte(),
        end: name_node.end_byte(),
        ignored: annotations.iter().any(|(n, _)| simple(n) == "JsonIgnore"),
        explicit: annotations.iter().any(|(n, _)| simple(n) == "JsonProperty"),
        type_text: member
            .child_by_field_name("type")
            .map(|t| text(&t, source).trim().to_string())
            .unwrap_or_default(),
        public: words.iter().any(|w| w == "public"),
    })
}

fn read_method(member: Node<'_>, source: &str, dto: &mut Dto) -> Option<Property> {
    let words = modifier_words(member, source);
    if words.iter().any(|w| w == "static") {
        return None;
    }
    let name_node = member.child_by_field_name("name")?;
    let name = text(&name_node, source);
    let annotations = annotations_of(member, source);
    note(dto, &annotations);

    let params = member
        .child_by_field_name("parameters")
        .map(|p| p.named_child_count())
        .unwrap_or(0);
    let (kind, bare) = if params == 0 {
        let bare = name
            .strip_prefix("get")
            .or_else(|| name.strip_prefix("is"))
            .filter(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_uppercase()))?;
        (MemberKind::Getter, bare)
    } else if params == 1 {
        let bare = name
            .strip_prefix("set")
            .filter(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_uppercase()))?;
        (MemberKind::Setter, bare)
    } else {
        return None;
    };

    let decapitalised = decapitalise(bare);
    Some(Property {
        name: json_name(&annotations, source).unwrap_or_else(|| decapitalised.clone()),
        member: decapitalised,
        kind,
        start: name_node.start_byte(),
        end: name_node.end_byte(),
        ignored: annotations.iter().any(|(n, _)| simple(n) == "JsonIgnore"),
        explicit: annotations.iter().any(|(n, _)| simple(n) == "JsonProperty"),
        type_text: String::new(),
        public: words.iter().any(|w| w == "public"),
    })
}

/// Record that a member's annotations mark this class as Jackson's business.
fn note(dto: &mut Dto, annotations: &[(String, Node<'_>)]) {
    if annotations.iter().any(|(n, _)| is_jackson(n)) {
        dto.jackson = true;
    }
    if annotations.iter().any(|(n, _)| simple(n) == "JsonAutoDetect") {
        dto.auto_detect = true;
    }
}

/// `getOrderId` → `orderId`. Java Beans decapitalises the first letter unless the first two are
/// both capitals (`getURL` → `URL`), which is a rule Jackson follows and people forget.
fn decapitalise(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return String::new() };
    let second = name.chars().nth(1);
    if second.is_some_and(|c| c.is_ascii_uppercase()) {
        return name.to_string();
    }
    format!("{}{}", first.to_ascii_lowercase(), chars.as_str())
}

/// The name a `@JsonProperty("…")` gives, when it gives one.
fn json_name(annotations: &[(String, Node<'_>)], source: &str) -> Option<String> {
    let (_, node) = annotations.iter().find(|(n, _)| simple(n) == "JsonProperty")?;
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        let literal = match child.kind() {
            "string_literal" => child,
            "element_value_pair" => {
                let key = child.child_by_field_name("key")?;
                if !matches!(text(&key, source), "value") {
                    continue;
                }
                child.child_by_field_name("value")?
            }
            _ => continue,
        };
        if literal.kind() != "string_literal" {
            continue;
        }
        let raw = text(&literal, source);
        if raw.len() >= 2 && raw.starts_with('"') {
            let inner = &raw[1..raw.len() - 1];
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }
    None
}

fn is_jackson(name: &str) -> bool {
    simple(name).starts_with("Json")
}

fn simple(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn annotations_of<'t>(decl: Node<'t>, source: &str) -> Vec<(String, Node<'t>)> {
    let Some(modifiers) = child_of_kind(decl, "modifiers") else { return Vec::new() };
    let mut out = Vec::new();
    let mut cursor = modifiers.walk();
    for child in modifiers.named_children(&mut cursor) {
        if !matches!(child.kind(), "annotation" | "marker_annotation") {
            continue;
        }
        if let Some(name) = child.child_by_field_name("name") {
            out.push((text(&name, source).to_string(), child));
        }
    }
    out
}

fn modifier_words(decl: Node<'_>, source: &str) -> Vec<String> {
    let Some(modifiers) = child_of_kind(decl, "modifiers") else { return Vec::new() };
    let mut out = Vec::new();
    let mut cursor = modifiers.walk();
    for child in modifiers.children(&mut cursor) {
        if matches!(child.kind(), "annotation" | "marker_annotation") {
            continue;
        }
        out.push(text(&child, source).to_string());
    }
    out
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|c| c.kind() == kind);
    found
}

fn text<'a>(node: &Node<'_>, source: &'a str) -> &'a str {
    source.get(node.start_byte()..node.end_byte()).unwrap_or_default()
}
