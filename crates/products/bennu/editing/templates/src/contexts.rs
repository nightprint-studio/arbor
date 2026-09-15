//! What each kind of template is rendered with — except validation tests, whose context lives with the
//! DTO Lab — and the templates Bennu ships for each.
//!
//! Every field here is something a template can read, and its doc comment is what the editor shows
//! when completing it: the schema is generated from these structs.

use std::path::Path;

use bennu_java::prelude::{postfix_indent_unit, PostfixOptional, PostfixShape};
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
        TemplateKind::Postfix => POSTFIX,
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

// A starter for the same reason: `for`, `nn` and the rest are the built-in postfix templates. Named `logv`
// rather than `log`, because a built-in's name is one the user can no longer give a template of their own,
// and `log` is the one they are likeliest to want.
const POSTFIX: &[Builtin] = &[
    Builtin { name: "logv", extension: "java", starter: true, text: include_str!("../builtin/postfix/logv.java.jinja") },
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

/// What a postfix template is rendered with, before its snippet stops are read: the value before the dot,
/// and the file it is typed in.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PostfixTemplateContext {
    /// The expression before the dot, as written (`repo.findAll()`). What accepting the template replaces.
    pub expr: String,
    /// Its type, as a declaration writes it (`List<Order>`, `int[]`). Empty for a call that returns nothing.
    /// Writing it adds the imports it needs.
    #[serde(rename = "type")]
    pub type_name: String,
    /// The type without its package or arguments (`List`).
    pub type_simple: String,
    /// A name for a variable holding the value (`orders`).
    pub name: String,
    /// What the value holds, for an array, an `Iterable` or an `Optional` (`Order`) — empty otherwise.
    pub element_type: String,
    /// A name for one of those (`order`) — empty when `element_type` is.
    pub element_name: String,
    /// The Java language level of the module the file belongs to (`8`, `17`).
    pub level: u32,
    /// Whether `var` can declare a local here: Java 10 or later.
    pub var: bool,
    /// The file the template is typed in, without its extension (`OrderService`).
    pub file_name: String,
    /// The class that file declares.
    pub class_name: String,
    /// The file's package.
    pub package: String,
    /// Today, `YYYY-MM-DD`.
    pub date: String,
    pub year: String,
    /// One indentation step, as the file writes it — for a template whose text spans lines.
    pub indent: String,
    /// The classes `type` names, to import when the output writes it.
    #[serde(skip)]
    pub type_imports: Vec<String>,
    /// The classes `element_type` names, likewise.
    #[serde(skip)]
    pub element_imports: Vec<String>,
}

impl PostfixTemplateContext {
    /// The context for `expr`, whose type is described by `shape`, typed in `file` at a module of `level`.
    pub fn new(expr: &str, shape: &PostfixShape, level: u32, file: &str, source: &str) -> Self {
        let live = LiveContext::new(file, source);
        let element = element_of(shape);
        Self {
            expr: expr.trim().to_string(),
            type_simple: match shape.ty.text.is_empty() {
                true => String::new(),
                false => type_simple(&shape.ty.text),
            },
            type_name: shape.ty.text.clone(),
            name: shape.name.clone(),
            element_type: element.type_name,
            element_name: element.name,
            level,
            var: level >= 10,
            file_name: live.file_name,
            class_name: live.class_name,
            package: live.package,
            date: live.date,
            year: live.year,
            indent: postfix_indent_unit(source),
            type_imports: shape.ty.imports.clone(),
            element_imports: element.imports,
        }
    }

    /// What a preview renders with, where no value is being typed after: `orders`, a `List<Order>`. The file's
    /// own fields come from `file` when one is open, else from an `OrderService` that is not.
    pub fn sample(file: Option<(&str, &str)>, level: u32) -> Self {
        let live = match file {
            Some((file, source)) => LiveContext::new(file, source),
            None => {
                let (date, year) = today();
                LiveContext {
                    file_name: "OrderService".to_string(),
                    class_name: "OrderService".to_string(),
                    package: "com.example".to_string(),
                    date,
                    year,
                }
            }
        };
        Self {
            expr: "orders".to_string(),
            type_name: "List<Order>".to_string(),
            type_simple: "List".to_string(),
            name: "orders".to_string(),
            element_type: "Order".to_string(),
            element_name: "order".to_string(),
            level,
            var: level >= 10,
            file_name: live.file_name,
            class_name: live.class_name,
            package: live.package,
            date: live.date,
            year: live.year,
            indent: file.map_or_else(|| "    ".to_string(), |(_, source)| postfix_indent_unit(source)),
            type_imports: vec!["java.util.List".to_string()],
            element_imports: vec!["com.example.Order".to_string()],
        }
    }

