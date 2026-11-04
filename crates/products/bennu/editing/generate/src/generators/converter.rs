//! JPA `AttributeConverter`: dates as text, durations as millis, enums by code, booleans as `Y`/`N`,
//! lists as CSV, JSON columns, AES-GCM encrypted strings — with `autoApply` or `@Convert` wired onto a
//! field of the class at the caret.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::JavaFile;
use crate::context::GenContext;
use crate::names::{java_string, pascal, simple_name, strip_generics};
use crate::output::{parse_options, require_class_name, require_package, GenFile, Output, SourceRoot, WireTarget, Wiring};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct AttributeConverter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConverterOptions {
    /// `date-string` | `duration-millis` | `csv-list` | `boolean-char` | `enum` | `json` | `encrypted` | `custom`
    pub kind: String,
    /// The entity-side type: `LocalDate`, an enum's name, `Map<String, Object>`, …
    pub java_type: String,
    /// The column-side type, for the kinds that choose it (`custom`, `enum`).
    pub db_type: String,
    /// Empty derives one from the kind and the type.
    pub class_name: String,
    pub package: String,
    pub auto_apply: bool,
    pub null_safe: bool,
    /// The field of the class at the caret to annotate with `@Convert`; empty writes no wiring.
    pub field: String,
    pub separator: String,
    pub true_value: String,
    pub false_value: String,
    /// `name` | `code` (an enum stored by an accessor of its own).
    pub enum_mode: String,
    pub code_getter: String,
    pub key_env: String,
    /// `custom`: the expressions converting each way (`attribute`, `dbData` in scope).
    pub to_expr: String,
    pub from_expr: String,
}

impl Default for ConverterOptions {
    fn default() -> Self {
        ConverterOptions {
            kind: "date-string".into(),
            java_type: "LocalDate".into(),
            db_type: "String".into(),
            class_name: String::new(),
            package: String::new(),
            auto_apply: false,
            null_safe: true,
            field: String::new(),
            separator: ",".into(),
            true_value: "Y".into(),
            false_value: "N".into(),
            enum_mode: "name".into(),
            code_getter: "getCode".into(),
            key_env: "APP_ENCRYPTION_KEY".into(),
            to_expr: "attribute".into(),
            from_expr: "dbData".into(),
        }
    }
}

