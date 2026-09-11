//! A class read as the fields the DTO Lab works with — each one's type, JSON name, accessors and
//! constraints.
//!
//! Read from **source**, which is instant and needs nothing compiled. It is the static half of the
//! lab: the JVM answers the same question from the project's own validator — including constraints
//! this reader cannot recognise, such as a custom `@Constraint` — and wins wherever both have an
//! answer. See [`LabClass::apply_described`].

use std::collections::BTreeMap;

use bennu_jackson::prelude::{dtos_in, MemberKind};
use bennu_jakarta::prelude::{constraint, constraints_in};
use bennu_java::prelude::parse_java;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::protocol::Described;

/// A class, as the lab needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabClass {
    pub name: String,
    /// Empty for the default package.
    pub package: String,
    /// `com.example.Order.Line` — how Java source names it.
    pub fqn: String,
    /// `com.example.Order$Line` — how a class loader is asked for it.
    pub binary: String,
    pub record: bool,
    pub fields: Vec<LabField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabField {
    pub name: String,
    /// What Jackson calls it: `@JsonProperty`'s value, else the field's own name.
    pub json_name: String,
    /// As written (`List<String>`, `java.time.LocalDate`).
    pub type_name: String,
    /// Carries `@JsonIgnore`, so no payload can set it.
    pub ignored: bool,
    /// The method that sets it: `setName`, a fluent `name`, or one Lombok generates.
    pub setter: Option<String>,
    pub getter: Option<String>,
    pub constraints: Vec<LabConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabConstraint {
    /// Simple name (`Size`).
    pub name: String,
    /// Fully qualified when known (`jakarta.validation.constraints.Size`), else the simple name.
    pub fqn: String,
    /// Every attribute written, as source text; a string literal without its quotes.
    pub attributes: BTreeMap<String, String>,
    /// The `message` attribute, when written.
    pub message: Option<String>,
}

impl LabClass {
    /// Replace what the source said about constraints with what the validator says.
    ///
    /// The validator is the authority on both halves: a property it lists is constrained exactly as
    /// it lists it, and a field it does not list is not constrained at all — an annotation imported
    /// from the wrong package reads as a constraint in the source and is nothing at run time.
    pub fn apply_described(&mut self, described: &Described) {
        for property in &described.properties {
            let constraints: Vec<LabConstraint> = property
                .constraints
                .iter()
                .map(|c| LabConstraint {
                    name: simple_name(&c.annotation).to_string(),
                    fqn: c.annotation.clone(),
                    attributes: c.attributes.clone(),
                    message: Some(c.template.clone()).filter(|t| !t.is_empty()),
                })
                .collect();
            match self.fields.iter_mut().find(|f| f.name == property.name) {
                Some(field) => field.constraints = constraints,
                // A property the source reader did not see as a field — one declared only through
                // a constrained getter.
                None => self.fields.push(LabField {
                    name: property.name.clone(),
                    json_name: property.name.clone(),
                    type_name: property.type_name.clone(),
                    ignored: false,
                    setter: None,
                    getter: None,
                    constraints,
                }),
            }
        }
        for field in &mut self.fields {
            if !described.properties.iter().any(|p| p.name == field.name) {
                field.constraints.clear();
            }
        }
    }
}

/// The class the offset is in — the innermost one — or, with no offset or none enclosing it, the
/// first class the file declares.
pub fn class_at(source: &str, offset: Option<usize>) -> Option<LabClass> {
    let tree = parse_java(source)?;
    let root = tree.root_node();
    let decl = offset.and_then(|at| enclosing_type(root, at)).or_else(|| first_type(root))?;
    Some(read_class(root, decl, source))
}

/// The class with this simple name, at any depth.
pub fn class_named(source: &str, name: &str) -> Option<LabClass> {
    let tree = parse_java(source)?;
    let root = tree.root_node();
    let decl = find_type(root, source, name)?;
    Some(read_class(root, decl, source))
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

fn read_class(root: Node<'_>, decl: Node<'_>, source: &str) -> LabClass {
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
    let record = decl.kind() == "record_declaration";

    let class_lombok = Lombok::read(&modifiers_text(decl, source));
    let methods = methods_of(decl, source);
    let json = dtos_in(source)
        .into_iter()
        .find(|d| d.name == name && d.offset >= decl.start_byte() && d.offset < decl.end_byte());

    let mut fields: Vec<LabField> = declared_fields(decl, source)
        .into_iter()
        .map(|declared| {
            let lombok = class_lombok.merge(Lombok::read(&declared.modifiers));
            let property = json.as_ref().and_then(|d| {
                d.properties.iter().find(|p| p.member == declared.name && p.kind == MemberKind::Field)
            });
            LabField {
                json_name: property.map(|p| p.name.clone()).unwrap_or_else(|| declared.name.clone()),
                ignored: property.is_some_and(|p| p.ignored),
                setter: setter_for(&declared.name, record, &methods, lombok),
                getter: getter_for(&declared.name, &declared.type_name, record, &methods, lombok),
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
        field.constraints.push(LabConstraint {
            fqn: constraint(&used.name)
                .map(|c| format!("{}.{}", c.package, c.name))
                .unwrap_or_else(|| used.name.clone()),
            message: attributes.get("message").cloned(),
            name: used.name,
            attributes,
        });
    }

    LabClass { name, package, fqn, binary, record, fields }
}

struct DeclaredField {
    name: String,
    type_name: String,
    modifiers: String,
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
            let (Some(name), Some(ty)) =
                (param.child_by_field_name("name"), param.child_by_field_name("type"))
            else {
                continue;
            };
            out.push(DeclaredField {
                name: text(name, source).to_string(),
                type_name: text(ty, source).trim().to_string(),
                modifiers: modifiers_text(param, source),
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
        if modifiers.split_whitespace().any(|w| w == "static") {
            continue;
        }
        let Some(ty) = member.child_by_field_name("type") else { continue };
        let type_name = text(ty, source).trim().to_string();
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
                });
            }
        }
    }
    out
}

/// Every method the class body declares, as `(name, parameter count)`.
fn methods_of(decl: Node<'_>, source: &str) -> Vec<(String, usize)> {
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
        out.push((text(name, source).to_string(), params));
    }
    out
}

