//! Centralised access to the system `git` executable.
//!
//! Git2 covers most read-paths but Arbor still shells out to `git` for
//! several commands (rebase, stash, submodule, recovery snapshots, …) where
//! libgit2 is incomplete or buggy.  This module is the single point of truth
//! for *which* `git` binary those calls invoke:
//!
//!   1. Path explicitly set in `~/.config/arbor/config.toml` (`[git]`).
//!   2. First `git` discovered on the user's `PATH`.
//!   3. PortableGit bundled at `~/.config/arbor/git/cmd/git.exe`
//!      (Windows only — populated by [`download_portable`]).
//!
//! All `git` invocations across the codebase go through [`command`] so that
//! changing the configured path takes effect immediately without restart.
//!
//! ## Plugins are NOT routed through here
//!
//! Lua plugins reach the shell via `arbor.terminal.exec` — that path takes
//! whatever argv string the plugin author wrote.  A plugin that runs
//! `arbor.terminal.exec("git status")` is therefore using the system PATH
//! `git`, NOT the binary configured in Arbor's settings.  This is
//! intentional: silently rewriting plugin commands would change semantics
//! behind the author's back.  The convention — documented in the Plugin
//! Dev docs and in [`crate::plugin::api`] — is that plugins should use
//! the built-in Arbor APIs (e.g. `arbor.repo.fetch_active_tab`,
//! `arbor.repo.clone`) instead of shelling out to `git` themselves.  If
//! the API is missing an operation, the right fix is to extend the API,
//! not to bypass it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// PortableGit download cancellation
// ---------------------------------------------------------------------------
//
// Cancellation is best-effort and cooperative: the running download checks
// this flag at the top of every chunk read and inside the 7z extraction
// loop, returning `AppError::Cancelled` when set.  Reset by
// `download_portable` at start, so each new download begins uncancelled.

static DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);

/// Signal a running PortableGit download to stop at the next checkpoint.
/// No-op if no download is active.  Wired to the `cancel_git_download`
/// Tauri command.
pub fn request_download_cancel() {
    DOWNLOAD_CANCEL.store(true, Ordering::Relaxed);
}

fn reset_download_cancel() { DOWNLOAD_CANCEL.store(false, Ordering::Relaxed); }
fn is_download_cancelled() -> bool { DOWNLOAD_CANCEL.load(Ordering::Relaxed) }

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
    path: None, version: None, source: None,
});

/// Snapshot of the current detection state.
pub fn snapshot() -> GitCliState {
    GIT_CLI.read().map(|g| g.clone()).unwrap_or_default()
}

/// Returns the resolved path or `"git"` so that callers always get a usable
/// `Command`.  Callers should treat a [`snapshot().path`] of `None` as the
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
// HTTP auth injection for CLI shell-outs
// ---------------------------------------------------------------------------

/// Look up the OAuth token / PAT Arbor has stored for the host of `url`,
/// regardless of whether the URL is HTTPS or SSH.  Returns `(username, secret)`
/// where `username == "x-oauth-basic"` means the secret is an OAuth bearer
/// token, anything else is a PAT-style basic-auth pair.
fn token_for_url(url: &str) -> Option<(String, String)> {
    let host = crate::git::url::extract_host(url)?;
    crate::auth::credential_store::resolve_credentials(&host).ok().flatten()
}

fn build_auth_header(username: &str, secret: &str, host: &str) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD as B64};
    // Neither GitHub nor GitLab accept `Authorization: Bearer …` for the
    // smart-HTTP git protocol (Bearer works only for REST APIs).  Both want
    // HTTP Basic with a host-specific sentinel username:
    //   * GitHub → `x-access-token`
    //   * GitLab → `oauth2`   (any forge whose host starts with `gitlab.`)
    //   * other  → `x-access-token` (safe fallback; ignored by token-based forges)
    // For PAT-style credentials (username != "x-oauth-basic") the real
    // username is preserved.
    let user = if username == "x-oauth-basic" {
        if host == "gitlab.com" || host.starts_with("gitlab.") { "oauth2" } else { "x-access-token" }
    } else {
        username
    };
    let basic = B64.encode(format!("{user}:{secret}"));
    format!("Authorization: Basic {basic}")
}

