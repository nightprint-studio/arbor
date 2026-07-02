//! Disk-backed captures library: scan the output dir for produced mp4/png files
//! and expose rename/remove/resolve. Backs the `library` domain handlers.
//!
//! The capture id is the file stem (filenames are unique per the timestamp/uuid
//! template), so operations are stem lookups within the output dir.

use std::path::{Path, PathBuf};

use crate::library::Capture;

const VIDEO_EXT: &str = "mp4";
const IMAGE_EXT: &str = "png";

/// List every capture in `dir`, newest first. Missing dir → empty.
pub fn scan(dir: &Path) -> Vec<Capture> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
        let kind = match ext.as_deref() {
            Some(VIDEO_EXT) => "record",
            Some(IMAGE_EXT) => "screenshot",
            _ => continue,
        };
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        out.push(Capture {
            id: stem.clone(),
            name: stem,
            kind: kind.to_string(),
            target: String::new(),
            duration_ms: None, // not probed (would need an ffprobe pass)
            size_bytes: meta.len(),
            created_at: mtime_ms(&meta),
            path: path.to_string_lossy().to_string(),
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Rename the capture with stem `id` to `name` (extension preserved).
pub fn rename(dir: &Path, id: &str, name: &str) -> Result<(), String> {
    let src = resolve_path(dir, id)?;
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("");
    let clean = sanitize(name);
    if clean.is_empty() {
        return Err("invalid name".to_string());
    }
    let dst = dir.join(if ext.is_empty() { clean } else { format!("{clean}.{ext}") });
    std::fs::rename(&src, &dst).map_err(|e| e.to_string())
}

/// Delete the capture with stem `id`.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let p = resolve_path(dir, id)?;
    std::fs::remove_file(&p).map_err(|e| e.to_string())
}

/// The absolute path of the capture with stem `id`.
pub fn resolve_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
        if !matches!(ext.as_deref(), Some(VIDEO_EXT) | Some(IMAGE_EXT)) {
            continue;
        }
        if path.file_stem().and_then(|s| s.to_str()) == Some(id) {
            return Ok(path);
        }
    }
    Err(format!("capture '{id}' not found"))
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Keep a user-supplied capture name to a safe single filename segment.
fn sanitize(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>()
        .trim()
        .to_string()
}
