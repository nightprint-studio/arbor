//! A `.properties` file read as the same tree its YAML would be — `servers[0].url` building a list.

use crate::config_tree::{canonical, ConfigValue};

pub fn read_properties(text: &str) -> ConfigValue {
    let mut root = ConfigValue::Map(Vec::new());
    let mut pending = String::new();
    for raw in text.lines() {
        let line = raw.trim_start();
        if pending.is_empty() && (line.is_empty() || line.starts_with('#') || line.starts_with('!')) {
            continue;
        }
        match line.strip_suffix('\\') {
            Some(continued) => {
                pending.push_str(continued);
                continue;
            }
            None => pending.push_str(line),
        }
        let entry = std::mem::take(&mut pending);
        let split = entry.find(['=', ':']).or_else(|| entry.find(char::is_whitespace));
        let (key, value) = match split {
            Some(at) => (entry[..at].trim(), entry[at + 1..].trim()),
            None => (entry.trim(), ""),
        };
        if !key.is_empty() {
            insert(&mut root, &segments(key), ConfigValue::scalar(value));
        }
    }
    root
}

enum Segment {
    Key(String),
    Index(usize),
}

/// `servers[0].url` → `servers`, `0`, `url`.
fn segments(key: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    for part in key.split('.') {
        let mut rest = part;
        if let Some(open) = rest.find('[') {
            if open > 0 {
                out.push(Segment::Key(rest[..open].to_string()));
            }
            rest = &rest[open..];
            while let Some(inner) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
                out.push(match inner.0.parse::<usize>() {
                    Ok(index) => Segment::Index(index),
                    Err(_) => Segment::Key(inner.0.to_string()),
                });
                rest = inner.1;
            }
        } else if !rest.is_empty() {
            out.push(Segment::Key(rest.to_string()));
        }
    }
    out
}

fn insert(node: &mut ConfigValue, path: &[Segment], value: ConfigValue) {
    let Some((first, rest)) = path.split_first() else {
        *node = value;
        return;
    };
    match first {
        Segment::Key(key) => {
            if !matches!(node, ConfigValue::Map(_)) {
                *node = ConfigValue::Map(Vec::new());
            }
            let ConfigValue::Map(entries) = node else { unreachable!() };
            let at = match entries.iter().position(|(k, _)| canonical(k) == canonical(key)) {
                Some(at) => at,
                None => {
                    entries.push((key.clone(), ConfigValue::empty()));
                    entries.len() - 1
                }
            };
            insert(&mut entries[at].1, rest, value);
        }
        Segment::Index(index) => {
            if !matches!(node, ConfigValue::List(_)) {
                *node = ConfigValue::List(Vec::new());
            }
            let ConfigValue::List(items) = node else { unreachable!() };
            while items.len() <= *index {
                items.push(ConfigValue::empty());
            }
            insert(&mut items[*index], rest, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_properties_file_builds_the_same_tree_lists_included() {
        let tree = read_properties("app.mail.host=smtp.example.com\napp.mail.servers[1].url=b\napp.mail.servers[0].url=a\n# no\n");
        assert_eq!(tree.at("app.mail.host").unwrap().text(), "smtp.example.com");
        let Some(ConfigValue::List(servers)) = tree.at("app.mail.servers") else { panic!("{tree:?}") };
        assert_eq!(servers[0].at("url").unwrap().text(), "a");
        assert_eq!(servers[1].at("url").unwrap().text(), "b");
    }
}
