//! `samply` — finding it, choosing it, and installing it from the official release.
//!
//! Three things live here:
//!
//! * **the configured path** (`get/set_bennu_profiler_tools`, `bennu_samply_validate`): a user-picked
//!   executable, validated by running `samply --version`;
//! * **status** (`bennu_samply_status`): which copy would be used and why;
//! * **the installer** (`bennu_samply_latest`, `bennu_samply_install`, `bennu_samply_remove`): look
//!   up the latest `mstange/samply` release, ask the user (the frontend shows asset, size, source and
//!   how it will be verified), download with progress, verify the SHA-256, unpack safely and install
//!   under Arbor's data directory.
//!
//! The decisions (which asset, does the digest match, is the path safe) are pure and tested in
//! [`asset`] and [`extract`]; this file is the orchestration.

mod asset;
mod extract;
mod net;
mod store;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use arbor_ipc::prelude::EventSink;
use bennu_core::config::{load, save, ProfilerToolsConfig};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::json;

use asset::{digests_match, parse_checksum_file, pick_asset, Pick, Release};
use store::{resolve, Resolved, Source};


/// Progress / outcome of an install, for the profiler panel.
pub const EVT_INSTALL: &str = "arbor://bennu/samply-install";
/// A release archive is ~5-15 MB; this is a ceiling for a misbehaving server.
const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

// ── configured path ───────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn get_bennu_profiler_tools(_state: &BennuState) -> Result<ProfilerToolsConfig, String> {
    Ok(load().profiler_tools)
}

/// Own pair of handlers rather than a field of the settings snapshot (see `set_bennu_config`).
#[arbor_rpc::handler]
fn set_bennu_profiler_tools(_state: &BennuState, config: ProfilerToolsConfig) -> Result<(), String> {
    let mut current = load();
    current.profiler_tools = ProfilerToolsConfig { samply_path: config.samply_path.trim().to_string() };
    save(&current)
}

/// The path to use for profiling: the configured one, the downloaded one, or the one on `PATH`.
pub fn resolve_configured() -> Result<Option<Resolved>, String> {
    resolve(&load().profiler_tools.samply_path)
}

/// Run `samply --version` on a candidate path; the detected version on success. Runs on the
/// blocking pool (a sync handler), never the UI.
#[arbor_rpc::handler]
fn bennu_samply_validate(_state: &BennuState, path: String) -> Result<String, String> {
    let p = PathBuf::from(path.trim());
    if path.trim().is_empty() || !p.is_file() {
        return Err("That file does not exist.".into());
    }
    store::run_version(&p)
}

#[derive(Serialize)]
pub struct ResolvedOut {
    path: String,
    source: Source,
    version: Option<String>,
}

#[derive(Serialize)]
pub struct SamplyStatus {
    /// Whether a release exists for this OS / architecture.
    supported: bool,
    user_path: String,
    resolved: Option<ResolvedOut>,
    /// Why nothing resolved (a configured path that is gone), when that is the reason.
    problem: Option<String>,
    managed_version: Option<String>,
    installing: bool,
}

#[arbor_rpc::handler]
fn bennu_samply_status(_state: &BennuState) -> Result<SamplyStatus, String> {
    let user_path = load().profiler_tools.samply_path;
    let (resolved, problem) = match resolve(&user_path) {
        Ok(r) => (r, None),
        Err(e) => (None, Some(e)),
    };
    let resolved = resolved.map(|r| ResolvedOut {
        version: store::run_version(&r.path).ok(),
        path: r.path.display().to_string(),
        source: r.source,
    });
    Ok(SamplyStatus {
        supported: !asset::target_triples(std::env::consts::OS, std::env::consts::ARCH).is_empty(),
        user_path,
        resolved,
        problem,
        managed_version: store::managed_version(),
        installing: installing().load(Ordering::SeqCst),
    })
}

// ── latest release ────────────────────────────────────────────────────────────

