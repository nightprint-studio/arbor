//! Jackson `JsonSerializer` / `JsonDeserializer` for a type, written either as a JSON object field by
//! field or as a single string, and registered the way the project prefers: Spring Boot's
//! `@JsonComponent`, an explicit `Module` bean, or `@JsonSerialize(using = …)` on the type or a field.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::{Imports, JavaFile, Lines};
use crate::context::{ClassInfo, FieldInfo, GenContext};
use crate::names::{java_string, kebab, pascal, snake, strip_generics};
use crate::output::{parse_options, require_class_name, require_inline, require_package, GenFile, Output, SourceRoot, WireTarget, Wiring};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct JacksonSerde;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JacksonOptions {
    /// The simple name of the type to (de)serialize.
    pub type_name: String,
    /// The package of that type, for the import; empty when it is in the generated file's package.
    pub type_package: String,
    /// `both` | `ser` | `de`
    pub mode: String,
    /// `object` | `string`
    pub representation: String,
    /// `json-component` | `module-bean` | `annotation` | `none`
    pub registration: String,
    /// `as-is` | `snake_case` | `kebab-case` | `camelCase`
    pub naming: String,
    pub include_nulls: bool,
    /// `string`: the expression writing the value (`value` in scope) and the one reading it (`text` in scope).
    pub to_string_expr: String,
    pub from_string_expr: String,
    pub class_name: String,
    pub package: String,
    /// `annotation`: a field of the class at the caret to annotate instead of the type.
    pub field: String,
}

impl Default for JacksonOptions {
    fn default() -> Self {
        JacksonOptions {
            type_name: String::new(),
            type_package: String::new(),
            mode: "both".into(),
            representation: "object".into(),
            registration: "json-component".into(),
            naming: "as-is".into(),
            include_nulls: true,
            to_string_expr: "value.toString()".into(),
            from_string_expr: String::new(),
            class_name: String::new(),
            package: String::new(),
            field: String::new(),
        }
    }
}

