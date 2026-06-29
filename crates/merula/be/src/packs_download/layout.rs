//! Per-pack `registry.toml` generation: how an extracted archive's file tree maps
//! to merula sound-registry entries. The **install-time** half of the packs domain
//! (the [`Layout`] *data* lives in `crate::packs`; the tree-walking generators that
//! consume it live here).
//!
//! Layouts covered:
//! - [`Layout::SfzTree`] — a tree of `.sfz` instruments, each named
//!   `<parent-folder>.<file-stem>` (a dotted `bank.instrument`).
//! - [`Layout::FolderOfWavs`] — Strudel/Tidal's model: folders of `.wav` variants,
//!   one sound name per leaf folder (`bd`, `casio`, …; drum machines
//!   `RolandTR808_bd`). Each folder becomes a `kind=sample` `dir=` entry.
//! - [`Layout::VersilianWavTree`] — VSCO 2 / VCSL raw-wav trees, indexed by
//!   [`super::versilian`].
//! - [`Layout::Sf2`] — General MIDI; converted directly by [`super::gm`], so it
//!   never reaches the tree-walking generators here.
//!
//! The generated `registry.toml` lives at the archive root, so every `file =` /
//! `dir =` path is written relative to that root.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::packs::Layout;

/// Generate the `registry.toml` body for `root`, returning `(toml, entry_count)`.
pub fn generate(root: &Path, layout: Layout) -> (String, usize) {
    match layout {
        Layout::SfzTree => generate_sfz_tree(root),
        Layout::FolderOfWavs { strip_segments, joiner } => {
            generate_folder_of_wavs(root, strip_segments, joiner)
        }
        // The GM pack converts a .sf2 directly (see `super::gm`); it never reaches
        // the tree-walking generators.
        Layout::Sf2 => (String::new(), 0),
        // VSCO 2 / VCSL ship raw wavs — build the missing layer from the filenames.
        Layout::VersilianWavTree => super::versilian::generate(root),
    }
}

// ── SfzTree (VSCO 2) ──────────────────────────────────────────────────────────

/// Scan `root` for `.sfz` instruments. Each is named `<parent-folder>.<file-stem>`
/// (a dotted `bank.instrument`), with its `.sfz` path relative to `root`.
fn generate_sfz_tree(root: &Path) -> (String, usize) {
    let mut sfz: Vec<PathBuf> = Vec::new();
    collect_by_ext(root, "sfz", &mut sfz);
    sfz.sort();

    let mut out = String::from("# Auto-generated sound registry (merula).\n\n");
    let mut seen: HashSet<String> = HashSet::new();
    let mut count = 0;
    for path in &sfz {
        let Ok(rel) = path.strip_prefix(root) else { continue };
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
        let bank = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string());
        let (Some(stem), Some(bank)) = (stem, bank) else { continue };
        // Dotted names are case-folded (the SFZ banks are a flat namespace).
        let name = format!("{}.{}", sanitize_lower(&bank), sanitize_lower(&stem));
        if !seen.insert(name.clone()) {
            continue;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        out.push_str(&format!("[\"{name}\"]\nkind = \"sfz\"\nfile = \"{rel_str}\"\n\n"));
        count += 1;
    }
    (out, count)
}

// ── FolderOfWavs (Dirt-Samples, drum machines) ────────────────────────────────

/// Map every folder that directly contains audio files to a `kind=sample` `dir=`
/// entry. The name is the folder's path relative to `root`, with `strip_segments`
/// leading components dropped and the rest joined by `joiner` (case preserved, to
/// match Strudel's spelling like `RolandTR808_bd`).
fn generate_folder_of_wavs(root: &Path, strip_segments: usize, joiner: &str) -> (String, usize) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    collect_sample_dirs(root, &mut dirs);
    dirs.sort();

    let mut out = String::from("# Auto-generated sound registry (merula).\n\n");
    let mut seen: HashSet<String> = HashSet::new();
    let mut count = 0;
    for dir in &dirs {
        let Ok(rel) = dir.strip_prefix(root) else { continue };
        let comps: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        if comps.len() <= strip_segments {
            continue;
        }
        let name = comps[strip_segments..]
            .iter()
            .map(|c| sanitize_token(c))
            .collect::<Vec<_>>()
            .join(joiner);
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        out.push_str(&format!("[\"{name}\"]\nkind = \"sample\"\ndir = \"{rel_str}\"\n\n"));
        count += 1;
    }
    (out, count)
}

/// Collect every directory under `root` (inclusive) that directly contains at least
/// one decodable audio file.
fn collect_sample_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut has_audio = false;
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if is_audio(&path) {
            has_audio = true;
        }
    }
    if has_audio {
        out.push(dir.to_path_buf());
    }
    for sub in subdirs {
        collect_sample_dirs(&sub, out);
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Recursively collect files with extension `ext` (case-insensitive) under `dir`.
fn collect_by_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_by_ext(&path, ext, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            out.push(path);
        }
    }
}

/// Whether a path is an audio file merula can decode (matches the engine's set).
fn is_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("wav" | "wave" | "flac" | "ogg" | "mp3")
    )
}

/// Case-folding token sanitiser (spaces / odd chars → `_`), for the flat SFZ bank
/// namespace.
fn sanitize_lower(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

/// Case-preserving token sanitiser (only non-alphanumerics → `_`), so folder pack
/// names match Strudel exactly (`RolandTR808_bd`, `casio`).
fn sanitize_token(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_name_strips_segments_and_preserves_case() {
        // drum-machines: machines/RolandTR808/bd → RolandTR808_bd (strip `machines`).
        let root = Path::new("/x");
        let dir = Path::new("/x/machines/RolandTR808/bd");
        let rel = dir.strip_prefix(root).unwrap();
        let comps: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        let name = comps[1..]
            .iter()
            .map(|c| sanitize_token(c))
            .collect::<Vec<_>>()
            .join("_");
        assert_eq!(name, "RolandTR808_bd");
    }
}