impl Generator for AttributeConverter {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "attribute-converter",
            title: "Generate JPA attribute converter",
            description: "AttributeConverter for dates, durations, enums, booleans, lists, JSON or encrypted columns, with autoApply or @Convert on a field.",
            icon: "ArrowLeftRight",
            group: "Persistence",
            language: "java",
            targets: &[Target::Java, Target::Folder, Target::Project],
            related: &["jakarta.persistence-api", "javax.persistence-api", "hibernate-core", "spring-boot-starter-data-jpa"],
        }
    }

    fn wanted_types(&self, options: &Value, _ctx: &GenContext) -> Vec<String> {
        let Ok(o) = parse_options::<ConverterOptions>(options) else { return Vec::new() };
        if o.kind == "enum" && !o.java_type.trim().is_empty() {
            vec![simple_name(o.java_type.trim()).to_string()]
        } else {
            Vec::new()
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut o = ConverterOptions { package: ctx.package_for("converter"), ..ConverterOptions::default() };
        if let Some(class) = &ctx.class {
            if let Some(field) = class.fields.iter().find(|f| !f.id && !f.has_annotation("Convert") && guess(ctx, &f.type_name).is_some()) {
                if let Some((kind, java_type)) = guess(ctx, &field.type_name) {
                    o.kind = kind.to_string();
                    o.java_type = java_type;
                    o.field = field.name.clone();
                }
            }
        }
        sync_db_type(&mut o, ctx);
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let mut o: ConverterOptions = parse_options(options)?;
        sync_db_type(&mut o, ctx);
        let name = if o.class_name.trim().is_empty() { derive_name(&o) } else { o.class_name.trim().to_string() };
        require_class_name(&name, "Class name")?;
        require_package(&o.package)?;
        if o.java_type.trim().is_empty() {
            return Err("Choose the type to convert".into());
        }

        let ee = ctx.project.ee();
        let mut f = JavaFile::new(&o.package);
        f.imports.add(format!("{ee}.persistence.AttributeConverter"));
        f.imports.add(format!("{ee}.persistence.Converter"));
        let body = Body::for_options(&o, ctx)?;
        f.imports.extend(body.imports.iter());
        let java_type = o.java_type.trim().to_string();
        let db_type = body.db_type.clone();
        f.body.line(format!("@Converter(autoApply = {})", o.auto_apply));
        f.body.block(format!("public class {name} implements AttributeConverter<{java_type}, {db_type}>"), |l| {
            for field in &body.fields {
                l.line(field);
            }
            if !body.fields.is_empty() {
                l.blank();
            }
            l.line("@Override");
            l.block(format!("public {db_type} convertToDatabaseColumn({java_type} attribute)"), |l| {
                if o.null_safe && body.null_guard {
                    l.line("if (attribute == null) {");
                    l.line("    return null;");
                    l.line("}");
                }
                l.text(&body.to_db);
            });
            l.blank();
            l.line("@Override");
            l.block(format!("public {java_type} convertToEntityAttribute({db_type} dbData)"), |l| {
                if o.null_safe && body.null_guard {
                    l.line("if (dbData == null) {");
                    l.line("    return null;");
                    l.line("}");
                }
                l.text(&body.from_db);
            });
            for helper in &body.helpers {
                l.blank();
                l.text(helper);
            }
        });

        let mut out = Output::file(GenFile::java(SourceRoot::Main, &o.package, &name, f.render()));
        for note in body.notes {
            out = out.note(note);
        }
        if !o.field.trim().is_empty() {
            if o.auto_apply {
                out = out.note("`autoApply = true` converts every attribute of this type, so the field needs no `@Convert`.");
            } else {
                let mut imports = vec![format!("{ee}.persistence.Convert")];
                let same_package = ctx.class.as_ref().is_some_and(|c| c.package == o.package);
                if !same_package {
                    imports.push(format!("{}.{name}", o.package));
                }
                out.wiring.push(Wiring {
                    target: WireTarget::Field(o.field.trim().to_string()),
                    annotation: format!("@Convert(converter = {name}.class)"),
                    imports,
                });
            }
        }
        if o.auto_apply && matches!(o.kind.as_str(), "boolean-char" | "json" | "encrypted") {
            out = out.note("`autoApply = true` on a type as common as this one converts every such attribute in the persistence unit — a field-level `@Convert` is usually safer.");
        }
        Ok(out)
    }
}

/// The kind and entity-side type a field's type suggests.
fn guess(ctx: &GenContext, type_name: &str) -> Option<(&'static str, String)> {
    let t = type_name.trim();
    match strip_generics(t) {
        "LocalDate" | "LocalDateTime" | "LocalTime" | "Instant" | "OffsetDateTime" | "ZonedDateTime" => Some(("date-string", t.to_string())),
        "Duration" => Some(("duration-millis", t.to_string())),
        "Boolean" | "boolean" => Some(("boolean-char", "Boolean".to_string())),
        "List" | "Set" if t.contains("<String>") => Some(("csv-list", "List<String>".to_string())),
        "Map" | "List" | "Set" => Some(("json", t.to_string())),
        other => ctx.types.get(other).filter(|c| c.is_enum()).map(|_| ("enum", other.to_string())),
    }
}

