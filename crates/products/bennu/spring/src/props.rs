//! Property sources — `application*.properties` / `application*.yml`, flattened to keys.
//!
//! What `@Value("${app.timeout}")` is resolved against. Two formats, one flat key space:
//! YAML's nesting is flattened to the dotted form Spring itself uses, so
//!
//! ```yaml
//! spring:
//!   datasource:
//!     url: jdbc:postgresql://localhost/x
//! ```
//!
//! and `spring.datasource.url=…` are the same key, and a project mixing both resolves
//! either way.
//!
//! ## Which file is "the" one
//!
//! A real project has several: `application.yml`, `application-dev.yml`,
//! `application-prod.yml`, one per module, plus the odd `bootstrap.yml`. There is no way
//! to know from the sources which one is running — that is a launch argument — so the
//! editor cannot guess and must not pretend. [`PropertySources::with_active`] takes the
//! user's choice (persisted per project), and lookup order is:
//!
//! 1. the **chosen** file, if one is set;
//! 2. the **profile-less** files (`application.yml`), which is what Spring always loads;
//! 3. everything else, so a key that only exists in `application-prod.yml` still hovers
//!    with a value rather than reading as missing.
//!
//! Step 3 is why the unresolved-key diagnostic is a warning and not an error: the answer
//! depends on how the app is launched, and the editor is not the authority on that.
//!
//! ## Parsing, deliberately shallow
//!
//! The YAML reader handles nested mappings and scalars, and **skips sequences entirely**.
//! `servers:\n  - url: a` would flatten to a key (`servers.url`) that does not exist in
//! Spring's relaxed binding, and a wrong key is worse than a missing one everywhere in
//! this crate. Anchors, multi-document files and block scalars are likewise left alone.

use std::collections::BTreeSet;

/// Which syntax a property file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyFormat {
    Properties,
    Yaml,
}

/// One resolved key in one file, with the spans a go-to lands on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyEntry {
    /// Dotted key (`spring.datasource.url`), already flattened for YAML.
    pub key: String,
    /// Value as written, with surrounding quotes stripped. May be empty.
    pub value: String,
    /// Byte span of the key text where it is written — for YAML that is the *leaf*
    /// segment, since that is where the caret should land.
    pub key_start: usize,
    pub key_end: usize,
    /// 1-based line of the declaration.
    pub line: u32,
}

/// One property file, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyFile {
    /// Absolute path, forward-slashed.
    pub path: String,
    /// File name (`application-dev.yml`).
    pub name: String,
    /// Profile taken from the file name (`dev`), empty for the base file.
    pub profile: String,
    pub format: PropertyFormat,
    pub entries: Vec<PropertyEntry>,
}

impl PropertyFile {
    /// The entry for `key`, if this file declares it.
    pub fn get(&self, key: &str) -> Option<&PropertyEntry> {
        self.entries.iter().find(|e| e.key == key)
    }
}

/// Every property file of a project, plus which one the user pinned as active.
#[derive(Debug, Clone, Default)]
pub struct PropertySources {
    files: Vec<PropertyFile>,
    active: Option<String>,
}

impl PropertySources {
    /// Build from parsed files, with no active choice (the base files win).
    pub fn new(files: Vec<PropertyFile>) -> Self {
        Self { files, active: None }
    }

    /// Pin `path` (forward-slashed, matched case-insensitively on the tail) as the file
    /// that answers first. An unknown path is simply ignored — a project can be
    /// reconfigured out from under a stale setting without breaking.
    pub fn with_active(mut self, path: Option<&str>) -> Self {
        let wanted = path.map(|p| p.replace('\\', "/")).filter(|p| !p.is_empty());
        let known =
            wanted.as_ref().is_some_and(|p| self.files.iter().any(|f| paths_match(&f.path, p)));
        self.active = if known { wanted } else { None };
        self
    }

    pub fn files(&self) -> &[PropertyFile] {
        &self.files
    }