/// Everything the consent dialog shows.
#[derive(Serialize)]
pub struct LatestOut {
    version: String,
    asset_name: String,
    size: u64,
    url: String,
    /// `api` (the release's own digest), `file` (the published `.sha256`), or `none`.
    checksum_source: &'static str,
    sha256: Option<String>,
    installed_version: Option<String>,
    update_available: bool,
    /// Something that makes installing impossible on this machine, in a sentence.
    blocker: Option<String>,
}

struct Latest {
    release: Release,
    pick: Pick,
    sha256: Option<String>,
    checksum_source: &'static str,
}

async fn fetch_latest() -> Result<Latest, String> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let json = net::get_json(asset::LATEST_API_URL).await?;
    let release = asset::parse_release(&json)?;
    let pick = pick_asset(&release, os, arch).ok_or_else(|| {
        format!(
            "samply {} has no download for {os}/{arch}. Build it with `cargo install samply` and pick it in the profiler settings.",
            release.version
        )
    })?;
    let (sha256, checksum_source) = match (&pick.api_sha256, &pick.checksum_url) {
        (Some(d), _) => (Some(d.clone()), "api"),
        (None, Some(url)) => match net::get_text(url, 4096).await {
            Ok(text) => match parse_checksum_file(&text, &pick.asset.name) {
                Some(d) => (Some(d), "file"),
                None => (None, "none"),
            },
            Err(_) => (None, "none"),
        },
        (None, None) => (None, "none"),
    };
    Ok(Latest { release, pick, sha256, checksum_source })
}

#[arbor_rpc::handler]
async fn bennu_samply_latest(_state: &BennuState) -> Result<LatestOut, String> {
    let latest = fetch_latest().await?;
    let installed = store::managed_version();
    let update_available = installed.as_deref().is_none_or(|v| asset::is_newer(&latest.release.version, v));
    let blocker = (!extract::can_extract(latest.pick.kind)).then(|| {
        "Unpacking needs the `tar` command with xz support, which was not found on this machine. Install it (on Linux, the `xz-utils` / `xz` package) or pick a samply you installed yourself.".to_string()
    });
    Ok(LatestOut {
        version: latest.release.version,
        asset_name: latest.pick.asset.name,
        size: latest.pick.asset.size,
        url: latest.pick.asset.url,
        checksum_source: latest.checksum_source,
        sha256: latest.sha256,
        installed_version: installed,
        update_available,
        blocker,
    })
}

// ── install ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct InstallArgs {
    /// The asset the user agreed to. A newer release appearing in between makes the install stop.
    asset_name: String,
    /// The user explicitly accepted installing without a checksum.
    #[serde(default)]
    allow_unverified: bool,
}

fn installing() -> &'static AtomicBool {
    static FLAG: AtomicBool = AtomicBool::new(false);
    &FLAG
}

/// Clears the in-progress flag on every exit path.
struct InstallGuard;
impl Drop for InstallGuard {
    fn drop(&mut self) {
        installing().store(false, Ordering::SeqCst);
    }
}

/// Removes the scratch directory on every exit path.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn emit(sink: &Arc<dyn EventSink>, payload: serde_json::Value) {
    sink.emit(EVT_INSTALL, payload);
}

/// Start the install on its own thread and return at once; progress and the outcome arrive as
/// [`EVT_INSTALL`] events and the job appears in the Jobs panel.
#[arbor_rpc::handler]
fn bennu_samply_install(ctx: &BennuState, args: InstallArgs) -> Result<(), String> {
    if installing().swap(true, Ordering::SeqCst) {
        return Err("samply is already being installed.".into());
    }
    let guard = InstallGuard;
    let sink = ctx.event_sink();
    let job = ctx.host_caller().and_then(|host| {
        crate::index_service::register_bennu_job(&host, &sink, "Install samply", asset::LATEST_API_URL, "downloads", false)
    });
    std::thread::Builder::new()
        .name("bennu-samply-install".into())
        .spawn(move || {
            let _guard = guard;
            let result = run_install(&sink, &args);
            match &result {
                Ok(version) => emit(&sink, json!({ "kind": "done", "version": version })),
                Err(e) => emit(&sink, json!({ "kind": "error", "error": e })),
            }
            crate::index_service::finish_bennu_job(&sink, job, result.is_ok(), result.err());
        })
        .map(|_| ())
        .map_err(|e| {
            installing().store(false, Ordering::SeqCst);
            format!("could not start the install: {e}")
        })
}