    /// The imports `text` needs because it writes `type` or `element_type`: each class of theirs whose simple
    /// name `text` uses as a word. Sorted, each once.
    ///
    /// Read off the output rather than asked of the template, because `{{ type }}` is how a template says it
    /// declares one — an `imported` filter around every use would be the template doing Bennu's bookkeeping.
    /// A class the file already imports, or never needs to, is the caller's to skip.
    pub fn implied_imports(&self, text: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .type_imports
            .iter()
            .chain(&self.element_imports)
            .filter(|fqn| names_word(text, fqn.rsplit('.').next().unwrap_or(fqn)))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// The element half of a postfix context.
struct ElementFields {
    type_name: String,
    name: String,
    imports: Vec<String>,
}

/// What an array, an `Iterable` or an `Optional` holds — empty fields for anything else.
fn element_of(shape: &PostfixShape) -> ElementFields {
    let held = shape.array_element.as_ref().or(shape.iterable_element.as_ref()).or(match &shape.optional {
        Some(PostfixOptional::Of(element)) => Some(element),
        _ => None,
    });
    if let Some(element) = held {
        return ElementFields { type_name: element.ty.text.clone(), name: element.name.clone(), imports: element.ty.imports.clone() };
    }
    let primitive = match &shape.optional {
        Some(PostfixOptional::Int) => "int",
        Some(PostfixOptional::Long) => "long",
        Some(PostfixOptional::Double) => "double",
        _ => return ElementFields { type_name: String::new(), name: String::new(), imports: Vec::new() },
    };
    ElementFields { type_name: primitive.to_string(), name: "value".to_string(), imports: Vec::new() }
}

/// Whether `word` occurs in `text` with no identifier character on either side.
fn names_word(text: &str, word: &str) -> bool {
    let identifier = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    !word.is_empty()
        && text.match_indices(word).any(|(at, _)| {
            !text[..at].chars().next_back().is_some_and(identifier)
                && !text[at + word.len()..].chars().next().is_some_and(identifier)
        })
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

    fn list_of_orders() -> PostfixShape {
        use bennu_java::prelude::{PostfixElement, PostfixWritten};
        let written = |text: &str, imports: &[&str]| PostfixWritten {
            text: text.to_string(),
            imports: imports.iter().map(|i| i.to_string()).collect(),
        };
        PostfixShape {
            ty: written("List<Order>", &["com.example.Order", "java.util.List"]),
            name: "orders".to_string(),
            iterable_element: Some(PostfixElement { ty: written("Order", &["com.example.Order"]), name: "order".to_string(), primitive: None }),
            collection: true,
            ..PostfixShape::default()
        }
    }

    const ORDER_SERVICE: (&str, &str) = ("/p/src/main/java/com/example/OrderService.java", "package com.example;\nclass OrderService {}\n");

    #[test]
    fn a_postfix_context_is_the_value_its_type_what_it_holds_and_the_file() {
        let context = PostfixTemplateContext::new(" this.orders ", &list_of_orders(), 17, ORDER_SERVICE.0, ORDER_SERVICE.1);
        assert_eq!(context.expr, "this.orders");
        assert_eq!((context.type_name.as_str(), context.type_simple.as_str()), ("List<Order>", "List"));
        assert_eq!((context.element_type.as_str(), context.element_name.as_str()), ("Order", "order"));
        assert!(context.var, "Java 17 has var");
        assert!(!PostfixTemplateContext::new("orders", &list_of_orders(), 8, ORDER_SERVICE.0, ORDER_SERVICE.1).var);
        assert_eq!((context.class_name.as_str(), context.package.as_str()), ("OrderService", "com.example"));
        // `type` is the name a template reads it by, whatever the field is called in Rust.
        assert_eq!(render("{{ type }} {{ element_type }} {{ name }}", &context).unwrap(), "List<Order> Order orders");
    }

    #[test]
    fn a_primitive_optional_holds_a_value_of_its_primitive() {
        let shape = PostfixShape { optional: Some(PostfixOptional::Long), ..PostfixShape::default() };
        let context = PostfixTemplateContext::new("total", &shape, 11, ORDER_SERVICE.0, ORDER_SERVICE.1);
        assert_eq!((context.element_type.as_str(), context.element_name.as_str()), ("long", "value"));
        let plain = PostfixTemplateContext::new("flag", &PostfixShape::default(), 11, ORDER_SERVICE.0, ORDER_SERVICE.1);
        assert_eq!((plain.element_type.as_str(), plain.element_name.as_str()), ("", ""));
    }

    #[test]
    fn the_postfix_starter_logs_the_expression_by_its_text_and_its_value() {
        let context = PostfixTemplateContext::new("repo.count()", &PostfixShape::default(), 17, ORDER_SERVICE.0, ORDER_SERVICE.1);
        let out = render(builtins(TemplateKind::Postfix)[0].text, &context).unwrap();
        assert_eq!(out.trim_end(), "log.debug(\"repo.count() = {}\", repo.count());$0");
    }

    /// Only what the output writes is imported: a template that logs a list never declares a `List`.
    #[test]
    fn a_postfix_output_imports_the_types_it_writes_and_no_other() {
        let context = PostfixTemplateContext::new("orders", &list_of_orders(), 17, ORDER_SERVICE.0, ORDER_SERVICE.1);
        assert_eq!(context.implied_imports("for (Order order : orders) {}"), ["com.example.Order"]);
        assert_eq!(context.implied_imports("List<Order> copy = orders;"), ["com.example.Order", "java.util.List"]);
        assert!(context.implied_imports("log.debug(\"{}\", orders.size()); // OrderList").is_empty(), "a word, not a substring");
    }

    #[test]
    fn a_postfix_preview_renders_a_list_of_orders_in_the_open_file_or_a_sample_one() {
        let sample = PostfixTemplateContext::sample(None, 21);
        assert_eq!((sample.expr.as_str(), sample.type_name.as_str(), sample.element_type.as_str()), ("orders", "List<Order>", "Order"));
        assert_eq!(sample.class_name, "OrderService");
        let open = PostfixTemplateContext::sample(Some(("/p/src/Invoice.java", "package billing;\nclass Invoice {}\n")), 8);
        assert_eq!((open.class_name.as_str(), open.package.as_str(), open.var), ("Invoice", "billing", false));
    }
}
