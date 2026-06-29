// TODO(prune): merula moved to merula-be — these commands are no longer registered.
//! merula **external libraries**: GitHub-hosted `.merula` modules a project depends
//! on, declared in `merula.toml`'s `[libraries]` table, fetched (pinned to a commit
//! SHA) into a shared content-addressed cache, and imported via
//! `import { … } from "$lib/<name>/<file>"`.
//!
//! ```toml
//! # merula.toml
//! [libraries]
//! drums = "github:octocat/merula-drums@v1.2"   # owner/repo[@ref]; ref optional
//! ```
//!
//! A **sync** resolves each declared library's ref to a commit SHA, downloads that
//! commit's zipball, extracts it (flattened) under
//! `…/merula/libraries/<sha>/`, and records `name → { source, sha }` in the
//! project's `merula.lock` — so a re-sync (or another machine) rebuilds the exact
//! same tree. Imports resolve through the lock: `$lib/<name>/x` → the locked
//! `<sha>`'s cache dir. Public GitHub only; no auth.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use arbor_core::prelude::{client, merula_data_dir};

use crate::error::AppError;
use crate::jobs::{JobInfo, JobRegistry, JobStatus};

// ── Source spec ──────────────────────────────────────────────────────────────

/// A parsed `"github:owner/repo@ref"` (the `github:` prefix and `@ref` are
/// optional; a missing ref means the repo's default branch head).
pub struct GithubSource {
    pub owner: String,
    pub repo: String,
    pub git_ref: String,
}

/// Parse a library source spec. Accepts `github:owner/repo@ref`, `owner/repo@ref`,
/// or `owner/repo` (ref defaults to `HEAD`).
pub fn parse_source(spec: &str) -> Result<GithubSource, String> {
    let s = spec.trim();
    let s = s.strip_prefix("github:").unwrap_or(s).trim();
    let (path, git_ref) = match s.split_once('@') {
        Some((p, r)) => (p.trim(), r.trim().to_string()),
        None => (s, "HEAD".to_string()),
    };
    let (owner, repo) = path
        .split_once('/')
        .ok_or_else(|| format!("invalid library source `{spec}` (expected owner/repo)"))?;
    if owner.is_empty() || repo.is_empty() || git_ref.is_empty() {
        return Err(format!("invalid library source `{spec}`"));
    }
    Ok(GithubSource { owner: owner.to_string(), repo: repo.to_string(), git_ref })
}

// ── Manifest `[libraries]` ────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct LibManifest {
    #[serde(default)]
    libraries: BTreeMap<String, String>,
}

/// The libraries declared in `<dir>/merula.toml` (name → source spec), or empty
/// when there's no manifest / no `[libraries]` table.
pub fn declared(dir: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(dir.join("merula.toml")) else {
        return BTreeMap::new();
    };
    toml::from_str::<LibManifest>(&text).map(|m| m.libraries).unwrap_or_default()
}

// ── Lock file (`merula.lock`) ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct LockFile {
    #[serde(default)]
    libraries: BTreeMap<String, LockEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
struct LockEntry {
    /// The source spec as declared (so a changed spec re-resolves).
    source: String,
    /// The resolved commit SHA the cache dir is keyed by.
    sha: String,
}

fn lock_path(dir: &Path) -> PathBuf {
    dir.join("merula.lock")
}

fn read_lock(dir: &Path) -> LockFile {
    std::fs::read_to_string(lock_path(dir))
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_lock(dir: &Path, lock: &LockFile) -> Result<(), String> {
    let header = "# merula.lock — auto-generated. Pins each [libraries] entry to a commit SHA.\n\n";
    let body = toml::to_string_pretty(lock).map_err(|e| format!("serialize lock: {e}"))?;
    std::fs::write(lock_path(dir), format!("{header}{body}"))
        .map_err(|e| format!("write merula.lock: {e}"))
}

// ── Cache ─────────────────────────────────────────────────────────────────────

/// Root of the shared library cache (`…/merula/libraries`).
fn cache_root() -> PathBuf {
    merula_data_dir().join("libraries")
}

/// The (content-addressed) cache dir for a commit `sha`.
fn cache_dir(sha: &str) -> PathBuf {
    cache_root().join(sha)
}

/// Map every **locked + present** library to its cache dir (name → dir) — what the
/// import loader resolves `$lib/<name>/…` against. Skips entries whose cache is
/// missing (not yet synced) so the import surfaces a clear "not synced" error.
pub fn resolve_dirs(project_dir: &Path) -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    for (name, entry) in read_lock(project_dir).libraries {
        let dir = cache_dir(&entry.sha);
        if dir.is_dir() {
            out.insert(name, dir);
        }
    }
    out
}

