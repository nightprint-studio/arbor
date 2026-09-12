//! A class read the way a template needs it: its annotations, supertypes and imports, and each field's
//! type, JSON name, accessors, annotations and constraints.
//!
//! Read from **source**, which is instant and needs nothing compiled. It is the one model every kind
//! of template that starts from a class reads — so a builder template, a repository template and the
//! DTO Lab's validation tests agree about what a field is.

use std::collections::BTreeMap;

use bennu_jackson::prelude::{dtos_in, MemberKind};
use bennu_jakarta::prelude::{constraint, constraints_in};
use bennu_java::prelude::parse_java;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

/// A class or a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClassModel {
    /// Simple name (`Order`).
    pub name: String,
    /// Empty for the default package.
    pub package: String,
    /// `com.example.Order.Line` — how Java source names it.
    pub fqn: String,
    /// `com.example.Order$Line` — how a class loader is asked for it.
    pub binary: String,
    /// `Order.Line` — where it is inside its file.
    pub path_in_file: String,
    /// A record rather than a class.
    pub record: bool,
    /// The annotations on the class.
    pub annotations: Vec<AnnotationModel>,
    /// Their simple names — `"Entity" in class.annotation_names`.
    pub annotation_names: Vec<String>,
    /// What it extends, as written.
    pub superclass: Option<String>,
    /// What it implements, as written.
    pub interfaces: Vec<String>,
    /// The file's imports, as written (`java.util.List`, `static org.junit.Assert.*`).
    pub imports: Vec<String>,
    /// The type of the field annotated `@Id` or `@EmbeddedId`, boxed (`long` is `Long`), which is the
    /// only form a generic argument such as a repository's can take.
    pub id_type: Option<String>,
    /// Instance fields in declaration order — a record's components.
    pub fields: Vec<FieldModel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FieldModel {
    pub name: String,
    /// What Jackson calls it: `@JsonProperty`'s value, else the field's own name.
    pub json_name: String,
    /// As written (`List<String>`, `java.time.LocalDate`).
    pub type_name: String,
    /// The simple name without generics (`List`, `LocalDate`, `String[]`).
    pub type_simple: String,
    /// Declared `final` — a record component always is.
    #[serde(rename = "final")]
    pub is_final: bool,
    /// Annotated `@Id` or `@EmbeddedId`.
    pub id: bool,
    /// Carries `@JsonIgnore`, so no payload can set it.
    pub ignored: bool,
    /// The method that sets it: `setName`, a fluent `name`, or one Lombok generates.
    pub setter: Option<String>,
    /// The setter returns the object, so calls chain — `order.setName(n).setQuantity(1)` — Lombok's
    /// `@Accessors(chain = true)` included.
    pub setter_chains: bool,
    pub getter: Option<String>,
    /// The method that returns a copy with it changed: a declared `withName`, or Lombok's `@With`.
    pub wither: Option<String>,
    pub annotations: Vec<AnnotationModel>,
    /// Their simple names — `"Column" in field.annotation_names`.
    pub annotation_names: Vec<String>,
    /// Its Bean Validation constraints.
    pub constraints: Vec<ConstraintModel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnnotationModel {
    /// Simple name (`Column`).
    pub name: String,
    /// Every attribute written, as source text; a string literal without its quotes. A lone value
    /// (`@Size(3)`) is `value`.
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintModel {
    /// Simple name (`Size`).
    pub name: String,
    /// Fully qualified when known (`jakarta.validation.constraints.Size`), else the simple name.
    pub fqn: String,
    pub attributes: BTreeMap<String, String>,
    /// The `message` attribute, when written.
    pub message: Option<String>,
}

/// The class the offset is in — the innermost one — or, with no offset or none enclosing it, the
/// first class the file declares.
pub fn class_at(source: &str, offset: Option<usize>) -> Option<ClassModel> {
    let tree = parse_java(source)?;
    let root = tree.root_node();
    let decl = offset.and_then(|at| enclosing_type(root, at)).or_else(|| first_type(root))?;
    Some(read_class(root, decl, source))
}

/// The class with this simple name, at any depth.
pub fn class_named(source: &str, name: &str) -> Option<ClassModel> {
    let tree = parse_java(source)?;
    let root = tree.root_node();
    let decl = find_type(root, source, name)?;
    Some(read_class(root, decl, source))
}

/// `java.util.List<java.lang.String>` → `List`, `String[]` → `String[]`.
pub fn type_simple(type_name: &str) -> String {
    let array = type_name.trim_end().ends_with("[]");
    let base = type_name.split('<').next().unwrap_or(type_name).trim().trim_end_matches("[]").trim();
    let simple = base.rsplit('.').next().unwrap_or(base);
    match array {
        true => format!("{simple}[]"),
        false => simple.to_string(),
    }
}

fn is_class(node: &Node<'_>) -> bool {
    matches!(node.kind(), "class_declaration" | "record_declaration")
}

fn enclosing_type(root: Node<'_>, at: usize) -> Option<Node<'_>> {
    let mut node = root.descendant_for_byte_range(at, at)?;
    loop {
        if is_class(&node) {
            return Some(node);
        }
        node = node.parent()?;
    }
}

fn first_type(root: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = root.walk();
    let found = root.named_children(&mut cursor).find(is_class);
    found
}

fn find_type<'t>(node: Node<'t>, source: &str, name: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'t>> = node.named_children(&mut cursor).collect();
    for child in children {
        if is_class(&child) && name_of(child, source) == name {
            return Some(child);
        }
        if let Some(found) = find_type(child, source, name) {
            return Some(found);
        }
    }
    None
}

fn read_class(root: Node<'_>, decl: Node<'_>, source: &str) -> ClassModel {
    let name = name_of(decl, source);
    let mut chain = vec![name.clone()];
    let mut up = decl.parent();
    while let Some(node) = up {
        if matches!(
            node.kind(),
            "class_declaration" | "record_declaration" | "interface_declaration" | "enum_declaration"
        ) {
            chain.insert(0, name_of(node, source));
        }
        up = node.parent();
    }
    let package = package_of(root, source);
    let qualify = |separator: &str| match package.is_empty() {
        true => chain.join(separator),
        false => format!("{package}.{}", chain.join(separator)),
    };
    let fqn = qualify(".");
    let binary = qualify("$");
    let path_in_file = chain.join(".");
    let record = decl.kind() == "record_declaration";

    let class_lombok = Lombok::read(&modifiers_text(decl, source));
    let methods = methods_of(decl, source);
    let json = dtos_in(source)
        .into_iter()
        .find(|d| d.name == name && d.offset >= decl.start_byte() && d.offset < decl.end_byte());

    let mut fields: Vec<FieldModel> = declared_fields(decl, source)
        .into_iter()
        .map(|declared| {
            let lombok = class_lombok.merge(Lombok::read(&declared.modifiers));
            let property = json.as_ref().and_then(|d| {
                d.properties.iter().find(|p| p.member == declared.name && p.kind == MemberKind::Field)
            });
            let annotation_names: Vec<String> = declared.annotations.iter().map(|a| a.name.clone()).collect();
            let json_name = property.map(|p| p.name.clone()).unwrap_or_else(|| declared.name.clone());
            let (setter, setter_chains) = match setter_for(&declared.name, record, &methods, lombok) {
                Some((setter, chains)) => (Some(setter), chains),
                None => (None, false),
            };
            let getter = getter_for(&declared.name, &declared.type_name, record, &methods, lombok);
            let wither = wither_for(&declared.name, &methods, lombok);
            FieldModel {
                json_name,
                type_simple: type_simple(&declared.type_name),
                is_final: declared.is_final || record,
                id: annotation_names.iter().any(|a| a == "Id" || a == "EmbeddedId"),
                ignored: property.is_some_and(|p| p.ignored),
                setter,
                setter_chains,
                getter,
                wither,
                annotations: declared.annotations,
                annotation_names,
                constraints: Vec::new(),
                type_name: declared.type_name,
                name: declared.name,
            }
        })
        .collect();

    for used in constraints_in(root, source) {
        let inside = used.start >= decl.start_byte() && used.end <= decl.end_byte();
        if !inside || used.owner.as_deref() != Some(name.as_str()) {
            continue;
        }
        let Some(target) = used.target_name.as_deref() else { continue };
        let property = property_of_accessor(target);
        let Some(field) = fields
            .iter_mut()
            .find(|f| f.name == target || property.as_deref() == Some(f.name.as_str()))
        else {
            continue;
        };
        let attributes = annotation_at(root, used.start)
            .map(|node| attributes_of(node, source))
            .unwrap_or_default();
        field.constraints.push(ConstraintModel {
            fqn: constraint(&used.name)
                .map(|c| format!("{}.{}", c.package, c.name))
                .unwrap_or_else(|| used.name.clone()),
            message: attributes.get("message").cloned(),
            name: used.name,
            attributes,
        });
    }

    let annotations = annotations_on(decl, source);
    let annotation_names = annotations.iter().map(|a| a.name.clone()).collect();
    let superclass = decl
        .child_by_field_name("superclass")
        .map(|n| text(n, source).trim().trim_start_matches("extends").trim().to_string())
        .filter(|s| !s.is_empty());
    let interfaces = decl
        .child_by_field_name("interfaces")
        .map(|n| split_top_level(text(n, source).trim().trim_start_matches("implements")))
        .unwrap_or_default();
    let id_type = fields.iter().find(|f| f.id).map(|f| boxed(&f.type_name));

    ClassModel {
        name,
        package,
        fqn,
        binary,
        path_in_file,
        record,
        annotations,
        annotation_names,
        superclass,
        interfaces,
        imports: imports_of(root, source),
        id_type,
        fields,
    }
}

struct DeclaredField {
    name: String,
    type_name: String,
    modifiers: String,
    annotations: Vec<AnnotationModel>,
    is_final: bool,
}

/// Instance fields in declaration order — a record's components, or a class's non-static fields.
fn declared_fields(decl: Node<'_>, source: &str) -> Vec<DeclaredField> {
    let mut out = Vec::new();
    if decl.kind() == "record_declaration" {
        let Some(params) = decl.child_by_field_name("parameters") else { return out };
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if param.kind() != "formal_parameter" {
                continue;
            }
            let (Some(name), Some(ty)) = (param.child_by_field_name("name"), param.child_by_field_name("type"))
            else {
                continue;
            };
            out.push(DeclaredField {
                name: text(name, source).to_string(),
                type_name: text(ty, source).trim().to_string(),
                modifiers: modifiers_text(param, source),
                annotations: annotations_on(param, source),
                is_final: true,
            });
        }
        return out;
    }
    let Some(body) = decl.child_by_field_name("body") else { return out };
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "field_declaration" {
            continue;
        }
        let modifiers = modifiers_text(member, source);
        let words: Vec<&str> = modifiers.split_whitespace().collect();
        if words.contains(&"static") {
            continue;
        }
        let Some(ty) = member.child_by_field_name("type") else { continue };
        let type_name = text(ty, source).trim().to_string();
        let annotations = annotations_on(member, source);
        let is_final = words.contains(&"final");
        let mut inner = member.walk();
        for declarator in member.named_children(&mut inner) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            if let Some(name) = declarator.child_by_field_name("name") {
                out.push(DeclaredField {
                    name: text(name, source).to_string(),
                    type_name: type_name.clone(),
                    modifiers: modifiers.clone(),
                    annotations: annotations.clone(),
                    is_final,
                });
            }
        }
    }
    out
}

