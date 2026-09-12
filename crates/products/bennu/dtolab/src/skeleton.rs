//! The JSON a class reads, sketched from source — the instant answer, before the JVM has one.
//!
//! Deliberately a sketch: per property, the value a value rule gives its field ([`crate::values`]) or
//! else an empty value of the right JSON type — nothing nested. The
//! real shape — what the project's `ObjectMapper` writes for a new instance, with its modules and
//! naming strategy — comes from the JVM, and the lab replaces this with it as soon as it arrives.

use serde_json::{json, Map, Value};

use bennu_templates::prelude::{type_simple, ClassModel};

use crate::cases::{shape, Shape};
use crate::values::{named_valid, ValueRule};

pub fn skeleton(class: &ClassModel, rules: &[ValueRule]) -> Value {
    let mut out = Map::new();
    for field in class.fields.iter().filter(|f| !f.ignored) {
        let value = named_valid(field, rules)
            .and_then(|named| named.to_json(&type_simple(&field.type_name)))
            .unwrap_or_else(|| placeholder(&field.type_name));
        out.insert(field.json_name.clone(), value);
    }
    Value::Object(out)
}

fn placeholder(type_name: &str) -> Value {
    match shape(type_name) {
        Shape::Text => json!(""),
        Shape::Integer => json!(0),
        Shape::Decimal => json!(0.0),
        Shape::Bool => json!(false),
        Shape::Collection if type_simple(type_name).ends_with("Map") => json!({}),
        Shape::Collection => json!([]),
        Shape::Date | Shape::Other => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_templates::prelude::class_at;

    #[test]
    fn a_skeleton_has_one_empty_value_per_property_under_its_json_name() {
        let src = "package p;\npublic class A {\n  @com.fasterxml.jackson.annotation.JsonProperty(\"full_name\") private String name;\n  private int count;\n  private java.util.List<String> tags;\n  @com.fasterxml.jackson.annotation.JsonIgnore private String secret;\n}\n";
        let class = class_at(src, None).unwrap();
        assert_eq!(skeleton(&class, &[]), json!({ "full_name": "", "count": 0, "tags": [] }));
    }

    #[test]
    fn a_property_a_value_rule_answers_starts_from_that_value() {
        let src = "package p;\npublic class A {\n  private String email;\n  private int age;\n  private String note;\n}\n";
        let class = class_at(src, None).unwrap();
        let rules = crate::values::builtin_rules();
        assert_eq!(skeleton(&class, &rules), json!({ "email": "mario.rossi@example.com", "age": 30, "note": "" }));
    }
}
