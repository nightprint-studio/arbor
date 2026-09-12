//! Which keys the caret or a selection names in a configuration file, and the tree narrowed to them —
//! what a configuration class is written for.
//!
//! The caret names a **prefix**: the line's key and its parents, never a key inside a list element nor the
//! list's own key, since neither can be one. A selection names **keys**: every line it touches, a list's
//! own key standing for the lines of its elements — selecting a list's items selects the list, which a
//! class binds as one field.

use serde::Serialize;

use crate::config_tree::{canonical, ConfigValue};
use crate::config_types::{group, looks_like_map};
use crate::config_yaml::{is_item, split_key, yaml_lines, Line};

/// One key of a group, for choosing which of them a class gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyNode {
    /// Relative to the group: `smtp.auth`.
    pub key: String,
    /// Its own name: `auth`.
    pub name: String,
    pub depth: usize,
    /// Has keys under it: choosing it chooses them.
    pub group: bool,
    /// A group whose entries look like names of one thing — bound as a `Map` unless chosen otherwise.
    pub map: bool,
    /// The value of a single one, `[3]` for a list of three.
    pub sample: String,
}

fn is_properties(file_name: &str) -> bool {
    file_name.to_ascii_lowercase().ends_with(".properties")
}

/// The dotted key the line at `offset` declares, with its parents — the prefix a class would bind.
pub fn key_at(file_name: &str, text: &str, offset: usize) -> Option<String> {
    if is_properties(file_name) {
        let start = text[..offset.min(text.len())].rfind('\n').map_or(0, |n| n + 1);
        return properties_key(text[start..].lines().next()?);
    }
    let lines = yaml_lines(text);
    let caret = lines.iter().rposition(|l| l.start <= offset)?;
    yaml_path(&lines, caret, false)
}

/// Every key the lines between `start` and `end` declare, each once, in the order written.
pub fn keys_in(file_name: &str, text: &str, start: usize, end: usize) -> Vec<String> {
    let end = end.max(start);
    let mut out: Vec<String> = Vec::new();
    if is_properties(file_name) {
        let mut offset = 0;
        for line in text.split('\n') {
            let line_end = offset + line.len();
            if line_end > start && offset < end {
                add(&mut out, properties_key(line));
            }
            offset = line_end + 1;
        }
        return out;
    }
    let lines = yaml_lines(text);
    let Some(first) = lines.iter().position(|l| l.start + l.indent + l.text.len() > start) else { return out };
    for (index, line) in lines.iter().enumerate().skip(first) {
        // A selection that ends at the start of a line — the usual one, made by whole lines — leaves it out.
        if index > first && line.start >= end {
            break;
        }
        add(&mut out, yaml_path(&lines, index, true));
    }
    out
}

fn add(out: &mut Vec<String>, key: Option<String>) {
    if let Some(key) = key.filter(|key| !out.contains(key)) {
        out.push(key);
    }
}

fn properties_key(line: &str) -> Option<String> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
        return None;
    }
    let key = line.split(['=', ':']).next()?.trim();
    let key = key.split('[').next()?.trim_end_matches('.');
    (!key.is_empty()).then(|| key.to_string())
}