/// A method the class body declares, as far as telling an accessor needs.
struct Method {
    name: String,
    params: usize,
    /// The return type as written: `void`, `Order`, `Builder<T>`.
    returns: String,
}

/// Every method the class body declares.
fn methods_of(decl: Node<'_>, source: &str) -> Vec<Method> {
    let Some(body) = decl.child_by_field_name("body") else { return Vec::new() };
    let mut out = Vec::new();
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "method_declaration" {
            continue;
        }
        let Some(name) = member.child_by_field_name("name") else { continue };
        let params = member
            .child_by_field_name("parameters")
            .map(|p| {
                let mut c = p.walk();
                let count = p
                    .named_children(&mut c)
                    .filter(|n| matches!(n.kind(), "formal_parameter" | "spread_parameter"))
                    .count();
                count
            })
            .unwrap_or(0);
        let returns = member.child_by_field_name("type").map(|t| text(t, source).to_string()).unwrap_or_default();
        out.push(Method { name: text(name, source).to_string(), params, returns });
    }
    out
}

/// What Lombok generates, as far as the annotations written on a class or a field say.
#[derive(Debug, Clone, Copy, Default)]
struct Lombok {
    getters: bool,
    setters: bool,
    fluent: bool,
    /// Setters return the object.
    chain: bool,
    with: bool,
}

