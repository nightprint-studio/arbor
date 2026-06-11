//! The grove **project model**: a folder + a `grove.toml` manifest + its
//! `.grove` files.
//!
//! `grove.toml` is small and declarative:
//! ```toml
//! name = "My Song"
//! audience = "for the festival set"
//! libraries = ["lib/drums.grove"]   # imported-only; their tracks(…) are ignored
//! ```
//! Everything is optional; a folder with no `grove.toml` is still a valid project
//! (name = folder name, no libraries). `.grove` files are discovered by walking
//! the folder recursively; `library` flags those listed under `libraries`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Parsed `grove.toml`. Every field optional — a missing manifest is treated as
/// an empty one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Manifest {
    name: Option<String>,
    audience: Option<String>,
    /// Project-relative paths (forward slashes) of import-only library files.
    libraries: Option<Vec<String>>,
}

/// One `.grove` file in a project (source read lazily on the FE via `fs_*`).
#[derive(Debug, Clone, Serialize)]
pub struct GroveProjectFile {
    /// Absolute path.
    pub path: String,
    /// Project-relative path (forward slashes), e.g. `lib/drums.grove`.
    pub rel: String,
    /// File name with extension.
    pub name: String,
    /// Listed under `libraries`: imported-only, its `tracks(…)` ignored.
    pub library: bool,
}

/// A grove project manifest (`grove.toml`) + its `.grove` files.
#[derive(Debug, Clone, Serialize)]
pub struct GroveProjectInfo {
    /// Absolute project folder.
    pub path: String,
    /// `name` from grove.toml (falls back to the folder name).
    pub name: String,
    /// `audience` ("for whom") from grove.toml.
    pub audience: String,
    pub files: Vec<GroveProjectFile>,
}

/// Open a grove project folder: parse `grove.toml` (or treat it as empty), list
/// its `.grove` files sorted by relative path.
#[tauri::command]
pub async fn grove_open_project(dir: String) -> Result<GroveProjectInfo, AppError> {
    let dir = PathBuf::from(&dir);
    let manifest = read_manifest(&dir)?;

    let name = manifest
        .name
        .clone()
        .unwrap_or_else(|| folder_name(&dir));
    let audience = manifest.audience.clone().unwrap_or_default();
    let libraries = manifest.libraries.clone().unwrap_or_default();

    let mut grove_files: Vec<PathBuf> = Vec::new();
    collect_grove(&dir, &mut grove_files);

    let mut files: Vec<GroveProjectFile> = grove_files
        .into_iter()
        .filter_map(|p| project_file(&dir, &p, &libraries))
        .collect();
    files.sort_by(|a, b| a.rel.cmp(&b.rel));

    Ok(GroveProjectInfo {
        path: dir.to_string_lossy().to_string(),
        name,
        audience,
        files,
    })
}

/// Scaffold a new grove project at `dir`: write `grove.toml` + a starter
/// `song.grove`, then return the opened manifest.
#[tauri::command]
pub async fn grove_create_project(
    dir: String,
    name: String,
    audience: String,
) -> Result<GroveProjectInfo, AppError> {
    let dir_path = PathBuf::from(&dir);
    std::fs::create_dir_all(&dir_path).map_err(|e| AppError::Other(e.to_string()))?;

    let manifest = format!("name = {}\naudience = {}\n", toml_str(&name), toml_str(&audience));
    std::fs::write(dir_path.join("grove.toml"), manifest)
        .map_err(|e| AppError::Other(e.to_string()))?;

    // A tiny valid program so the editor opens to something that plays.
    const STARTER: &str =
        "cps(0.5)\n\ntracks(\n  track(\"lead\", n(c4 e4 g4 c5).inst(\"synth.lead\")),\n)\n";
    std::fs::write(dir_path.join("song.grove"), STARTER)
        .map_err(|e| AppError::Other(e.to_string()))?;

    grove_open_project(dir).await
}

// ── Internals ────────────────────────────────────────────────────────────────

/// Read + parse `<dir>/grove.toml`. A missing file is an empty manifest (not an
/// error); a present-but-malformed file *is* an error so the user can fix it.
fn read_manifest(dir: &Path) -> Result<Manifest, AppError> {
    let path = dir.join("grove.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| AppError::Other(e.to_string())),
        Err(_) => Ok(Manifest::default()),
    }
}

/// Build a [`GroveProjectFile`] for `path` relative to `dir`.
fn project_file(dir: &Path, path: &Path, libraries: &[String]) -> Option<GroveProjectFile> {
    let rel = path
        .strip_prefix(dir)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    let name = path.file_name()?.to_string_lossy().to_string();
    Some(GroveProjectFile {
        path: path.to_string_lossy().to_string(),
        library: libraries.iter().any(|l| l == &rel),
        rel,
        name,
    })
}

/// Recursively collect `*.grove` files under `dir`.
fn collect_grove(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_grove(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("grove")) {
            out.push(path);
        }
    }
}

/// The folder's own name (the project-name fallback).
fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Quote a value as a TOML basic string (escaping `\` and `"`).
fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
