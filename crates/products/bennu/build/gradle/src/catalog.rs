//! The version catalog — `gradle/libs.versions.toml`.
//!
//! A catalog is where a modern build keeps its versions: `libs.spring.web` in a build script is an
//! *accessor* for an alias declared here, and the version number is two tables away
//! (`[libraries]` → `version.ref` → `[versions]`). Reading it is what lets the dependency panel say
//! `org.springframework:spring-web 5.3.10` instead of `libs.spring.web`, and say **where** the
//! version was decided.
//!
//! The reader is line-based and tolerant: one entry per line, arrays allowed to span lines, inline
//! tables flattened to dotted keys (`version.ref`). That is all a catalog is allowed to be — TOML
//! forbids newlines inside an inline table — so a TOML crate would buy nothing here.

use std::collections::BTreeMap;

/// One `[libraries]` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogLibrary {
    pub alias: String,
    pub group: String,
    pub name: String,
    /// The version after `version.ref` has been looked up; empty for an entry with none (the version
    /// then comes from a BOM).
    pub version: String,
    /// The `[versions]` key it pointed at, when it did.
    pub version_ref: String,
    pub offset: usize,
    pub line: u32,
}

/// One `[plugins]` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogPlugin {
    pub alias: String,
    pub id: String,
    pub version: String,
}

/// A parsed catalog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// The name build scripts use: `libs` for the default one.
    pub name: String,
    /// Absolute path, forward-slashed; empty for a catalog built in memory.
    pub file: String,
    pub versions: BTreeMap<String, String>,
    pub libraries: Vec<CatalogLibrary>,
    pub bundles: BTreeMap<String, Vec<String>>,
    pub plugins: Vec<CatalogPlugin>,
}

/// The accessor spelling of an alias: separators `-` `_` `.` all read as `.`.
fn norm(s: &str) -> String {
    s.chars().map(|c| if c == '-' || c == '_' { '.' } else { c }).collect()
}

/// Strip the suffixes an accessor carries when it is used as a value.
fn strip_provider(mut s: &str) -> &str {
    loop {
        let before = s.len();
        for suffix in [".get()", ".asProvider()", ".get", ".asProvider", ".toString()"] {
            if let Some(rest) = s.strip_suffix(suffix) {
                s = rest;
            }
        }
        if s.len() == before {
            return s;
        }
    }
}

impl Catalog {
    /// Parse the text of a `*.versions.toml`.
    pub fn parse(name: &str, file: &str, text: &str) -> Catalog {
        let mut catalog = Catalog { name: name.to_string(), file: file.to_string(), ..Catalog::default() };
        let mut section = String::new();
        // `(alias, table-or-string, offset, line)` — libraries wait until `[versions]` is complete,
        // since a `version.ref` may point at a table declared further down.
        let mut pending_libs: Vec<(String, Value, usize, u32)> = Vec::new();
        let mut pending_plugins: Vec<(String, Value)> = Vec::new();

        let mut offset = 0usize;
        let mut line_no = 0u32;
        let mut lines = text.split_inclusive('\n').peekable();
        while let Some(raw) = lines.next() {
            let entry_offset = offset;
            offset += raw.len();
            line_no += 1;
            let entry_line = line_no;
            let mut logical = strip_comment(raw).trim().to_string();
            if logical.is_empty() {
                continue;
            }
            // An array that spans lines: keep reading until its brackets balance.
            while bracket_depth(&logical) > 0 {
                let Some(next) = lines.next() else { break };
                offset += next.len();
                line_no += 1;
                logical.push(' ');
                logical.push_str(strip_comment(next).trim());
            }
            if let Some(header) = logical.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
                if !header.starts_with('[') {
                    section = header.trim().to_string();
                    continue;
                }
            }
            let Some((key, value)) = logical.split_once('=') else { continue };
            let key = key.trim().trim_matches('"').to_string();
            let value = parse_value(value.trim());
            match section.as_str() {
                "versions" => {
                    if let Some(v) = value.version_text() {
                        catalog.versions.insert(key, v);
                    }
                }
                "libraries" => pending_libs.push((key, value, entry_offset, entry_line)),
                "bundles" => {
                    if let Value::List(items) = value {
                        catalog.bundles.insert(key, items);
                    }
                }
                "plugins" => pending_plugins.push((key, value)),
                _ => {}
            }
        }