impl Lombok {
    fn read(modifiers: &str) -> Self {
        let has = |annotation: &str| annotated(modifiers, annotation);
        let accessors = arguments_of(modifiers, "Accessors").unwrap_or_default();
        let fluent = accessors.contains("fluent=true");
        Self {
            getters: has("Data") || has("Getter") || has("Value"),
            setters: has("Data") || has("Setter"),
            fluent,
            // A fluent accessor chains unless told not to, as Lombok has it.
            chain: accessors.contains("chain=true") || (fluent && !accessors.contains("chain=false")),
            with: has("With") || has("Wither"),
        }
    }

    fn merge(self, other: Self) -> Self {
        Self {
            getters: self.getters || other.getters,
            setters: self.setters || other.setters,
            fluent: self.fluent || other.fluent,
            chain: self.chain || other.chain,
            with: self.with || other.with,
        }
    }
}

/// `@Name` or `@some.package.Name` among `modifiers` — the whole name, so `@With` is not `@WithMockUser` and
/// `@Data` is not `@DataJpaTest`.
fn annotated(modifiers: &str, name: &str) -> bool {
    annotation_end(modifiers, name).is_some()
}

/// Where the text right after the first `@Name` / `.Name` starts.
fn annotation_end(modifiers: &str, name: &str) -> Option<usize> {
    ['@', '.'].iter().find_map(|lead| {
        let needle = format!("{lead}{name}");
        modifiers
            .match_indices(&needle)
            .map(|(at, _)| at + needle.len())
            .find(|end| !modifiers[*end..].starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

/// Annotation `name`'s arguments without whitespace: `@Accessors(chain = true)` → `chain=true`.
fn arguments_of(modifiers: &str, name: &str) -> Option<String> {
    let rest = modifiers[annotation_end(modifiers, name)?..].trim_start();
    let inner = rest.strip_prefix('(')?;
    let close = inner.find(')')?;
    Some(inner[..close].chars().filter(|c| !c.is_whitespace()).collect())
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The method that sets `name`, and whether it returns the object so calls chain.
fn setter_for(name: &str, record: bool, methods: &[Method], lombok: Lombok) -> Option<(String, bool)> {
    if record {
        return None;
    }
    let conventional = format!("set{}", capitalized(name));
    for candidate in [conventional.as_str(), name] {
        if let Some(method) = methods.iter().find(|m| m.name == candidate && m.params == 1) {
            return Some((method.name.clone(), method.returns != "void"));
        }
    }
    lombok.setters.then(|| (if lombok.fluent { name.to_string() } else { conventional }, lombok.chain))
}

/// The method that returns a copy with `name` changed: a declared `withName`, or Lombok's `@With`.
fn wither_for(name: &str, methods: &[Method], lombok: Lombok) -> Option<String> {
    let wither = format!("with{}", capitalized(name));
    (lombok.with || methods.iter().any(|m| m.name == wither && m.params == 1)).then_some(wither)
}

fn getter_for(name: &str, type_name: &str, record: bool, methods: &[Method], lombok: Lombok) -> Option<String> {
    if record {
        return Some(name.to_string());
    }
    let get = format!("get{}", capitalized(name));
    let is = format!("is{}", capitalized(name));
    if let Some(found) = methods.iter().find(|m| (m.name == get || m.name == is) && m.params == 0) {
        return Some(found.name.clone());
    }
    lombok.getters.then(|| match (lombok.fluent, type_name.trim()) {
        (true, _) => name.to_string(),
        (false, "boolean") => is,
        (false, _) => get,
    })
}

/// `getCustomerName` → `customerName`, `isActive` → `active`.
fn property_of_accessor(method: &str) -> Option<String> {
    let rest = method.strip_prefix("get").or_else(|| method.strip_prefix("is"))?;
    let mut chars = rest.chars();
    let first = chars.next().filter(|c| c.is_uppercase())?;
    Some(first.to_lowercase().chain(chars).collect())
}

fn boxed(type_name: &str) -> String {
    match type_name.trim() {
        "long" => "Long",
        "int" => "Integer",
        "short" => "Short",
        "byte" => "Byte",
        "char" => "Character",
        "boolean" => "Boolean",
        "double" => "Double",
        "float" => "Float",
        other => other,
    }
    .to_string()
}

/// `A, B<C, D>` → `["A", "B<C, D>"]`.
fn split_top_level(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for c in list.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                }
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

fn annotation_at(root: Node<'_>, start: usize) -> Option<Node<'_>> {
    let mut node = root.descendant_for_byte_range(start, start + 1)?;
    loop {
        if matches!(node.kind(), "annotation" | "marker_annotation") && node.start_byte() == start {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// The annotations written in a declaration's modifiers.
fn annotations_on(node: Node<'_>, source: &str) -> Vec<AnnotationModel> {
    let mut cursor = node.walk();
    let Some(modifiers) = node.named_children(&mut cursor).find(|c| c.kind() == "modifiers") else {
        return Vec::new();
    };
    let mut inner = modifiers.walk();
    let found = modifiers
        .named_children(&mut inner)
        .filter(|c| matches!(c.kind(), "annotation" | "marker_annotation"))
        .map(|a| AnnotationModel {
            name: a
                .child_by_field_name("name")
                .map(|n| text(n, source).rsplit('.').next().unwrap_or_default().to_string())
                .unwrap_or_default(),
            attributes: attributes_of(a, source),
        })
        .collect();
    found
}

/// The attributes an annotation writes. A single unnamed value is `value`, which is what Java makes
/// of `@Min(1)`.
fn attributes_of(node: Node<'_>, source: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(args) = node.child_by_field_name("arguments") else { return out };
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        match child.kind() {
            "line_comment" | "block_comment" => {}
            "element_value_pair" => {
                let (Some(key), Some(value)) = (child.child_by_field_name("key"), child.child_by_field_name("value"))
                else {
                    continue;
                };
                out.insert(text(key, source).to_string(), value_text(value, source));
            }
            _ => {
                out.insert("value".to_string(), value_text(child, source));
            }
        }
    }
    out
}

fn value_text(node: Node<'_>, source: &str) -> String {
    let raw = text(node, source).trim();
    if node.kind() != "string_literal" || raw.len() < 2 || !raw.starts_with('"') {
        return raw.to_string();
    }
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn modifiers_text(node: Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|c| c.kind() == "modifiers")
        .map(|m| text(m, source).to_string())
        .unwrap_or_default();
    found
}

fn imports_of(root: Node<'_>, source: &str) -> Vec<String> {
    let mut cursor = root.walk();
    let found = root
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "import_declaration")
        .map(|n| text(n, source).trim().trim_start_matches("import").trim().trim_end_matches(';').trim().to_string())
        .collect();
    found
}

fn package_of(root: Node<'_>, source: &str) -> String {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "package_declaration" {
            continue;
        }
        let mut inner = child.walk();
        let name = child
            .named_children(&mut inner)
            .find(|n| matches!(n.kind(), "scoped_identifier" | "identifier"))
            .map(|n| text(n, source).to_string());
        if let Some(name) = name {
            return name;
        }
    }
    String::new()
}

fn name_of(node: Node<'_>, source: &str) -> String {
    node.child_by_field_name("name").map(|n| text(n, source).to_string()).unwrap_or_default()
}

fn text<'s>(node: Node<'_>, source: &'s str) -> &'s str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: &str = r#"package com.example.orders;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.validation.constraints.*;
import com.fasterxml.jackson.annotation.JsonProperty;

@Entity
public class Order extends BaseEntity implements Serializable, Comparable<Order> {
    @Id
    private long id;

    @NotBlank
    @Size(max = 40, message = "{order.customer.length}")
    @JsonProperty("customer")
    private String customerName;

    @Min(1)
    private final int quantity = 1;

    @Pattern(regexp = "^[A-Z]{3}$")
    private String currency;

    private static final int LIMIT = 3;
    private String note;

    public void setCustomerName(String value) { this.customerName = value; }
    public String getCurrency() { return currency; }

    public static class Line {
        @Positive private long amount;
    }
}
"#;

    #[test]
    fn a_class_is_read_with_its_annotations_supertypes_and_imports() {
        let order = class_at(ORDER, None).unwrap();
        assert_eq!(order.fqn, "com.example.orders.Order");
        assert_eq!(order.path_in_file, "Order");
        assert_eq!(order.annotation_names, ["Entity"]);
        assert_eq!(order.superclass.as_deref(), Some("BaseEntity"));
        assert_eq!(order.interfaces, ["Serializable", "Comparable<Order>"]);
        assert!(order.imports.contains(&"jakarta.persistence.Entity".to_string()));
        assert_eq!(order.id_type.as_deref(), Some("Long"), "the id is boxed");
    }

    #[test]
    fn a_class_is_read_with_its_fields_accessors_and_constraints() {
        let order = class_at(ORDER, None).unwrap();
        let names: Vec<&str> = order.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["id", "customerName", "quantity", "currency", "note"], "the static field is not one");

        assert!(order.fields[0].id);
        let customer = &order.fields[1];
        assert_eq!(customer.json_name, "customer");
        assert_eq!(customer.setter.as_deref(), Some("setCustomerName"));
        assert_eq!(customer.annotation_names, ["NotBlank", "Size", "JsonProperty"]);
        let constraints: Vec<&str> = customer.constraints.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(constraints, ["NotBlank", "Size"]);
        assert_eq!(customer.constraints[1].attributes.get("max").map(String::as_str), Some("40"));
        assert_eq!(customer.constraints[1].message.as_deref(), Some("{order.customer.length}"));

        assert!(order.fields[2].is_final);
        assert_eq!(order.fields[2].constraints[0].attributes.get("value").map(String::as_str), Some("1"));
        assert_eq!(order.fields[3].constraints[0].attributes.get("regexp").map(String::as_str), Some("^[A-Z]{3}$"));
        assert_eq!(order.fields[3].getter.as_deref(), Some("getCurrency"));
        assert_eq!(order.fields[3].setter, None);
    }

    #[test]
    fn a_nested_class_is_found_from_an_offset_inside_it() {
        let at = ORDER.find("@Positive").unwrap();
        let line = class_at(ORDER, Some(at)).unwrap();
        assert_eq!(line.fqn, "com.example.orders.Order.Line");
        assert_eq!(line.binary, "com.example.orders.Order$Line");
        assert_eq!(line.path_in_file, "Order.Line");
        assert_eq!(line.fields[0].constraints[0].name, "Positive");
    }

    #[test]
    fn a_record_has_its_components_as_final_fields_and_no_setters() {
        let src = "package p;\npublic record Point(@Min(0) int x, int y) {}\n";
        let point = class_at(src, None).unwrap();
        assert!(point.record);
        assert_eq!(point.fields.len(), 2);
        assert!(point.fields[0].is_final);
        assert_eq!(point.fields[0].setter, None);
        assert_eq!(point.fields[0].getter.as_deref(), Some("x"));
        assert_eq!(point.fields[0].constraints[0].name, "Min");
    }

    #[test]
    fn lombok_accessors_are_assumed_where_lombok_writes_them() {
        let src = "package p;\n@lombok.Data\npublic class A { @NotNull private Boolean active; private boolean done; }\n";
        let a = class_at(src, None).unwrap();
        assert_eq!(a.fields[0].setter.as_deref(), Some("setActive"));
        assert_eq!(a.fields[1].getter.as_deref(), Some("isDone"));
    }

    #[test]
    fn a_setter_that_returns_the_object_chains_and_a_wither_is_found() {
        let src = "package p;\npublic class A {\n    private String name;\n    private int size;\n    private String note;\n    \
                   public A setName(String name) { this.name = name; return this; }\n    \
                   public void setSize(int size) { this.size = size; }\n    \
                   public A withNote(String note) { return this; }\n}\n";
        let a = class_at(src, None).unwrap();
        assert!(a.fields[0].setter_chains);
        assert!(!a.fields[1].setter_chains);
        assert_eq!(a.fields[2].setter, None);
        assert_eq!(a.fields[2].wither.as_deref(), Some("withNote"));
        assert_eq!(a.fields[0].wither, None);
    }

    #[test]
    fn lombok_chained_setters_and_withers_are_assumed_and_a_longer_annotation_is_not_one() {
        let src = "package p;\n@lombok.Setter\n@lombok.experimental.Accessors(chain = true)\n@lombok.With\npublic class B { private String name; }\n";
        let b = class_at(src, None).unwrap();
        assert_eq!(b.fields[0].setter.as_deref(), Some("setName"));
        assert!(b.fields[0].setter_chains);
        assert_eq!(b.fields[0].wither.as_deref(), Some("withName"));

        let src = "package p;\n@WithMockUser\n@DataJpaTest\npublic class C { private String name; }\n";
        let c = class_at(src, None).unwrap();
        assert_eq!(c.fields[0].wither, None);
        assert_eq!(c.fields[0].setter, None);
    }

    #[test]
    fn a_type_is_reduced_to_its_simple_name() {
        assert_eq!(type_simple("java.util.List<java.lang.String>"), "List");
        assert_eq!(type_simple("String[]"), "String[]");
    }
}