/// Returns `https://host/` prefix that git's `http.<url>.<setting>`
/// matching uses to scope a config option to a specific host.  Falls back
/// to bare scheme+host when no trailing slash is present.
fn url_match_prefix(url: &str) -> Option<String> {
    let host = crate::git::url::extract_host(url)?;
    let scheme = if url.starts_with("https://") { "https" } else { "http" };
    Some(format!("{scheme}://{host}/"))
}

/// Build the global `-c` overrides that inject the right `Authorization`
/// header when shelling out to the git CLI for an HTTPS URL.  Empty when:
///   - the URL isn't HTTP(S) (SSH falls back to ssh-agent / `~/.ssh/`),
///   - or Arbor has no stored token for that host.
///
/// Uses git's host-scoped form (`http.<https://host/>.extraHeader=…`) so the
/// token only travels to the matching host — important when an operation
/// (e.g. `submodule update --recursive`) might hit several remotes.  Also
/// clears the credential-helper chain (host-scoped AND globally) so GCM /
/// other helpers don't pop a UI prompt or double-inject auth headers.
///
/// Auth scheme is always HTTP Basic — GitHub's git/HTTPS endpoint rejects
/// `Authorization: Bearer` for the smart-HTTP protocol (works for REST API
/// only), so we use `x-access-token:<token>` for OAuth tokens and the real
/// `<user>:<pat>` pair for PAT-style credentials.
///
/// Returns args to insert **before the subcommand**, e.g.
/// `git -c http.https://github.com/.extraHeader="Authorization: Basic …" clone <url>`.
///
/// IMPORTANT: the returned vector contains the secret token in plaintext.
/// Callers MUST NOT log it, splice it into job-display strings, or surface
/// it in error messages.  The `Command` itself is fine because Tauri / OS
/// don't echo argv to the user.
pub fn http_auth_args_for_url(url: &str) -> Vec<String> {
    http_auth_args_for_urls(std::slice::from_ref(&url.to_string()))
}

/// Like [`http_auth_args_for_url`] but for several URLs that may span
/// multiple hosts (typical for `git submodule update --recursive`).  One
/// host-scoped `-c` pair per known host with a stored Arbor token; URLs
/// without a token, plus SSH/file URLs, are silently skipped.  Duplicates
/// (multiple URLs with the same host) emit a single config entry.
pub fn http_auth_args_for_urls(urls: &[String]) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut seen_hosts: BTreeSet<String> = BTreeSet::new();
    let mut args: Vec<String> = Vec::new();

    for url in urls {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            continue;
        }
        let Some(host) = crate::git::url::extract_host(url) else { continue; };
        if !seen_hosts.insert(host.clone()) { continue; }
        let Some(prefix) = url_match_prefix(url) else { continue; };
        let Some((username, secret)) = token_for_url(url) else { continue; };
        let header = build_auth_header(&username, &secret, &host);
        args.push("-c".into());
        args.push(format!("http.{prefix}.extraHeader={header}"));
        // Reset the credential-helper chain for this URL so GCM (or any other
        // helper configured globally in ~/.gitconfig) doesn't ALSO inject its
        // own Authorization header — duplicate auth makes GitHub return 400,
        // and an eager helper invocation can show a UI prompt that freezes
        // the WebView until dismissed.  The correct namespace is `credential.*`,
        // NOT `http.*` (which has no `helper` option — the previous form was a
        // silent no-op).  We set both the host-scoped form and a global empty
        // value because some helpers ignore the URL-scoped reset.
        args.push("-c".into());
        args.push(format!("credential.{prefix}.helper="));
    }
    if !args.is_empty() {
        args.push("-c".into());
        args.push("credential.helper=".into());
    }
    args
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
        .map_err(|e| AppError::Other(format!("could not exec '{}': {e}", path.display())))?;
    if !output.status.success() {
        return Err(AppError::Other(format!(
            "'{} --version' exited with {}",
            path.display(), output.status,
        )));
    }
    let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if v.is_empty() {
        return Err(AppError::Other("git --version returned empty output".into()));
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

/// Path where [`download_portable`] extracts PortableGit and where we look for
/// it on subsequent launches.
pub fn portable_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("git")
}

fn portable_executable() -> PathBuf {
    if cfg!(windows) {
        portable_dir().join("cmd").join("git.exe")
    } else {
        portable_dir().join("bin").join("git")
    }
}

