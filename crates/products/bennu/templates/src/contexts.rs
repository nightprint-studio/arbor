//! What each kind of template is rendered with — except validation tests, whose context lives with the
//! DTO Lab — and the templates Bennu ships for each.
//!
//! Every field here is something a template can read, and its doc comment is what the editor shows
//! when completing it: the schema is generated from these structs.

use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::dates::today;
use crate::kind::{Builtin, TemplateKind};
use crate::model::{type_simple, ClassModel};

/// The templates Bennu ships for a kind. Empty for validation tests, whose built-in lives with the
/// DTO Lab.
pub fn builtins(kind: TemplateKind) -> &'static [Builtin] {
    match kind {
        TemplateKind::NewFile => NEW_FILE,
        TemplateKind::Class => CLASS,
        TemplateKind::ConfigProperties => CONFIG_PROPERTIES,
        TemplateKind::ConfigClass => CONFIG_CLASS,
        TemplateKind::Live => LIVE,
        TemplateKind::ValidationTests => &[],
    }
}

// New file templates are starters: the dialog already makes a class, and a second "class" beside it
// would be the same thing twice.
const NEW_FILE: &[Builtin] = &[
    Builtin { name: "class", extension: "java", starter: true, text: include_str!("../builtin/new-file/class.java.jinja") },
    Builtin { name: "entity", extension: "java", starter: true, text: include_str!("../builtin/new-file/entity.java.jinja") },
];

const CLASS: &[Builtin] = &[
    Builtin { name: "builder", extension: "java", starter: false, text: include_str!("../builtin/class/builder.java.jinja") },
    Builtin { name: "repository", extension: "java", starter: false, text: include_str!("../builtin/class/repository.java.jinja") },
];

const CONFIG_PROPERTIES: &[Builtin] = &[
    Builtin { name: "yaml", extension: "yml", starter: false, text: include_str!("../builtin/config-properties/yaml.yml.jinja") },
    Builtin { name: "properties", extension: "properties", starter: false, text: include_str!("../builtin/config-properties/properties.properties.jinja") },
];

// Which one a project starts from depends on its Java level and on Lombok — see the backend's choice of a
// default — so the order here only decides how they are listed.
const CONFIG_CLASS: &[Builtin] = &[
    Builtin { name: "record", extension: "java", starter: false, text: include_str!("../builtin/config-class/record.java.jinja") },
    Builtin { name: "lombok", extension: "java", starter: false, text: include_str!("../builtin/config-class/lombok.java.jinja") },
    Builtin { name: "class", extension: "java", starter: false, text: include_str!("../builtin/config-class/class.java.jinja") },
];

// A starter: `psf` and the rest are the built-in abbreviations, and an example offered in the popup
// would be one nobody asked for.
const LIVE: &[Builtin] = &[
    Builtin { name: "example", extension: "java", starter: true, text: include_str!("../builtin/live/example.java.jinja") },
];

/// What a New file template is rendered with.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NewFileContext {
    /// What was typed, without the extension.
    pub name: String,
    /// The package the folder maps to — empty outside a source root.
    pub package: String,
    /// The file being created: the name and the template's extension.
    pub file_name: String,
    /// The folder it is created in.
    pub directory: String,
    /// Today, `YYYY-MM-DD`.
    pub date: String,
    pub year: String,
}

impl NewFileContext {
    pub fn new(directory: &Path, typed: &str, extension: &str) -> Self {
        let typed = typed.trim();
        let suffix = format!(".{}", extension.to_ascii_lowercase());
        let name = match !extension.is_empty() && typed.to_ascii_lowercase().ends_with(&suffix) {
            true => &typed[..typed.len() - suffix.len()],
            false => typed,
        }
        .to_string();
        let file_name = match extension.is_empty() {
            true => name.clone(),
            false => format!("{name}.{extension}"),
        };
        let (date, year) = today();
        Self {
            package: bennu_java::prelude::infer_package(directory).unwrap_or_default(),
            directory: directory.display().to_string().replace('\\', "/"),
            name,
            file_name,
            date,
            year,
        }
    }
}

/// What a template generated from a class is rendered with.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ClassTemplateContext {
    /// The class at the caret.
    pub class: ClassModel,
    /// The file it is in.
    pub file: String,
    /// That file's folder — where a generated file goes by default.
    pub directory: String,
    /// Today, `YYYY-MM-DD`.
    pub date: String,
    pub year: String,
}

impl ClassTemplateContext {
    pub fn new(class: ClassModel, file: &str) -> Self {
        let file = file.replace('\\', "/");
        let directory = file.rsplit_once('/').map(|(dir, _)| dir.to_string()).unwrap_or_default();
        let (date, year) = today();
        Self { class, file, directory, date, year }
    }
}

