//! The JSON a class reads, sketched from source — the instant answer, before the JVM has one.
//!
//! Deliberately a sketch: an empty value of the right JSON type per property, nothing nested. The
//! real shape — what the project's `ObjectMapper` writes for a new instance, with its modules and
//! naming strategy — comes from the JVM, and the lab replaces this with it as soon as it arrives.

use serde_json::{json, Map, Value};

use crate::cases::{shape, type_simple, Shape};
use crate::model::LabClass;

pub fn skeleton(class: &LabClass) -> Value {
    let mut out = Map::new();
    for field in class.fields.iter().filter(|f| !f.ignored) {
        out.insert(field.json_name.clone(), placeholder(&field.type_name));
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
    use crate::model::class_at;

    #[test]
    fn a_skeleton_has_one_empty_value_per_property_under_its_json_name() {
        let src = "package p;\npublic class A {\n  @com.fasterxml.jackson.annotation.JsonProperty(\"full_name\") private String name;\n  private int count;\n  private java.util.List<String> tags;\n  @com.fasterxml.jackson.annotation.JsonIgnore private String secret;\n}\n";
        let class = class_at(src, None).unwrap();
        assert_eq!(skeleton(&class), json!({ "full_name": "", "count": 0, "tags": [] }));
    }
}
