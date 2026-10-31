//! What the project already writes inside annotations — and where the caret is inside one.
//!
//! `@PostMapping(produces = |)` has an answer the type system cannot give: the value the other forty controllers
//! use. And `@Value("${spring.da|")` has one only the project's `application.yml` can give: the keys that exist.
//! This module is the pure half — finding the annotation the caret is in, reading the values the sources write, and
//! reading the keys of the configuration files. `project_values` keeps them per project, off the request path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tree_sitter::Node;

/// Annotations nobody counts values for: the position says nothing, or the value is code, not data.
const IGNORED: &[&str] = &["Override", "SuppressWarnings", "Deprecated", "FunctionalInterface", "SafeVarargs"];
/// A value longer than this is an expression, not something to offer.
const MAX_VALUE: usize = 120;

/// One value written for one element of one annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    /// The annotation's simple name — `PostMapping`.
    pub annotation: String,
    /// The element: `produces`, or `value` for the unnamed one.
    pub element: String,
    /// As written: `"/orders"` (with its quotes), `MediaType.APPLICATION_JSON_VALUE`.
    pub value: String,
}

/// Every annotation value written in `source`.
pub fn usages_in(source: &str) -> Vec<Usage> {
    let mut out = Vec::new();
    if let Some(tree) = bennu_java::prelude::parse_java(source) {
        walk(tree.root_node(), source.as_bytes(), &mut out);
    }
    out
}

fn text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).unwrap_or("")
}

fn walk(node: Node, src: &[u8], out: &mut Vec<Usage>) {
    if node.kind() == "annotation" {
        annotation(node, src, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, out);
    }
}

fn annotation(node: Node, src: &[u8], out: &mut Vec<Usage>) {
    let (Some(name), Some(args)) = (node.child_by_field_name("name"), node.child_by_field_name("arguments")) else { return };
    let simple = text(name, src).rsplit('.').next().unwrap_or("").to_string();
    if simple.is_empty() || IGNORED.contains(&simple.as_str()) {
        return;
    }
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        match (arg.kind(), arg.child_by_field_name("key"), arg.child_by_field_name("value")) {
            ("element_value_pair", Some(key), Some(value)) => values(value, src, &simple, text(key, src), out),
            ("element_value_pair", ..) => {}
            _ => values(arg, src, &simple, "value", out),
        }
    }
}

fn values(node: Node, src: &[u8], annotation: &str, element: &str, out: &mut Vec<Usage>) {
    match node.kind() {
        "element_value_array_initializer" => {
            let mut cursor = node.walk();
            for item in node.named_children(&mut cursor) {
                values(item, src, annotation, element, out);
            }
        }
        // a nested annotation is its own annotation and is counted on its own
        "annotation" | "marker_annotation" | "line_comment" | "block_comment" => {}
        _ => {
            let value = text(node, src).trim();
            if !value.is_empty() && value.len() <= MAX_VALUE && !value.contains('\n') {
                out.push(Usage { annotation: annotation.to_string(), element: element.to_string(), value: value.to_string() });
            }
        }
    }
}

// ---- where the caret is ---------------------------------------------------------------------------------------------

/// The caret inside the parentheses of an annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgSite {
    /// The annotation's simple name.
    pub annotation: String,
    /// The element being written: `produces`, or `value` when the argument is unnamed.
    pub element: String,
    /// Whether the caret is inside a string literal.
    pub in_string: bool,
    /// What has been typed of the value: the string's content so far, or the identifier being written.
    pub typed: String,
    /// Byte offset where `typed` starts.
    pub typed_start: usize,
}

/// Looks back for the annotation whose parentheses are still open at `offset`, and reads which element the caret is
/// writing. Plain text, not a parse: `@PostMapping(produces = ` does not parse, and that is the state completion runs in.
pub fn arg_site(text: &str, offset: usize) -> Option<ArgSite> {
    const REACH: usize = 1200;
    let mut from = offset.saturating_sub(REACH);
    while from < offset && !text.is_char_boundary(from) {
        from += 1;
    }
    let window = &text[from..offset];
    window.match_indices('@').rev().find_map(|(i, _)| site_from(text, from + i, offset))
}

