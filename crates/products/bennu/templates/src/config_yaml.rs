//! YAML read as the subset configuration files are written in — block mappings and sequences, `[a, b]` and
//! `{a: 1}` on one line, quoted scalars, `|` / `>` block scalars, comments, and `---` between documents
//! (merged, since a profile document still declares keys). `spring.mail.host: x` written as one key is the
//! same tree as the three nested ones.

use crate::config_tree::ConfigValue;

#[derive(Clone, Copy)]
pub(crate) struct Line<'a> {
    pub(crate) indent: usize,
    pub(crate) text: &'a str,
    /// Byte offset of the line's first character in the file.
    pub(crate) start: usize,
}

pub(crate) fn yaml_lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    for raw in text.split('\n') {
        let line_start = start;
        start += raw.len() + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let content = strip_comment(line);
        let trimmed = content.trim_start();
        if trimmed.trim().is_empty() || trimmed.starts_with("---") || trimmed.starts_with("...") {
            continue;
        }
        out.push(Line { indent: content.len() - trimmed.len(), text: trimmed.trim_end(), start: line_start });
    }
    out
}

/// The line without a `#` comment — one that starts the line or follows a space, outside quotes.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && previous.is_whitespace() => return &line[..i],
            None => {}
        }
        previous = c;
    }
    line
}

pub(crate) fn is_item(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

/// `key: rest` — the colon that ends a key is followed by a space or the end of the line, outside
/// quotes, so `url: http://x` splits once and `- http://x` not at all.
pub(crate) fn split_key(text: &str) -> Option<(String, &str)> {
    let mut quote: Option<char> = None;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if (c == '"' || c == '\'') && i == 0 => quote = Some(c),
            None if c == ':' && (i + 1 == text.len() || bytes[i + 1] == b' ' || bytes[i + 1] == b'\t') => {
                let key = text[..i].trim().trim_matches(['"', '\'']).to_string();
                return (!key.is_empty() && !key.starts_with('{') && !key.starts_with('['))
                    .then(|| (key, text[i + 1..].trim()));
            }
            None => {}
        }
    }
    None
}

pub fn read_yaml(text: &str) -> ConfigValue {
    let mut lines = yaml_lines(text);
    let mut root = ConfigValue::Map(Vec::new());
    let mut i = 0;
    while i < lines.len() {
        let indent = lines[i].indent;
        let before = i;
        let block = parse_block(&mut lines, &mut i, indent);
        root.merge(expand(block));
        if i == before {
            i += 1;
        }
    }
    root
}