/// The column type the kind implies; `custom` and `enum` keep what the user chose.
fn sync_db_type(o: &mut ConverterOptions, ctx: &GenContext) {
    let fixed = match o.kind.as_str() {
        "date-string" | "csv-list" | "json" | "encrypted" => Some("String"),
        "duration-millis" => Some("Long"),
        "boolean-char" => Some("String"),
        _ => None,
    };
    if let Some(db) = fixed {
        o.db_type = db.to_string();
    } else if o.kind == "enum" {
        if o.enum_mode == "code" {
            if o.db_type != "Integer" && o.db_type != "String" && o.db_type != "Long" {
                o.db_type = "String".to_string();
            }
        } else {
            o.db_type = "String".to_string();
        }
        if o.java_type == "LocalDate" || o.java_type.is_empty() {
            if let Some(shape) = ctx.types.values().find(|c| c.is_enum()) {
                o.java_type = shape.name.clone();
            }
        }
    } else if o.db_type.trim().is_empty() {
        o.db_type = "String".to_string();
    }
}

fn derive_name(o: &ConverterOptions) -> String {
    let simple = pascal(&simple_name(&o.java_type).replace(['<', '>', ','], " "));
    match o.kind.as_str() {
        "date-string" => format!("{simple}StringConverter"),
        "duration-millis" => "DurationMillisConverter".to_string(),
        "csv-list" => "StringListConverter".to_string(),
        "boolean-char" => "BooleanYesNoConverter".to_string(),
        "enum" => format!("{simple}Converter"),
        "json" => format!("{}JsonConverter", json_label(&o.java_type)),
        "encrypted" => "EncryptedStringConverter".to_string(),
        _ => format!("{simple}Converter"),
    }
}

/// `Map<String, Object>` → `Map`, `List<Item>` → `ItemList`.
fn json_label(java_type: &str) -> String {
    let outer = pascal(simple_name(java_type));
    match crate::names::generic_args(java_type).last() {
        Some(inner) if matches!(outer.as_str(), "List" | "Set") => format!("{}{outer}", pascal(simple_name(inner))),
        _ => outer,
    }
}

/// The pieces of one kind: the two method bodies, the column type, and what they pull in.
struct Body {
    db_type: String,
    to_db: String,
    from_db: String,
    imports: Vec<String>,
    fields: Vec<String>,
    helpers: Vec<String>,
    notes: Vec<String>,
    /// The two null guards are redundant where the body maps null itself.
    null_guard: bool,
}

impl Body {
    fn new(db_type: &str, to_db: &str, from_db: &str) -> Body {
        Body {
            db_type: db_type.to_string(),
            to_db: to_db.to_string(),
            from_db: from_db.to_string(),
            imports: Vec::new(),
            fields: Vec::new(),
            helpers: Vec::new(),
            notes: Vec::new(),
            null_guard: true,
        }
    }