impl Generator for JacksonSerde {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "jackson-serde",
            title: "Generate Jackson serializer / deserializer",
            description: "JsonSerializer and JsonDeserializer for a type, as an object or a string, registered through @JsonComponent, a Module bean or annotations.",
            icon: "Braces",
            group: "JSON",
            language: "java",
            targets: &[Target::Java, Target::Folder, Target::Project],
            related: &["jackson-databind", "spring-boot-starter-json"],
        }
    }

    fn wanted_types(&self, options: &Value, ctx: &GenContext) -> Vec<String> {
        let Ok(o) = parse_options::<JacksonOptions>(options) else { return Vec::new() };
        if o.type_name.is_empty() || ctx.class.as_ref().is_some_and(|c| c.name == o.type_name) {
            return Vec::new();
        }
        vec![o.type_name]
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut o = JacksonOptions { package: ctx.package_for("config"), ..JacksonOptions::default() };
        if let Some(class) = &ctx.class {
            o.type_name = class.name.clone();
            o.type_package = class.package.clone();
            if class.fields.is_empty() {
                o.representation = "string".into();
            }
        }
        if ctx.project.spring_boot().is_none() && ctx.project.resolved {
            o.registration = "module-bean".into();
        }
        o.from_string_expr = format!("{}.parse(text)", if o.type_name.is_empty() { "Type" } else { &o.type_name });
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let mut o: JacksonOptions = parse_options(options)?;
        require_class_name(&o.type_name, "Type")?;
        require_package(&o.package)?;
        require_package(&o.type_package)?;
        require_inline(&o.to_string_expr, "The write expression")?;
        require_inline(&o.from_string_expr, "The read expression")?;
        if o.from_string_expr.trim().is_empty() {
            o.from_string_expr = format!("{}.parse(text)", o.type_name);
        }
        let name = if o.class_name.trim().is_empty() { format!("{}Json", o.type_name) } else { o.class_name.trim().to_string() };
        require_class_name(&name, "Class name")?;

        let shape = shape_of(&o, ctx);
        let mut notes = Vec::new();
        let object = o.representation != "string";
        if object && shape.is_none() {
            notes.push(format!("The fields of `{}` are not known here — the bodies are skeletons to complete.", o.type_name));
        }

        let mut f = JavaFile::new(&o.package);
        if !o.type_package.is_empty() && o.type_package != o.package {
            f.imports.add(format!("{}.{}", o.type_package, o.type_name));
        }
        f.imports.add("java.io.IOException");
        let ser = o.mode != "de";
        let de = o.mode != "ser";
        let mut body = Lines::new();
        let mut inner = Lines::new();
        if ser {
            write_serializer(&mut inner, &mut f.imports, &o, shape);
        }
        if ser && de {
            inner.blank();
        }
        if de {
            write_deserializer(&mut inner, &mut f.imports, &o, shape);
        }
        let header = match o.registration.as_str() {
            "json-component" => {
                f.imports.add("org.springframework.boot.jackson.JsonComponent");
                body.line("@JsonComponent");
                if ctx.project.spring_boot().is_some_and(|v| v.major >= 4) {
                    notes.push("Spring Boot 4 moved to Jackson 3 (`tools.jackson`) and `@JacksonComponent` — this code targets Jackson 2.".to_string());
                }
                format!("public class {name}")
            }
            "module-bean" => {
                f.imports.add("org.springframework.context.annotation.Configuration");
                body.line("@Configuration");
                format!("public class {name}")
            }
            _ => format!("public final class {name}"),
        };
        body.block(header, |l| {
            if o.registration == "module-bean" {
                module_bean(l, &mut f.imports, &o, ser, de);
                l.blank();
            }
            l.append(&inner);
        });
        f.body = body;

        let mut out = Output::file(GenFile::java(SourceRoot::Main, &o.package, &name, f.render()));
        if o.registration == "annotation" {
            out = wire_annotations(out, &o, &name, ctx, ser, de);
        }
        if o.registration == "json-component" && ctx.project.spring_boot().is_none() && ctx.project.resolved {
            out = out.note("`@JsonComponent` is Spring Boot's — this project has no Spring Boot; choose the Module bean instead.");
        }
        for n in notes {
            out = out.note(n);
        }
        if o.registration == "none" {
            out = out.note("Register the classes yourself: `SimpleModule.addSerializer` / `addDeserializer`, or `@JsonSerialize(using = …)`.");
        }
        Ok(out.needs(ctx.project.missing(&["com.fasterxml.jackson.core:jackson-databind"])))
    }
}

fn shape_of<'a>(o: &JacksonOptions, ctx: &'a GenContext) -> Option<&'a ClassInfo> {
    ctx.class
        .as_ref()
        .filter(|c| c.name == o.type_name && !c.fields.is_empty())
        .or_else(|| ctx.types.get(&o.type_name).filter(|c| !c.fields.is_empty()))
}

fn json_name(naming: &str, field: &str) -> String {
    match naming {
        "snake_case" => snake(field),
        "kebab-case" => kebab(field),
        "camelCase" => crate::names::camel(field),
        _ => field.to_string(),
    }
}

fn getter(class: &ClassInfo, field: &FieldInfo) -> String {
    if class.is_record() {
        return format!("{}()", field.name);
    }
    let name = field.getter.clone().unwrap_or_else(|| {
        let prefix = if strip_generics(&field.type_name) == "boolean" { "is" } else { "get" };
        format!("{prefix}{}", pascal(&field.name))
    });
    format!("{name}()")
}

fn write_serializer(l: &mut Lines, im: &mut Imports, o: &JacksonOptions, shape: Option<&ClassInfo>) {
    let t = &o.type_name;
    im.add("com.fasterxml.jackson.core.JsonGenerator");
    im.add("com.fasterxml.jackson.databind.JsonSerializer");
    im.add("com.fasterxml.jackson.databind.SerializerProvider");
    l.block(format!("public static class Serializer extends JsonSerializer<{t}>"), |l| {
        l.line("@Override");
        l.block(format!("public void serialize({t} value, JsonGenerator gen, SerializerProvider serializers) throws IOException"), |l| {
            if o.representation == "string" {
                l.line(format!("gen.writeString({});", o.to_string_expr.trim()));
                return;
            }
            l.line("gen.writeStartObject();");
            match shape {
                Some(class) => {
                    for field in &class.fields {
                        write_field(l, class, field, o);
                    }
                }
                None => {
                    l.line("// TODO: write the fields, e.g. gen.writeStringField(\"name\", value.getName());");
                }
            }
            l.line("gen.writeEndObject();");
        });
    });
}

