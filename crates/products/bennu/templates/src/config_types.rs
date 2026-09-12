//! The type of each key, read off its value — the half of a configuration class that is a decision.
//!
//! `true` is a `Boolean`, `587` an `Integer`, `30s` a `Duration`, `10MB` a `DataSize`, a list a `List` of
//! what its elements are, and a group of keys a nested type named after its key. Boxed types throughout:
//! a key a profile does not set binds to `null`, which says *not configured* where a `0` would claim a
//! value. A placeholder is typed by its default — `${PORT:8080}` is an `Integer` — and is text without one.
//!
//! Every element of a list is read, not the first: `weight: 1` in one and `weight: 2.5` in the next is a
//! `Double`. And a group whose keys cannot be names — `404`, `a.b` — is a `Map<String, …>` rather than a
//! type with fields nobody could write.

use std::collections::{BTreeMap, BTreeSet};

use crate::config_class::{ConfigField, ConfigType};
use crate::config_tree::{canonical, ConfigValue};
use crate::names::{camel, pascal};

const DURATION: &str = "java.time.Duration";
const DATA_SIZE: &str = "org.springframework.util.unit.DataSize";
const LIST: &str = "java.util.List";
const MAP: &str = "java.util.Map";

/// Names a field cannot have.
const KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue",
    "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if",
    "implements", "import", "instanceof", "int", "interface", "long", "native", "new", "package", "private",
    "protected", "public", "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
    "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false", "null",
];

/// Each key once, with every value it has — one for a group of keys, one per element for a list of them.
pub(crate) type Grouped<'a> = Vec<(String, Vec<&'a ConfigValue>)>;

pub(crate) fn group<'a>(maps: &[&'a [(String, ConfigValue)]]) -> Grouped<'a> {
    let mut out: Grouped<'a> = Vec::new();
    for entries in maps {
        for (key, value) in entries.iter() {
            match out.iter_mut().find(|(k, _)| canonical(k) == canonical(key)) {
                Some((_, values)) => values.push(value),
                None => out.push((key.clone(), vec![value])),
            }
        }
    }
    out
}

