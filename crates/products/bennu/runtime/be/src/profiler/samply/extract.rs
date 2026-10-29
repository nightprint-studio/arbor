//! Unpacking the downloaded archive and finding the `samply` executable in it.
//!
//! The archive is hostile until proven otherwise (it passed a digest, but a digest only says it is
//! the file the publisher meant). A `.zip` goes through `arbor-fs`'s hardened extractor. A
//! `.tar.xz` goes through the system `tar` — the workspace has no tar/xz reader and a new
//! dependency was not part of this feature — after every entry name has been checked here, and the
//! extracted tree is refused if it holds any symlink. Only the one executable is ever copied out of
//! the scratch directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use arbor_fs::prelude::{entry_components, unzip_limited, UnzipLimits};
use arbor_process_ext::prelude::NoWindowExt;

use super::asset::ArchiveKind;

/// Release archives hold one binary and a few docs.
const LIMITS: UnzipLimits = UnzipLimits { max_entries: 200, max_entry_bytes: 256 * 1024 * 1024, max_total_bytes: 512 * 1024 * 1024 };
const MAX_TREE_DEPTH: usize = 4;

pub fn exe_name() -> &'static str {
    if cfg!(windows) { "samply.exe" } else { "samply" }
}

fn tar_command() -> Command {
    let mut c = Command::new("tar");
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.no_window();
    c
}

/// Whether a `tar` that reads `.tar.xz` is on this machine (always true for `.zip`).
pub fn can_extract(kind: ArchiveKind) -> bool {
    match kind {
        ArchiveKind::Zip => true,
        ArchiveKind::TarXz => tar_command().arg("--version").output().is_ok_and(|o| o.status.success()),
    }
}

/// Every name in a `tar -t` listing must stay inside the destination.
pub fn validate_listing(listing: &str) -> Result<(), String> {
    for name in listing.lines().filter(|l| !l.trim().is_empty()) {
        entry_components(name).map_err(|e| format!("the archive was rejected: {e}"))?;
    }
    Ok(())
}

fn extract_tar_xz(archive: &Path, dest: &Path) -> Result<(), String> {
    let missing = "Extracting needs the `tar` command with xz support (on Linux, install the `xz-utils` / `xz` package).";
    let listing = tar_command().arg("-tJf").arg(archive).output().map_err(|e| format!("{missing} ({e})"))?;
    if !listing.status.success() {
        return Err(format!("{missing} tar said: {}", String::from_utf8_lossy(&listing.stderr).trim()));
    }
    validate_listing(&String::from_utf8_lossy(&listing.stdout))?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let out = tar_command().arg("-xJf").arg(archive).arg("-C").arg(dest).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("tar failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// Extract `archive` into the (empty, private) `scratch` directory.
pub fn extract(kind: ArchiveKind, archive: &Path, scratch: &Path) -> Result<(), String> {
    match kind {
        ArchiveKind::Zip => unzip_limited(archive, scratch, &LIMITS, false)
            .map(|_| ())
            .map_err(|e| format!("the archive was rejected: {e}")),
        ArchiveKind::TarXz => extract_tar_xz(archive, scratch),
    }
}

/// The regular file named `exe` in `root` (a few levels down). A symlink anywhere in the tree is an
/// error: a link and a file written through it is the classic way past a name check.
pub fn find_binary(root: &Path, exe: &str) -> Result<PathBuf, String> {
    fn walk(dir: &Path, exe: &str, depth: usize, found: &mut Option<PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err(format!("the archive contains a symbolic link ({}) and was rejected", path.display()));
            }
            if meta.is_dir() {
                if depth < MAX_TREE_DEPTH {
                    walk(&path, exe, depth + 1, found)?;
                }
            } else if meta.is_file() && path.file_name().is_some_and(|n| n == exe) && found.is_none() {
                *found = Some(path);
            }
        }
        Ok(())
    }
    let mut found = None;
    walk(root, exe, 0, &mut found)?;
    found.ok_or_else(|| format!("the archive does not contain `{exe}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arbor-samply-extract-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn listings_with_escaping_names_are_rejected() {
        assert!(validate_listing("samply-x/\nsamply-x/samply\nsamply-x/README.md\n").is_ok());
        for bad in ["../samply", "a/../../b", "/etc/passwd", "C:/x/samply", "a\\..\\b"] {
            assert!(validate_listing(&format!("ok/file\n{bad}\n")).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_binary_is_found_inside_the_shared_folder() {
        let d = scratch("find");
        std::fs::create_dir_all(d.join("samply-x")).unwrap();
        std::fs::write(d.join("samply-x").join("samply"), b"bin").unwrap();
        std::fs::write(d.join("samply-x").join("README.md"), b"doc").unwrap();
        assert_eq!(std::fs::read(find_binary(&d, "samply").unwrap()).unwrap(), b"bin");
        assert!(find_binary(&d, "nothing").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_the_tree_is_refused() {
        let d = scratch("link");
        std::os::unix::fs::symlink("/etc/passwd", d.join("samply")).unwrap();
        assert!(find_binary(&d, "samply").unwrap_err().contains("symbolic link"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
