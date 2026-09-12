//! A `@ConfigurationProperties` class written from the keys under a prefix — the other direction of
//! [`crate::contexts::ConfigPropertiesContext`]. How each key's type is read off its value is
//! [`crate::config_types`]; which keys a selection names, [`crate::config_keys`].

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;

use crate::config_tree::ConfigValue;
use crate::config_types::{group, looks_like_map, Builder};
use crate::dates::today;
use crate::names::pascal;

/// What a configuration class template is rendered with.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConfigClassContext {
    /// The prefix the class binds (`app.mail`).
    pub prefix: String,
    pub class_name: String,
    /// The package the target folder maps to — empty outside a source root.
    pub package: String,
    /// What the fields' types need imported, sorted.
    pub imports: Vec<String>,
    /// The class: its fields, and its nested types — each the same shape — at any depth.
    pub root: ConfigType,
    /// The files the keys were read from.
    pub sources: Vec<String>,
    /// What is worth knowing about the result.
    pub notes: Vec<String>,
    /// Today, `YYYY-MM-DD`.
    pub date: String,
    pub year: String,
}

/// The class, or one of the types nested in it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConfigType {
    pub name: String,
    /// The key below the prefix that leads here (`smtp`, `servers`); empty for the class.
    pub path: String,
    /// `0` for the class, `1` for a type nested in it.
    pub depth: usize,
    /// Four spaces a level — what a nested type's lines start with.
    pub indent: String,
    pub fields: Vec<ConfigField>,
    /// The types nested in this one.
    pub types: Vec<ConfigType>,
}

/// One key, as a field.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConfigField {
    /// The key as written (`read-timeout`).
    pub key: String,
    /// The full key (`app.mail.read-timeout`; `[0]` inside a list).
    pub full_key: String,
    /// The Java name (`readTimeout`).
    pub name: String,
    /// `ReadTimeout` — for `getReadTimeout`.
    pub pascal: String,
    /// The type as written in the class: `Duration`, `List<Server>`, `Map<String, String>`.
    pub type_name: String,
    pub list: bool,
    pub map: bool,
    /// Its type — or its list's element — is one of the nested types.
    pub nested: bool,
    /// The value it was read with, for a single value.
    pub sample: String,
}

impl ConfigClassContext {
    /// `value` is what the configuration holds under `prefix`, every file merged. `maps` is the shape chosen
    /// for a group, by its key below the prefix — `true` a `Map`, `false` a type per key; a group not named
    /// there is a `Map` when its entries look like one.
    pub fn new(
        prefix: &str,
        class_name: &str,
        package: &str,
        value: &ConfigValue,
        sources: Vec<String>,
        maps: &BTreeMap<String, bool>,
    ) -> Result<Self, String> {
        let ConfigValue::Map(entries) = value else {
            return Err(format!("`{prefix}` is a single value, not a group of keys — choose the key above it"));
        };
        if entries.is_empty() {
            return Err(format!("There are no keys under `{prefix}`"));
        }
        let mut builder = Builder { shapes: maps.clone(), ..Builder::default() };
        let grouped = group(&[entries.as_slice()]);
        if looks_like_map(&grouped) {
            // A class cannot itself be the map: the prefix above it can bind this group as one.
            let names: Vec<String> = grouped.iter().take(3).map(|(k, _)| format!("`{k}`")).collect();
            let above = match prefix.rsplit_once('.') {
                Some((parent, _)) => format!("choose `{parent}` as the prefix"),
                None => "choose the key above it".to_string(),
            };
            builder.notes.push(format!(
                "The keys under `{prefix}` — {} — look like names of one thing: to bind them as a `Map`, {above}.",
                names.join(", ")
            ));
        }
        let root = builder.object(class_name.to_string(), "", prefix, 0, &grouped, &[]);
        let (date, year) = today();
        Ok(Self {
            prefix: prefix.to_string(),
            class_name: class_name.to_string(),
            package: package.to_string(),
            imports: builder.imports.into_iter().collect(),
            root,
            sources,
            notes: builder.notes,
            date,
            year,
        })
    }
}