fn maps_in<'a>(values: &[&'a ConfigValue]) -> Vec<&'a [(String, ConfigValue)]> {
    values
        .iter()
        .filter_map(|v| match v {
            ConfigValue::Map(entries) => Some(entries.as_slice()),
            _ => None,
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct Builder {
    pub(crate) imports: BTreeSet<String>,
    pub(crate) notes: Vec<String>,
    /// The shape chosen for a group, by its key below the prefix: `true` a `Map`, `false` a type per key.
    pub(crate) shapes: BTreeMap<String, bool>,
}

impl Builder {
    /// A type for a group of keys. `enclosing` are the types around it, whose names Java does not let a
    /// nested type repeat.
    pub(crate) fn object(
        &mut self,
        name: String,
        path: &str,
        full: &str,
        depth: usize,
        entries: &Grouped<'_>,
        enclosing: &[String],
    ) -> ConfigType {
        let mut around = enclosing.to_vec();
        around.push(name.clone());
        let mut fields = Vec::new();
        let mut types: Vec<ConfigType> = Vec::new();
        for (key, values) in entries {
            let full_key = join(full, key);
            let child_path = join(path, key);
            let field_name = self.field_name(key, &full_key);
            let mut field = ConfigField {
                key: key.clone(),
                full_key: full_key.clone(),
                pascal: pascal(&field_name),
                name: field_name,
                type_name: String::new(),
                list: false,
                map: false,
                nested: false,
                sample: String::new(),
            };
            let maps = maps_in(values);
            let items: Vec<&ConfigValue> = values
                .iter()
                .filter_map(|v| match v {
                    ConfigValue::List(items) => Some(items.iter()),
                    _ => None,
                })
                .flatten()
                .collect();
            if !maps.is_empty() {
                let grouped = group(&maps);
                let as_map = self.shapes.get(&child_path).copied().unwrap_or_else(|| looks_like_map(&grouped));
                let entries: Vec<&[(String, ConfigValue)]> = grouped.iter().flat_map(|(_, values)| maps_in(values)).collect();
                let every_entry_a_group = entries.len() == grouped.iter().map(|(_, values)| values.len()).sum::<usize>();
                if as_map && !entries.is_empty() && every_entry_a_group {
                    // One type for every entry, keyed by its name: `postgres1`, `postgres2` → `Map<String, …>`.
                    let type_name = unique(pascal(&singular(key)), &around, &types);
                    let element_key = format!("{full_key}.*");
                    let nested = self.object(type_name.clone(), &child_path, &element_key, depth + 1, &group(&entries), &around);
                    types.push(nested);
                    self.imports.insert(MAP.into());
                    field.map = true;
                    field.nested = true;
                    field.type_name = format!("Map<String, {type_name}>");
                } else if as_map || !names_fields(&grouped) {
                    field.map = true;
                    field.type_name = self.map_of(&grouped);
                } else {
                    let type_name = unique(pascal(key), &around, &types);
                    let nested = self.object(type_name.clone(), &child_path, &full_key, depth + 1, &grouped, &around);
                    types.push(nested);
                    field.nested = true;
                    field.type_name = type_name;
                }
            } else if values.iter().any(|v| matches!(v, ConfigValue::List(_))) {
                self.imports.insert(LIST.into());
                field.list = true;
                let element_key = format!("{full_key}[0]");
                let element = self.element(key, &child_path, &element_key, depth, &items, &around, &mut types);
                field.nested = types.iter().any(|t| t.name == element);
                field.type_name = format!("List<{element}>");
            } else {
                field.type_name = self.common(values);
                field.sample = values
                    .iter()
                    .find_map(|v| match v {
                        ConfigValue::Scalar { text, .. } if !text.is_empty() => Some(text.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
            }
            fields.push(field);
        }
        ConfigType { name, path: path.to_string(), depth, indent: "    ".repeat(depth), fields, types }
    }

    /// A list's element type: a nested type for elements that are groups of keys, else what the values share.
    #[allow(clippy::too_many_arguments)]
    fn element(
        &mut self,
        key: &str,
        path: &str,
        full: &str,
        depth: usize,
        items: &[&ConfigValue],
        around: &[String],
        types: &mut Vec<ConfigType>,
    ) -> String {
        let maps = maps_in(items);
        if maps.is_empty() {
            return self.common(items);
        }
        let grouped = group(&maps);
        if !names_fields(&grouped) {
            return self.map_of(&grouped);
        }
        let name = unique(pascal(&singular(key)), around, types);
        let nested = self.object(name.clone(), path, full, depth + 1, &grouped, around);
        types.push(nested);
        name
    }

    fn map_of(&mut self, grouped: &Grouped<'_>) -> String {
        self.imports.insert(MAP.into());
        let values: Vec<&ConfigValue> = grouped.iter().flat_map(|(_, v)| v.iter().copied()).collect();
        format!("Map<String, {}>", self.common(&values))
    }

    /// The one type values share: whole numbers that are sometimes decimals are `Double`, a mix of kinds
    /// is text, and an empty value says nothing.
    fn common(&mut self, values: &[&ConfigValue]) -> String {
        let kinds: BTreeSet<&str> = values
            .iter()
            .filter_map(|v| match v {
                ConfigValue::Scalar { text, quoted } => scalar_kind(text, *quoted),
                _ => Some("String"),
            })
            .collect();
        let kind = if kinds.len() == 1 {
            kinds.iter().next().copied().unwrap_or("String")
        } else if !kinds.is_empty() && kinds.iter().all(|k| matches!(*k, "Integer" | "Long")) {
            "Long"
        } else if !kinds.is_empty() && kinds.iter().all(|k| matches!(*k, "Integer" | "Long" | "Double")) {
            "Double"
        } else {
            "String"
        };
        match kind {
            "Duration" => {
                self.imports.insert(DURATION.into());
            }
            "DataSize" => {
                self.imports.insert(DATA_SIZE.into());
            }
            _ => {}
        }
        kind.to_string()
    }

    fn field_name(&mut self, key: &str, full_key: &str) -> String {
        let name = camel(key);
        let name = match name.chars().next() {
            None => "value".to_string(),
            Some(c) if c.is_ascii_digit() => format!("_{name}"),
            Some(_) => name,
        };
        if KEYWORDS.contains(&name.as_str()) {
            let renamed = format!("{name}Value");
            self.notes.push(format!(
                "`{full_key}` cannot be a field name in Java, so the field is `{renamed}` — which Spring binds to `{renamed}`, not to `{key}`: rename the key, or the field."
            ));
            return renamed;
        }
        name
    }
}

/// What a single value is, in Java. `None` for an empty one, which could be anything.
fn scalar_kind(text: &str, quoted: bool) -> Option<&'static str> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if quoted {
        return Some("String");
    }
    if let Some(inner) = text.strip_prefix("${").and_then(|t| t.strip_suffix('}')) {
        return match inner.split_once(':') {
            Some((_, default)) if !default.is_empty() && !default.contains("${") => scalar_kind(default, false),
            _ => Some("String"),
        };
    }
    if text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false") {
        return Some("Boolean");
    }
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        // `0587` is a code, not a number: the zero would be lost.
        if digits.len() > 1 && digits.starts_with('0') {
            return Some("String");
        }
        return Some(match text.parse::<i64>() {
            Ok(n) if i32::try_from(n).is_ok() => "Integer",
            Ok(_) => "Long",
            Err(_) => "String",
        });
    }
    if let Some((whole, fraction)) = digits.split_once('.') {
        if !whole.is_empty() && !fraction.is_empty() && whole.bytes().chain(fraction.bytes()).all(|b| b.is_ascii_digit()) {
            return Some("Double");
        }
    }
    let unit = text.trim_start_matches(|c: char| c.is_ascii_digit());
    if unit.len() < text.len() && matches!(unit, "ns" | "us" | "ms" | "s" | "m" | "h" | "d") {
        return Some("Duration");
    }
    // `get`, not a slice: a value starting with `é` has no character boundary at byte 2.
    if text.len() > 2
        && text.get(..2).is_some_and(|head| head.eq_ignore_ascii_case("pt"))
        && text.get(2..).is_some_and(|rest| rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'))
    {
        return Some("Duration");
    }
    if unit.len() < text.len() && matches!(unit, "B" | "KB" | "MB" | "GB" | "TB") {
        return Some("DataSize");
    }
    Some("String")
}

/// Whether a group's entries are groups alike enough to be one thing under different names — `postgres1`
/// and `postgres2` with the same keys — which Spring binds as a `Map<String, T>`. Alike: at least half of
/// all the keys they have, every one of them has.
pub(crate) fn looks_like_map(entries: &Grouped<'_>) -> bool {
    if entries.len() < 2 {
        return false;
    }
    let mut sets: Vec<BTreeSet<String>> = Vec::new();
    for (_, values) in entries {
        let maps = maps_in(values);
        if maps.is_empty() || maps.len() != values.len() {
            return false;
        }
        let keys: BTreeSet<String> = maps.iter().flat_map(|m| m.iter().map(|(k, _)| canonical(k))).collect();
        if keys.is_empty() {
            return false;
        }
        sets.push(keys);
    }
    let union: BTreeSet<&String> = sets.iter().flatten().collect();
    let common = sets[1..].iter().fold(sets[0].clone(), |acc, set| acc.intersection(set).cloned().collect());
    common.len() * 2 >= union.len()
}

/// Whether a group's keys can be the names of fields.
fn names_fields(entries: &Grouped<'_>) -> bool {
    !entries.is_empty()
        && entries.iter().all(|(key, _)| {
            key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// `servers` → `server`, `entries` → `entry` — the element type of a list is named for one of them.
fn singular(key: &str) -> String {
    if let Some(stem) = key.strip_suffix("ies").filter(|s| s.len() > 1) {
        return format!("{stem}y");
    }
    match key.len() > 3 && key.ends_with('s') && !key.ends_with("ss") {
        true => key[..key.len() - 1].to_string(),
        false => key.to_string(),
    }
}

fn unique(name: String, around: &[String], siblings: &[ConfigType]) -> String {
    let taken = |candidate: &str| around.iter().any(|n| n == candidate) || siblings.iter().any(|t| t.name == candidate);
    if !taken(&name) {
        return name;
    }
    (2..).map(|i| format!("{name}{i}")).find(|candidate| !taken(candidate)).unwrap_or(name)
}

fn join(base: &str, key: &str) -> String {
    match base.is_empty() {
        true => key.to_string(),
        false => format!("{base}.{key}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_says_its_type_and_an_empty_one_says_nothing() {
        assert_eq!(scalar_kind("587", false), Some("Integer"));
        assert_eq!(scalar_kind("5000000000", false), Some("Long"));
        assert_eq!(scalar_kind("0587", false), Some("String"), "a code, not a number");
        assert_eq!(scalar_kind("587", true), Some("String"), "quoted");
        assert_eq!(scalar_kind("PT30S", false), Some("Duration"));
        assert_eq!(scalar_kind("${X}", false), Some("String"));
        assert_eq!(scalar_kind("émile", false), Some("String"), "no byte boundary at 2");
        assert_eq!(scalar_kind("", false), None);
    }

    #[test]
    fn a_list_element_is_named_for_one_of_them() {
        assert_eq!(singular("servers"), "server");
        assert_eq!(singular("entries"), "entry");
        assert_eq!(singular("address"), "address");
        assert_eq!(singular("ids"), "ids");
    }
}