fn write_field(l: &mut Lines, class: &ClassInfo, field: &FieldInfo, o: &JacksonOptions) {
    let key = java_string(&json_name(&o.naming, &field.name));
    let access = format!("value.{}", getter(class, field));
    let ty = field.type_name.trim();
    let primitive = crate::names::is_primitive(ty);
    let statement = match ty {
        "String" => format!("gen.writeStringField({key}, {access});"),
        "int" | "long" | "short" | "double" | "float" | "byte" => format!("gen.writeNumberField({key}, {access});"),
        "boolean" => format!("gen.writeBooleanField({key}, {access});"),
        "BigDecimal" => format!("gen.writeNumberField({key}, {access});"),
        _ => format!("serializers.defaultSerializeField({key}, {access}, gen);"),
    };
    if !o.include_nulls && !primitive {
        l.block(format!("if ({access} != null)"), |l| {
            l.line(&statement);
        });
    } else {
        l.line(statement);
    }
}

fn write_deserializer(l: &mut Lines, im: &mut Imports, o: &JacksonOptions, shape: Option<&ClassInfo>) {
    let t = &o.type_name;
    im.add("com.fasterxml.jackson.core.JsonParser");
    im.add("com.fasterxml.jackson.databind.DeserializationContext");
    im.add("com.fasterxml.jackson.databind.JsonDeserializer");
    let needs_read_helper = o.representation != "string" && shape.is_some_and(|c| c.fields.iter().any(|f| read_expr(f, o).1));
    l.block(format!("public static class Deserializer extends JsonDeserializer<{t}>"), |l| {
        l.line("@Override");
        l.block(format!("public {t} deserialize(JsonParser p, DeserializationContext ctxt) throws IOException"), |l| {
            if o.representation == "string" {
                l.line("String text = p.getValueAsString();");
                l.line(format!("return text == null ? null : {};", o.from_string_expr.trim()));
                return;
            }
            im.add("com.fasterxml.jackson.databind.JsonNode");
            l.line("JsonNode node = p.getCodec().readTree(p);");
            let Some(class) = shape else {
                l.line("// TODO: read the fields, e.g. node.path(\"name\").asText()");
                l.line(format!("return new {t}();"));
                return;
            };
            let mut helper = false;
            let args: Vec<String> = class
                .fields
                .iter()
                .map(|field| {
                    let (expr, needs_helper) = read_expr(field, o);
                    helper |= needs_helper;
                    expr
                })
                .collect();
            if helper {
                im.add("com.fasterxml.jackson.core.type.TypeReference");
            }
            l.line(format!("return new {t}("));
            l.indent().indent();
            let count = args.len();
            for (i, arg) in args.iter().enumerate() {
                l.line(format!("{arg}{}", if i + 1 == count { ");" } else { "," }));
            }
            l.dedent().dedent();
        });
        if needs_read_helper {
            l.blank();
            l.block("private <T> T read(DeserializationContext ctxt, JsonNode node, String name, TypeReference<T> type) throws IOException", |l| {
                l.line("JsonNode child = node.get(name);");
                l.line("return child == null || child.isNull() ? null : ctxt.readTreeAsValue(child, ctxt.getTypeFactory().constructType(type));");
            });
        }
    });
}

