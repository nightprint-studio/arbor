//! `equals`, `hashCode` and `toString` for the class at the caret — over every field, over the id only
//! (the right default for a JPA entity), or over a chosen set.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::{Imports, Lines};
use crate::context::{ClassInfo, FieldInfo, GenContext};
use crate::names::java_string;
use crate::output::{parse_options, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct ValueMembers;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ValueOptions {
    pub equals: bool,
    pub hash_code: bool,
    pub to_string: bool,
    /// Fields used; empty means every field (or the id fields of an entity).
    pub fields: Vec<String>,
    /// `auto` | `instanceof` | `get-class`
    pub class_check: String,
    /// Read through the getters instead of the fields.
    pub use_getters: bool,
}

impl Default for ValueOptions {
    fn default() -> Self {
        ValueOptions { equals: true, hash_code: true, to_string: true, fields: Vec::new(), class_check: "auto".into(), use_getters: false }
    }
}

impl Generator for ValueMembers {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "value-members",
            title: "Generate equals, hashCode and toString",
            description: "equals/hashCode/toString for the class at the caret, over all fields, the id (entities) or a chosen set.",
            icon: "Hash",
            group: "Class",
            language: "java",
            targets: &[Target::Class],
            related: &[],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut o = ValueOptions::default();
        if let Some(c) = &ctx.class {
            o.fields = default_fields(c).iter().map(|f| f.name.clone()).collect();
        }
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: ValueOptions = parse_options(options)?;
        let class = ctx.class.as_ref().ok_or("Place the caret in a class")?;
        if class.kind == "interface" || class.kind == "enum" {
            return Err(format!("Not meaningful for an {}", class.kind));
        }
        if !(o.equals || o.hash_code || o.to_string) {
            return Err("Choose at least one method".into());
        }
        let chosen: Vec<&FieldInfo> = class.fields.iter().filter(|f| o.fields.contains(&f.name)).collect();
        let equality_fields = if chosen.is_empty() { default_fields(class) } else { chosen.clone() };
        let all: Vec<&FieldInfo> = if chosen.is_empty() { class.fields.iter().collect() } else { chosen };
        if (o.equals || o.hash_code) && equality_fields.is_empty() {
            return Err("The class has no fields to compare".into());
        }
        let mut imports = Imports::default();
        let mut l = Lines::new();
        let pattern = match o.class_check.as_str() {
            "instanceof" => ctx.project.java_level() >= 16,
            "get-class" => false,
            _ => ctx.project.java_level() >= 16 && !class.has_annotation("Entity"),
        };
        let mut wrote = false;
        let mut sep = |l: &mut Lines| {
            if wrote {
                l.blank();
            }
            wrote = true;
        };
        if o.equals {
            sep(&mut l);
            write_equals(&mut l, &mut imports, class, &equality_fields, &o, pattern);
        }
        if o.hash_code {
            sep(&mut l);
            write_hash_code(&mut l, &mut imports, &equality_fields, &o);
        }
        if o.to_string {
            sep(&mut l);
            write_to_string(&mut l, class, &all, &o);
        }
        let mut out = Output::members(l.finish().trim_end().to_string(), imports.resolved(""));
        for method in [("equals", o.equals), ("hashCode", o.hash_code), ("toString", o.to_string)] {
            if method.1 && class.has_method(method.0) {
                out = out.note(format!("`{}` already exists in {} — the new one would be a duplicate.", method.0, class.name));
            }
        }
        if class.has_annotation("Entity") && equality_fields.iter().any(|f| !f.id) {
            out = out.note("On a JPA entity prefer comparing the id only: mutable and lazy fields break `equals` across states and proxies.");
        }
        if class.has_annotation("Data") || class.has_annotation("EqualsAndHashCode") {
            out = out.note("Lombok already generates these from the annotation on the class.");
        }
        Ok(out)
    }
}