        for (alias, value, offset, line) in pending_libs {
            catalog.libraries.push(library_of(&catalog.versions, alias, &value, offset, line));
        }
        for (alias, value) in pending_plugins {
            catalog.plugins.push(plugin_of(&catalog.versions, alias, &value));
        }
        catalog
    }

    /// The library an accessor such as `libs.spring.web` names.
    pub fn library_for(&self, accessor: &str) -> Option<&CatalogLibrary> {
        let key = self.accessor_tail(accessor)?;
        if key.starts_with("bundles.") || key.starts_with("versions.") || key.starts_with("plugins.") {
            return None;
        }
        self.libraries.iter().find(|l| norm(&l.alias) == key)
    }

    /// The libraries of a bundle accessor such as `libs.bundles.spring`.
    pub fn bundle_for(&self, accessor: &str) -> Vec<&CatalogLibrary> {
        let Some(key) = self.accessor_tail(accessor) else { return Vec::new() };
        let Some(bundle) = key.strip_prefix("bundles.") else { return Vec::new() };
        let Some((_, aliases)) = self.bundles.iter().find(|(name, _)| norm(name) == bundle) else {
            return Vec::new();
        };
        aliases
            .iter()
            .filter_map(|a| self.libraries.iter().find(|l| norm(&l.alias) == norm(a)))
            .collect()
    }

    /// The plugin an accessor such as `libs.plugins.spring.boot` names.
    pub fn plugin_for(&self, accessor: &str) -> Option<&CatalogPlugin> {
        let key = self.accessor_tail(accessor)?;
        let alias = key.strip_prefix("plugins.")?;
        self.plugins.iter().find(|p| norm(&p.alias) == alias)
    }

    /// The version an accessor such as `libs.versions.spring.get()` names.
    pub fn version_for(&self, accessor: &str) -> Option<&str> {
        let key = self.accessor_tail(accessor)?;
        let alias = key.strip_prefix("versions.")?;
        self.versions.iter().find(|(name, _)| norm(name) == alias).map(|(_, v)| v.as_str())
    }

    /// `libs.a.b.get()` → `a.b`; `None` when the accessor is not about this catalog.
    fn accessor_tail(&self, accessor: &str) -> Option<String> {
        let trimmed = strip_provider(accessor.trim());
        let rest = trimmed.strip_prefix(self.name.as_str())?.strip_prefix('.')?;
        Some(rest.to_string())
    }
}

fn library_of(
    versions: &BTreeMap<String, String>,
    alias: String,
    value: &Value,
    offset: usize,
    line: u32,
) -> CatalogLibrary {
    let mut lib = CatalogLibrary { alias, offset, line, ..CatalogLibrary::default() };
    match value {
        Value::Str(s) => {
            let mut parts = s.splitn(3, ':');
            lib.group = parts.next().unwrap_or_default().to_string();
            lib.name = parts.next().unwrap_or_default().to_string();
            lib.version = parts.next().unwrap_or_default().to_string();
        }
        Value::Table(t) => {
            if let Some(module) = t.get("module") {
                let (g, n) = module.split_once(':').unwrap_or((module.as_str(), ""));
                lib.group = g.to_string();
                lib.name = n.to_string();
            } else {
                lib.group = t.get("group").cloned().unwrap_or_default();
                lib.name = t.get("name").cloned().unwrap_or_default();
            }
            if let Some(r) = t.get("version.ref") {
                lib.version_ref = r.clone();
                lib.version = versions.get(r).cloned().unwrap_or_default();
            } else {
                lib.version = ["version", "version.strictly", "version.require", "version.prefer"]
                    .iter()
                    .find_map(|k| t.get(*k).cloned())
                    .unwrap_or_default();
            }
        }
        Value::List(_) => {}
    }
    lib
}

fn plugin_of(versions: &BTreeMap<String, String>, alias: String, value: &Value) -> CatalogPlugin {
    let mut plugin = CatalogPlugin { alias, ..CatalogPlugin::default() };
    match value {
        Value::Str(s) => {
            let (id, version) = s.split_once(':').unwrap_or((s.as_str(), ""));
            plugin.id = id.to_string();
            plugin.version = version.to_string();
        }
        Value::Table(t) => {
            plugin.id = t.get("id").cloned().unwrap_or_default();
            plugin.version = match t.get("version.ref") {
                Some(r) => versions.get(r).cloned().unwrap_or_default(),
                None => t.get("version").cloned().unwrap_or_default(),
            };
        }
        Value::List(_) => {}
    }
    plugin
}

