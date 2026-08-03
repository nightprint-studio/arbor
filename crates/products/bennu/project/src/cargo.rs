//! Lightweight Cargo `Cargo.toml` extraction — the Rust counterpart of [`crate::pom`].
//!
//! Bennu opens a Rust project to **edit** it: tree, go-to-file, find-in-files,
//! highlighting. It needs four things out of the manifest and nothing more: the
//! display name, whether the root is a workspace, its `members` (so the header can
//! list the crates the way it lists Maven modules), and the declared edition.
//!
//! Extracted by targeted section/key scanning rather than pulling in a TOML crate,
//! for the same reason [`crate::pom`] doesn't pull in an XML one: `bennu-project` is
//! a leaf crate whose dependency list is part of its contract, and the shape we need
//! is shallow enough that a parser would be the heavier half of the module. `toml`
//! *is* already a workspace dependency — the day this needs real Cargo metadata
//! (feature graphs, dependency kinds, `workspace = true` inheritance resolved
//! against the root) that is the moment to reach for it, not before.
//!
//! Deliberately tolerant: an unexpected structure yields an absent field, never an
//! error. A `Cargo.toml` that doesn't parse is still a project you can open and fix.

use std::path::Path;

/// The slice of a `Cargo.toml` Bennu reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CargoManifest {
    /// `[package] name`. Empty for a pure virtual workspace manifest (no `[package]`),
    /// where the caller falls back to the directory name.
    pub name: String,
    /// `[package] edition` (or `[workspace.package] edition`), e.g. `"2021"`.
    pub edition: Option<String>,
    /// `true` when the manifest declares a `[workspace]` — a virtual manifest or a
    /// root crate that also owns members.
    pub is_workspace: bool,
    /// `[workspace] members` exactly as written, globs included (`"crates/*"`).
    /// Expand them against the filesystem with [`expand_members`].
    pub members: Vec<String>,
    /// The declared dependency names across `[dependencies]`,
    /// `[dev-dependencies]`, `[build-dependencies]` and
    /// `[workspace.dependencies]`, deduplicated and lowercased.
    pub dependencies: Vec<String>,
}

/// Parse the slice Bennu reads out of raw `Cargo.toml` text.
pub fn parse(text: &str) -> CargoManifest {
    let mut m = CargoManifest::default();
    // The current `[section]` header, dotted-path normalised (`[workspace.package]`
    // stays `workspace.package`); `None` before the first header.
    let mut section: Option<String> = None;
    // `members = [` can span lines; while open, every line feeds the member list.
    let mut members_open = false;

    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if members_open {
            members_open = !collect_array_items(line, &mut m.members);
            continue;
        }

        if let Some(header) = section_header(line) {
            // `[[bin]]` and friends normalise to their table name — we don't read
            // them, but they must still end the previous section.
            section = Some(header.to_string());
            // `[workspace]`, but also `[workspace.package]` / `[workspace.dependencies]`:
            // Cargo requires the bare table for a workspace, so either is proof, and
            // accepting both means an unusual key order can't hide it.
            if header == "workspace" || header.starts_with("workspace.") {
                m.is_workspace = true;
            }
            continue;
        }

        let Some((key, value)) = split_key_value(line) else { continue };
        match (section.as_deref().unwrap_or(""), key) {
            ("package", "name") if m.name.is_empty() => m.name = unquote(value).to_string(),
            ("package" | "workspace.package", "edition") => {
                // A workspace-inherited `edition.workspace = true` carries no version;
                // only a literal string is an answer.
                if let Some(v) = string_value(value) {
                    m.edition = Some(v);
                }
            }
            ("workspace", "members") => {
                members_open = !collect_array_items(value, &mut m.members);
            }
            (
                "dependencies" | "dev-dependencies" | "build-dependencies"
                | "workspace.dependencies",
                _,
            ) => {
                let dep = key.to_ascii_lowercase();
                if !m.dependencies.contains(&dep) {
                    m.dependencies.push(dep);
                }
            }
            _ => {}
        }
    }
    m
}