/// An entity compares by id; any other class by all its fields.
fn default_fields(class: &ClassInfo) -> Vec<&FieldInfo> {
    let ids: Vec<&FieldInfo> = class.fields.iter().filter(|f| f.id).collect();
    if class.has_annotation("Entity") && !ids.is_empty() { ids } else { class.fields.iter().collect() }
}

fn read(class: &ClassInfo, f: &FieldInfo, o: &ValueOptions, owner: &str) -> String {
    if class.is_record() {
        return format!("{owner}{}()", f.name);
    }
    if o.use_getters {
        let prefix = if f.type_name == "boolean" { "is" } else { "get" };
        let name = f.getter.clone().unwrap_or_else(|| format!("{prefix}{}", crate::names::pascal(&f.name)));
        return format!("{owner}{name}()");
    }
    format!("{owner}{}", f.name)
}

fn compare(class: &ClassInfo, f: &FieldInfo, o: &ValueOptions, im: &mut Imports) -> String {
    let (mine, theirs) = (read(class, f, o, "this."), read(class, f, o, "other."));
    let mine = if class.is_record() || o.use_getters { mine } else { f.name.clone() };
    let ty = f.type_name.trim();
    if ty.ends_with("[]") {
        im.add("java.util.Arrays");
        return format!("Arrays.equals({mine}, {theirs})");
    }
    match ty {
        "double" => format!("Double.compare({mine}, {theirs}) == 0"),
        "float" => format!("Float.compare({mine}, {theirs}) == 0"),
        "int" | "long" | "short" | "byte" | "char" | "boolean" => format!("{mine} == {theirs}"),
        _ => {
            im.add("java.util.Objects");
            format!("Objects.equals({mine}, {theirs})")
        }
    }
}

fn write_equals(l: &mut Lines, im: &mut Imports, class: &ClassInfo, fields: &[&FieldInfo], o: &ValueOptions, pattern: bool) {
    let name = &class.name;
    l.line("@Override");
    l.block("public boolean equals(Object o)", |l| {
        l.block("if (this == o)", |l| {
            l.line("return true;");
        });
        if pattern {
            l.block(format!("if (!(o instanceof {name} other))"), |l| {
                l.line("return false;");
            });
        } else {
            l.block(format!("if (o == null || getClass() != o.getClass())"), |l| {
                l.line("return false;");
            });
            l.line(format!("{name} other = ({name}) o;"));
        }
        let terms: Vec<String> = fields.iter().map(|f| compare(class, f, o, im)).collect();
        l.line(format!("return {};", terms.join(" && ")));
    });
}

fn write_hash_code(l: &mut Lines, im: &mut Imports, fields: &[&FieldInfo], o: &ValueOptions) {
    let class_stub = ClassInfo::default();
    let arrays = fields.iter().any(|f| f.type_name.ends_with("[]"));
    l.line("@Override");
    l.block("public int hashCode()", |l| {
        if arrays {
            im.add("java.util.Arrays");
            im.add("java.util.Objects");
            let plain: Vec<String> = fields.iter().filter(|f| !f.type_name.ends_with("[]")).map(|f| read(&class_stub, f, o, "")).collect();
            let seed = if plain.is_empty() { "1".to_string() } else { format!("Objects.hash({})", plain.join(", ")) };
            l.line(format!("int result = {seed};"));
            for f in fields.iter().filter(|f| f.type_name.ends_with("[]")) {
                l.line(format!("result = 31 * result + Arrays.hashCode({});", read(&class_stub, f, o, "")));
            }
            l.line("return result;");
        } else {
            im.add("java.util.Objects");
            let args: Vec<String> = fields.iter().map(|f| read(&class_stub, f, o, "")).collect();
            l.line(format!("return Objects.hash({});", args.join(", ")));
        }
    });
}