    /// The pinned file's path, if the user chose one that still exists.
    pub fn active_path(&self) -> Option<&str> {
        self.active.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Total number of declared entries across every file.
    pub fn entry_count(&self) -> usize {
        self.files.iter().map(|f| f.entries.len()).sum()
    }

    /// Resolve `key`, honouring the precedence in the module docs. Returns the file it was
    /// found in together with the entry.
    pub fn lookup(&self, key: &str) -> Option<(&PropertyFile, &PropertyEntry)> {
        self.ordered().into_iter().find_map(|f| f.get(key).map(|e| (f, e)))
    }

    /// Whether ANY file declares `key` — the question the unresolved-key check asks, which
    /// is deliberately more forgiving than [`Self::lookup`]'s precedence.
    pub fn declares(&self, key: &str) -> bool {
        self.files.iter().any(|f| f.get(key).is_some())
    }

    /// Every declared key across every file, deduplicated and sorted — the completion set.
    pub fn keys(&self) -> BTreeSet<String> {
        self.files.iter().flat_map(|f| f.entries.iter().map(|e| e.key.clone())).collect()
    }

    /// Files in lookup order: pinned first, then profile-less, then the rest.
    fn ordered(&self) -> Vec<&PropertyFile> {
        let mut out: Vec<&PropertyFile> = Vec::with_capacity(self.files.len());
        if let Some(active) = &self.active {
            for f in &self.files {
                if paths_match(&f.path, active) {
                    out.push(f);
                }
            }
        }
        for f in &self.files {
            if f.profile.is_empty() && !out.iter().any(|o| o.path == f.path) {
                out.push(f);
            }
        }
        for f in &self.files {
            if !out.iter().any(|o| o.path == f.path) {
                out.push(f);
            }
        }
        out
    }
}

/// Compare two forward-slashed paths case-insensitively — the setting is stored as the
/// user's path and Windows will hand back a differently-cased one.
fn paths_match(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Whether a file NAME is a Spring property source (`application*.properties|yml|yaml`,
/// and the `bootstrap*` twin Spring Cloud uses).
pub fn is_property_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let stem_ok = n.starts_with("application") || n.starts_with("bootstrap");
    stem_ok && (n.ends_with(".properties") || n.ends_with(".yml") || n.ends_with(".yaml"))
}

/// The profile encoded in a property file name: `application-dev.yml` → `dev`, and the
/// empty string for the base file.
pub fn profile_of(name: &str) -> String {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    stem.split_once('-').map(|(_, p)| p.to_string()).unwrap_or_default()
}

/// Parse a property file by extension. `None` for a name that isn't one.
pub fn parse_property_file(path: &str, text: &str) -> Option<PropertyFile> {
    let path = path.replace('\\', "/");
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    if !is_property_file(&name) {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    Some(if lower.ends_with(".properties") {
        parse_properties(&path, &name, text)
    } else {
        parse_yaml(&path, &name, text)
    })
}

/// Parse `key=value` / `key: value` lines, with `#` and `!` comments.
fn parse_properties(path: &str, name: &str, text: &str) -> PropertyFile {
    let mut entries = Vec::new();
    let mut offset = 0usize;
    for (i, raw) in text.split('\n').enumerate() {
        let line_start = offset;
        offset += raw.len() + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        // The separator is the first `=` or `:` — `db.url=jdbc:postgresql://…` splits at
        // the `=`, and the colons in the value stay in the value.
        let Some(sep) = line.find(|c: char| c == '=' || c == ':') else { continue };
        let key = line[..sep].trim();
        if key.is_empty() {
            continue;
        }
        let indent = line.len() - trimmed.len();
        entries.push(PropertyEntry {
            key: key.to_string(),
            value: unquote(line[sep + 1..].trim()),
            key_start: line_start + indent,
            key_end: line_start + indent + key.len(),
            line: i as u32 + 1,
        });
    }
    PropertyFile {
        path: path.to_string(),
        name: name.to_string(),
        profile: profile_of(name),
        format: PropertyFormat::Properties,
        entries,
    }
}

/// Flatten a YAML mapping tree to dotted keys. Sequences are skipped whole — see the
/// module docs.
fn parse_yaml(path: &str, name: &str, text: &str) -> PropertyFile {
    let mut entries = Vec::new();
    // (indent, key) for each open level.
    let mut stack: Vec<(usize, String)> = Vec::new();
    // Indent of the sequence currently being skipped, if any: everything indented deeper
    // than the `-` belongs to it.
    let mut skip_deeper_than: Option<usize> = None;
    let mut offset = 0usize;

    for (i, raw) in text.split('\n').enumerate() {
        let line_start = offset;
        offset += raw.len() + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
            continue;
        }
        if let Some(seq_indent) = skip_deeper_than {
            if indent > seq_indent {
                continue;
            }
            skip_deeper_than = None;
        }
        if trimmed.starts_with('-') {
            // A sequence item: skip it and everything nested under it.
            skip_deeper_than = Some(indent);
            continue;
        }
        let Some(colon) = split_key(trimmed) else { continue };
        let key = trimmed[..colon].trim();
        if key.is_empty() {
            continue;
        }
        while stack.last().is_some_and(|(ind, _)| *ind >= indent) {
            stack.pop();
        }
        let rest = trimmed[colon + 1..].trim();
        if rest.is_empty() {
            stack.push((indent, key.to_string()));
            continue;
        }
        let mut full = String::new();
        for (_, k) in &stack {
            full.push_str(k);
            full.push('.');
        }
        full.push_str(key);
        entries.push(PropertyEntry {
            key: full,
            value: unquote(rest),
            key_start: line_start + indent,
            key_end: line_start + indent + key.len(),
            line: i as u32 + 1,
        });
    }
    PropertyFile {
        path: path.to_string(),
        name: name.to_string(),
        profile: profile_of(name),
        format: PropertyFormat::Yaml,
        entries,
    }
}

/// The index of the `:` that separates a YAML key from its value — the first one that is
/// not inside a quoted key. Returns `None` for a line with no separator at all.
fn split_key(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut quote: Option<u8> = None;
    for (i, &c) in b.iter().enumerate() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == b'\'' || c == b'"' => quote = Some(c),
            None if c == b':' => return Some(i),
            None => {}
        }
    }
    None
}

