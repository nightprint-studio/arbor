//! `project` domain — the merula **project model**: a folder + a `merula.toml`
//! manifest + its `.merula` files.
//!
//! `merula.toml` is small and declarative:
//! ```toml
//! name = "My Song"
//! audience = "for the festival set"
//!
//! [libraries]                        # external GitHub modules (see libraries.rs)
//! drums = "github:octocat/merula-drums@v1"
//! ```
//! Everything is optional; a folder with no `merula.toml` is still a valid project
//! (name = folder name). `.merula` files are discovered by walking the folder
//! recursively. The `[libraries]` table is owned by [`crate::libraries`]; this
//! module only reads `name` / `audience` and lists the project's own files.
//!
//! Pure filesystem work (no audio, no live state); ported verbatim from the
//! shell's `src-tauri/src/merula/project.rs` with the `AppError` mapped to the
//! wire `String`, the `async fn` commands lowered to sync `#[handler]`s (the open
//! is a directory walk, run on the dispatcher's `spawn_blocking` worker), and
//! `create`/`set_name` re-resolving the open inline rather than `.await`-ing it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use merula_core::prelude::MerulaState;

/// Parsed `merula.toml`. Every field optional — a missing manifest is treated as
/// an empty one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Manifest {
    name: Option<String>,
    audience: Option<String>,
    // `[libraries]` (external GitHub modules) is parsed by `crate::libraries`; it
    // is intentionally NOT a field here (serde ignores the unknown table), so this
    // module stays decoupled from the dependency model.
}

/// One `.merula` file in a project (source read lazily on the FE via `fs_*`).
#[derive(Debug, Clone, Serialize)]
pub struct MerulaProjectFile {
    /// Absolute path.
    pub path: String,
    /// Project-relative path (forward slashes), e.g. `lib/drums.merula`.
    pub rel: String,
    /// File name with extension.
    pub name: String,
    /// Listed under `libraries`: imported-only, its `tracks(…)` ignored.
    pub library: bool,
}

/// A merula project manifest (`merula.toml`) + its `.merula` files.
#[derive(Debug, Clone, Serialize)]
pub struct MerulaProjectInfo {
    /// Absolute project folder.
    pub path: String,
    /// `name` from merula.toml (falls back to the folder name).
    pub name: String,
    /// `audience` ("for whom") from merula.toml.
    pub audience: String,
    pub files: Vec<MerulaProjectFile>,
}

/// Open a merula project folder: parse `merula.toml` (or treat it as empty), list
/// its `.merula` files sorted by relative path. The shared open used by `create`
/// / `set_name` too (so they return the freshly-opened project).
fn open_project(dir: &str) -> Result<MerulaProjectInfo, String> {
    let dir = PathBuf::from(dir);
    let manifest = read_manifest(&dir)?;

    let name = manifest.name.clone().unwrap_or_else(|| folder_name(&dir));
    let audience = manifest.audience.clone().unwrap_or_default();

    let mut merula_files: Vec<PathBuf> = Vec::new();
    collect_merula(&dir, &mut merula_files);

    let mut files: Vec<MerulaProjectFile> = merula_files
        .into_iter()
        .filter_map(|p| project_file(&dir, &p))
        .collect();
    files.sort_by(|a, b| a.rel.cmp(&b.rel));

    Ok(MerulaProjectInfo {
        path: dir.to_string_lossy().to_string(),
        name,
        audience,
        files,
    })
}

/// Open a merula project folder (parse `merula.toml`, list `.merula` files).
#[arbor_rpc::handler]
fn merula_open_project(_ctx: &MerulaState, dir: String) -> Result<MerulaProjectInfo, String> {
    open_project(&dir)
}

/// Scaffold a new merula project at `dir`: write `merula.toml` + a starter
/// `song.merula`, then return the opened manifest.
#[arbor_rpc::handler]
fn merula_create_project(
    _ctx: &MerulaState,
    dir: String,
    name: String,
    audience: String,
) -> Result<MerulaProjectInfo, String> {
    let dir_path = PathBuf::from(&dir);
    std::fs::create_dir_all(&dir_path).map_err(|e| e.to_string())?;

    let manifest = format!("name = {}\naudience = {}\n", toml_str(&name), toml_str(&audience));
    std::fs::write(dir_path.join("merula.toml"), manifest).map_err(|e| e.to_string())?;

    // A tiny valid program so the editor opens to something that plays.
    const STARTER: &str =
        "cps(0.5)\n\ntracks(\n  track(\"lead\", n(c4 e4 g4 c5).inst(\"synth.lead\")),\n)\n";
    std::fs::write(dir_path.join("song.merula"), STARTER).map_err(|e| e.to_string())?;

    open_project(&dir)
}

