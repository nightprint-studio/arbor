//! A builder for the class at the caret: a nested static `Builder` with fluent setters (inserted into the
//! class), or Lombok's `@Builder` wired onto it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::{Imports, Lines};
use crate::context::{ClassInfo, FieldInfo, GenContext};
use crate::names::is_primitive;
use crate::output::{parse_options, Output, WireTarget, Wiring};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct Builder;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BuilderOptions {
    /// `nested` | `lombok`
    pub style: String,
    /// Fields the builder sets; empty means every non-static field.
    pub fields: Vec<String>,
    /// `Class.toBuilder()` copying the instance back.
    pub to_builder: bool,
    /// `Objects.requireNonNull` in `build()` for reference-typed fields chosen here.
    pub required: Vec<String>,
    /// Name of the static factory.
    pub factory: String,
}

impl Default for BuilderOptions {
    fn default() -> Self {
        BuilderOptions { style: "nested".into(), fields: Vec::new(), to_builder: false, required: Vec::new(), factory: "builder".into() }
    }
}

impl Generator for Builder {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "builder",
            title: "Generate builder",
            description: "A nested fluent Builder for the class at the caret (or Lombok's @Builder), with toBuilder() and required-field checks.",
            icon: "Hammer",
            group: "Class",
            language: "java",
            targets: &[Target::Class],
            related: &["lombok"],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut o = BuilderOptions::default();
        if let Some(c) = &ctx.class {
            o.fields = settable(c).iter().map(|f| f.name.clone()).collect();
            o.required = c.fields.iter().filter(|f| f.is_final && !is_primitive(&f.type_name)).map(|f| f.name.clone()).collect();
        }
        if ctx.project.lombok() && ctx.class.as_ref().is_some_and(|c| c.has_annotation("Data") || c.has_annotation("Value")) {
            o.style = "lombok".into();
        }
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: BuilderOptions = parse_options(options)?;
        let class = ctx.class.as_ref().ok_or("Place the caret in a class")?;
        if class.kind == "interface" || class.kind == "enum" {
            return Err(format!("A builder needs a class or a record, not an {}", class.kind));
        }
        if o.style == "lombok" {
            if ctx.project.resolved && !ctx.project.lombok() {
                return Err("Lombok is not on the classpath".into());
            }
            let mut out = Output::default();
            out.wiring.push(Wiring { target: WireTarget::Class, annotation: "@Builder".into(), imports: vec!["lombok.Builder".into()] });
            return Ok(out.note("Lombok generates the builder at compile time — nothing is inserted into the class."));
        }
        let fields: Vec<&FieldInfo> = settable(class).into_iter().filter(|f| o.fields.is_empty() || o.fields.contains(&f.name)).collect();
        if fields.is_empty() {
            return Err("Choose at least one field".into());
        }
        let mut imports = Imports::default();
        let mut l = Lines::new();
        write_builder(&mut l, &mut imports, &o, class, &fields);
        let mut out = Output::members(l.finish().trim_end().to_string(), imports.resolved(""));
        if class.has_method(&o.factory) {
            out = out.note(format!("`{}()` already exists in {} — rename the factory or remove it first.", o.factory, class.name));
        }
        if class.is_record() {
            out = out.note("A record has no private constructor: `build()` calls the canonical one.");
        }
        Ok(out)
    }
}

fn settable(class: &ClassInfo) -> Vec<&FieldInfo> {
    class.fields.iter().collect()
}