fn site_from(text: &str, at: usize, offset: usize) -> Option<ArgSite> {
    let after = &text[at + 1..];
    let name_len = after.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '.')).unwrap_or(after.len());
    let name = &after[..name_len];
    if name.is_empty() || name.ends_with('.') {
        return None;
    }
    let rest = &after[name_len..];
    let open = at + 1 + name_len + (rest.len() - rest.trim_start().len());
    if text.as_bytes().get(open) != Some(&b'(') || open >= offset {
        return None;
    }
    // walk from the parenthesis to the caret, outside strings: depth, the argument being written, the string state
    let (mut paren, mut brace) = (1i32, 0i32);
    let (mut arg_start, mut value_start) = (open + 1, open + 1);
    let mut string_start: Option<usize> = None;
    let mut i = open + 1;
    let b = text.as_bytes();
    while i < offset {
        let c = b[i];
        if let Some(_) = string_start {
            match c {
                b'\\' => i += 1,
                b'"' => string_start = None,
                _ => {}
            }
        } else {
            match c {
                b'"' => string_start = Some(i + 1),
                b'\'' => {
                    // a char literal: skip it whole
                    i += 1;
                    while i < offset && b[i] != b'\'' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                }
                b'(' => paren += 1,
                b')' => {
                    paren -= 1;
                    if paren == 0 {
                        return None;
                    }
                }
                b'{' => {
                    brace += 1;
                    value_start = i + 1;
                }
                b'}' => brace -= 1,
                b',' if paren == 1 && brace == 0 => {
                    arg_start = i + 1;
                    value_start = i + 1;
                }
                b',' if paren == 1 => value_start = i + 1,
                _ => {}
            }
        }
        i += 1;
    }
    if paren < 1 {
        return None;
    }
    let argument = &text[arg_start..offset];
    let element = match argument.trim_start().split_once('=') {
        Some((key, _)) if key.trim().chars().all(|c| c.is_alphanumeric() || c == '_') && !key.trim().is_empty() && !argument.contains("==") => {
            key.trim().to_string()
        }
        _ => "value".to_string(),
    };
    let simple = name.rsplit('.').next().unwrap_or(name).to_string();
    let (in_string, typed_start) = match string_start {
        Some(s) => (true, s),
        None => {
            // after the `=` of a named argument, or after the last `{`/`,`
            let eq = argument.find('=').filter(|_| element != "value").map(|e| arg_start + e + 1);
            let from = value_start.max(eq.unwrap_or(0));
            let value = &text[from..offset];
            let word = value.len() - value.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '$').len();
            (false, offset - word)
        }
    };
    Some(ArgSite { annotation: simple, element, in_string, typed: text[typed_start..offset].to_string(), typed_start })
}

/// The part of a string that is a configuration key being written, for the annotations that take one:
/// `@Value("${spring.da|")`, `@ConfigurationProperties(prefix = "app.|")`, `@ConditionalOnProperty(name = "|")`.
/// Returns the key typed so far and where it starts.
pub fn config_key_typed(site: &ArgSite) -> Option<(String, usize)> {
    if !site.in_string {
        return None;
    }
    let (key_start, closed_default) = match site.annotation.as_str() {
        "Value" => {
            let at = site.typed.rfind("${")? + 2;
            (at, site.typed[at..].contains(':') || site.typed[at..].contains('}'))
        }
        "ConfigurationProperties" if matches!(site.element.as_str(), "prefix" | "value") => (0, false),
        "ConditionalOnProperty" if matches!(site.element.as_str(), "name" | "value" | "prefix") => (0, false),
        _ => return None,
    };
    let key = &site.typed[key_start..];
    (!closed_default && key.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '[' | ']'))).then(|| (key.to_string(), site.typed_start + key_start))
}

// ---- the configuration files ----------------------------------------------------------------------------------------

/// Whether a file name is one of the Spring configuration files whose keys are offered.
pub fn is_config_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem_ok = name.starts_with("application") || name.starts_with("bootstrap");
    stem_ok && matches!(path.extension().and_then(|e| e.to_str()), Some("properties" | "yml" | "yaml"))
}

/// The keys a `.properties` or `.yml` file defines, nested YAML flattened with dots.
pub fn config_keys(content: &str, yaml: bool) -> Vec<String> {
    if yaml { yaml_keys(content) } else { properties_keys(content) }
}

fn properties_keys(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim_start)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
        .filter_map(|l| {
            let end = l.find(['=', ':']).unwrap_or(l.len());
            let key = l[..end].trim();
            (!key.is_empty() && !key.contains(' ')).then(|| key.to_string())
        })
        .collect()
}