impl CargoManifest {
    /// Whether any declared dependency name contains `needle` (case-insensitive
    /// substring). The Cargo analogue of [`crate::pom::Pom::has_dependency`].
    pub fn has_dependency(&self, needle: &str) -> bool {
        let n = needle.to_ascii_lowercase();
        self.dependencies.iter().any(|d| d.contains(&n))
    }
}

/// Expand the manifest's `members` against the filesystem, into paths **relative to
/// `root`** with forward slashes — the shape [`ProjectInfo::modules`] carries.
///
/// A trailing-`*` glob (the near-universal `"crates/*"`) lists the directories under
/// that prefix that actually hold a `Cargo.toml`; a plain path is kept when it holds
/// one. Anything that resolves to nothing is dropped rather than reported: a member
/// that isn't there is Cargo's error to raise, and the header listing a crate that
/// doesn't exist would be worse than a shorter list.
///
/// Globs with an interior `*` (`"crates/*/be"`) are not expanded — they're vanishingly
/// rare and half-expanding one would put a literal `*` in the header.
///
/// [`ProjectInfo::modules`]: bennu_proto::prelude::ProjectInfo::modules
pub fn expand_members(root: &Path, members: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for pattern in members {
        let pattern = pattern.trim().trim_end_matches('/');
        if pattern.is_empty() {
            continue;
        }
        match pattern.strip_suffix("/*").or_else(|| pattern.strip_suffix('*')) {
            // `crates/*` → every child dir of `crates` that is itself a crate.
            Some(prefix) if !prefix.contains('*') => {
                let dir = root.join(prefix.trim_end_matches('/'));
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                let mut found: Vec<String> = entries
                    .flatten()
                    .filter(|e| e.path().join("Cargo.toml").is_file())
                    .filter_map(|e| e.file_name().to_str().map(str::to_string))
                    .map(|name| join_rel(prefix, &name))
                    .collect();
                // `read_dir` order is the filesystem's; a header that reshuffles
                // between two opens of the same project reads as a change.
                found.sort();
                push_unique(&mut out, found);
            }
            // An interior glob, or a plain path.
            _ if !pattern.contains('*') && root.join(pattern).join("Cargo.toml").is_file() => {
                push_unique(&mut out, vec![pattern.replace('\\', "/")]);
            }
            _ => {}
        }
    }
    out
}

fn push_unique(out: &mut Vec<String>, items: Vec<String>) {
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
}

fn join_rel(prefix: &str, name: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") }
}

// ── tiny TOML-shaped helpers ─────────────────────────────────────────────────

/// Drop a trailing `#` comment, respecting double-quoted strings (a `#` inside a
/// dependency's `version = "1.0 # not a comment"` is content).
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_string = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_string = !in_string,
            b'#' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

/// The dotted name of a `[section]` / `[[array-of-tables]]` header, or `None`.
fn section_header(line: &str) -> Option<&str> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    // `[[bin]]` → strip the inner pair too.
    let inner = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(inner);
    Some(inner.trim())
}

/// Split `key = value` at the FIRST `=`, trimming both sides and unquoting the key
/// (`"dev-dependencies"` style quoted keys, and `dep = { … }` inline tables keep
/// their value verbatim). `None` for a line that isn't an assignment.
fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let key = unquote(key.trim());
    // A dotted key (`package.name = "x"`) is not a shape we read; skipping it keeps
    // us from mistaking `edition.workspace = true` for an edition.
    if key.is_empty() {
        return None;
    }
    Some((key, value.trim()))
}

/// `value` as a plain string literal, or `None` when it is a bool / number / table
/// (`edition.workspace = true` must not read as the edition `"true"`).
fn string_value(value: &str) -> Option<String> {
    let v = value.trim();
    (v.starts_with('"') && v.ends_with('"') && v.len() >= 2).then(|| unquote(v).to_string())
}