fn write_to_string(l: &mut Lines, class: &ClassInfo, fields: &[&FieldInfo], o: &ValueOptions) {
    l.line("@Override");
    l.block("public String toString()", |l| {
        let mut parts: Vec<String> = Vec::new();
        for (i, f) in fields.iter().enumerate() {
            let label = format!("{}{}=", if i == 0 { "" } else { ", " }, f.name);
            let value = read(class, f, o, "");
            let value = if f.type_name.ends_with("[]") { format!("java.util.Arrays.toString({value})") } else { value };
            let quoted = f.type_name == "String" || f.type_name == "char";
            parts.push(java_string(&if quoted { format!("{label}'") } else { label }));
            parts.push(value);
            if quoted {
                parts.push("\"'\"".to_string());
            }
        }
        let head = java_string(&format!("{}{{", class.name));
        l.line(format!("return {head}"));
        l.indent().indent();
        for part in &parts {
            l.line(format!("+ {part}"));
        }
        l.line("+ \"}\";");
        l.dedent().dedent();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn text(ctx: &GenContext, options: serde_json::Value) -> (String, Output) {
        let out = ValueMembers.generate(&options, ctx).expect("generates");
        (out.members.as_ref().unwrap().text.clone(), out)
    }

    fn plain(fields: Vec<FieldInfo>) -> GenContext {
        let mut ctx = sample_context();
        ctx.class = Some(ClassInfo::new("Point", "p", fields));
        ctx
    }

    #[test]
    fn an_entity_compares_by_id_with_a_class_check_that_survives_proxies() {
        let (text, out) = text(&sample_context(), serde_json::json!({ "fields": ["id"] }));
        assert!(text.contains("if (o == null || getClass() != o.getClass()) {"), "{text}");
        assert!(text.contains("return Objects.equals(id, other.id);"), "{text}");
        assert!(text.contains("return Objects.hash(id);"), "{text}");
        assert!(out.members.unwrap().imports.contains(&"java.util.Objects".to_string()));
    }

    #[test]
    fn defaults_for_an_entity_select_only_the_id() {
        let d = ValueMembers.defaults(&sample_context());
        assert_eq!(d["fields"], serde_json::json!(["id"]));
    }

    #[test]
    fn a_plain_class_on_java_17_uses_pattern_matching_and_primitives_compare_directly() {
        let ctx = plain(vec![FieldInfo::new("x", "int"), FieldInfo::new("w", "double"), FieldInfo::new("label", "String")]);
        let (text, _) = text(&ctx, serde_json::json!({ "fields": ["x", "w", "label"] }));
        assert!(text.contains("if (!(o instanceof Point other)) {"), "{text}");
        assert!(text.contains("x == other.x && Double.compare(w, other.w) == 0 && Objects.equals(label, other.label)"), "{text}");
    }

    #[test]
    fn arrays_use_the_arrays_helpers() {
        let ctx = plain(vec![FieldInfo::new("data", "byte[]"), FieldInfo::new("id", "long")]);
        let (text, _) = text(&ctx, serde_json::json!({ "fields": ["data", "id"] }));
        assert!(text.contains("Arrays.equals(data, other.data)"), "{text}");
        assert!(text.contains("Arrays.hashCode(data)"), "{text}");
    }

    #[test]
    fn to_string_quotes_strings_and_lists_every_chosen_field() {
        let ctx = plain(vec![FieldInfo::new("x", "int"), FieldInfo::new("label", "String")]);
        let (text, _) = text(&ctx, serde_json::json!({ "fields": ["x", "label"], "equals": false, "hash_code": false }));
        assert!(text.contains("return \"Point{\""), "{text}");
        assert!(text.contains("+ \"x=\""), "{text}");
        assert!(text.contains("+ \", label='\""), "{text}");
        assert!(text.contains("+ \"}\";"), "{text}");
    }

    #[test]
    fn nothing_selected_or_an_enum_is_refused_and_existing_methods_are_reported() {
        let ctx = sample_context();
        assert!(ValueMembers.generate(&serde_json::json!({ "equals": false, "hash_code": false, "to_string": false }), &ctx).is_err());
        let mut with_equals = sample_context();
        with_equals.class.as_mut().unwrap().methods.push("equals".into());
        let (_, out) = text(&with_equals, serde_json::json!({}));
        assert!(out.notes.iter().any(|n| n.contains("`equals` already exists")));
    }
}