/// The constructor argument reading `field` out of `node`, and whether it uses the generic `read` helper.
fn read_expr(field: &FieldInfo, o: &JacksonOptions) -> (String, bool) {
    let key = java_string(&json_name(&o.naming, &field.name));
    let ty = field.type_name.trim();
    let node = format!("node.path({key})");
    match ty {
        "String" => (format!("{node}.asText(null)"), false),
        "int" => (format!("{node}.asInt()"), false),
        "long" => (format!("{node}.asLong()"), false),
        "double" => (format!("{node}.asDouble()"), false),
        "float" => (format!("(float) {node}.asDouble()"), false),
        "short" => (format!("(short) {node}.asInt()"), false),
        "byte" => (format!("(byte) {node}.asInt()"), false),
        "boolean" => (format!("{node}.asBoolean()"), false),
        "BigDecimal" => (format!("{node}.isMissingNode() || {node}.isNull() ? null : {node}.decimalValue()"), false),
        _ => (format!("read(ctxt, node, {key}, new TypeReference<{ty}>() {{}})"), true),
    }
}

fn module_bean(l: &mut Lines, im: &mut Imports, o: &JacksonOptions, ser: bool, de: bool) {
    let t = &o.type_name;
    im.add("com.fasterxml.jackson.databind.Module");
    im.add("com.fasterxml.jackson.databind.module.SimpleModule");
    im.add("org.springframework.context.annotation.Bean");
    l.line("@Bean");
    l.block(format!("public Module {}Module()", crate::names::camel(t)), |l| {
        l.line(format!("SimpleModule module = new SimpleModule({});", java_string(&format!("{t}Module"))));
        if ser {
            l.line(format!("module.addSerializer({t}.class, new Serializer());"));
        }
        if de {
            l.line(format!("module.addDeserializer({t}.class, new Deserializer());"));
        }
        l.line("return module;");
    });
}