/// Strip matching surrounding quotes and a trailing `#` comment from a scalar.
fn unquote(v: &str) -> String {
    let v = v.trim();
    if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
        || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
    {
        return v[1..v.len() - 1].to_string();
    }
    // An unquoted scalar can carry a trailing comment (` # note`); a `#` with no space
    // before it is part of the value (a colour, a fragment).
    match v.find(" #") {
        Some(at) => v[..at].trim().to_string(),
        None => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(text: &str) -> PropertyFile {
        parse_property_file("/p/application.properties", text).unwrap()
    }
    fn yaml(text: &str) -> PropertyFile {
        parse_property_file("/p/application.yml", text).unwrap()
    }

    #[test]
    fn properties_split_at_the_first_separator_only() {
        let f = props("db.url=jdbc:postgresql://localhost/x\n# a comment\nname : Bennu\n");
        assert_eq!(f.get("db.url").unwrap().value, "jdbc:postgresql://localhost/x");
        assert_eq!(f.get("name").unwrap().value, "Bennu");
        assert_eq!(f.entries.len(), 2, "the comment is not a key");
    }

    #[test]
    fn properties_key_span_points_at_the_key() {
        let text = "  app.timeout=30\n";
        let f = props(text);
        let e = f.get("app.timeout").unwrap();
        assert_eq!(&text[e.key_start..e.key_end], "app.timeout");
        assert_eq!(e.line, 1);
    }

    #[test]
    fn yaml_nesting_flattens_to_dotted_keys() {
        let f = yaml(
            "spring:\n  datasource:\n    url: jdbc:postgresql://localhost/x\n    username: sa\napp:\n  timeout: 30\n",
        );
        assert_eq!(f.get("spring.datasource.url").unwrap().value, "jdbc:postgresql://localhost/x");
        assert_eq!(f.get("spring.datasource.username").unwrap().value, "sa");
        assert_eq!(f.get("app.timeout").unwrap().value, "30");
        assert_eq!(f.entries.len(), 3, "the parent nodes are not entries");
    }

    #[test]
    fn yaml_dedents_back_to_a_sibling_branch() {
        let f = yaml("a:\n  b:\n    c: 1\n  d: 2\ne: 3\n");
        let keys: Vec<_> = f.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["a.b.c", "a.d", "e"]);
    }

    #[test]
    fn yaml_key_span_is_the_leaf_segment() {
        let text = "spring:\n  profiles:\n    active: dev\n";
        let f = yaml(text);
        let e = f.get("spring.profiles.active").unwrap();
        assert_eq!(&text[e.key_start..e.key_end], "active");
        assert_eq!(e.line, 3);
    }

    #[test]
    fn yaml_sequences_are_skipped_whole() {
        // `servers.url` is NOT a Spring key — inventing it would be worse than missing it.
        let f = yaml("servers:\n  - url: a\n    name: x\n  - url: b\napp:\n  ok: 1\n");
        let keys: Vec<_> = f.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["app.ok"]);
    }

    #[test]
    fn yaml_values_lose_quotes_and_trailing_comments() {
        let f = yaml("a: \"quoted\"\nb: 'single'\nc: plain # note\nd: '#notacomment'\n");
        assert_eq!(f.get("a").unwrap().value, "quoted");
        assert_eq!(f.get("b").unwrap().value, "single");
        assert_eq!(f.get("c").unwrap().value, "plain");
        assert_eq!(f.get("d").unwrap().value, "#notacomment");
    }

    #[test]
    fn yaml_url_value_keeps_its_colon() {
        let f = yaml("url: http://example.com:8080/x\n");
        assert_eq!(f.get("url").unwrap().value, "http://example.com:8080/x");
    }

    #[test]
    fn file_recognition_and_profile_from_the_name() {
        assert!(is_property_file("application.yml"));
        assert!(is_property_file("application-dev.properties"));
        assert!(is_property_file("bootstrap.yaml"));
        assert!(!is_property_file("messages.properties"), "not a Spring config source");
        assert!(!is_property_file("application.xml"));
        assert_eq!(profile_of("application-dev.yml"), "dev");
        assert_eq!(profile_of("application.yml"), "");
    }

    #[test]
    fn lookup_prefers_the_pinned_file_then_the_base_one() {
        let base = parse_property_file("/p/application.yml", "app:\n  mode: base\n").unwrap();
        let dev = parse_property_file("/p/application-dev.yml", "app:\n  mode: dev\n").unwrap();
        let sources = PropertySources::new(vec![base, dev]);

        assert_eq!(sources.lookup("app.mode").unwrap().1.value, "base", "no pin → the base file");
        let pinned = sources.clone().with_active(Some("/p/application-dev.yml"));
        assert_eq!(pinned.lookup("app.mode").unwrap().1.value, "dev");
        assert_eq!(pinned.active_path(), Some("/p/application-dev.yml"));
    }

    #[test]
    fn a_stale_pin_is_ignored_rather_than_breaking_lookup() {
        let base = parse_property_file("/p/application.yml", "a: 1\n").unwrap();
        let s = PropertySources::new(vec![base]).with_active(Some("/p/gone.yml"));
        assert_eq!(s.active_path(), None);
        assert_eq!(s.lookup("a").unwrap().1.value, "1");
    }

    #[test]
    fn a_profile_only_key_still_resolves_so_it_never_reads_as_missing() {
        let base = parse_property_file("/p/application.yml", "a: 1\n").unwrap();
        let prod = parse_property_file("/p/application-prod.yml", "only.in.prod: 9\n").unwrap();
        let s = PropertySources::new(vec![base, prod]);
        assert!(s.declares("only.in.prod"));
        assert_eq!(s.lookup("only.in.prod").unwrap().1.value, "9");
        assert!(!s.declares("nowhere.at.all"));
    }

    #[test]
    fn keys_are_the_union_across_files() {
        let a = parse_property_file("/p/application.yml", "x: 1\n").unwrap();
        let b = parse_property_file("/p/application-dev.yml", "x: 2\ny: 3\n").unwrap();
        let s = PropertySources::new(vec![a, b]);
        assert_eq!(s.keys().into_iter().collect::<Vec<_>>(), ["x", "y"]);
        assert_eq!(s.entry_count(), 3);
    }

    #[test]
    fn a_non_property_file_is_not_parsed() {
        assert!(parse_property_file("/p/messages.properties", "a=1").is_none());
    }
}
