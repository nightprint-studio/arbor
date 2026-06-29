//! libraries_sync — job-tracked sync of a project's declared external `.merula`
//! libraries (`merula.toml`'s `[libraries]`) from public GitHub, pinned to a commit
//! SHA in `merula.lock`.
//!
//! Ported from `src-tauri/src/merula/libraries.rs`'s sync command + plumbing. A
//! sync resolves each declared library's ref → SHA via the GitHub commits API,
//! downloads any missing commit's zipball, extracts it (flattened) into the shared
//! content-addressed cache (`<merula-data>/libraries/<sha>/`), and rewrites the
//! lock — so a re-sync (or another machine) rebuilds the exact same tree. Public
//! GitHub only; **no auth**.
//!
//! The library **listing** (`merula_libraries`) + the lock/declared/cache helpers
//! live in the sibling `libraries` domain (`crate::libraries`); this module owns
//! only the network-bearing sync, tracked as a job in the shell's `JobRegistry`
//! over the reverse channel via [`JobHandle`]. The async fetch runs on a detached
//! `std::thread` with its own current-thread Tokio runtime (no shared runtime handle
//! in `MerulaState`).

use std::path::{Path, PathBuf};

use crate::jobs::{category, JobHandle};
use crate::libraries::{self, LockEntry};
use merula_core::prelude::MerulaState;

// ── Command ───────────────────────────────────────────────────────────────────

/// Start a background sync of `project_dir`'s declared libraries; returns the job
/// id. Resolves each ref → SHA, downloads any missing commit, and rewrites the lock.
#[arbor_rpc::handler]
fn merula_sync_libraries(ctx: &MerulaState, project_dir: String) -> Result<String, String> {
    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_sync_libraries: no reverse channel".to_string())?;
    let job = JobHandle::register(
        host,
        ctx.event_sink(),
        "Sync libraries",
        "merula_sync_libraries",
        category::DOWNLOADS,
    )?;
    let job_id = job.id.clone();

    let project_dir = PathBuf::from(project_dir);
    let spawn = std::thread::Builder::new()
        .name(format!("merula-lib-sync-{job_id}"))
        .spawn(move || {
            let outcome = run_sync(&job, &project_dir);
            match outcome {
                Ok(SyncOutcome::Completed) => job.finish_ok(),
                Ok(SyncOutcome::Cancelled) => job.finish_cancelled(),
                Err(e) => job.finish_failed(e),
            }
        });
    if let Err(e) = spawn {
        return Err(format!("failed to spawn library-sync thread: {e}"));
    }
    Ok(job_id)
}

// ── Sync orchestration ────────────────────────────────────────────────────────

/// Terminal outcome of a library sync.
enum SyncOutcome {
    Completed,
    /// The user stopped it via `cancel_job`; libraries already pinned stay pinned.
    Cancelled,
}

/// Drive the async sync on a fresh current-thread runtime (the worker thread has no
/// ambient reactor).
fn run_sync(job: &JobHandle, project_dir: &Path) -> Result<SyncOutcome, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(sync_all(job, project_dir))
}

/// Resolve + download every declared library, rewriting the lock as it goes (so a
/// partial run still pins what it fetched). Returns an aggregated error message when
/// one or more libraries fail. Checks `is_cancelled` before each library so
/// `cancel_job` stops the run at the next boundary (what's already fetched stays).
async fn sync_all(job: &JobHandle, project_dir: &Path) -> Result<SyncOutcome, String> {
    let declared = libraries::declared(project_dir);
    if declared.is_empty() {
        job.append("No [libraries] declared.");
        return Ok(SyncOutcome::Completed);
    }
    let http = arbor_core::prelude::client();
    let mut lock = libraries::read_lock(project_dir);
    let mut errors: Vec<String> = Vec::new();

    for (name, source) in declared {
        if job.is_cancelled() {
            job.append("Cancelled.");
            return Ok(SyncOutcome::Cancelled);
        }
        job.append(&format!("Resolving {name} = {source}"));
        match sync_one(&http, &source).await {
            Ok(sha) => {
                lock.libraries
                    .insert(name.clone(), LockEntry { source, sha: sha.clone() });
                // Persist incrementally so a later failure doesn't lose this pin.
                if let Err(e) = libraries::write_lock(project_dir, &lock) {
                    errors.push(format!("{name}: {e}"));
                } else {
                    job.append(&format!("  {name} → {}", &sha[..sha.len().min(10)]));
                }
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }

    if errors.is_empty() {
        Ok(SyncOutcome::Completed)
    } else {
        Err(errors.join("; "))
    }
}

/// Resolve one source's ref → SHA and ensure the commit's tree is cached.
async fn sync_one(http: &reqwest::Client, source: &str) -> Result<String, String> {
    let src = libraries::parse_source(source)?;
    let sha = resolve_sha(http, &src).await?;
    if !libraries::cache_dir(&sha).is_dir() {
        download_commit(http, &src.owner, &src.repo, &sha).await?;
    }
    Ok(sha)
}

/// Resolve `owner/repo@ref` to a full commit SHA via the GitHub commits API
/// (`Accept: …sha` returns the SHA as the response body).
async fn resolve_sha(
    http: &reqwest::Client,
    src: &libraries::GithubSource,
) -> Result<String, String> {
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
        return Err(format!(
            "GitHub returned {} for {}@{}",
            resp.status(),
            src.repo,
            src.git_ref
        ));
    }
    let sha = resp
        .text()
        .await
        .map_err(|e| format!("read sha: {e}"))?
        .trim()
        .to_string();
    if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("unexpected SHA for {}@{}", src.repo, src.git_ref));
    }
    Ok(sha)
}

/// Download a commit's zipball and extract it (flattened) into the cache dir.
async fn download_commit(
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<(), String> {
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
    let dest = libraries::cache_dir(sha);
    extract_flattened(&bytes, &dest, sha)
}

/// Unzip `bytes` into `dest`, stripping the archive's single top-level
/// `<repo>-<sha>/` directory so files land directly under `dest`. Extracts into a
/// temp sibling first, then atomically swaps, so a half-written cache dir is never
/// observed as "synced".
fn extract_flattened(bytes: &[u8], dest: &Path, sha: &str) -> Result<(), String> {
    use std::io::Cursor;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("read zip: {e}"))?;
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