fn wire_annotations(mut out: Output, o: &JacksonOptions, class_name: &str, ctx: &GenContext, ser: bool, de: bool) -> Output {
    let same_package = ctx.class.as_ref().is_some_and(|c| c.package == o.package);
    let target = if !o.field.trim().is_empty() {
        Some(WireTarget::Field(o.field.trim().to_string()))
    } else if ctx.class.as_ref().is_some_and(|c| c.name == o.type_name) {
        Some(WireTarget::Class)
    } else {
        None
    };
    let Some(target) = target else {
        return out.note(format!("Put `@JsonSerialize(using = {class_name}.Serializer.class)` / `@JsonDeserialize(using = {class_name}.Deserializer.class)` on `{}` — it is not the class at the caret.", o.type_name));
    };
    let mut add = |annotation: String, import: &str| {
        let mut imports = vec![import.to_string()];
        if !same_package {
            imports.push(format!("{}.{class_name}", o.package));
        }
        out.wiring.push(Wiring { target: target.clone(), annotation, imports });
    };
    if ser {
        add(format!("@JsonSerialize(using = {class_name}.Serializer.class)"), "com.fasterxml.jackson.databind.annotation.JsonSerialize");
    }
    if de {
        add(format!("@JsonDeserialize(using = {class_name}.Deserializer.class)"), "com.fasterxml.jackson.databind.annotation.JsonDeserialize");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn money_context() -> GenContext {
        let mut ctx = sample_context();
        let mut money = ClassInfo::new("Money", "com.acme.shop.model", vec![FieldInfo::new("amount", "BigDecimal"), FieldInfo::new("currency", "String"), FieldInfo::new("scale", "int")]);
        money.kind = "record".into();
        ctx.class = Some(money);
        ctx
    }

    fn gen(ctx: &GenContext, options: serde_json::Value) -> Output {
        JacksonSerde.generate(&options, ctx).expect("generates")
    }

    #[test]
    fn an_object_serializer_writes_each_field_by_type() {
        let ctx = money_context();
        let out = gen(&ctx, serde_json::json!({ "type_name": "Money", "type_package": "com.acme.shop.model", "package": "com.acme.shop.config" }));
        let text = &out.files[0].text;
        assert_eq!(out.files[0].name, "MoneyJson.java");
        assert!(text.contains("@JsonComponent"), "{text}");
        assert!(text.contains("gen.writeNumberField(\"amount\", value.amount());"), "{text}");
        assert!(text.contains("gen.writeStringField(\"currency\", value.currency());"), "{text}");
        assert!(text.contains("gen.writeNumberField(\"scale\", value.scale());"), "{text}");
        assert!(text.contains("import com.acme.shop.model.Money;"), "{text}");
        assert!(text.contains("node.path(\"currency\").asText(null)"), "{text}");
        assert!(text.contains("return new Money("), "{text}");
    }

    #[test]
    fn classes_use_getters_and_boolean_getters_use_is() {
        let mut ctx = sample_context();
        ctx.class = Some(ClassInfo::new("Flag", "p", vec![FieldInfo::new("on", "boolean"), FieldInfo::new("label", "String")]));
        let out = gen(&ctx, serde_json::json!({ "type_name": "Flag", "package": "p" }));
        let text = &out.files[0].text;
        assert!(text.contains("gen.writeBooleanField(\"on\", value.isOn());"), "{text}");
        assert!(text.contains("value.getLabel()"), "{text}");
    }

    #[test]
    fn naming_strategies_rename_the_json_properties_both_ways() {
        let ctx = sample_context();
        let out = gen(&ctx, serde_json::json!({ "type_name": "Customer", "naming": "snake_case", "package": "p" }));
        let text = &out.files[0].text;
        assert!(text.contains("\"birth_date\""), "{text}");
        assert_eq!(text.matches("\"birth_date\"").count(), 2, "{text}");
    }

    #[test]
    fn omitting_nulls_guards_non_primitives_only() {
        let ctx = money_context();
        let out = gen(&ctx, serde_json::json!({ "type_name": "Money", "include_nulls": false, "package": "p", "mode": "ser" }));
        let text = &out.files[0].text;
        assert!(text.contains("if (value.currency() != null) {"), "{text}");
        assert!(!text.contains("if (value.scale() != null)"), "{text}");
        assert!(!text.contains("Deserializer"), "{text}");
    }

    #[test]
    fn a_string_representation_round_trips_through_text() {
        let ctx = money_context();
        let out = gen(
            &ctx,
            serde_json::json!({ "type_name": "Money", "representation": "string", "from_string_expr": "Money.parse(text)", "package": "p" }),
        );
        let text = &out.files[0].text;
        assert!(text.contains("gen.writeString(value.toString());"), "{text}");
        assert!(text.contains("return text == null ? null : Money.parse(text);"), "{text}");
    }

    #[test]
    fn a_module_bean_registers_both_and_a_none_registration_says_how_to() {
        let ctx = money_context();
        let module = gen(&ctx, serde_json::json!({ "type_name": "Money", "registration": "module-bean", "package": "p" }));
        let text = &module.files[0].text;
        assert!(text.contains("@Configuration"), "{text}");
        assert!(text.contains("module.addSerializer(Money.class, new Serializer());"), "{text}");
        assert!(text.contains("module.addDeserializer(Money.class, new Deserializer());"), "{text}");
        let none = gen(&ctx, serde_json::json!({ "type_name": "Money", "registration": "none", "package": "p" }));
        assert!(none.notes.iter().any(|n| n.contains("Register")));
    }

    #[test]
    fn annotation_registration_wires_the_type_at_the_caret() {
        let ctx = money_context();
        let out = gen(&ctx, serde_json::json!({ "type_name": "Money", "registration": "annotation", "package": "p" }));
        assert_eq!(out.wiring.len(), 2);
        assert_eq!(out.wiring[0].target, WireTarget::Class);
        assert!(out.wiring[0].annotation.contains("MoneyJson.Serializer.class"));
        let elsewhere = gen(&sample_context(), serde_json::json!({ "type_name": "Money", "registration": "annotation", "package": "p" }));
        assert!(elsewhere.wiring.is_empty());
        assert!(!elsewhere.notes.is_empty());
    }

    #[test]
    fn generic_fields_go_through_the_read_helper() {
        let ctx = sample_context();
        let out = gen(&ctx, serde_json::json!({ "type_name": "Customer", "package": "p" }));
        let text = &out.files[0].text;
        assert!(text.contains("new TypeReference<List<String>>() {}"), "{text}");
        assert!(text.contains("private <T> T read(DeserializationContext ctxt"), "{text}");
    }

    #[test]
    fn a_bad_type_name_is_refused() {
        assert!(JacksonSerde.generate(&serde_json::json!({ "type_name": "not a name" }), &sample_context()).is_err());
    }
}