/// A TOML value, as far as a catalog uses them.
#[derive(Debug, Clone)]
enum Value {
    Str(String),
    /// An inline table, flattened: `{ version.ref = "x" }` and `{ version = { ref = "x" } }` both
    /// become the key `version.ref`.
    Table(BTreeMap<String, String>),
    List(Vec<String>),
}

impl Value {
    /// The version a `[versions]` entry stands for: the string, or the strictest key of a rich one.
    fn version_text(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.clone()),
            Value::Table(t) => ["strictly", "require", "prefer"].iter().find_map(|k| t.get(*k).cloned()),
            Value::List(_) => None,
        }
    }
}

fn parse_value(s: &str) -> Value {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        let mut map = BTreeMap::new();
        flatten_table(inner, "", &mut map);
        return Value::Table(map);
    }
    if let Some(inner) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        let items = crate::script::split_top_level(inner)
            .iter()
            .map(|p| unquote(p.trim()))
            .filter(|p| !p.is_empty())
            .collect();
        return Value::List(items);
    }
    Value::Str(unquote(s))
}

fn flatten_table(inner: &str, prefix: &str, out: &mut BTreeMap<String, String>) {
    for part in crate::script::split_top_level(inner) {
        let Some((key, value)) = part.split_once('=') else { continue };
        let key = format!("{prefix}{}", key.trim().trim_matches('"'));
        let value = value.trim();
        if let Some(nested) = value.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
            flatten_table(nested, &format!("{key}."), out);
        } else {
            out.insert(key, unquote(value));
        }
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner.to_string();
        }
    }
    s.to_string()
}

/// `line` without a trailing `# comment` — a `#` inside a string is kept.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return &line[..i],
            None => {}
        }
    }
    line
}

/// How many `[` / `{` are still open at the end of `s`, ignoring strings.
fn bracket_depth(s: &str) -> i32 {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[versions]
spring = "5.3.10"
junit = { strictly = "5.9.0" }

[libraries]
spring-web = { module = "org.springframework:spring-web", version.ref = "spring" }
junit-api = { group = "org.junit.jupiter", name = "junit-jupiter-api", version.ref = "junit" }
guava = "com.google.guava:guava:32.0.0-jre"
managed = { module = "org.x:managed" } # no version: a BOM decides

[bundles]
testing = [
  "junit-api",
  "guava",
]

[plugins]
boot = { id = "org.springframework.boot", version = "3.1.0" }
"#;

    fn catalog() -> Catalog {
        Catalog::parse("libs", "/p/gradle/libs.versions.toml", SAMPLE)
    }

    #[test]
    fn an_accessor_resolves_through_version_ref() {
        let c = catalog();
        let lib = c.library_for("libs.spring.web").expect("spring-web");
        assert_eq!((lib.group.as_str(), lib.name.as_str(), lib.version.as_str()), ("org.springframework", "spring-web", "5.3.10"));
        assert_eq!(lib.version_ref, "spring");
    }

    #[test]
    fn a_rich_version_reads_as_its_strict_value() {
        let c = catalog();
        assert_eq!(c.library_for("libs.junit.api").unwrap().version, "5.9.0");
    }

    #[test]
    fn the_string_form_and_a_missing_version_both_work() {
        let c = catalog();
        assert_eq!(c.library_for("libs.guava").unwrap().version, "32.0.0-jre");
        assert_eq!(c.library_for("libs.managed").unwrap().version, "");
    }

    #[test]
    fn a_bundle_expands_across_lines() {
        let c = catalog();
        let names: Vec<&str> = c.bundle_for("libs.bundles.testing").iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["junit-jupiter-api", "guava"]);
    }

    #[test]
    fn versions_and_plugins_have_their_own_accessors() {
        let c = catalog();
        assert_eq!(c.version_for("libs.versions.spring.get()"), Some("5.3.10"));
        assert_eq!(c.plugin_for("libs.plugins.boot").unwrap().version, "3.1.0");
        assert!(c.library_for("other.spring.web").is_none());
    }
}
