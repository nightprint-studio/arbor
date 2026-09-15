//! A span-aware reader for `package.json`.
//!
//! ## Why not `serde_json`
//!
//! Two reasons, and both are about what an editor is for.
//!
//! **Spans.** Every feature built on this needs to know where a value *is*: the version string to
//! replace when you accept an upgrade, the script name to draw a run control over. A deserialized
//! tree has thrown that away, and re-finding a value by searching the text for it is wrong the
//! first time two dependencies share a version string.
//!
//! **Tolerance.** A `package.json` being edited is regularly not valid JSON — a trailing comma, a
//! half-typed key, an unclosed brace. A strict parser answers nothing at all about such a file,
//! which means every feature here would blink out exactly while you are working. This reader
//! walks what it can and stops where it cannot, so the dependencies above a broken line are still
//! found.
//!
//! It is deliberately **shallow**: it reads the top-level object's string-valued members inside the
//! few sections that matter. A nested object inside `scripts` is not a thing, and a dependency
//! whose value is an object is not one npm understands either.

use std::path::Path;

/// Which section a dependency was declared in. npm treats all four as dependencies of the package
/// and only differs on when they are installed, which is not a distinction a version check cares
/// about — but it is one the reader should not throw away, because a panel listing them will.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Runtime,
    Dev,
    Peer,
    Optional,
}

impl DependencyKind {
    /// The section name as it is written in the file.
    pub fn section(self) -> &'static str {
        match self {
            Self::Runtime => "dependencies",
            Self::Dev => "devDependencies",
            Self::Peer => "peerDependencies",
            Self::Optional => "optionalDependencies",
        }
    }

    fn of(section: &str) -> Option<Self> {
        match section {
            "dependencies" => Some(Self::Runtime),
            "devDependencies" => Some(Self::Dev),
            "peerDependencies" => Some(Self::Peer),
            "optionalDependencies" => Some(Self::Optional),
            _ => None,
        }
    }
}

/// One declared dependency, with the span of its **version string** — the inside of the quotes,
/// which is what an accepted upgrade replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    /// The declared range, verbatim: `^1.2.3`, `~4.0.0`, `workspace:*`, `github:owner/repo`.
    pub range: String,
    pub kind: DependencyKind,
    /// Byte offset of the dependency's **name**, for anchoring something to the line.
    pub offset: usize,
    /// 1-based line of the declaration.
    pub line: usize,
    /// Byte range of the version string's contents, quotes excluded.
    pub range_start: usize,
    pub range_end: usize,
}

/// One `scripts` entry, with the span of its **name** — where a run control is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub name: String,
    /// The command line, verbatim.
    pub command: String,
    pub offset: usize,
    pub line: usize,
}

/// What one `package.json` declares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageManifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub dependencies: Vec<Dependency>,
    pub scripts: Vec<Script>,
}

/// Whether `path` is a package manifest.
///
/// The **name**, not the extension: a project is full of `.json` files that are not manifests, and
/// a `tsconfig.json` handed the dependency rules would have every key in it looked up on a
/// registry. `node_modules` is excluded for a different reason — every installed package has one,
/// and none of them is yours to edit.
pub fn is_package_manifest(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if !name.eq_ignore_ascii_case("package.json") {
        return false;
    }
    !path.components().any(|c| c.as_os_str() == "node_modules")
}

/// Read a manifest out of `source`.
///
/// Never fails: a file this cannot understand yields an empty manifest, which every caller already
/// has to handle (a manifest with no dependencies is an ordinary thing).
pub fn parse(source: &str) -> PackageManifest {
    let bytes = source.as_bytes();
    let mut out = PackageManifest::default();
    let mut lines = LineIndex::new(source);

    // The top-level sections, found by their key at depth 1. Depth matters: a `"scripts"` key
    // inside a nested object — `"pnpm": { "scripts": … }` is not a thing but `"workspaces"` and
    // `"exports"` nest arbitrarily — is not the top-level `scripts`, and reading it as one would
    // put run controls on somebody else's data.
    for (key, value_start) in top_level_members(bytes) {
        if key == "name" {
            out.name = string_at(bytes, value_start).map(|(s, _, _)| s);
        } else if key == "version" {
            out.version = string_at(bytes, value_start).map(|(s, _, _)| s);
        } else if key == "scripts" {
            for (name, name_at, val_at) in object_members(bytes, value_start) {
                let Some((command, _, _)) = string_at(bytes, val_at) else { continue };
                out.scripts.push(Script {
                    name,
                    command,
                    offset: name_at,
                    line: lines.line_of(name_at),
                });
            }
        } else if let Some(kind) = DependencyKind::of(&key) {
            for (name, name_at, val_at) in object_members(bytes, value_start) {
                let Some((range, from, to)) = string_at(bytes, val_at) else { continue };
                out.dependencies.push(Dependency {
                    name,
                    range,
                    kind,
                    offset: name_at,
                    line: lines.line_of(name_at),
                    range_start: from,
                    range_end: to,
                });
            }
        }
    }
    out
}