fn parse_block(lines: &mut [Line<'_>], i: &mut usize, indent: usize) -> ConfigValue {
    match is_item(lines[*i].text) {
        true => parse_sequence(lines, i, indent),
        false => parse_mapping(lines, i, indent),
    }
}

fn parse_mapping(lines: &mut [Line<'_>], i: &mut usize, indent: usize) -> ConfigValue {
    let mut entries: Vec<(String, ConfigValue)> = Vec::new();
    while *i < lines.len() {
        let line = lines[*i];
        if line.indent > indent {
            *i += 1; // nothing here owns it: a stray line, skipped rather than looped on
            continue;
        }
        if line.indent < indent || is_item(line.text) {
            break;
        }
        *i += 1;
        let Some((key, rest)) = split_key(line.text) else { continue };
        let value = if rest.is_empty() {
            // Below it, deeper — or a sequence at the key's own indent, which YAML allows.
            match lines.get(*i) {
                Some(next) if next.indent > indent || (next.indent == indent && is_item(next.text)) => {
                    let child = next.indent;
                    parse_block(lines, i, child)
                }
                _ => ConfigValue::empty(),
            }
        } else if rest.starts_with('|') || rest.starts_with('>') {
            let mut parts = Vec::new();
            while *i < lines.len() && lines[*i].indent > indent {
                parts.push(lines[*i].text);
                *i += 1;
            }
            ConfigValue::Scalar { text: parts.join(" "), quoted: true }
        } else {
            inline(rest)
        };
        entries.push((key, value));
    }
    ConfigValue::Map(entries)
}

fn parse_sequence(lines: &mut [Line<'_>], i: &mut usize, indent: usize) -> ConfigValue {
    let mut items = Vec::new();
    while *i < lines.len() && lines[*i].indent == indent && is_item(lines[*i].text) {
        let line = lines[*i];
        let content = line.text[1..].trim_start();
        if content.is_empty() {
            *i += 1;
            match lines.get(*i) {
                Some(next) if next.indent > indent => {
                    let child = next.indent;
                    items.push(parse_block(lines, i, child));
                }
                _ => items.push(ConfigValue::empty()),
            }
        } else if !content.starts_with('[') && !content.starts_with('{') && split_key(content).is_some() {
            // `- name: a` opens a mapping whose keys line up with `name`.
            let column = line.indent + (line.text.len() - content.len());
            lines[*i] = Line { indent: column, text: content, start: line.start };
            items.push(expand(parse_mapping(lines, i, column)));
        } else {
            *i += 1;
            items.push(inline(content));
        }
        while *i < lines.len() && lines[*i].indent > indent {
            *i += 1;
        }
    }
    ConfigValue::List(items)
}

/// A value written on the key's own line: `[a, b]`, `{a: 1}`, or a scalar.
fn inline(rest: &str) -> ConfigValue {
    let rest = rest.trim();
    let rest = match rest.strip_prefix("!!") {
        Some(tagged) => tagged.split_once(' ').map_or("", |(_, value)| value),
        None => rest,
    };
    if let Some(body) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return ConfigValue::List(split_flow(body).into_iter().map(inline).collect());
    }
    if let Some(body) = rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        let entries = split_flow(body)
            .into_iter()
            .filter_map(|pair| {
                let (key, value) = pair.split_once(':')?;
                Some((key.trim().trim_matches(['"', '\'']).to_string(), inline(value)))
            })
            .collect();
        return ConfigValue::Map(entries);
    }
    ConfigValue::scalar(rest)
}

/// Split flow content on the commas at its own level.
fn split_flow(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut quote, mut start) = (0i32, None::<char>, 0usize);
    for (i, c) in body.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(body[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    parts.push(body[start..].trim());
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}

/// `spring.mail.host: x` written as one key is the same tree as the three nested ones.
fn expand(value: ConfigValue) -> ConfigValue {
    match value {
        ConfigValue::Map(entries) => {
            let mut out = ConfigValue::Map(Vec::new());
            for (key, value) in entries {
                let value = expand(value);
                let nested = key.split('.').rev().fold(value, |inner, segment| {
                    ConfigValue::Map(vec![(segment.to_string(), inner)])
                });
                out.merge(nested);
            }
            out
        }
        ConfigValue::List(items) => ConfigValue::List(items.into_iter().map(expand).collect()),
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "\
app:
  mail:
    host: smtp.example.com   # the relay
    port: 587
    from: \"noreply@example.com\"
    recipients: [a@example.com, b@example.com]
    servers:
      - url: https://one.example.com
        timeout: 30s
      - url: https://two.example.com
spring.application.name: demo
---
app:
  mail:
    debug: true
";

    #[test]
    fn a_yaml_file_is_a_tree_with_its_lists_and_every_document() {
        let tree = read_yaml(YAML);
        assert_eq!(tree.at("app.mail.host").unwrap().text(), "smtp.example.com");
        assert_eq!(tree.at("app.mail.port").unwrap().text(), "587");
        assert!(matches!(tree.at("app.mail.from"), Some(ConfigValue::Scalar { quoted: true, .. })));
        assert!(matches!(tree.at("app.mail.recipients"), Some(ConfigValue::List(items)) if items.len() == 2));
        let Some(ConfigValue::List(servers)) = tree.at("app.mail.servers") else { panic!("{tree:?}") };
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].at("timeout").unwrap().text(), "30s");
        assert_eq!(tree.at("app.mail.debug").unwrap().text(), "true", "the second document merges in");
        assert_eq!(tree.at("spring.application.name").unwrap().text(), "demo", "a dotted key is nested");
    }

    #[test]
    fn a_key_is_found_the_way_spring_binds_it() {
        let tree = read_yaml("app:\n  mail-server:\n    read_timeout: 5s\n");
        assert_eq!(tree.at("app.mailServer.readTimeout").unwrap().text(), "5s");
    }
}