fn yaml_keys(content: &str) -> Vec<String> {
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("- ") || trimmed == "---" {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let Some((key, value)) = trimmed.split_once(':') else { continue };
        let key = key.trim().trim_matches(|c| c == '"' || c == '\'');
        // `key:value` with no space is part of a scalar, not a mapping key
        if key.is_empty() || key.contains(' ') || (!value.is_empty() && !value.starts_with(' ')) {
            continue;
        }
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let full: String = stack.iter().map(|(_, k)| k.as_str()).chain([key]).collect::<Vec<_>>().join(".");
        let has_value = !value.trim().is_empty() && !value.trim().starts_with('#');
        if has_value {
            out.push(full);
        } else {
            stack.push((indent, key.to_string()));
        }
    }
    out
}

// ---- the tables -----------------------------------------------------------------------------------------------------

/// The values written for each annotation element, per file, so a file's contribution can be replaced.
#[derive(Default)]
pub struct AnnotationCensus {
    by_file: HashMap<PathBuf, Vec<Usage>>,
}

impl AnnotationCensus {
    pub fn set_file(&mut self, path: PathBuf, usages: Vec<Usage>) {
        if usages.is_empty() {
            self.by_file.remove(&path);
        } else {
            self.by_file.insert(path, usages);
        }
    }

    pub fn remove_file(&mut self, path: &Path) {
        self.by_file.remove(path);
    }

    /// The values written for `annotation`'s `element` in the files other than `current`, most written first.
    pub fn values(&self, annotation: &str, element: &str, current: &Path) -> Vec<(String, u32)> {
        let mut counts: HashMap<&str, u32> = HashMap::new();
        for (file, usages) in &self.by_file {
            if file == current {
                continue;
            }
            for u in usages.iter().filter(|u| u.annotation == annotation && u.element == element) {
                *counts.entry(&u.value).or_insert(0) += 1;
            }
        }
        let mut out: Vec<(String, u32)> = counts.into_iter().map(|(v, n)| (v.to_string(), n)).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }
}

/// The configuration keys, per file.
#[derive(Default)]
pub struct ConfigKeys {
    by_file: HashMap<PathBuf, Vec<String>>,
}

impl ConfigKeys {
    pub fn set_file(&mut self, path: PathBuf, keys: Vec<String>) {
        self.by_file.insert(path, keys);
    }

    pub fn remove_file(&mut self, path: &Path) {
        self.by_file.remove(path);
    }