// ── The reader ────────────────────────────────────────────────────────────────

/// Every `"key": ` at the top level of the document's first object, with the offset its value
/// starts at. Stops at the first thing it cannot walk, keeping what came before.
fn top_level_members(bytes: &[u8]) -> Vec<(String, usize)> {
    let Some(open) = skip_ws(bytes, 0).filter(|&i| bytes[i] == b'{') else { return Vec::new() };
    object_members(bytes, open).into_iter().map(|(k, _, v)| (k, v)).collect()
}

/// The members of the object starting at `open` (which must be its `{`): each key, the offset of
/// the key string's contents, and the offset its value starts at.
///
/// Values are **skipped over** rather than parsed, so a member whose value is an array or a nested
/// object does not derail the ones after it.
fn object_members(bytes: &[u8], open: usize) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    if bytes.get(open) != Some(&b'{') {
        return out;
    }
    let mut i = open + 1;
    loop {
        let Some(k) = skip_ws(bytes, i) else { return out };
        match bytes[k] {
            b'}' => return out,
            b',' => {
                i = k + 1;
                continue;
            }
            b'"' => {}
            // Anything else here is a document being typed. Stop, keeping what was read.
            _ => return out,
        }
        let Some((key, key_from, _)) = string_at(bytes, k) else { return out };
        let Some(colon) = skip_ws(bytes, string_end(bytes, k)) else { return out };
        if bytes[colon] != b':' {
            return out;
        }
        let Some(value) = skip_ws(bytes, colon + 1) else { return out };
        out.push((key, key_from, value));
        let Some(after) = skip_value(bytes, value) else { return out };
        i = after;
    }
}

/// The string starting at `at` (which must be its opening quote): its unescaped contents, and the
/// byte range of those contents in the buffer.
fn string_at(bytes: &[u8], at: usize) -> Option<(String, usize, usize)> {
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let from = at + 1;
    let mut i = from;
    // **Bytes**, not chars. A package name or a script command can be any UTF-8, and pushing a raw
    // byte as a `char` would read every multi-byte sequence as Latin-1 — `à` coming back as two
    // characters. Collected as bytes and decoded once at the end, the sequences survive intact.
    let mut text: Vec<u8> = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some((String::from_utf8_lossy(&text).into_owned(), from, i)),
            b'\\' if i + 1 < bytes.len() => {
                // Only the escapes a manifest actually contains. An unknown one is kept as its
                // literal byte, which is what a tolerant reader should do with a file somebody is
                // still typing.
                text.push(match bytes[i + 1] {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    other => other,
                });
                i += 2;
            }
            b => {
                text.push(b);
                i += 1;
            }
        }
    }
    None
}

/// The offset just past the string starting at `at`.
fn string_end(bytes: &[u8], at: usize) -> usize {
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return i + 1,
            b'\\' => i += 2,
            _ => i += 1,
        }
    }
    i
}