/// One key a `@ConfigurationProperties` class binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ConfigProperty {
    /// The full key, as Spring binds it: `app.http.client.read-timeout`.
    pub key: String,
    /// The key below the prefix: `client.read-timeout`.
    pub relative_key: String,
    /// The full key's segments — `[0]` marks a list, `<key>` a map key.
    pub segments: Vec<String>,
    /// The Java field it comes from.
    pub field: String,
    /// The simple name of the class declaring that field.
    pub owner: String,
    /// The field's type, as written.
    pub type_name: String,
    pub type_simple: String,
    /// A plausible value for the type (`0`, `false`, `30s`), or empty.
    pub sample: String,
}

impl ConfigProperty {
    pub fn new(key: &str, prefix: &str, field: &str, owner_fqcn: &str, type_name: &str) -> Self {
        let relative_key = key
            .strip_prefix(prefix)
            .map(|rest| rest.trim_start_matches('.'))
            .unwrap_or(key)
            .to_string();
        let simple = type_simple(type_name);
        Self {
            key: key.to_string(),
            relative_key,
            segments: key.split('.').map(str::to_string).collect(),
            field: field.to_string(),
            owner: owner_fqcn.rsplit(['.', '$']).next().unwrap_or(owner_fqcn).to_string(),
            sample: sample_of(&simple),
            type_simple: simple,
            type_name: type_name.to_string(),
        }
    }
}

fn sample_of(type_simple: &str) -> String {
    match type_simple {
        "boolean" | "Boolean" => "false",
        "int" | "Integer" | "long" | "Long" | "short" | "Short" | "byte" | "Byte" | "double" | "Double"
        | "float" | "Float" | "BigDecimal" | "BigInteger" => "0",
        "Duration" => "30s",
        "DataSize" => "10MB",
        "Period" => "1d",
        "Charset" => "UTF-8",
        "Locale" => "en",
        _ => "",
    }
    .to_string()
}

/// One line of the YAML a set of keys makes, with its nesting worked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct YamlLine {
    /// The whitespace before the key — `- ` included for the first key of a list element.
    pub indent: String,
    pub key: String,
    /// A key with a value, rather than one with keys under it.
    pub leaf: bool,
    /// The first key of a list element.
    pub item: bool,
    /// The sample value, for a leaf.
    pub sample: String,
    /// The Java type, for a leaf.
    pub type_name: String,
}

/// What a configuration properties template is rendered with.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConfigPropertiesContext {
    /// The class's prefix (`app.http`).
    pub prefix: String,
    pub class_name: String,
    pub class_fqcn: String,
    /// Every key the class binds, nested classes included, in key order.
    pub properties: Vec<ConfigProperty>,
    /// The same keys as YAML lines.
    pub lines: Vec<YamlLine>,
    /// Today, `YYYY-MM-DD`.
    pub date: String,
    pub year: String,
}

impl ConfigPropertiesContext {
    pub fn new(prefix: &str, class_name: &str, class_fqcn: &str, mut properties: Vec<ConfigProperty>) -> Self {
        properties.sort_by(|a, b| a.segments.cmp(&b.segments));
        properties.dedup_by(|a, b| a.key == b.key);
        let lines = yaml_lines(&properties);
        let (date, year) = today();
        Self {
            prefix: prefix.to_string(),
            class_name: class_name.to_string(),
            class_fqcn: class_fqcn.to_string(),
            properties,
            lines,
            date,
            year,
        }
    }
}

/// Keys, sorted by segment, as nested YAML: a shared prefix is written once, and a `[0]` segment is a
/// list whose element's first key carries the `- `.
fn yaml_lines(properties: &[ConfigProperty]) -> Vec<YamlLine> {
    let mut lines = Vec::new();
    let mut previous: &[String] = &[];
    for property in properties {
        let segments = property.segments.as_slice();
        let common = previous.iter().zip(segments).take_while(|(a, b)| a == b).count();
        let mut column = 0usize;
        let mut parent_sequence = false;
        let mut parent_emitted = false;
        for (i, segment) in segments.iter().enumerate() {
            let (key, sequence) = match segment.strip_suffix("[0]") {
                Some(key) => (key, true),
                None => (segment.as_str(), false),
            };
            let emitted = i >= common;
            if emitted {
                let item = parent_sequence && parent_emitted;
                let leaf = i + 1 == segments.len();
                lines.push(YamlLine {
                    indent: match item {
                        true => format!("{}- ", " ".repeat(column.saturating_sub(2))),
                        false => " ".repeat(column),
                    },
                    key: key.to_string(),
                    leaf,
                    item,
                    sample: if leaf { property.sample.clone() } else { String::new() },
                    type_name: if leaf { property.type_name.clone() } else { String::new() },
                });
            }
            column += if sequence { 4 } else { 2 };
            parent_sequence = sequence;
            parent_emitted = emitted;
        }
        previous = segments;
    }
    lines
}