/// What Lombok generates, as far as the annotations written on a class or a field say.
#[derive(Debug, Clone, Copy, Default)]
struct Lombok {
    getters: bool,
    setters: bool,
    fluent: bool,
}

impl Lombok {
    fn read(modifiers: &str) -> Self {
        // `@Data` and `@lombok.Data` both.
        let has = |annotation: &str| {
            modifiers.contains(&format!("@{annotation}")) || modifiers.contains(&format!(".{annotation}"))
        };
        Self {
            getters: has("Data") || has("Getter") || has("Value"),
            setters: has("Data") || has("Setter"),
            fluent: has("Accessors") && modifiers.contains("fluent"),
        }
    }

    fn merge(self, other: Self) -> Self {
        Self {
            getters: self.getters || other.getters,
            setters: self.setters || other.setters,
            fluent: self.fluent || other.fluent,
        }
    }
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn setter_for(name: &str, record: bool, methods: &[(String, usize)], lombok: Lombok) -> Option<String> {
    if record {
        return None;
    }
    let conventional = format!("set{}", capitalized(name));
    if methods.iter().any(|(m, n)| *m == conventional && *n == 1) {
        return Some(conventional);
    }
    if methods.iter().any(|(m, n)| m == name && *n == 1) {
        return Some(name.to_string());
    }
    lombok.setters.then(|| if lombok.fluent { name.to_string() } else { conventional })
}

fn getter_for(
    name: &str,
    type_name: &str,
    record: bool,
    methods: &[(String, usize)],
    lombok: Lombok,
) -> Option<String> {
    if record {
        return Some(name.to_string());
    }
    let get = format!("get{}", capitalized(name));
    let is = format!("is{}", capitalized(name));
    if let Some((found, _)) = methods.iter().find(|(m, n)| (*m == get || *m == is) && *n == 0) {
        return Some(found.clone());
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

fn annotation_at(root: Node<'_>, start: usize) -> Option<Node<'_>> {
    let mut node = root.descendant_for_byte_range(start, start + 1)?;
    loop {
        if matches!(node.kind(), "annotation" | "marker_annotation") && node.start_byte() == start {
            return Some(node);
        }
        node = node.parent()?;
    }
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
                let (Some(key), Some(value)) =
                    (child.child_by_field_name("key"), child.child_by_field_name("value"))
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

fn simple_name(qualified: &str) -> &str {
    qualified.rsplit(['.', '$']).next().unwrap_or(qualified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DescribedConstraint, DescribedProperty};

    const ORDER: &str = r#"package com.example.orders;

import jakarta.validation.constraints.*;
import com.fasterxml.jackson.annotation.JsonProperty;

public class Order {
    @NotBlank
    @Size(max = 40, message = "{order.customer.length}")
    @JsonProperty("customer")
    private String customerName;

    @Min(1)
    private int quantity;

    @Pattern(regexp = "^[A-Z]{3}$")
    private String currency;

    private static final int LIMIT = 3;
    private String note;

    public void setCustomerName(String value) { this.customerName = value; }
    public void setQuantity(int value) { this.quantity = value; }
    public String getCurrency() { return currency; }

    public static class Line {
        @Positive private long amount;
    }
}
"#;

    #[test]
    fn a_class_is_read_with_its_fields_accessors_and_constraints() {
        let order = class_at(ORDER, None).unwrap();
        assert_eq!(order.fqn, "com.example.orders.Order");
        assert_eq!(order.binary, "com.example.orders.Order");
        let names: Vec<&str> = order.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["customerName", "quantity", "currency", "note"], "the static field is not one");

        let customer = &order.fields[0];
        assert_eq!(customer.json_name, "customer");
        assert_eq!(customer.setter.as_deref(), Some("setCustomerName"));
        let constraints: Vec<&str> = customer.constraints.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(constraints, ["NotBlank", "Size"]);
        let size = &customer.constraints[1];
        assert_eq!(size.attributes.get("max").map(String::as_str), Some("40"));
        assert_eq!(size.message.as_deref(), Some("{order.customer.length}"));

        assert_eq!(order.fields[1].constraints[0].attributes.get("value").map(String::as_str), Some("1"));
        assert_eq!(order.fields[2].constraints[0].attributes.get("regexp").map(String::as_str), Some("^[A-Z]{3}$"));
        assert_eq!(order.fields[2].getter.as_deref(), Some("getCurrency"));
        assert_eq!(order.fields[2].setter, None);
    }

    /// A nested class is found from inside it, and named the way each side of Java needs.
    #[test]
    fn a_nested_class_is_found_from_an_offset_inside_it() {
        let at = ORDER.find("@Positive").unwrap();
        let line = class_at(ORDER, Some(at)).unwrap();
        assert_eq!(line.fqn, "com.example.orders.Order.Line");
        assert_eq!(line.binary, "com.example.orders.Order$Line");
        assert_eq!(line.fields[0].constraints[0].name, "Positive");
    }

    #[test]
    fn a_record_has_its_components_as_fields_and_no_setters() {
        let src = "package p;\npublic record Point(@Min(0) int x, int y) {}\n";
        let point = class_at(src, None).unwrap();
        assert!(point.record);
        assert_eq!(point.fields.len(), 2);
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

    /// The validator is the authority: what it lists wins, and what it does not list is not
    /// constrained — however the source reads.
    #[test]
    fn what_the_validator_describes_replaces_what_the_source_says() {
        let mut order = class_at(ORDER, None).unwrap();
        order.apply_described(&Described {
            properties: vec![DescribedProperty {
                name: "note".into(),
                type_name: "String".into(),
                constraints: vec![DescribedConstraint {
                    annotation: "com.example.validation.Reference".into(),
                    attributes: BTreeMap::new(),
                    template: "{reference.invalid}".into(),
                }],
            }],
        });
        let note = order.fields.iter().find(|f| f.name == "note").unwrap();
        assert_eq!(note.constraints[0].name, "Reference");
        assert!(order.fields.iter().filter(|f| f.name != "note").all(|f| f.constraints.is_empty()));
    }
}