fn write_builder(l: &mut Lines, im: &mut Imports, o: &BuilderOptions, class: &ClassInfo, fields: &[&FieldInfo]) {
    let name = &class.name;
    let record = class.is_record();
    l.block(format!("public static Builder {}()", o.factory), |l| {
        l.line("return new Builder();");
    });
    if !record {
        l.blank();
        l.block(format!("private {name}(Builder builder)"), |l| {
            for f in fields {
                l.line(format!("this.{0} = builder.{0};", f.name));
            }
        });
    }
    if o.to_builder {
        l.blank();
        l.block("public Builder toBuilder()", |l| {
            let access = |f: &FieldInfo| if record { format!("{}()", f.name) } else { f.name.clone() };
            l.line("return new Builder()");
            l.indent().indent();
            let count = fields.len();
            for (i, f) in fields.iter().enumerate() {
                l.line(format!(".{}({}){}", f.name, access(f), if i + 1 == count { ";" } else { "" }));
            }
            l.dedent().dedent();
        });
    }
    l.blank();
    l.block("public static final class Builder", |l| {
        for f in fields {
            l.line(format!("private {} {};", f.type_name, f.name));
        }
        l.blank();
        l.line("private Builder() {}");
        for f in fields {
            l.blank();
            l.block(format!("public Builder {0}({1} {0})", f.name, f.type_name), |l| {
                l.line(format!("this.{0} = {0};", f.name));
                l.line("return this;");
            });
        }
        l.blank();
        l.block(format!("public {name} build()"), |l| {
            for f in fields.iter().filter(|f| o.required.contains(&f.name) && !is_primitive(&f.type_name)) {
                im.add("java.util.Objects");
                l.line(format!("Objects.requireNonNull({0}, \"{0} is required\");", f.name));
            }
            if record {
                let args: Vec<String> = class
                    .fields
                    .iter()
                    .map(|f| if fields.iter().any(|x| x.name == f.name) { f.name.clone() } else { default_value(&f.type_name) })
                    .collect();
                l.line(format!("return new {name}({});", args.join(", ")));
            } else {
                l.line(format!("return new {name}(this);"));
            }
        });
    });
}

fn default_value(ty: &str) -> String {
    match ty.trim() {
        "boolean" => "false".to_string(),
        "int" | "long" | "short" | "byte" | "float" | "double" | "char" => "0".to_string(),
        _ => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn members(ctx: &GenContext, options: serde_json::Value) -> (String, Output) {
        let out = Builder.generate(&options, ctx).expect("generates");
        (out.members.as_ref().map(|m| m.text.clone()).unwrap_or_default(), out)
    }

    #[test]
    fn a_nested_builder_with_setters_and_a_private_constructor() {
        let (text, out) = members(&sample_context(), serde_json::json!({ "fields": ["name", "email"] }));
        assert!(text.contains("public static Builder builder() {"), "{text}");
        assert!(text.contains("private Customer(Builder builder) {"), "{text}");
        assert!(text.contains("this.name = builder.name;"), "{text}");
        assert!(text.contains("public Builder email(String email) {"), "{text}");
        assert!(text.contains("public Customer build() {"), "{text}");
        assert!(!text.contains("birthDate"), "{text}");
        assert!(out.notes.is_empty());
    }

    #[test]
    fn to_builder_and_required_checks() {
        let (text, out) = members(&sample_context(), serde_json::json!({ "fields": ["name", "email"], "to_builder": true, "required": ["name"] }));
        assert!(text.contains("public Builder toBuilder() {"), "{text}");
        assert!(text.contains(".email(email);"), "{text}");
        assert!(text.contains("Objects.requireNonNull(name, \"name is required\");"), "{text}");
        assert!(out.members.unwrap().imports.contains(&"java.util.Objects".to_string()));
    }

    #[test]
    fn a_record_builder_calls_the_canonical_constructor() {
        let mut ctx = sample_context();
        let mut c = ClassInfo::new("Point", "p", vec![FieldInfo::new("x", "int"), FieldInfo::new("y", "int")]);
        c.kind = "record".into();
        ctx.class = Some(c);
        let (text, _) = members(&ctx, serde_json::json!({}));
        assert!(text.contains("return new Point(x, y);"), "{text}");
        assert!(!text.contains("private Point(Builder"), "{text}");
    }

    #[test]
    fn lombok_style_wires_the_annotation_and_needs_lombok() {
        let mut ctx = sample_context();
        ctx.project.versions.insert("org.projectlombok:lombok".into(), "1.18.30".into());
        let out = Builder.generate(&serde_json::json!({ "style": "lombok" }), &ctx).unwrap();
        assert_eq!(out.wiring[0].annotation, "@Builder");
        assert!(out.members.is_none());
        assert!(Builder.generate(&serde_json::json!({ "style": "lombok" }), &sample_context()).is_err());
    }

    #[test]
    fn an_interface_or_no_class_is_refused() {
        let mut ctx = sample_context();
        ctx.class.as_mut().unwrap().kind = "interface".into();
        assert!(Builder.generate(&serde_json::json!({}), &ctx).is_err());
        ctx.class = None;
        assert!(Builder.generate(&serde_json::json!({}), &ctx).is_err());
    }

    #[test]
    fn an_existing_factory_is_reported() {
        let mut ctx = sample_context();
        ctx.class.as_mut().unwrap().methods.push("builder".into());
        let (_, out) = members(&ctx, serde_json::json!({}));
        assert!(out.notes.iter().any(|n| n.contains("already exists")));
    }
}