/// The offset just past the value starting at `at`, whatever kind it is.
fn skip_value(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes.get(at)? {
        b'"' => Some(string_end(bytes, at)),
        b'{' | b'[' => {
            let (open, close) = if bytes[at] == b'{' { (b'{', b'}') } else { (b'[', b']') };
            let mut depth = 0usize;
            let mut i = at;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => {
                        i = string_end(bytes, i);
                        continue;
                    }
                    b if b == open => depth += 1,
                    b if b == close => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        // A number, `true`, `false`, `null` — everything up to the next separator.
        _ => {
            let mut i = at;
            while i < bytes.len() && !matches!(bytes[i], b',' | b'}' | b']') {
                i += 1;
            }
            Some(i)
        }
    }
}

/// The next non-whitespace offset at or after `i`, or `None` at end of input.
fn skip_ws(bytes: &[u8], i: usize) -> Option<usize> {
    let mut i = i;
    while i < bytes.len() {
        if !bytes[i].is_ascii_whitespace() {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Byte offset → 1-based line, walked once and reused.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { starts }
    }

    fn line_of(&mut self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "name": "@arbor/app",
  "version": "0.3.1",
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build && tsc"
  },
  "dependencies": {
    "svelte": "^5.0.0",
    "@sveltejs/kit": "~2.1.0"
  },
  "devDependencies": {
    "typescript": "5.6.0"
  },
  "workspaces": ["packages/*"],
  "exports": { ".": { "import": "./dist/index.js" } }
}"#;

    #[test]
    fn the_declarations_come_back_with_their_sections_and_their_spans() {
        let m = parse(SAMPLE);
        assert_eq!(m.name.as_deref(), Some("@arbor/app"));
        assert_eq!(m.version.as_deref(), Some("0.3.1"));

        let names: Vec<_> = m.dependencies.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["svelte", "@sveltejs/kit", "typescript"]);
        assert_eq!(m.dependencies[1].kind, DependencyKind::Runtime);
        assert_eq!(m.dependencies[2].kind, DependencyKind::Dev);

        // The span is the version string's CONTENTS — what an accepted upgrade replaces. Checked
        // by slicing the source, because an off-by-one here would eat a quote and produce a file
        // that no longer parses.
        let d = &m.dependencies[0];
        assert_eq!(&SAMPLE[d.range_start..d.range_end], "^5.0.0");
        assert_eq!(d.range, "^5.0.0");

        let scripts: Vec<_> = m.scripts.iter().map(|s| (s.name.as_str(), s.command.as_str())).collect();
        assert_eq!(scripts, [("dev", "vite dev"), ("build", "vite build && tsc")]);
        // The line is the one the name is on, so a control drawn from it lands where it belongs.
        assert_eq!(m.scripts[0].line, 6);
    }

    #[test]
    fn a_nested_scripts_key_is_not_the_scripts_section() {
        // `exports` and `workspaces` nest arbitrarily, and a key matched at any depth would put
        // run controls on somebody else's data. Only depth 1 counts.
        let src = r#"{
  "exports": { "./thing": { "scripts": { "not-a-script": "rm -rf /" } } },
  "scripts": { "real": "echo hi" }
}"#;
        let m = parse(src);
        assert_eq!(m.scripts.len(), 1);
        assert_eq!(m.scripts[0].name, "real");
    }

    #[test]
    fn a_manifest_being_typed_still_answers_about_what_came_before() {
        // The state of the file for most of the time anybody is editing it. A strict parser says
        // nothing at all here, which would blink every hint out exactly while you work.
        let src = r#"{
  "dependencies": {
    "svelte": "^5.0.0",
    "vite": "^6.0
"#;
        let m = parse(src);
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].name, "svelte");
    }

    #[test]
    fn a_manifest_is_recognised_by_name_and_never_inside_node_modules() {
        assert!(is_package_manifest(Path::new("/w/app/package.json")));
        assert!(is_package_manifest(Path::new("/w/app/Package.JSON")));
        // Not every `.json` is a manifest — a `tsconfig.json` handed these rules would have every
        // key in it looked up on a registry.
        assert!(!is_package_manifest(Path::new("/w/app/tsconfig.json")));
        assert!(!is_package_manifest(Path::new("/w/app/package-lock.json")));
        // Every installed package has one, and none of them is yours to edit.
        assert!(!is_package_manifest(Path::new("/w/app/node_modules/svelte/package.json")));
    }

    #[test]
    fn a_value_that_is_not_a_string_is_skipped_without_derailing_the_rest() {
        // npm ignores a non-string dependency value; so does this, and — the part that matters —
        // it keeps reading the ones after it.
        let src = r#"{
  "dependencies": { "a": { "version": "1.0.0" }, "b": "^2.0.0", "c": 3, "d": "~4.0.0" }
}"#;
        let names: Vec<_> = parse(src).dependencies.iter().map(|d| d.name.clone()).collect();
        assert_eq!(names, ["b", "d"]);
    }

    #[test]
    fn a_multibyte_value_survives_being_read() {
        // Bytes accumulated and decoded once, not pushed as chars — the latter reads every
        // multi-byte sequence as Latin-1.
        let src = r#"{ "scripts": { "prova": "echo città —" } }"#;
        assert_eq!(parse(src).scripts[0].command, "echo città —");
    }
}