/// Re-resolve the path according to the priority order and update the global
/// state.  Returns the resolved snapshot (path may be `None` when nothing is
/// available — the caller is expected to drive the GitSetupModal in that case).
pub fn detect(configured: Option<&Path>) -> GitCliState {
    let mut state = GitCliState::default();

    // 1) Explicit override from config.toml.
    if let Some(p) = configured {
        if p.is_file() {
            if let Ok(v) = verify(p) {
                state.path    = Some(p.to_path_buf());
                state.version = Some(v);
                state.source  = Some("config");
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
            state.path    = Some(p);
            state.version = Some(v);
            state.source  = Some("path");
            store(&state);
            return state;
        }
    }

    // 3) Bundled portable copy.
    let portable = portable_executable();
    if portable.is_file() {
        if let Ok(v) = verify(&portable) {
            state.path    = Some(portable);
            state.version = Some(v);
            state.source  = Some("portable");
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
pub fn download_supported() -> bool { cfg!(windows) }

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
    fn err(s: impl Into<String>) -> AppError { AppError::Other(s.into()) }

    // Reset any prior cancel signal so a fresh attempt isn't aborted before
    // it starts.
    reset_download_cancel();

    on_progress(DownloadProgress {
        stage: "resolving", message: "Querying git-for-windows latest release…".into(), bytes: 0, total: 0,
    });

    let client = reqwest::Client::builder()
        .user_agent("arbor-git-gui")
        .build()
        .map_err(|e| err(format!("http client: {e}")))?;

    // Resolve the latest PortableGit asset URL via the GitHub releases API.
    let release_url = "https://api.github.com/repos/git-for-windows/git/releases/latest";
    let release: serde_json::Value = client
        .get(release_url)
        .header("Accept", "application/vnd.github+json")
        .send().await
        .map_err(|e| err(format!("github api: {e}")))?
        .error_for_status()
        .map_err(|e| err(format!("github api: {e}")))?
        .json().await
        .map_err(|e| err(format!("github api parse: {e}")))?;

    let arch_token = if std::env::consts::ARCH == "x86_64" { "64-bit" } else { "32-bit" };
    let asset = release.get("assets").and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().find(|a| {
            a.get("name").and_then(|n| n.as_str())
                .map(|n| n.starts_with("PortableGit-")
                      && n.contains(arch_token)
                      && n.ends_with(".7z.exe"))
                .unwrap_or(false)
        }))
        .ok_or_else(|| err("no PortableGit asset found in latest release"))?;

    let download_url = asset.get("browser_download_url").and_then(|v| v.as_str())
        .ok_or_else(|| err("asset missing browser_download_url"))?
        .to_string();
    let asset_name = asset.get("name").and_then(|v| v.as_str()).unwrap_or("PortableGit.7z.exe").to_string();
    let asset_size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

    on_progress(DownloadProgress {
        stage: "downloading", message: format!("Downloading {asset_name}…"), bytes: 0, total: asset_size,
    });

    // Download into a temp file inside our config dir so the rename to the
    // final extraction folder stays on the same volume.
    let work_dir = portable_dir();
    std::fs::create_dir_all(&work_dir)
        .map_err(|e| err(format!("create {}: {e}", work_dir.display())))?;
    let installer_path = work_dir.join(format!(".download-{asset_name}"));

    let mut response = client.get(&download_url)
        .send().await
        .map_err(|e| err(format!("download: {e}")))?
        .error_for_status()
        .map_err(|e| err(format!("download: {e}")))?;
    let total = response.content_length().unwrap_or(asset_size);
    let mut out = std::fs::File::create(&installer_path)
        .map_err(|e| err(format!("create installer: {e}")))?;
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    while let Some(chunk) = response.chunk().await
        .map_err(|e| err(format!("download chunk: {e}")))?
    {
        if is_download_cancelled() {
            drop(out);
            let _ = std::fs::remove_file(&installer_path);
            return Err(AppError::Cancelled);
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
        return Err(AppError::Cancelled);
    }

    // Everything from here on is synchronous and CPU/IO heavy (reading the
    // ~50 MB SFX, writing the stripped archive, decompressing ~3500 files,
    // running `git --version`).  Run it on the blocking pool so the Tokio
    // worker thread stays responsive — otherwise the main IPC channel
    // freezes and the cancel button stops working.
    let extract_target = work_dir.clone();
    let installer_path_b = installer_path.clone();
    let extract_target_b = extract_target.clone();
    let join = tokio::task::spawn_blocking(move || -> Result<PathBuf> {
        // Wipe any previous extraction so partial state doesn't shadow the new install.
        if extract_target_b.join("cmd").exists() || extract_target_b.join("bin").exists() {
            on_progress(DownloadProgress {
                stage: "extracting", message: "Removing previous PortableGit…".into(), bytes: 0, total: 0,
            });
            // Best-effort cleanup of the contents (keep the dir itself so it stays the same volume).
            if let Ok(entries) = std::fs::read_dir(&extract_target_b) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p == installer_path_b { continue; }
                    if p.is_dir() { let _ = std::fs::remove_dir_all(&p); }
                    else          { let _ = std::fs::remove_file(&p); }
                }
            }
        }

        if is_download_cancelled() {
            let _ = std::fs::remove_file(&installer_path_b);
            return Err(AppError::Cancelled);
        }

        on_progress(DownloadProgress {
            stage: "extracting", message: "Reading PortableGit archive…".into(), bytes: 0, total: 0,
        });

        // The PortableGit asset is a 7-Zip self-extracting archive: a small PE
        // bootstrapper concatenated with a real `.7z` payload.  Running the .exe
        // directly would pop the bundled 7-Zip GUI extraction dialog (looks like
        // an installer to users), so instead we locate the 7z signature in the
        // file and hand the payload to a pure-Rust extractor.
        const SEVENZ_SIG: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
        let raw = std::fs::read(&installer_path_b)
            .map_err(|e| err(format!("read installer: {e}")))?;
        let offset = raw.windows(SEVENZ_SIG.len())
            .position(|w| w == SEVENZ_SIG)
            .ok_or_else(|| err("7z signature not found in PortableGit installer"))?;
        let archive_path = installer_path_b.with_extension("7z");
        std::fs::write(&archive_path, &raw[offset..])
            .map_err(|e| err(format!("write archive payload: {e}")))?;
        drop(raw);

        if is_download_cancelled() {
            let _ = std::fs::remove_file(&installer_path_b);
            let _ = std::fs::remove_file(&archive_path);
            return Err(AppError::Cancelled);
        }

        // Stream entries one by one so we can emit per-file progress.  We can't
        // know the uncompressed size cheaply, so progress is reported in
        // "files extracted / total files" — total is taken from the 7z header.
        let mut sz = sevenz_rust2::ArchiveReader::open(&archive_path, sevenz_rust2::Password::empty())
            .map_err(|e| err(format!("open 7z: {e}")))?;
        let total_files: u64 = sz.archive().files.iter()
            .filter(|f| !f.is_directory())
            .count() as u64;

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
        }).map_err(|e| err(format!("extract 7z: {e}")))?;

        if cancelled_during {
            let _ = std::fs::remove_file(&installer_path_b);
            let _ = std::fs::remove_file(&archive_path);
            return Err(AppError::Cancelled);
        }

        // Drop the installer + intermediate archive; we don't need them anymore.
        let _ = std::fs::remove_file(&installer_path_b);
        let _ = std::fs::remove_file(&archive_path);

        let exe = portable_executable();
        on_progress(DownloadProgress {
            stage: "verifying", message: format!("Verifying {}…", exe.display()), bytes: 0, total: 0,
        });
        let version = verify(&exe).map_err(|e| {
            err(format!("Extraction succeeded but {} failed: {e}", exe.display()))
        })?;
        let _ = set_path(&exe, "portable");

        on_progress(DownloadProgress {
            stage: "done", message: version, bytes: 0, total: 0,
        });
        Ok(exe)
    }).await;

    let _ = extract_target; // silence unused-binding warning (held for scope clarity)
    join.map_err(|e| err(format!("blocking task panicked: {e}")))?
}

#[cfg(not(windows))]
pub async fn download_portable<F>(_on_progress: F) -> Result<PathBuf>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    Err(AppError::Unsupported(
        "Portable git download is only available on Windows. Install git via your package manager (apt/dnf/brew) and either ensure it's on PATH or set its path manually in Settings → Git CLI.".into(),
    ))
}