/// The class name a prefix suggests: `app.mail` → `MailProperties`.
pub fn class_name_for(prefix: &str) -> String {
    format!("{}Properties", pascal(prefix.rsplit('.').next().unwrap_or(prefix)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_yaml::read_yaml;
    use crate::contexts::builtins;
    use crate::engine::render;
    use crate::kind::TemplateKind;

    const YAML: &str = "\
app:
  mail:
    host: smtp.example.com
    port: 587
    read-timeout: 30s
    max-size: 10MB
    debug: ${MAIL_DEBUG:false}
    recipients: [a@example.com, b@example.com]
    smtp:
      auth: true
    servers:
      - url: https://one.example.com
        weight: 1
      - url: https://two.example.com
        weight: 2.5
    codes:
      404: not-found
      500: error
    default: x
";

    fn context() -> ConfigClassContext {
        let tree = read_yaml(YAML);
        ConfigClassContext::new(
            "app.mail",
            "MailProperties",
            "com.example.config",
            tree.at("app.mail").unwrap(),
            vec!["application.yml".into()],
            &BTreeMap::new(),
        )
        .unwrap()
    }

    fn field<'a>(t: &'a ConfigType, name: &str) -> &'a ConfigField {
        t.fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no {name}: {:?}", t.fields))
    }

    #[test]
    fn a_type_is_read_off_each_value() {
        let c = context();
        assert_eq!(field(&c.root, "host").type_name, "String");
        assert_eq!(field(&c.root, "port").type_name, "Integer");
        assert_eq!(field(&c.root, "readTimeout").type_name, "Duration");
        assert_eq!(field(&c.root, "maxSize").type_name, "DataSize");
        assert_eq!(field(&c.root, "debug").type_name, "Boolean", "a placeholder is typed by its default");
        assert_eq!(field(&c.root, "recipients").type_name, "List<String>");
        assert_eq!(field(&c.root, "codes").type_name, "Map<String, String>", "keys that cannot be names");
        assert_eq!(
            c.imports,
            ["java.time.Duration", "java.util.List", "java.util.Map", "org.springframework.util.unit.DataSize"]
        );
    }

    #[test]
    fn a_group_of_keys_is_a_nested_type_and_every_list_element_is_read() {
        let c = context();
        assert_eq!(field(&c.root, "smtp").type_name, "Smtp");
        assert_eq!(field(&c.root, "servers").type_name, "List<Server>");
        let server = c.root.types.iter().find(|t| t.name == "Server").unwrap();
        assert_eq!(field(server, "weight").type_name, "Double", "1 in one element, 2.5 in the next");
        assert_eq!((server.depth, server.indent.as_str()), (1, "    "));
    }

    #[test]
    fn a_keyword_is_renamed_and_said_so() {
        let c = context();
        assert_eq!(field(&c.root, "defaultValue").key, "default");
        assert!(c.notes.iter().any(|n| n.contains("app.mail.default")), "{:?}", c.notes);
    }

    #[test]
    fn a_single_value_is_not_a_class() {
        let tree = read_yaml(YAML);
        let host = tree.at("app.mail.host").unwrap();
        assert!(ConfigClassContext::new("app.mail.host", "X", "", host, vec![], &BTreeMap::new()).is_err());
    }

    #[test]
    fn every_built_in_writes_the_nested_types_inside_the_class() {
        let c = context();
        for builtin in builtins(TemplateKind::ConfigClass) {
            let out = render(builtin.text, &c).unwrap_or_else(|e| panic!("{}: {e}", builtin.name));
            let name = builtin.name;
            assert!(out.starts_with("package com.example.config;\n"), "{name}:\n{out}");
            assert!(out.contains("@ConfigurationProperties(prefix = \"app.mail\")"), "{name}:\n{out}");
            assert!(out.contains("import java.time.Duration;"), "{name}:\n{out}");
            let class_at = out.find("MailProperties").unwrap();
            let nested_at = out.find(" Server").unwrap_or_else(|| panic!("{name}:\n{out}"));
            assert!(nested_at > class_at, "{name}:\n{out}");
            assert_eq!(out.matches('{').count(), out.matches('}').count(), "{name}:\n{out}");
            assert!(out.trim_end().ends_with('}'), "{name}:\n{out}");
        }
    }

    #[test]
    fn the_record_template_separates_components_and_nests_records() {
        let c = context();
        let record = builtins(TemplateKind::ConfigClass).iter().find(|b| b.name == "record").unwrap();
        let out = render(record.text, &c).unwrap();
        assert!(out.contains("        String host,\n"), "{out}");
        assert!(out.contains("    public record Smtp(\n            Boolean auth\n    ) {\n    }\n"), "{out}");
    }

    /// `postgres1` and `postgres2` alike are one type under two names: a `Map`, unless chosen otherwise.
    #[test]
    fn groups_alike_are_a_map_of_one_type_and_the_choice_can_say_otherwise() {
        let yaml = "app:\n  datasources:\n    postgres1:\n      url: a\n      user: x\n    postgres2:\n      url: b\n      user: y\n";
        let tree = read_yaml(yaml);
        let value = tree.at("app").unwrap();
        let alike = ConfigClassContext::new("app", "AppProperties", "", value, vec![], &BTreeMap::new()).unwrap();
        assert_eq!(field(&alike.root, "datasources").type_name, "Map<String, Datasource>");
        assert_eq!(alike.root.types.len(), 1);
        let typed = BTreeMap::from([("datasources".to_string(), false)]);
        let apart = ConfigClassContext::new("app", "AppProperties", "", value, vec![], &typed).unwrap();
        assert_eq!(field(&apart.root, "datasources").type_name, "Datasources");
        let under = ConfigClassContext::new("app.datasources", "X", "", tree.at("app.datasources").unwrap(), vec![], &BTreeMap::new()).unwrap();
        assert!(under.notes.iter().any(|n| n.contains("choose `app`")), "{:?}", under.notes);
    }

    #[test]
    fn a_prefix_suggests_its_class_name() {
        assert_eq!(class_name_for("app.mail"), "MailProperties");
        assert_eq!(class_name_for("app.http-client"), "HttpClientProperties");
    }
}