    fn for_options(o: &ConverterOptions, ctx: &GenContext) -> Result<Body, String> {
        let t = o.java_type.trim();
        match o.kind.as_str() {
            "date-string" => {
                let simple = strip_generics(t);
                let mut b = Body::new("String", "return attribute.toString();", &format!("return {simple}.parse(dbData);"));
                b.imports.push(format!("java.time.{simple}"));
                if matches!(simple, "LocalDateTime" | "Instant" | "OffsetDateTime" | "ZonedDateTime") {
                    b.notes.push("ISO-8601 text sorts correctly as a string only while the offset/zone is the same on every row.".into());
                }
                Ok(b)
            }
            "duration-millis" => {
                let mut b = Body::new("Long", "return attribute.toMillis();", "return Duration.ofMillis(dbData);");
                b.imports.push("java.time.Duration".into());
                Ok(b)
            }
            "csv-list" => {
                let sep = java_string(&o.separator);
                let mut b = Body::new(
                    "String",
                    &format!("return String.join({sep}, attribute);"),
                    &format!("return dbData.isBlank() ? new ArrayList<>() : new ArrayList<>(Arrays.asList(dbData.split(Pattern.quote({sep}))));"),
                );
                b.imports.extend(["java.util.ArrayList".to_string(), "java.util.Arrays".to_string(), "java.util.regex.Pattern".to_string()]);
                b.notes.push("No escaping: a value containing the separator is split in two when read back.".into());
                Ok(b)
            }
            "boolean-char" => {
                let (yes, no) = (java_string(&o.true_value), java_string(&o.false_value));
                let mut b = Body::new(
                    "String",
                    &format!("return Boolean.TRUE.equals(attribute) ? {yes} : {no};"),
                    &format!("return {yes}.equals(dbData);"),
                );
                b.null_guard = false;
                if o.null_safe {
                    b.to_db = format!("if (attribute == null) {{\n    return null;\n}}\nreturn attribute ? {yes} : {no};");
                    b.from_db = format!("if (dbData == null) {{\n    return null;\n}}\nreturn {yes}.equals(dbData);");
                }
                Ok(b)
            }
            "enum" => enum_body(o, ctx),
            "json" => {
                let mut b = Body::new(
                    "String",
                    "try {\n    return MAPPER.writeValueAsString(attribute);\n} catch (JsonProcessingException e) {\n    throw new IllegalArgumentException(\"Cannot write JSON column\", e);\n}",
                    &format!(
                        "try {{\n    return MAPPER.readValue(dbData, new TypeReference<{t}>() {{}});\n}} catch (JsonProcessingException e) {{\n    throw new IllegalArgumentException(\"Cannot read JSON column\", e);\n}}"
                    ),
                );
                b.imports.extend([
                    "com.fasterxml.jackson.core.JsonProcessingException".to_string(),
                    "com.fasterxml.jackson.core.type.TypeReference".to_string(),
                    "com.fasterxml.jackson.databind.ObjectMapper".to_string(),
                ]);
                b.fields.push("private static final ObjectMapper MAPPER = new ObjectMapper().findAndRegisterModules();".into());
                if !ctx.project.has("jackson-databind") && ctx.project.resolved {
                    b.notes.push("jackson-databind is not on the classpath.".into());
                }
                Ok(b)
            }
            "encrypted" => Ok(encrypted_body(o)),
            "custom" => {
                require_inline_expr(&o.to_expr)?;
                require_inline_expr(&o.from_expr)?;
                let mut b = Body::new(o.db_type.trim(), &format!("return {};", o.to_expr.trim()), &format!("return {};", o.from_expr.trim()));
                b.notes.push("The two expressions are placeholders — write the real conversion.".into());
                Ok(b)
            }
            other => Err(format!("Unknown converter kind `{other}`")),
        }
    }
}

fn require_inline_expr(text: &str) -> Result<(), String> {
    crate::output::require_inline(text, "A conversion expression")
}

fn enum_body(o: &ConverterOptions, ctx: &GenContext) -> Result<Body, String> {
    let t = o.java_type.trim();
    if o.enum_mode != "code" {
        let mut b = Body::new("String", "return attribute.name();", &format!("return {t}.valueOf(dbData);"));
        b.notes.push("Stored by name: renaming a constant breaks existing rows — `@Enumerated(EnumType.STRING)` has the same property.".into());
        return Ok(b);
    }
    let getter = o.code_getter.trim();
    if !crate::names::is_identifier(getter) {
        return Err(format!("`{getter}` is not a valid method name"));
    }
    let db = o.db_type.trim();
    let mut b = Body::new(
        db,
        &format!("return attribute.{getter}();"),
        &format!(
            "for ({t} value : {t}.values()) {{\n    if (Objects.equals(value.{getter}(), dbData)) {{\n        return value;\n    }}\n}}\nthrow new IllegalArgumentException(\"Unknown {t} code: \" + dbData);"
        ),
    );
    b.imports.push("java.util.Objects".into());
    if let Some(shape) = ctx.types.get(t) {
        if !shape.has_method(getter) && !shape.methods.is_empty() {
            b.notes.push(format!("{t} declares no `{getter}()` — add it, or change the accessor."));
        }
    }
    Ok(b)
}

