//! Tauri-free access to the system `git` executable.
//!
//! git2 covers most read-paths but Arbor still shells out to `git` for several
//! commands (rebase, stash, submodule, recovery snapshots, …) where libgit2 is
//! incomplete or buggy. This crate is the single point of truth for *which*
//! `git` binary those calls invoke:
//!
//!   1. Path explicitly set in the app config (`[git] executable_path`).
//!   2. First `git` discovered on the user's `PATH`.
//!   3. PortableGit bundled at [`portable_dir`]`/cmd/git.exe`
//!      (Windows only — populated by [`download_portable`]).
//!
//! All git invocations route through [`command`] so changing the configured
//! path takes effect immediately without restart. The detection state is a
//! process-global ([`detect`] writes it, [`snapshot`] reads it): the shell and
//! the headless `corvus-be` process each own their own instance and self-detect.
//!
//! The keyring-coupled HTTP auth-arg injection (`http_auth_args_for_url`) is
//! **not** here — it reads stored credentials, which never cross into a headless
//! backend, so it stays shell-side.

pub mod error;
pub mod prelude;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use arbor_process_ext::prelude::NoWindowExt;

use crate::error::{GitCliError, Result};

// ---------------------------------------------------------------------------
// PortableGit download cancellation
// ---------------------------------------------------------------------------
//
// Cancellation is best-effort and cooperative: the running download checks this
// flag at the top of every chunk read and inside the 7z extraction loop,
// returning `GitCliError::Cancelled` when set. Reset by `download_portable` at
// start, so each new download begins uncancelled.

static DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);

/// Signal a running PortableGit download to stop at the next checkpoint.
/// No-op if no download is active.
pub fn request_download_cancel() {
    DOWNLOAD_CANCEL.store(true, Ordering::Relaxed);
}