/// Strip one pair of surrounding double (or single) quotes.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// Append the quoted items on `chunk` to `out`. Returns whether the array **closed**
/// on this chunk (a `]` outside a string), so a multi-line `members = [` can be
/// followed across lines.
fn collect_array_items(chunk: &str, out: &mut Vec<String>) -> bool {
    let mut closed = false;
    // A flag + a buffer rather than an `Option<String>`: the state machine has to *end*
    // the current item, and reassigning the option while a match still borrows it is the
    // one shape the borrow checker refuses here.
    let mut in_item = false;
    let mut buf = String::new();

    for ch in chunk.chars() {
        let quote = ch == '"' || ch == '\'';
        if in_item {
            if quote {
                in_item = false;
                let item = std::mem::take(&mut buf);
                if !item.is_empty() && !out.contains(&item) {
                    out.push(item);
                }
            } else {
                buf.push(ch);
            }
        } else if quote {
            in_item = true;
        } else if ch == ']' {
            closed = true;
        }
    }
    closed
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACE: &str = r#"
        [workspace]
        resolver = "2"
        members = [
          "crates/foundation/*",   # a glob
          "crates/products/bennu/be",
        ]

        [workspace.package]
        edition = "2021"

        [workspace.dependencies]
        serde = { version = "1", features = ["derive"] }
        toml  = "0.8"
    "#;

    const CRATE: &str = r#"
        [package]
        name = "bennu-project"
        version.workspace = true
        edition.workspace = true

        [dependencies]
        bennu-proto = { path = "../proto" }
        encoding_rs = "0.8"

        [dev-dependencies]
        tempfile = "3"
    "#;

    #[test]
    fn reads_a_virtual_workspace_manifest() {
        let m = parse(WORKSPACE);
        assert!(m.is_workspace);
        assert_eq!(m.name, "", "a virtual manifest has no [package] name");
        assert_eq!(m.edition.as_deref(), Some("2021"));
        assert_eq!(m.members, vec!["crates/foundation/*", "crates/products/bennu/be"]);
        assert!(m.has_dependency("serde"));
        assert!(m.has_dependency("toml"));
    }

    #[test]
    fn reads_a_leaf_crate_manifest() {
        let m = parse(CRATE);
        assert_eq!(m.name, "bennu-project");
        assert!(!m.is_workspace);
        assert!(m.members.is_empty());
        // `edition.workspace = true` is an inheritance marker, NOT the edition — reading
        // it as one would put "true" in the project header.
        assert_eq!(m.edition, None, "workspace inheritance is not an edition");
        assert!(m.has_dependency("encoding_rs"));
        assert!(m.has_dependency("tempfile"), "dev-dependencies count too");
        assert!(!m.has_dependency("serde"));
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        let m = parse("[package]\nname = \"a#b\" # real comment\n");
        assert_eq!(m.name, "a#b");
    }

    #[test]
    fn members_on_one_line_are_read_too() {
        let m = parse("[workspace]\nmembers = [\"a\", \"b\"]\n[package]\nname = \"x\"\n");
        assert_eq!(m.members, vec!["a", "b"]);
        assert_eq!(m.name, "x", "the section after a closed array still parses");
    }

    #[test]
    fn expands_globs_and_plain_members_against_the_filesystem() {
        let root = temp_dir("expand");
        // crates/a and crates/b are crates; crates/notes is not.
        for rel in ["crates/a", "crates/b", "tools/cli"] {
            std::fs::create_dir_all(root.join(rel)).unwrap();
            std::fs::write(root.join(rel).join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        }
        std::fs::create_dir_all(root.join("crates/notes")).unwrap();

        let members = vec![
            "crates/*".to_string(),
            "tools/cli".to_string(),
            "gone".to_string(), // not on disk → dropped
        ];
        assert_eq!(
            expand_members(&root, &members),
            vec!["crates/a".to_string(), "crates/b".to_string(), "tools/cli".to_string()],
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A unique temp dir for a fixture tree, removed by the caller.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bennu-cargo-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