fn encrypted_body(o: &ConverterOptions) -> Body {
    let env = java_string(&o.key_env);
    let mut b = Body::new(
        "String",
        "try {\n    byte[] iv = new byte[IV_BYTES];\n    RANDOM.nextBytes(iv);\n    Cipher cipher = Cipher.getInstance(ALGORITHM);\n    cipher.init(Cipher.ENCRYPT_MODE, key, new GCMParameterSpec(TAG_BITS, iv));\n    byte[] encrypted = cipher.doFinal(attribute.getBytes(StandardCharsets.UTF_8));\n    return Base64.getEncoder().encodeToString(ByteBuffer.allocate(iv.length + encrypted.length).put(iv).put(encrypted).array());\n} catch (GeneralSecurityException e) {\n    throw new IllegalStateException(\"Cannot encrypt column value\", e);\n}",
        "try {\n    ByteBuffer buffer = ByteBuffer.wrap(Base64.getDecoder().decode(dbData));\n    byte[] iv = new byte[IV_BYTES];\n    buffer.get(iv);\n    byte[] encrypted = new byte[buffer.remaining()];\n    buffer.get(encrypted);\n    Cipher cipher = Cipher.getInstance(ALGORITHM);\n    cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(TAG_BITS, iv));\n    return new String(cipher.doFinal(encrypted), StandardCharsets.UTF_8);\n} catch (GeneralSecurityException e) {\n    throw new IllegalStateException(\"Cannot decrypt column value\", e);\n}",
    );
    b.imports.extend(
        [
            "java.nio.ByteBuffer",
            "java.nio.charset.StandardCharsets",
            "java.security.GeneralSecurityException",
            "java.security.SecureRandom",
            "java.util.Base64",
            "javax.crypto.Cipher",
            "javax.crypto.SecretKey",
            "javax.crypto.spec.GCMParameterSpec",
            "javax.crypto.spec.SecretKeySpec",
        ]
        .map(String::from),
    );
    b.fields = vec![
        "private static final String ALGORITHM = \"AES/GCM/NoPadding\";".into(),
        "private static final int IV_BYTES = 12;".into(),
        "private static final int TAG_BITS = 128;".into(),
        "private static final SecureRandom RANDOM = new SecureRandom();".into(),
        "".into(),
        "private final SecretKey key = loadKey();".into(),
    ];
    b.helpers.push(format!(
        "private static SecretKey loadKey() {{\n    String encoded = System.getenv({env});\n    if (encoded == null || encoded.isBlank()) {{\n        throw new IllegalStateException(\"Set the {} environment variable to a Base64-encoded AES key\");\n    }}\n    return new SecretKeySpec(Base64.getDecoder().decode(encoded), \"AES\");\n}}",
        o.key_env.replace('"', "")
    ));
    b.notes.push("A random IV per value means the column cannot be compared or indexed for equality.".into());
    b.notes.push("The converter is built by Hibernate, so the key comes from the environment rather than a Spring bean.".into());
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn gen(options: serde_json::Value) -> Output {
        AttributeConverter.generate(&options, &sample_context()).expect("generates")
    }

    #[test]
    fn a_local_date_converter_names_itself_and_imports_jakarta() {
        let out = gen(serde_json::json!({ "package": "com.acme.converter" }));
        let file = &out.files[0];
        assert_eq!(file.name, "LocalDateStringConverter.java");
        assert!(file.text.contains("import jakarta.persistence.AttributeConverter;"), "{}", file.text);
        assert!(file.text.contains("@Converter(autoApply = false)"));
        assert!(file.text.contains("implements AttributeConverter<LocalDate, String>"));
        assert!(file.text.contains("return LocalDate.parse(dbData);"));
        assert!(file.text.contains("if (attribute == null) {\n            return null;\n        }"), "{}", file.text);
    }

    #[test]
    fn javax_projects_import_javax_persistence() {
        let mut ctx = sample_context();
        ctx.project = legacy_project();
        let out = AttributeConverter.generate(&serde_json::json!({ "package": "p" }), &ctx).unwrap();
        assert!(out.files[0].text.contains("import javax.persistence.AttributeConverter;"));
    }

    #[test]
    fn a_field_gets_a_convert_wiring_unless_auto_apply_is_on() {
        let wired = gen(serde_json::json!({ "package": "com.acme.converter", "field": "birthDate" }));
        assert_eq!(wired.wiring.len(), 1);
        assert_eq!(wired.wiring[0].annotation, "@Convert(converter = LocalDateStringConverter.class)");
        assert!(wired.wiring[0].imports.contains(&"jakarta.persistence.Convert".to_string()));
        assert!(wired.wiring[0].imports.contains(&"com.acme.converter.LocalDateStringConverter".to_string()));
        let auto = gen(serde_json::json!({ "package": "com.acme.converter", "field": "birthDate", "auto_apply": true }));
        assert!(auto.wiring.is_empty());
    }

    #[test]
    fn defaults_pick_the_first_convertible_field_of_the_class_at_the_caret() {
        let defaults = AttributeConverter.defaults(&sample_context());
        assert_eq!(defaults["kind"], "date-string");
        assert_eq!(defaults["field"], "birthDate");
        assert_eq!(defaults["package"], "com.acme.shop.converter");
    }

    #[test]
    fn boolean_char_uses_the_chosen_letters() {
        let out = gen(serde_json::json!({ "kind": "boolean-char", "java_type": "Boolean", "true_value": "S", "false_value": "N", "package": "p" }));
        let text = &out.files[0].text;
        assert!(text.contains("attribute ? \"S\" : \"N\""), "{text}");
        assert!(text.contains("\"S\".equals(dbData)"), "{text}");
        assert_eq!(out.files[0].name, "BooleanYesNoConverter.java");
    }

    #[test]
    fn enum_by_code_loops_over_the_constants() {
        let out = gen(serde_json::json!({ "kind": "enum", "java_type": "Status", "enum_mode": "code", "db_type": "Integer", "package": "p" }));
        let text = &out.files[0].text;
        assert!(text.contains("implements AttributeConverter<Status, Integer>"), "{text}");
        assert!(text.contains("for (Status value : Status.values())"), "{text}");
        assert!(text.contains("return attribute.getCode();"), "{text}");
    }

    #[test]
    fn json_converters_use_a_type_reference_and_name_after_the_element() {
        let out = gen(serde_json::json!({ "kind": "json", "java_type": "List<OrderLine>", "package": "p" }));
        assert_eq!(out.files[0].name, "OrderLineListJsonConverter.java");
        assert!(out.files[0].text.contains("new TypeReference<List<OrderLine>>() {}"), "{}", out.files[0].text);
    }

    #[test]
    fn the_encrypted_converter_reads_its_key_from_the_environment() {
        let out = gen(serde_json::json!({ "kind": "encrypted", "java_type": "String", "key_env": "SHOP_KEY", "package": "p" }));
        let text = &out.files[0].text;
        assert!(text.contains("System.getenv(\"SHOP_KEY\")"), "{text}");
        assert!(text.contains("private final SecretKey key = loadKey();"), "{text}");
        assert!(out.notes.iter().any(|n| n.contains("equality")));
    }

    #[test]
    fn a_custom_expression_cannot_smuggle_a_statement() {
        let bad = AttributeConverter.generate(&serde_json::json!({ "kind": "custom", "java_type": "X", "to_expr": "a; b" }), &sample_context());
        assert!(bad.is_err());
    }

    #[test]
    fn csv_quotes_the_separator() {
        let out = gen(serde_json::json!({ "kind": "csv-list", "java_type": "List<String>", "separator": "|", "package": "p" }));
        assert!(out.files[0].text.contains("Pattern.quote(\"|\")"), "{}", out.files[0].text);
    }
}