/// What an abbreviation is rendered with, before its snippet stops are read.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LiveContext {
    /// The file the abbreviation is typed in, without its extension (`OrderService`).
    pub file_name: String,
    /// The class that file declares — its name, which Java makes the same.
    pub class_name: String,
    /// The file's package.
    pub package: String,
    /// Today, `YYYY-MM-DD`.
    pub date: String,
    pub year: String,
}

impl LiveContext {
    pub fn new(file: &str, source: &str) -> Self {
        let file_name = file.rsplit(['/', '\\']).next().unwrap_or(file);
        let stem = file_name.split('.').next().unwrap_or(file_name).to_string();
        let package = source
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix("package "))
            .map(|rest| rest.trim_end_matches(';').trim().to_string())
            .unwrap_or_default();
        let (date, year) = today();
        Self { file_name: stem.clone(), class_name: stem, package, date, year }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render;
    use crate::model::class_at;

    const ORDER: &str = "package com.example;\n\
        @jakarta.persistence.Entity\n\
        public class Order {\n\
            @jakarta.persistence.Id private Long id;\n\
            private String customerName;\n\
            public void setCustomerName(String v) {}\n\
        }\n";

    #[test]
    fn a_new_file_is_named_after_what_was_typed() {
        let context = NewFileContext::new(Path::new("/p/src/main/java/com/example"), "OrderService.java", "java");
        assert_eq!(context.name, "OrderService");
        assert_eq!(context.package, "com.example");
        assert_eq!(context.file_name, "OrderService.java");
        for template in builtins(TemplateKind::NewFile) {
            let out = render(template.text, &context).unwrap();
            assert!(out.starts_with("package com.example;"), "{}: {out}", template.name);
            assert!(out.contains("public class OrderService"), "{}: {out}", template.name);
        }
    }

    #[test]
    fn the_class_templates_render_with_the_class_at_the_caret() {
        let context = ClassTemplateContext::new(class_at(ORDER, None).unwrap(), "/p/src/main/java/com/example/Order.java");
        assert_eq!(context.directory, "/p/src/main/java/com/example");
        let builder = render(builtins(TemplateKind::Class)[0].text, &context).unwrap();
        assert!(builder.contains("public Builder customerName(String customerName)"), "{builder}");
        assert!(builder.contains("built.setCustomerName(customerName);"), "{builder}");
        let repository = render(builtins(TemplateKind::Class)[1].text, &context).unwrap();
        assert!(repository.contains("JpaRepository<Order, Long>"), "{repository}");
    }

    #[test]
    fn keys_become_nested_yaml_with_lists_as_lists() {
        let properties = vec![
            ConfigProperty::new("app.http.timeout", "app.http", "timeout", "com.example.HttpProperties", "Duration"),
            ConfigProperty::new("app.http.servers[0].port", "app.http", "port", "com.example.Server", "int"),
            ConfigProperty::new("app.http.servers[0].host", "app.http", "host", "com.example.Server", "String"),
        ];
        let context = ConfigPropertiesContext::new("app.http", "HttpProperties", "com.example.HttpProperties", properties);
        assert_eq!(context.properties[0].relative_key, "servers[0].host");
        let yaml = render(builtins(TemplateKind::ConfigProperties)[0].text, &context).unwrap();
        assert_eq!(yaml, "app:\n  http:\n    servers:\n      - host: \n        port: 0\n    timeout: 30s\n");
        let properties = render(builtins(TemplateKind::ConfigProperties)[1].text, &context).unwrap();
        assert_eq!(properties, "app.http.servers[0].host=\napp.http.servers[0].port=0\napp.http.timeout=30s\n");
    }

    #[test]
    fn an_abbreviation_knows_the_class_it_is_typed_in() {
        let context = LiveContext::new("/p/src/main/java/com/example/OrderService.java", "package com.example;\nclass OrderService {}");
        assert_eq!(context.class_name, "OrderService");
        assert_eq!(context.package, "com.example");
        let out = render(builtins(TemplateKind::Live)[0].text, &context).unwrap();
        assert!(out.contains("getLogger(OrderService.class);$0"), "{out}");
    }
}