// ── Status (FE) ───────────────────────────────────────────────────────────────

/// One library's state for the FE: declared source + (if locked) the pinned SHA
/// and whether its cache is present.
#[derive(Serialize)]
pub struct LibraryStatus {
    pub name: String,
    pub source: String,
    pub sha: Option<String>,
    pub synced: bool,
}

/// The declared libraries with their lock/sync state. `merula_libraries` command.
#[tauri::command]
pub async fn merula_libraries(project_dir: String) -> Result<Vec<LibraryStatus>, AppError> {
    let dir = PathBuf::from(project_dir);
    let lock = read_lock(&dir);
    Ok(declared(&dir)
        .into_iter()
        .map(|(name, source)| {
            let locked = lock.libraries.get(&name);
            // Synced when the lock matches the declared source AND the cache exists.
            let sha = locked.filter(|l| l.source == source).map(|l| l.sha.clone());
            let synced = sha.as_deref().map(|s| cache_dir(s).is_dir()).unwrap_or(false);
            LibraryStatus { name, source, sha, synced }
        })
        .collect())
}

// ── Sync (job-tracked) ────────────────────────────────────────────────────────

/// Start a background sync of `project_dir`'s declared libraries; returns the job
/// id. Resolves each ref → SHA, downloads any missing commit, and rewrites the
/// lock. `merula_sync_libraries` command.
#[tauri::command]
pub async fn merula_sync_libraries(app: AppHandle, project_dir: String) -> Result<String, AppError> {
    Ok(start_sync(&app, PathBuf::from(project_dir)))
}

fn start_sync(app: &AppHandle, project_dir: PathBuf) -> String {
    let state = app.state::<crate::AppState>();
    let job_id = {
        let Ok(mut jobs) = state.jobs.lock() else { return String::new() };
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id: id.clone(),
            name: "Sync libraries".to_string(),
            plugin_name: "merula".to_string(),
            command: "merula_sync_libraries".to_string(),
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("Downloads".to_string()),
            non_cancellable: false,
            hidden: true,
            is_system: false,
            finished_at: None,
            target: Some("merula".to_string()),
        });
        id
    };
    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id": &job_id, "name": "Sync libraries", "plugin_name": "merula",
        "command": "merula_sync_libraries", "category": "Downloads",
        "hidden": true, "target": "merula",
    }));

    let app = app.clone();
    let job = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = sync_all(&app, &project_dir, &job).await;
        finish_job(&app, &job, result);
    });
    job_id
}

/// Terminal outcome of a library sync.
enum SyncOutcome {
    Completed,
    /// The user stopped it via `cancel_job`; libraries already pinned stay pinned.
    Cancelled,
}

/// True once the user cancelled the sync job (Transfers overlay Stop button).
fn is_cancelled(app: &AppHandle, job_id: &str) -> bool {
    app.state::<crate::AppState>()
        .jobs
        .lock()
        .map(|j| j.is_cancelled(job_id))
        .unwrap_or(false)
}

