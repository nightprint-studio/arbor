//! Where samply lives and which copy is used.
//!
//! Resolution order: the path the user configured, then the copy Arbor downloaded, then `PATH`.
//! A configured path that does not exist is an **error**, not a reason to fall through — the user
//! named a specific build, and quietly profiling with a different one is worse than saying so.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use arbor_process_ext::prelude::{locate_executable, NoWindowExt};

use super::asset::parse_version_output;
use super::extract::exe_name;

const VERSION_FILE: &str = "version.txt";
const VERSION_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    User,
    Managed,
    Path,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub path: PathBuf,
    pub source: Source,
}

/// `<bennu data>/tools/samply` — the copy Arbor installs.
pub fn managed_dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("tools").join("samply")
}

pub fn managed_binary() -> Option<PathBuf> {
    Some(managed_dir().join(exe_name())).filter(|p| p.is_file())
}

/// The version recorded when the managed copy was installed.
pub fn managed_version() -> Option<String> {
    managed_binary()?;
    let text = std::fs::read_to_string(managed_dir().join(VERSION_FILE)).ok()?;
    Some(text.trim().to_string()).filter(|v| !v.is_empty())
}

pub fn record_version(version: &str) -> Result<(), String> {
    std::fs::write(managed_dir().join(VERSION_FILE), version).map_err(|e| e.to_string())
}

/// Pick the samply to run. `Ok(None)` = none anywhere.
pub fn resolve(user_path: &str) -> Result<Option<Resolved>, String> {
    let user = user_path.trim();
    if !user.is_empty() {
        let p = PathBuf::from(user);
        return if p.is_file() {
            Ok(Some(Resolved { path: p, source: Source::User }))
        } else {
            Err(format!("The samply path in the profiler settings does not exist: {user}"))
        };
    }
    if let Some(p) = managed_binary() {
        return Ok(Some(Resolved { path: p, source: Source::Managed }));
    }
    Ok(locate_executable("samply", None, &[], &[]).map(|p| Resolved { path: PathBuf::from(p), source: Source::Path }))
}

/// Run `<exe> --version` with a timeout and return the version it reports. Blocking — callers are
/// on the blocking pool or their own thread.
pub fn run_version(exe: &Path) -> Result<String, String> {
    let mut cmd = Command::new(exe);
    cmd.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.no_window();
    let mut child = cmd.spawn().map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(s) => break s,
            None if started.elapsed() > VERSION_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{} did not answer `--version` in time", exe.display()));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let mut out = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut s, &mut out);
    }
    if !status.success() {
        return Err(format!("`{} --version` failed — is it really samply?", exe.display()));
    }
    parse_version_output(&out).ok_or_else(|| format!("`{} --version` did not report a samply version", exe.display()))
}

/// Mark the file executable (no-op where there is no exec bit).
pub fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_path_that_is_missing_is_an_error_not_a_fallback() {
        let err = resolve("/definitely/not/here/samply").unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn a_configured_file_wins() {
        let f = std::env::temp_dir().join(format!("arbor-samply-resolve-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        let r = resolve(&f.to_string_lossy()).unwrap().unwrap();
        assert_eq!(r.source, Source::User);
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn running_something_that_is_not_samply_is_reported() {
        let missing = std::env::temp_dir().join("arbor-no-such-samply-binary");
        assert!(run_version(&missing).is_err());
    }
}
