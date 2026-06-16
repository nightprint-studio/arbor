//! ZIP archives: compress several sources into one archive, and extract an
//! archive (with zip-slip path sanitisation). Backed by the `zip` crate.

use std::path::{Path, PathBuf};

use crate::copy::unique_dest;
use crate::error::{FsError, Result};

/// Recursively add `path` to `zip`, naming entries relative to `base` (the
/// parent of the top-level item) with forward slashes (the ZIP convention).
fn zip_add_entry(
    zip: &mut zip::ZipWriter<std::fs::File>,
    base: &Path,
    path: &Path,
    opts: zip::write::SimpleFileOptions,
) -> std::io::Result<()> {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let name = rel.to_string_lossy().replace('\\', "/");
    if path.is_dir() {
        // Store the directory itself so empty folders survive the round-trip.
        if !name.is_empty() {
            zip.add_directory(format!("{name}/"), opts)?;
        }
        for entry in std::fs::read_dir(path)? {
            zip_add_entry(zip, base, &entry?.path(), opts)?;
        }
    } else {
        zip.start_file(name, opts)?;
        let mut f = std::fs::File::open(path)?;
        std::io::copy(&mut f, zip)?;
    }
    Ok(())
}

/// Compress `sources` into a new ZIP archive named `archive_name` inside
/// `dest_dir` (collision-resolved). Returns the created archive path. Each
/// source keeps its own name as the top-level entry; directories recurse.
pub fn zip(sources: &[String], dest_dir: &str, archive_name: &str) -> Result<String> {
    if sources.is_empty() {
        return Err(FsError::Invalid("Nothing to compress".into()));
    }
    let dir = Path::new(dest_dir);
    let out = unique_dest(dir, archive_name);
    let file = std::fs::File::create(&out)
        .map_err(|e| FsError::io("Cannot create archive", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for s in sources {
        let src = Path::new(s);
        let base = src.parent().unwrap_or(dir);
        zip_add_entry(&mut zip, base, src, opts)
            .map_err(|e| FsError::io(format!("Cannot add {s} to archive"), e))?;
    }
    zip.finish()
        .map_err(|e| FsError::Zip(format!("Cannot finalize archive: {e}")))?;
    Ok(out.to_string_lossy().to_string())
}

/// Extract a ZIP `archive` into `dest_dir`, or — when `dest_dir` is omitted —
/// into a new sibling folder named after the archive (collision-resolved).
/// Entry names are sanitised via `enclosed_name` to defeat path-traversal
/// ("zip slip"). Returns the destination folder path.
pub fn unzip(archive: &str, dest_dir: Option<String>) -> Result<String> {
    let arch = Path::new(archive);
    let file = std::fs::File::open(arch)
        .map_err(|e| FsError::io("Cannot open archive", e))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| FsError::Zip(format!("Not a valid ZIP archive: {e}")))?;

    let out_dir = match dest_dir {
        Some(d) => PathBuf::from(d),
        None => {
            let parent = arch.parent().unwrap_or_else(|| Path::new("."));
            let stem = arch
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "extracted".to_string());
            unique_dest(parent, &stem)
        }
    };
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| FsError::io("Cannot create output folder", e))?;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| FsError::Zip(format!("Cannot read archive entry: {e}")))?;
        // Skip entries with unsafe names (absolute paths / `..` traversal).
        let Some(rel) = entry.enclosed_name() else { continue };
        let outpath = out_dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| FsError::io("Cannot create dir", e))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| FsError::io("Cannot create dir", e))?;
            }
            let mut out = std::fs::File::create(&outpath)
                .map_err(|e| FsError::io("Cannot write file", e))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| FsError::io("Cannot extract file", e))?;
        }
    }
    Ok(out_dir.to_string_lossy().to_string())
}