/// Resolve + download every declared library, rewriting the lock as it goes (so a
/// partial run still pins what it fetched). Returns an aggregated error message
/// when one or more libraries fail. Checks `is_cancelled` before each library so
/// `cancel_job` stops the run at the next boundary (what's already fetched stays).
async fn sync_all(app: &AppHandle, project_dir: &Path, job_id: &str) -> Result<SyncOutcome, String> {
    let declared = declared(project_dir);
    if declared.is_empty() {
        log_line(app, job_id, "No [libraries] declared.");
        return Ok(SyncOutcome::Completed);
    }
    let http = client();
    let mut lock = read_lock(project_dir);
    let mut errors: Vec<String> = Vec::new();

    for (name, source) in declared {
        if is_cancelled(app, job_id) {
            log_line(app, job_id, "Cancelled.");
            return Ok(SyncOutcome::Cancelled);
        }
        log_line(app, job_id, &format!("Resolving {name} = {source}"));
        match sync_one(&http, &source).await {
            Ok(sha) => {
                lock.libraries.insert(name.clone(), LockEntry { source, sha: sha.clone() });
                // Persist incrementally so a later failure doesn't lose this pin.
                if let Err(e) = write_lock(project_dir, &lock) {
                    errors.push(format!("{name}: {e}"));
                } else {
                    log_line(app, job_id, &format!("  {name} → {}", &sha[..sha.len().min(10)]));
                }
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }

    if errors.is_empty() { Ok(SyncOutcome::Completed) } else { Err(errors.join("; ")) }
}

/// Resolve one source's ref → SHA and ensure the commit's tree is cached.
async fn sync_one(http: &reqwest::Client, source: &str) -> Result<String, String> {
    let src = parse_source(source)?;
    let sha = resolve_sha(http, &src).await?;
    if !cache_dir(&sha).is_dir() {
        download_commit(http, &src.owner, &src.repo, &sha).await?;
    }
    Ok(sha)
}

/// Resolve `owner/repo@ref` to a full commit SHA via the GitHub commits API
/// (`Accept: …sha` returns the SHA as the response body).
async fn resolve_sha(http: &reqwest::Client, src: &GithubSource) -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/commits/{}",
        src.owner, src.repo, src.git_ref
    );
    let resp = http
        .get(&url)
        .header("Accept", "application/vnd.github.sha")
        .send()
        .await
        .map_err(|e| format!("resolve ref: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub returned {} for {}@{}", resp.status(), src.repo, src.git_ref));
    }
    let sha = resp.text().await.map_err(|e| format!("read sha: {e}"))?.trim().to_string();
    if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("unexpected SHA for {}@{}", src.repo, src.git_ref));
    }
    Ok(sha)
}

/// Download a commit's zipball and extract it (flattened) into `cache_dir(sha)`.
async fn download_commit(http: &reqwest::Client, owner: &str, repo: &str, sha: &str) -> Result<(), String> {
    let url = format!("https://github.com/{owner}/{repo}/archive/{sha}.zip");
    let bytes = http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("download: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download body: {e}"))?;

    let dest = cache_dir(sha);
    let sha_owned = sha.to_string();
    // Zip read + fs writes on a blocking worker (never the async reactor).
    tauri::async_runtime::spawn_blocking(move || extract_flattened(&bytes, &dest, &sha_owned))
        .await
        .map_err(|e| format!("extract task: {e}"))?
}

/// Unzip `bytes` into `dest`, stripping the archive's single top-level
/// `<repo>-<sha>/` directory so files land directly under `dest`.
fn extract_flattened(bytes: &[u8], dest: &Path, sha: &str) -> Result<(), String> {
    use std::io::Cursor;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("read zip: {e}"))?;
    // Extract into a temp sibling, then atomically swap, so a half-written cache
    // dir is never observed as "synced".
    let tmp = dest.with_file_name(format!(".{sha}.tmp"));
    let _ = std::fs::remove_dir_all(&tmp);
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let Some(name) = entry.enclosed_name() else { continue };
        // Drop the leading `<repo>-<sha>/` component.
        let mut comps = name.components();
        comps.next();
        let rel = comps.as_path();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = tmp.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("mkdir: {e}"))?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            let mut f = std::fs::File::create(&out).map_err(|e| format!("create: {e}"))?;
            std::io::copy(&mut entry, &mut f).map_err(|e| format!("extract: {e}"))?;
        }
    }
    let _ = std::fs::remove_dir_all(dest);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir cache: {e}"))?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| format!("commit cache dir: {e}"))?;
    Ok(())
}

// ── Job plumbing ──────────────────────────────────────────────────────────────

fn log_line(app: &AppHandle, job_id: &str, line: &str) {
    let state = app.state::<crate::AppState>();
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.append_output(job_id, line.to_string());
    };
}

fn finish_job(app: &AppHandle, job_id: &str, result: Result<SyncOutcome, String>) {
    let (status, success, error) = match result {
        Ok(SyncOutcome::Completed) => (JobStatus::Completed { exit_code: 0 }, true, None),
        Ok(SyncOutcome::Cancelled) => (JobStatus::Cancelled, false, None),
        Err(e) => {
            tracing::warn!("merula: library sync failed ({job_id}): {e}");
            (JobStatus::Failed { error: e.clone() }, false, Some(e))
        }
    };
    let state = app.state::<crate::AppState>();
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.set_status(job_id, status);
    }
    let _ = app.emit(
        "arbor://job-done",
        serde_json::json!({ "job_id": job_id, "success": success, "error": error }),
    );
}