fn run_install(sink: &Arc<dyn EventSink>, args: &InstallArgs) -> Result<String, String> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    emit(sink, json!({ "kind": "phase", "phase": "lookup" }));
    let latest = rt.block_on(fetch_latest())?;
    if latest.pick.asset.name != args.asset_name {
        return Err("A different release was published since you looked. Check for the update again.".into());
    }
    if latest.sha256.is_none() && !args.allow_unverified {
        return Err("This release publishes no checksum, so it was not installed.".into());
    }
    if !extract::can_extract(latest.pick.kind) {
        return Err("Unpacking needs the `tar` command with xz support, which was not found.".into());
    }

    let base = store::managed_dir();
    let parent = base.parent().ok_or("no install location")?.to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    let scratch = Scratch(parent.join(format!(".samply-install-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    std::fs::create_dir_all(&scratch.0).map_err(|e| e.to_string())?;

    let archive = scratch.0.join(format!("download.{}", latest.pick.kind.extension()));
    emit(sink, json!({ "kind": "phase", "phase": "download" }));
    let progress_sink = Arc::clone(sink);
    let actual = rt.block_on(net::download(&latest.pick.asset.url, &archive, MAX_DOWNLOAD_BYTES, &move |done, total| {
        emit(&progress_sink, json!({ "kind": "progress", "done": done, "total": total }));
    }))?;

    emit(sink, json!({ "kind": "phase", "phase": "verify" }));
    if let Some(expected) = &latest.sha256 {
        if !digests_match(expected, &actual) {
            return Err("The download does not match its published checksum, so it was discarded.".into());
        }
    }

    emit(sink, json!({ "kind": "phase", "phase": "extract" }));
    let unpacked = scratch.0.join("unpacked");
    extract::extract(latest.pick.kind, &archive, &unpacked)?;
    let binary = extract::find_binary(&unpacked, extract::exe_name())?;
    let version = install_binary(&binary, &base, &latest.release.version)?;
    Ok(version)
}

/// Copy the unpacked executable into place (write beside, then rename) and record its version.
fn install_binary(binary: &Path, dir: &Path, fallback_version: &str) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let target = dir.join(extract::exe_name());
    let staged = dir.join(format!("{}.new", extract::exe_name()));
    std::fs::copy(binary, &staged).map_err(|e| format!("cannot write {}: {e}", staged.display()))?;
    store::make_executable(&staged)?;
    // Refuse a binary that cannot even say its version (wrong architecture, missing libraries).
    let version = match store::run_version(&staged) {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("The downloaded samply does not run on this machine: {e}"));
        }
    };
    if let Err(e) = std::fs::rename(&staged, &target) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("cannot replace {} (is a profile recording running?): {e}", target.display()));
    }
    store::record_version(if version.is_empty() { fallback_version } else { &version })?;
    Ok(version)
}

/// Delete the copy Arbor installed (never a path the user configured).
#[arbor_rpc::handler]
fn bennu_samply_remove(_state: &BennuState) -> Result<(), String> {
    if installing().load(Ordering::SeqCst) {
        return Err("samply is being installed right now.".into());
    }
    match std::fs::remove_dir_all(store::managed_dir()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove samply: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_flag_is_released_by_the_guard() {
        installing().store(true, Ordering::SeqCst);
        drop(InstallGuard);
        assert!(!installing().load(Ordering::SeqCst));
    }

    #[test]
    fn scratch_is_removed_on_drop() {
        let d = std::env::temp_dir().join(format!("arbor-samply-scratch-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        drop(Scratch(d.clone()));
        assert!(!d.exists());
    }
}