fn reset_download_cancel() {
    DOWNLOAD_CANCEL.store(false, Ordering::Relaxed);
}
fn is_download_cancelled() -> bool {
    DOWNLOAD_CANCEL.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct GitCliState {
    pub path:    Option<PathBuf>,
    pub version: Option<String>,
    /// "config" | "path" | "portable" | "missing"
    pub source:  Option<&'static str>,
}

static GIT_CLI: RwLock<GitCliState> = RwLock::new(GitCliState {
    path: None,
    version: None,
    source: None,
});

/// Snapshot of the current detection state.
pub fn snapshot() -> GitCliState {
    GIT_CLI.read().map(|g| g.clone()).unwrap_or_default()
}

/// Returns the resolved path or `"git"` so that callers always get a usable
/// `Command`. Callers should treat a [`snapshot().path`] of `None` as the
/// "no git found" case and short-circuit before invoking the binary.
fn current_path() -> PathBuf {
    GIT_CLI
        .read()
        .ok()
        .and_then(|g| g.path.clone())
        .unwrap_or_else(|| PathBuf::from("git"))
}

/// Build a pre-configured `Command` (no console window on Windows).
pub fn command() -> Command {
    let mut c = Command::new(current_path());
    c.no_window();
    c
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Run a candidate `git` path with `--version` and capture its trimmed output.
pub fn verify(path: &Path) -> Result<String> {
    let output = Command::new(path)
        .arg("--version")
        .no_window()
        .output()
        .map_err(|e| GitCliError::Other(format!("could not exec '{}': {e}", path.display())))?;
    if !output.status.success() {
        return Err(GitCliError::Other(format!(
            "'{} --version' exited with {}",
            path.display(),
            output.status,
        )));
    }
    let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if v.is_empty() {
        return Err(GitCliError::Other("git --version returned empty output".into()));
    }
    Ok(v)
}

/// Search `PATH` for an executable named `git` (or `git.exe` on Windows).
fn find_on_path() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { "git.exe" } else { "git" };
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Override for [`portable_dir`]. corvus-be — a separate process that never
/// resolves the active profile — sets this to the absolute path the shell pushes,
/// so its PortableGit detection + download target the shell's profile dir instead
/// of recomputing a (wrong) default-profile path. `None` (the shell, in-process)
/// → resolve via the active profile.
static PORTABLE_DIR_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Set the [`portable_dir`] override (corvus-be, from the shell-pushed absolute
/// path). See [`PORTABLE_DIR_OVERRIDE`].
pub fn set_portable_dir_override(dir: PathBuf) {
    if let Ok(mut w) = PORTABLE_DIR_OVERRIDE.write() {
        *w = Some(dir);
    }
}

/// Path where [`download_portable`] extracts PortableGit and where we look for it
/// on subsequent launches — the override when set (corvus-be), else the active
/// profile's git config dir (the shell).
pub fn portable_dir() -> PathBuf {
    if let Some(d) = PORTABLE_DIR_OVERRIDE.read().ok().and_then(|g| g.clone()) {
        return d;
    }
    arbor_core::prelude::arbor_config_path("git")
}

fn portable_executable() -> PathBuf {
    if cfg!(windows) {
        portable_dir().join("cmd").join("git.exe")
    } else {
        portable_dir().join("bin").join("git")
    }
}

/// Re-resolve the path according to the priority order and update the global
/// state. Returns the resolved snapshot (path may be `None` when nothing is
/// available — the caller is expected to drive the GitSetupModal in that case).
pub fn detect(configured: Option<&Path>) -> GitCliState {
    let mut state = GitCliState::default();

    // 1) Explicit override from config.
    if let Some(p) = configured {
        if p.is_file() {
            if let Ok(v) = verify(p) {
                state.path = Some(p.to_path_buf());
                state.version = Some(v);
                state.source = Some("config");
                store(&state);
                return state;
            } else {
                tracing::warn!("configured git path {} failed --version check", p.display());
            }
        } else {
            tracing::warn!("configured git path {} does not exist", p.display());
        }
    }

    // 2) PATH lookup.
    if let Some(p) = find_on_path() {
        if let Ok(v) = verify(&p) {
            state.path = Some(p);
            state.version = Some(v);
            state.source = Some("path");
            store(&state);
            return state;
        }
    }

    // 3) Bundled portable copy.
    let portable = portable_executable();
    if portable.is_file() {
        if let Ok(v) = verify(&portable) {
            state.path = Some(portable);
            state.version = Some(v);
            state.source = Some("portable");
            store(&state);
            return state;
        }
    }

    state.source = Some("missing");
    store(&state);
    state
}

fn store(state: &GitCliState) {
    if let Ok(mut w) = GIT_CLI.write() {
        *w = state.clone();
    }
}

/// Set the path explicitly (after a Browse selection or successful download).
/// Verifies before storing — returns the resolved version string on success.
pub fn set_path(path: &Path, source: &'static str) -> Result<String> {
    let version = verify(path)?;
    let state = GitCliState {
        path:    Some(path.to_path_buf()),
        version: Some(version.clone()),
        source:  Some(source),
    };
    store(&state);
    Ok(version)
}

/// Forget the explicit override and re-run [`detect`] without it.
pub fn clear_override() -> GitCliState {
    detect(None)
}

// ---------------------------------------------------------------------------
// PortableGit download (Windows only)
// ---------------------------------------------------------------------------

/// Whether [`download_portable`] is implemented on the current platform.
pub fn download_supported() -> bool {
    cfg!(windows)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub stage:   &'static str, // "resolving" | "downloading" | "extracting" | "verifying" | "done" | "error"
    pub message: String,
    /// Bytes downloaded so far (only meaningful during `downloading`).
    pub bytes:   u64,
    /// Total expected bytes (only meaningful during `downloading`).
    pub total:   u64,
}

#[cfg(windows)]
pub async fn download_portable<F>(mut on_progress: F) -> Result<PathBuf>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    fn err(s: impl Into<String>) -> GitCliError {
        GitCliError::Other(s.into())
    }

    // Reset any prior cancel signal so a fresh attempt isn't aborted before it
    // starts.
    reset_download_cancel();

    on_progress(DownloadProgress {
        stage: "resolving",
        message: "Querying git-for-windows latest release…".into(),
        bytes: 0,
        total: 0,
    });

    // Build manually (not through arbor_core::prelude::client) because the same
    // client streams the ~50 MB PortableGit SFX below — the standard 30s request
    // timeout would abort the download on slow connections, and reqwest's builder
    // offers no way to clear a timeout once it's set.
    let client = reqwest::Client::builder()
        .user_agent(arbor_core::prelude::USER_AGENT)
        .build()
        .map_err(|e| err(format!("http client: {e}")))?;

    // Resolve the latest PortableGit asset URL via the GitHub releases API.
    let release_url = "https://api.github.com/repos/git-for-windows/git/releases/latest";
    let release: serde_json::Value = client
        .get(release_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| err(format!("github api: {e}")))?
        .error_for_status()
        .map_err(|e| err(format!("github api: {e}")))?
        .json()
        .await
        .map_err(|e| err(format!("github api parse: {e}")))?;

    let arch_token = if std::env::consts::ARCH == "x86_64" { "64-bit" } else { "32-bit" };
    let asset = release
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|a| {
                a.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| {
                        n.starts_with("PortableGit-")
                            && n.contains(arch_token)
                            && n.ends_with(".7z.exe")
                    })
                    .unwrap_or(false)
            })
        })
        .ok_or_else(|| err("no PortableGit asset found in latest release"))?;

    let download_url = asset
        .get("browser_download_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err("asset missing browser_download_url"))?
        .to_string();
    let asset_name = asset
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("PortableGit.7z.exe")
        .to_string();
    let asset_size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

    on_progress(DownloadProgress {
        stage: "downloading",
        message: format!("Downloading {asset_name}…"),
        bytes: 0,
        total: asset_size,
    });

    // Download into a temp file inside our config dir so the rename to the final
    // extraction folder stays on the same volume.
    let work_dir = portable_dir();
    std::fs::create_dir_all(&work_dir)
        .map_err(|e| err(format!("create {}: {e}", work_dir.display())))?;
    let installer_path = work_dir.join(format!(".download-{asset_name}"));

    let mut response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| err(format!("download: {e}")))?
        .error_for_status()
        .map_err(|e| err(format!("download: {e}")))?;
    let total = response.content_length().unwrap_or(asset_size);
    let mut out = std::fs::File::create(&installer_path)
        .map_err(|e| err(format!("create installer: {e}")))?;
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| err(format!("download chunk: {e}")))?
    {
        if is_download_cancelled() {
            drop(out);
            let _ = std::fs::remove_file(&installer_path);
            return Err(GitCliError::Cancelled);
        }
        use std::io::Write;
        out.write_all(&chunk).map_err(|e| err(format!("write installer: {e}")))?;
        downloaded += chunk.len() as u64;
        // Throttle progress events: ~250 KB granularity to avoid drowning the IPC channel.
        if downloaded - last_emit >= 256 * 1024 || downloaded == total {
            on_progress(DownloadProgress {
                stage: "downloading",
                message: format!("Downloading {asset_name}…"),
                bytes: downloaded,
                total,
            });
            last_emit = downloaded;
        }
    }
    drop(out);

    if is_download_cancelled() {
        let _ = std::fs::remove_file(&installer_path);
        return Err(GitCliError::Cancelled);
    }

    // Everything from here on is synchronous and CPU/IO heavy (reading the ~50 MB
    // SFX, writing the stripped archive, decompressing ~3500 files, running
    // `git --version`). Run it on the blocking pool so the Tokio worker thread
    // stays responsive — otherwise the main IPC channel freezes and the cancel
    // button stops working.
    let extract_target = work_dir.clone();
    let installer_path_b = installer_path.clone();
    let extract_target_b = extract_target.clone();
    let join = tokio::task::spawn_blocking(move || -> Result<PathBuf> {
        // Wipe any previous extraction so partial state doesn't shadow the new install.
        if extract_target_b.join("cmd").exists() || extract_target_b.join("bin").exists() {
            on_progress(DownloadProgress {
                stage: "extracting",
                message: "Removing previous PortableGit…".into(),
                bytes: 0,
                total: 0,
            });
            // Best-effort cleanup of the contents (keep the dir itself so it stays the same volume).
            if let Ok(entries) = std::fs::read_dir(&extract_target_b) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p == installer_path_b {
                        continue;
                    }
                    if p.is_dir() {
                        let _ = std::fs::remove_dir_all(&p);
                    } else {
                        let _ = std::fs::remove_file(&p);
                    }
                }
            }
        }

        if is_download_cancelled() {
            let _ = std::fs::remove_file(&installer_path_b);
            return Err(GitCliError::Cancelled);
        }

        on_progress(DownloadProgress {
            stage: "extracting",
            message: "Reading PortableGit archive…".into(),
            bytes: 0,
            total: 0,
        });

        // The PortableGit asset is a 7-Zip self-extracting archive: a small PE
        // bootstrapper concatenated with a real `.7z` payload. Running the .exe
        // directly would pop the bundled 7-Zip GUI extraction dialog (looks like
        // an installer to users), so instead we locate the 7z signature in the
        // file and hand the payload to a pure-Rust extractor.
        const SEVENZ_SIG: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
        let raw = std::fs::read(&installer_path_b).map_err(|e| err(format!("read installer: {e}")))?;
        let offset = raw
            .windows(SEVENZ_SIG.len())
            .position(|w| w == SEVENZ_SIG)
            .ok_or_else(|| err("7z signature not found in PortableGit installer"))?;
        let archive_path = installer_path_b.with_extension("7z");
        std::fs::write(&archive_path, &raw[offset..])
            .map_err(|e| err(format!("write archive payload: {e}")))?;
        drop(raw);

        if is_download_cancelled() {
            let _ = std::fs::remove_file(&installer_path_b);
            let _ = std::fs::remove_file(&archive_path);
            return Err(GitCliError::Cancelled);
        }

        // Stream entries one by one so we can emit per-file progress. We can't
        // know the uncompressed size cheaply, so progress is reported in
        // "files extracted / total files" — total is taken from the 7z header.
        let mut sz = sevenz_rust2::ArchiveReader::open(&archive_path, sevenz_rust2::Password::empty())
            .map_err(|e| err(format!("open 7z: {e}")))?;
        let total_files: u64 = sz.archive().files.iter().filter(|f| !f.is_directory()).count() as u64;

        on_progress(DownloadProgress {
            stage: "extracting",
            message: format!("Extracting 0 / {total_files} files"),
            bytes: 0,
            total: total_files,
        });

        let mut extracted: u64 = 0;
        let mut last_emit: u64 = 0;
        let mut cancelled_during = false;
        sz.for_each_entries(|entry, reader| {
            if is_download_cancelled() {
                cancelled_during = true;
                // Stop iteration cleanly; we treat this as Cancelled below.
                return Ok(false);
            }
            let dest_path = extract_target_b.join(std::path::Path::new(entry.name()));
            if entry.is_directory() {
                std::fs::create_dir_all(&dest_path)?;
                return Ok(true);
            }
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&dest_path)?;
            std::io::copy(reader, &mut out)?;
            extracted += 1;
            // Throttle to ~every 25 files (or the final one) to keep the IPC
            // channel quiet — PortableGit ships ~3500 files.
            if extracted - last_emit >= 25 || extracted == total_files {
                on_progress(DownloadProgress {
                    stage: "extracting",
                    message: format!("Extracting {extracted} / {total_files} files"),
                    bytes: extracted,
                    total: total_files,
                });
                last_emit = extracted;
            }
            Ok(true)
        })
        .map_err(|e| err(format!("extract 7z: {e}")))?;

        if cancelled_during {
            let _ = std::fs::remove_file(&installer_path_b);
            let _ = std::fs::remove_file(&archive_path);
            return Err(GitCliError::Cancelled);
        }

        // Drop the installer + intermediate archive; we don't need them anymore.
        let _ = std::fs::remove_file(&installer_path_b);
        let _ = std::fs::remove_file(&archive_path);

        let exe = portable_executable();
        on_progress(DownloadProgress {
            stage: "verifying",
            message: format!("Verifying {}…", exe.display()),
            bytes: 0,
            total: 0,
        });
        let version = verify(&exe)
            .map_err(|e| err(format!("Extraction succeeded but {} failed: {e}", exe.display())))?;
        let _ = set_path(&exe, "portable");

        on_progress(DownloadProgress {
            stage: "done",
            message: version,
            bytes: 0,
            total: 0,
        });
        Ok(exe)
    })
    .await;

    let _ = extract_target; // silence unused-binding warning (held for scope clarity)
    join.map_err(|e| err(format!("blocking task panicked: {e}")))?
}

#[cfg(not(windows))]
pub async fn download_portable<F>(_on_progress: F) -> Result<PathBuf>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    Err(GitCliError::Unsupported(
        "Portable git download is only available on Windows. Install git via your package manager (apt/dnf/brew) and either ensure it's on PATH or set its path manually in Settings → Git CLI.".into(),
    ))
}