/// The full key of the line at `index`. Inside a list element, `keep_list` names the list — else nothing
/// below the list's parent.
fn yaml_path(lines: &[Line<'_>], index: usize, keep_list: bool) -> Option<String> {
    let mut path: Vec<String> = Vec::new();
    let mut indent = usize::MAX;
    let mut skip_list_key = false;
    for line in lines[..=index].iter().rev() {
        if line.indent >= indent {
            continue;
        }
        if is_item(line.text) {
            path.clear();
            skip_list_key = !keep_list;
            indent = line.indent + 1;
            continue;
        }
        let Some((key, _)) = split_key(line.text) else { continue };
        indent = line.indent;
        if skip_list_key {
            skip_list_key = false;
            continue;
        }
        for segment in key.split('.').rev() {
            path.push(segment.to_string());
        }
    }
    path.reverse();
    (!path.is_empty()).then(|| path.join("."))
}

/// The deepest group of keys every one of `keys` is in — the prefix a class for them binds. `None` when
/// they share no group: a class binds one prefix.
pub fn common_group(tree: &ConfigValue, keys: &[String]) -> Option<String> {
    let first: Vec<&str> = keys.first()?.split('.').collect();
    let mut length = first.len();
    for key in &keys[1..] {
        let segments: Vec<&str> = key.split('.').collect();
        length = length.min(first.iter().zip(&segments).take_while(|(a, b)| canonical(a) == canonical(b)).count());
    }
    (1..=length)
        .rev()
        .map(|n| first[..n].join("."))
        .find(|prefix| matches!(tree.at(prefix), Some(ConfigValue::Map(entries)) if !entries.is_empty()))
}

/// `keys` as keys relative to `prefix` — the empty key for the prefix itself, which stands for all of it.
pub fn relative_to(prefix: &str, keys: &[String]) -> Vec<String> {
    let prefix: Vec<&str> = prefix.split('.').collect();
    keys.iter()
        .filter_map(|key| {
            let segments: Vec<&str> = key.split('.').collect();
            let inside = segments.len() >= prefix.len()
                && segments.iter().zip(&prefix).all(|(a, b)| canonical(a) == canonical(b));
            inside.then(|| segments[prefix.len()..].join("."))
        })
        .collect()
}

/// The part of `value` that `keys` name — each relative to it, with everything below it. An empty key is
/// all of it.
pub fn narrow(value: &ConfigValue, keys: &[String]) -> ConfigValue {
    if keys.iter().any(|k| k.is_empty()) {
        return value.clone();
    }
    let ConfigValue::Map(entries) = value else { return value.clone() };
    ConfigValue::Map(
        entries
            .iter()
            .filter_map(|(key, child)| {
                let below: Vec<String> = keys
                    .iter()
                    .filter_map(|k| {
                        let (head, rest) = k.split_once('.').unwrap_or((k.as_str(), ""));
                        (canonical(head) == canonical(key)).then(|| rest.to_string())
                    })
                    .collect();
                (!below.is_empty()).then(|| (key.clone(), narrow(child, &below)))
            })
            .collect(),
    )
}

/// Every key under a group, each parent before its children. A list is one key: a class binds it whole.
pub fn key_nodes(value: &ConfigValue) -> Vec<KeyNode> {
    let mut out = Vec::new();
    collect_nodes(value, "", 0, &mut out);
    out
}

fn collect_nodes(value: &ConfigValue, base: &str, depth: usize, out: &mut Vec<KeyNode>) {
    let ConfigValue::Map(entries) = value else { return };
    for (name, child) in entries {
        let key = if base.is_empty() { name.clone() } else { format!("{base}.{name}") };
        let (is_group, map, sample) = match child {
            ConfigValue::Map(children) => (!children.is_empty(), looks_like_map(&group(&[children.as_slice()])), String::new()),
            ConfigValue::List(items) => (false, false, format!("[{}]", items.len())),
            ConfigValue::Scalar { text, .. } => (false, false, text.clone()),
        };
        out.push(KeyNode { key: key.clone(), name: name.clone(), depth, group: is_group, map, sample });
        if is_group {
            collect_nodes(child, &key, depth + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_yaml::read_yaml;

    const YAML: &str = "\
app:
  mail:
    host: smtp.example.com
    port: 587
    servers:
      - url: https://one.example.com
        timeout: 30s
    smtp:
      auth: true
spring.application.name: demo
";

    #[test]
    fn the_key_at_the_caret_leaves_lists_out() {
        let at = |needle: &str| key_at("application.yml", YAML, YAML.find(needle).unwrap());
        assert_eq!(at("host:").as_deref(), Some("app.mail.host"));
        assert_eq!(at("mail:").as_deref(), Some("app.mail"));
        assert_eq!(at("timeout:").as_deref(), Some("app.mail"), "inside a list element");
        assert_eq!(at("name: demo").as_deref(), Some("spring.application.name"));
        assert_eq!(key_at("application.properties", "app.mail.host=x\n", 3).as_deref(), Some("app.mail.host"));
    }

    #[test]
    fn a_selection_names_the_keys_on_its_lines_and_a_list_for_its_elements() {
        let whole_lines = keys_in("application.yml", YAML, YAML.find("host:").unwrap(), YAML.find("    servers:").unwrap());
        assert_eq!(whole_lines, ["app.mail.host", "app.mail.port"]);
        let items = keys_in("application.yml", YAML, YAML.find("url:").unwrap(), YAML.find("    smtp:").unwrap());
        assert_eq!(items, ["app.mail.servers"]);
        let props = "a.b=1\na.c=2\nx.y=3\n";
        assert_eq!(keys_in("application.properties", props, 0, props.find("x.y").unwrap()), ["a.b", "a.c"]);
    }

    #[test]
    fn the_group_the_keys_share_is_the_prefix_and_the_tree_narrows_to_them() {
        let tree = read_yaml(YAML);
        let keys = vec!["app.mail.host".to_string(), "app.mail.smtp.auth".to_string()];
        let prefix = common_group(&tree, &keys).unwrap();
        assert_eq!(prefix, "app.mail");
        let relative = relative_to(&prefix, &keys);
        assert_eq!(relative, ["host", "smtp.auth"]);
        let narrowed = narrow(tree.at(&prefix).unwrap(), &relative);
        let nodes: Vec<String> = key_nodes(&narrowed).into_iter().map(|n| n.key).collect();
        assert_eq!(nodes, ["host", "smtp", "smtp.auth"]);
        assert_eq!(common_group(&tree, &["app.mail".to_string()]).as_deref(), Some("app.mail"), "a group is its own");
        let apart = ["app.mail.host".to_string(), "spring.application.name".to_string()];
        assert_eq!(common_group(&tree, &apart), None, "a class binds one prefix");
    }
}