/// Rename a project: set the root `name` key in `<dir>/merula.toml`, preserving
/// everything else (audience, `[libraries]`, comments). Creates a minimal
/// manifest if none exists. Returns the re-opened project.
#[arbor_rpc::handler]
fn merula_set_project_name(
    _ctx: &MerulaState,
    dir: String,
    name: String,
) -> Result<MerulaProjectInfo, String> {
    let manifest_path = PathBuf::from(&dir).join("merula.toml");
    let existing = std::fs::read_to_string(&manifest_path).unwrap_or_default();
    let updated = set_root_name(&existing, &name);
    std::fs::write(&manifest_path, updated).map_err(|e| e.to_string())?;
    open_project(&dir)
}

// ── Internals ────────────────────────────────────────────────────────────────

/// Replace (or insert) the root-level `name = "…"` assignment in a merula.toml,
/// touching nothing else. The root scope ends at the first `[table]` header, so
/// a `name` key inside `[libraries]` (unlikely) is never disturbed.
fn set_root_name(text: &str, name: &str) -> String {
    let assignment = format!("name = {}", toml_str(name));
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    let mut in_root = true;
    for raw in text.lines() {
        let trimmed = raw.trim_start();
        if in_root && trimmed.starts_with('[') {
            in_root = false;
        }
        if in_root && !replaced && is_root_name_assignment(trimmed) {
            out.push(assignment.clone());
            replaced = true;
            continue;
        }
        out.push(raw.to_string());
    }
    if !replaced {
        out.insert(0, assignment);
    }
    let mut s = out.join("\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// A `name = …` assignment line (not a comment, not `names`/`name_x`).
fn is_root_name_assignment(trimmed: &str) -> bool {
    if trimmed.starts_with('#') {
        return false;
    }
    match trimmed.strip_prefix("name") {
        Some(rest) => rest.trim_start().starts_with('='),
        None => false,
    }
}

/// Read + parse `<dir>/merula.toml`. A missing file is an empty manifest (not an
/// error); a present-but-malformed file *is* an error so the user can fix it.
fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let path = dir.join("merula.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| e.to_string()),
        Err(_) => Ok(Manifest::default()),
    }
}

/// Build a [`MerulaProjectFile`] for `path` relative to `dir`.
fn project_file(dir: &Path, path: &Path) -> Option<MerulaProjectFile> {
    let rel = path.strip_prefix(dir).ok()?.to_string_lossy().replace('\\', "/");
    let name = path.file_name()?.to_string_lossy().to_string();
    Some(MerulaProjectFile {
        path: path.to_string_lossy().to_string(),
        // `library` (import-only local files) is deprecated — external modules now
        // live in `[libraries]` (GitHub) and import via `$lib/…`. Kept false so the
        // FE field stays stable.
        library: false,
        rel,
        name,
    })
}

/// Recursively collect `*.merula` files under `dir`.
fn collect_merula(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_merula(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("merula")) {
            out.push(path);
        }
    }
}

/// The folder's own name (the project-name fallback).
fn folder_name(dir: &Path) -> String {
    dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Quote a value as a TOML basic string (escaping `\` and `"`).
fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Setting a name into an empty manifest inserts a root `name = "…"` line.
    #[test]
    fn set_root_name_inserts_into_empty() {
        let out = set_root_name("", "My Song");
        assert_eq!(out, "name = \"My Song\"\n");
    }

    /// Setting a name replaces an existing root `name` and leaves other keys +
    /// the `[libraries]` table untouched.
    #[test]
    fn set_root_name_replaces_and_preserves() {
        let src = "name = \"Old\"\naudience = \"club\"\n\n[libraries]\ndrums = \"github:o/r\"\n";
        let out = set_root_name(src, "New");
        assert!(out.contains("name = \"New\""), "name replaced");
        assert!(!out.contains("\"Old\""), "old name gone");
        assert!(out.contains("audience = \"club\""), "audience preserved");
        assert!(out.contains("[libraries]"), "libraries table preserved");
        assert!(out.contains("drums = \"github:o/r\""), "library entry preserved");
    }

    /// A `name` key *inside* a table is never mistaken for the root name.
    #[test]
    fn set_root_name_ignores_table_scoped_name() {
        let src = "[libraries]\nname = \"not-root\"\n";
        let out = set_root_name(src, "Real");
        // The root assignment is prepended; the in-table `name` is left alone.
        assert!(out.starts_with("name = \"Real\""));
        assert!(out.contains("name = \"not-root\""));
    }

    /// Basic-string quoting escapes backslashes and quotes.
    #[test]
    fn toml_str_escapes() {
        assert_eq!(toml_str("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