    /// The keys starting with `typed`, those of the configuration nearest to `current` first.
    pub fn matching(&self, typed: &str, current: &Path) -> Vec<String> {
        let near = |p: &Path| p.components().zip(current.components()).take_while(|(a, b)| a == b).count();
        let mut files: Vec<(&PathBuf, &Vec<String>)> = self.by_file.iter().collect();
        files.sort_by(|a, b| near(b.0).cmp(&near(a.0)).then_with(|| a.0.cmp(b.0)));
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (_, keys) in files {
            for k in keys.iter().filter(|k| k.starts_with(typed) && k.len() > typed.len()) {
                if seen.insert(k.as_str()) {
                    out.push(k.clone());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROLLER: &str = "class A {\n  @PostMapping(value = \"/orders\", produces = {MediaType.APPLICATION_JSON_VALUE, \"x\"})\n  void f() {}\n  @GetMapping(\"/b\") void g() {}\n  @Secured({\"R1\", \"R2\"}) @Marker void h() {}\n}\n";

    fn at_bar(src: &str) -> Option<ArgSite> {
        let off = src.find('|').unwrap();
        let clean = src.replace('|', "");
        arg_site(&clean, off)
    }

    #[test]
    fn values_are_read_per_annotation_and_element() {
        let u = usages_in(CONTROLLER);
        let has = |a: &str, e: &str, v: &str| u.iter().any(|x| x.annotation == a && x.element == e && x.value == v);
        assert!(has("PostMapping", "value", "\"/orders\""));
        assert!(has("PostMapping", "produces", "MediaType.APPLICATION_JSON_VALUE"));
        assert!(has("PostMapping", "produces", "\"x\""));
        assert!(has("GetMapping", "value", "\"/b\""));
        assert!(has("Secured", "value", "\"R2\""));
        assert!(!u.iter().any(|x| x.annotation == "Marker"));
    }

    #[test]
    fn the_site_names_the_annotation_and_the_element() {
        let s = at_bar("@PostMapping(produces = |").unwrap();
        assert_eq!((s.annotation.as_str(), s.element.as_str(), s.in_string, s.typed.as_str()), ("PostMapping", "produces", false, ""));
        let s = at_bar("@PostMapping(produces = Media|").unwrap();
        assert_eq!(s.typed, "Media");
        let s = at_bar("@GetMapping(|").unwrap();
        assert_eq!(s.element, "value");
        let s = at_bar("@RequestMapping(path = \"/a\", value = \"/or|").unwrap();
        assert_eq!((s.element.as_str(), s.in_string, s.typed.as_str()), ("value", true, "/or"));
        let s = at_bar("@Secured({\"R1\", |").unwrap();
        assert_eq!((s.element.as_str(), s.in_string, s.typed.as_str()), ("value", false, ""));
        let s = at_bar("@org.springframework.web.bind.annotation.GetMapping(\"/x|").unwrap();
        assert_eq!(s.annotation, "GetMapping");
    }

    #[test]
    fn a_closed_annotation_or_a_plain_call_is_no_site() {
        assert!(at_bar("@GetMapping(\"/a\") void f() { call(|").is_none());
        assert!(at_bar("void f() { call(|").is_none());
        assert!(at_bar("@Marker void f() { |").is_none());
    }

    #[test]
    fn a_config_key_is_found_in_the_places_that_take_one() {
        let key = |s: &str| at_bar(s).and_then(|s| config_key_typed(&s)).map(|(k, _)| k);
        assert_eq!(key("@Value(\"${spring.da|").as_deref(), Some("spring.da"));
        assert_eq!(key("@ConfigurationProperties(prefix = \"app.|").as_deref(), Some("app."));
        assert_eq!(key("@ConditionalOnProperty(name = \"feat|").as_deref(), Some("feat"));
        assert_eq!(key("@Value(\"${a.b:def|").as_deref(), None);
        assert_eq!(key("@GetMapping(\"/x|").as_deref(), None);
    }

    #[test]
    fn properties_and_yaml_keys_are_flattened() {
        let props = "# c\nserver.port=8080\nspring.datasource.url: jdbc:x\n\n! also a comment\n";
        assert_eq!(config_keys(props, false), ["server.port", "spring.datasource.url"]);
        let yaml = "server:\n  port: 8080\nspring:\n  datasource:\n    url: jdbc:x\n    username: u\n  profiles:\n    active: dev\nlist:\n  - a\n";
        assert_eq!(config_keys(yaml, true), ["server.port", "spring.datasource.url", "spring.datasource.username", "spring.profiles.active"]);
    }

    #[test]
    fn values_are_counted_without_the_current_file_and_most_written_first() {
        let mut c = AnnotationCensus::default();
        let u = |v: &str| Usage { annotation: "GetMapping".into(), element: "value".into(), value: v.into() };
        c.set_file("/p/A.java".into(), vec![u("\"/a\""), u("\"/b\"")]);
        c.set_file("/p/B.java".into(), vec![u("\"/a\"")]);
        c.set_file("/p/C.java".into(), vec![u("\"/c\"")]);
        assert_eq!(c.values("GetMapping", "value", Path::new("/p/C.java")), [("\"/a\"".to_string(), 2), ("\"/b\"".to_string(), 1)]);
    }

    #[test]
    fn keys_of_the_nearest_configuration_come_first() {
        let mut k = ConfigKeys::default();
        k.set_file("/r/api/src/application.yml".into(), vec!["api.timeout".into(), "shared.name".into()]);
        k.set_file("/r/batch/src/application.yml".into(), vec!["batch.size".into(), "shared.name".into()]);
        let got = k.matching("", Path::new("/r/batch/src/Job.java"));
        assert_eq!(got[0], "batch.size");
        assert_eq!(got.iter().filter(|x| *x == "shared.name").count(), 1);
        assert_eq!(k.matching("api.", Path::new("/r/batch/src/Job.java")), ["api.timeout"]);
    }

    #[test]
    fn config_files_are_recognised_by_name() {
        for yes in ["application.yml", "application-dev.properties", "bootstrap.yaml"] {
            assert!(is_config_file(Path::new(yes)), "{yes}");
        }
        for no in ["pom.xml", "settings.yml", "application.txt"] {
            assert!(!is_config_file(Path::new(no)), "{no}");
        }
    }
}
