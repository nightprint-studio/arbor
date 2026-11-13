//! A `PATH` that holds the tools the user actually has.
//!
//! An app started from Finder (or a desktop launcher) inherits the session's bare `PATH` —
//! `/usr/bin:/bin:/usr/sbin:/sbin` on macOS — and none of what a login shell adds: `~/.cargo/bin`,
//! Homebrew, SDKMAN's Java and Maven. Every child the app starts inherits the same poverty, so a
//! `cargo` that works in the terminal is "command not found" (exit 127) in a process Imber
//! started, and a dependency resolution that shells out to `mvn` cannot find it. Each spawn site
//! fixing it for itself is how the same bug keeps coming back in a new place.
//!
//! So the shell widens its own `PATH` once, before anything is spawned, and every backend and
//! every process they start inherits it. Only **appended**, never prepended: an entry the user
//! really has on `PATH` keeps its precedence.

use std::ffi::OsString;
use std::path::PathBuf;

/// The folders where tools live that a windowed app's `PATH` usually lacks, that exist here.
pub fn tool_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    // rustup's own place for cargo, rustc and every `cargo install`.
    match std::env::var_os("CARGO_HOME").filter(|v| !v.is_empty()) {
        Some(home) => dirs.push(PathBuf::from(home).join("bin")),
        None => dirs.extend(arbor_core::prelude::user_home().map(|h| h.join(".cargo").join("bin"))),
    }
    if let Some(java) = std::env::var_os("JAVA_HOME").filter(|v| !v.is_empty()) {
        dirs.push(PathBuf::from(java).join("bin"));
    }
    if let Some(home) = arbor_core::prelude::user_home() {
        // SDKMAN's current Java, Maven and Gradle, the usual way to have them on a Mac or Linux box.
        for tool in ["java", "maven", "gradle"] {
            dirs.push(home.join(".sdkman").join("candidates").join(tool).join("current").join("bin"));
        }
    }
    dirs.extend(crate::locate::generic_bin_dirs());
    dirs.retain(|d| d.is_dir());
    dirs
}

/// `current` with every folder of `extra` it lacks appended, in order.
pub fn widened(current: Option<OsString>, extra: &[PathBuf]) -> Option<OsString> {
    let mut parts: Vec<PathBuf> = current.as_ref().map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
    let before = parts.len();
    for dir in extra {
        if !parts.iter().any(|p| p == dir) {
            parts.push(dir.clone());
        }
    }
    if parts.len() == before {
        return None;
    }
    std::env::join_paths(parts).ok()
}

/// Widen this process's `PATH` (see the module notes). Call it first thing in `main`, while the
/// process has one thread: changing the environment while another thread reads it is undefined.
pub fn widen_process_path() {
    if let Some(path) = widened(std::env::var_os("PATH"), &tool_dirs()) {
        std::env::set_var("PATH", path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_folders_are_appended_and_present_ones_kept_in_place() {
        let current = std::env::join_paths(["/usr/bin", "/bin"]).unwrap();
        let out = widened(Some(current), &[PathBuf::from("/bin"), PathBuf::from("/opt/tools/bin")]).unwrap();
        let parts: Vec<PathBuf> = std::env::split_paths(&out).collect();
        assert_eq!(parts, vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin"), PathBuf::from("/opt/tools/bin")]);
    }

    #[test]
    fn nothing_to_add_changes_nothing() {
        let current = std::env::join_paths(["/usr/bin"]).unwrap();
        assert!(widened(Some(current), &[PathBuf::from("/usr/bin")]).is_none());
    }
}
