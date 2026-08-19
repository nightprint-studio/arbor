//! Disk-backed captures library: scan the output dir for produced mp4/image files
//! **and frame-sequence directories**, and expose rename/remove/resolve. Backs the
//! `library` domain handlers.
//!
//! The capture id is the file stem (filenames are unique per the timestamp/uuid
//! template), so operations are stem lookups within the output dir. A frame sequence
//! is a *directory* named `<stem>.frames`, which is precisely why it carries that
//! suffix: the stem stays the id and a sequence resolves the same way an mp4 does.

use std::path::{Path, PathBuf};

use crate::library::Capture;

use super::{frames, mp4};

const VIDEO_EXT: &str = "mp4";
const IMAGE_EXTS: [&str; 3] = ["png", "jpg", "webp"];

/// Whether `ext` is one this library shows as a still capture.
fn is_image_ext(ext: &str) -> bool {
    IMAGE_EXTS.contains(&ext)
}

/// List every capture in `dir`, newest first. Missing dir → empty.
pub fn scan(dir: &Path) -> Vec<Capture> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());

        // A frame sequence answers for itself: its manifest already knows the size,
        // the length and what was captured, so the scan never has to walk the frames.
        if frames::is_sequence_dir(&path) {
            if let Some(cap) = sequence_capture(&path, stem) {
                out.push(cap);
            }
            continue;
        }

        let kind = match ext.as_deref() {
            Some(VIDEO_EXT) => "record",
            Some(e) if is_image_ext(e) => "screenshot",
            _ => continue,
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
            // Read from the video's own `moov/mvhd` header — a few seeks, no ffprobe
            // process per file per refresh. `None` only when the header can't be read.
            duration_ms: (kind == "record").then(|| mp4::duration_ms(&path)).flatten(),
            size_bytes: meta.len(),
            created_at: mtime_ms(&meta),
            path: path.to_string_lossy().to_string(),
            poster: None,
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Build the library entry for a frame-sequence directory from its manifest.
fn sequence_capture(path: &Path, stem: String) -> Option<Capture> {
    let m = frames::read_manifest(path).ok()?;
    let poster = path.join(frames::POSTER_NAME);
    Some(Capture {
        id: stem.clone(),
        name: stem,
        kind: "frames".to_string(),
        target: m.target,
        duration_ms: Some(m.duration_ms),
        size_bytes: m.size_bytes,
        created_at: m.created_at,
        path: path.to_string_lossy().to_string(),
        poster: poster.is_file().then(|| poster.to_string_lossy().to_string()),
    })
}

/// Rename the capture with stem `id` to `name` (extension preserved — including a
/// sequence directory's `.frames` suffix, which is what keeps it discoverable).
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

/// Delete the capture with stem `id` — a file, or a whole sequence directory.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let p = resolve_path(dir, id)?;
    if p.is_dir() {
        std::fs::remove_dir_all(&p).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(&p).map_err(|e| e.to_string())
    }
}

/// The absolute path of the capture with stem `id` (an mp4/image file, or a
/// `.frames` directory).
pub fn resolve_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.file_stem().and_then(|s| s.to_str()) != Some(id) {
            continue;
        }
        if frames::is_sequence_dir(&path) {
            return Ok(path);
        }
        let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
        match ext.as_deref() {
            Some(VIDEO_EXT) => return Ok(path),
            Some(e) if is_image_ext(e) => return Ok(path),
            _ => continue,
        }
    }
    Err(format!("capture '{id}' not found"))
}

/// The frame-sequence directory with stem `id`, or an error naming what it is
/// instead. Separate from [`resolve_path`] because "this capture is an mp4" is a
/// different answer than "there is no such capture".
pub fn resolve_sequence(dir: &Path, id: &str) -> Result<PathBuf, String> {
    let p = resolve_path(dir, id)?;
    if frames::is_sequence_dir(&p) {
        Ok(p)
    } else {
        Err(format!("capture '{id}' is not a frame sequence"))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tyto-lib-test-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A minimal but valid sequence directory.
    fn make_sequence(root: &Path, stem: &str) -> PathBuf {
        let dir = root.join(format!("{stem}.{}", frames::DIR_EXT));
        std::fs::create_dir_all(&dir).unwrap();
        let m = serde_json::json!({
            "version": 1, "kind": "tyto-frames", "width": 8, "height": 4,
            "format": "png", "sample_fps": 12, "duration_ms": 1500,
            "created_at": 1_700_000_000_000i64, "target": "Display 1",
            "size_bytes": 2048, "frame_count": 2, "times": [0, 500],
        });
        std::fs::write(dir.join(frames::MANIFEST_NAME), m.to_string()).unwrap();
        dir
    }

    #[test]
    fn a_sequence_is_scanned_from_its_manifest() {
        let root = temp_dir();
        make_sequence(&root, "clip");
        let caps = scan(&root);
        assert_eq!(caps.len(), 1);
        let c = &caps[0];
        assert_eq!(c.id, "clip", "the id is the stem, without the .frames suffix");
        assert_eq!(c.kind, "frames");
        assert_eq!(c.duration_ms, Some(1500), "length comes from the manifest, not an ffprobe");
        assert_eq!(c.size_bytes, 2048, "size too — the frames are never stat'ed");
        assert_eq!(c.target, "Display 1");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn renaming_a_sequence_keeps_it_a_sequence() {
        let root = temp_dir();
        make_sequence(&root, "clip");
        rename(&root, "clip", "tutorial").unwrap();
        assert!(frames::is_sequence_dir(&root.join("tutorial.frames")));
        assert_eq!(scan(&root)[0].id, "tutorial");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_a_sequence_takes_the_whole_directory() {
        let root = temp_dir();
        let dir = make_sequence(&root, "clip");
        std::fs::write(dir.join("frame_000000.png"), b"not really a png").unwrap();
        remove(&root, "clip").unwrap();
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_sequence_says_what_a_non_sequence_is() {
        let root = temp_dir();
        std::fs::write(root.join("shot.png"), b"x").unwrap();
        let err = resolve_sequence(&root, "shot").unwrap_err();
        assert!(err.contains("not a frame sequence"), "got: {err}");
        assert!(resolve_sequence(&root, "nope").unwrap_err().contains("not found"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
