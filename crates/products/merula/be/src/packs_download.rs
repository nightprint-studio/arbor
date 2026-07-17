//! packs_download — the **install-time** half of the packs domain: job-tracked
//! download + extract + index, plus reindex / delete, for the downloadable sample
//! packs (VSCO 2, VCSL, Dirt-Samples, drum machines, GM).
//!
//! Ported from `src-tauri/src/merula/packs/{download,layout,gm,versilian}.rs` + the
//! pack-management commands in `src-tauri/src/merula/mod.rs`. The sibling `packs`
//! read surface (`crate::packs`) owns the [`Pack`](crate::packs::Pack) descriptor
//! table, `pack_dir`, install status/listing, the install-marker reader, and the
//! per-profile active-pack allow-list; this module owns everything that *writes* a
//! pack to disk:
//! - streaming the GitHub archive (hashing as it goes, throttled `pack_progress`),
//! - the [`layout`] tree-walkers ([`gm`] for the GM `.sf2`, [`versilian`] for the
//!   VSCO/VCSL wav trees, folder-of-wavs / sfz-tree for the rest),
//! - writing the install marker + auto-activating the freshly-installed pack,
//! - [`reindex`] (regenerate `registry.toml` from the extracted tree) + [`delete`].
//!
//! Cancellation is cooperative: the download loop polls `JobHandle::is_cancelled`
//! per chunk (a `__job_is_cancelled` reverse-channel round-trip), and the GM
//! converter polls a passed-in cancel closure per preset. The async stream runs on a
//! detached `std::thread` with its own current-thread Tokio runtime (no shared
//! runtime handle in `MerulaState`); the audio RT thread / the dispatcher worker
//! must never block on IO.

mod gm;
mod layout;
mod versilian;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arbor_ipc::prelude::EventSink;

use merula_core::config::{self as config_cmds, MerulaConfig};
use merula_core::events::{self, PackProgress, EVT_PACK_PROGRESS};
use crate::jobs::{category, percent_of, JobHandle, ProgressThrottle};
use crate::packs::{self, Layout, Pack};
use merula_core::prelude::MerulaState;

// ── Commands ──────────────────────────────────────────────────────────────────

/// Start downloading + installing a sample pack by id (job-tracked). Returns the
/// job id; cancel via the standard `cancel_job`. `Err` for an unknown id.
#[arbor_rpc::handler]
fn merula_pack_download(ctx: &MerulaState, pack_id: String) -> Result<String, String> {
    let cfg = config_cmds::load();
    let pack = packs::pack(&pack_id).ok_or_else(|| format!("unknown sample pack `{pack_id}`"))?;

    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_pack_download: no reverse channel".to_string())?;
    let job = JobHandle::register(
        host,
        ctx.event_sink(),
        &format!("Download {} sample bank", pack.name),
        pack.archive_url,
        category::DOWNLOADS,
    )?;
    let job_id = job.id.clone();

    let sink = ctx.event_sink();
    let pack_id = pack_id.clone();
    let spawn = std::thread::Builder::new()
        .name(format!("merula-pack-dl-{job_id}"))
        .spawn(move || {
            let outcome = run_install(&job, &sink, &cfg, &pack_id);
            match outcome {
                Ok(Outcome::Completed) => job.finish_ok(),
                Ok(Outcome::Cancelled) => job.finish_cancelled(),
                Err(e) => job.finish_failed(e),
            }
        });
    if let Err(e) = spawn {
        return Err(format!("failed to spawn pack-download thread: {e}"));
    }
    Ok(job_id)
}

/// Re-index an already-installed pack: rebuild its `registry.toml` from the
/// extracted files on disk (no re-download), refreshing the instruments it exposes.
/// Returns the updated status; the caller re-reads packs + sounds. Pure FS work
/// (walking the tree + writing the manifest) — runs inline on the request worker.
#[arbor_rpc::handler]
fn merula_pack_reindex(_ctx: &MerulaState, pack_id: String) -> Result<packs::PackStatus, String> {
    let cfg = config_cmds::load();
    reindex(&cfg, &pack_id)
}

/// Delete an installed sample pack from disk (its whole install dir). No-op for an
/// unknown id; an already-absent pack succeeds. The caller re-reads the pack list +
/// sound registry afterwards. (`merula_pack_set_active` lives in the `packs` read
/// surface, which owns the active-pack allow-list.)
#[arbor_rpc::handler]
fn merula_pack_delete(_ctx: &MerulaState, pack_id: String) -> Result<(), String> {
    let cfg = config_cmds::load();
    delete(&cfg, &pack_id)
}

// ── Download + install worker ─────────────────────────────────────────────────

/// Terminal outcome of a pack download/install.
enum Outcome {
    Completed,
    Cancelled,
}

/// Drive the async download on a fresh current-thread runtime, then extract + index
/// synchronously, write the install marker, and auto-activate the pack.
fn run_install(
    job: &JobHandle,
    sink: &Arc<dyn EventSink>,
    cfg: &MerulaConfig,
    pack_id: &str,
) -> Result<Outcome, String> {
    let pack = packs::pack(pack_id).ok_or_else(|| format!("unknown sample pack `{pack_id}`"))?;
    let dir = packs::pack_dir(cfg, pack_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let archive_path = dir.join("archive.zip");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;

    // 1. Stream the archive (hashing as we go), with throttled `pack_progress`.
    let sha256 =
        match rt.block_on(stream_archive(job, sink, pack_id, pack.archive_url, &archive_path)) {
            Ok(Some(sha)) => sha,
            // Cancelled mid-download: the partial archive was removed by the streamer.
            Ok(None) => return Ok(Outcome::Cancelled),
            Err(e) => return Err(e),
        };

    // 2. Extract + index off the runtime (sync zip + fs walk + `.sf2` parse). The
    //    progress callback re-emits `pack_progress` for the extract phase; the cancel
    //    poll round-trips the job registry, so a mid-extract Stop still aborts.
    let (size_bytes, instrument_count, registry_rel) =
        match extract_and_index(job, sink, pack, &dir, &archive_path, pack_id) {
            Ok(triple) => triple,
            Err(e) if e == "cancelled" => {
                let _ = std::fs::remove_file(&archive_path);
                return Ok(Outcome::Cancelled);
            }
            Err(e) => return Err(e),
        };

    // 3. Remove the archive; write the install marker.
    let _ = std::fs::remove_file(&archive_path);
    let manifest = packs::InstallManifest {
        url: pack.archive_url.to_string(),
        sha256,
        size_bytes,
        instrument_count,
        registry_rel,
    };
    write_manifest(&dir, &manifest)?;

    // 4. Auto-activate the freshly-downloaded pack: if an allow-list already exists,
    //    append this id so the user never loses access to a pack they just installed
    //    (no-op when no allow-list exists — that state is already all-active).
    let installed_ids = packs::installed_ids(cfg);
    packs::active_packs::on_pack_installed(pack_id, &installed_ids);

    Ok(Outcome::Completed)
}

/// Unzip the archive into `dir`, generate the pack's `registry.toml`, and return
/// `(extracted_bytes, instrument_count, registry_rel)`. The GM pack downloads a
/// single `.sf2`, converted directly (wav + SFZ) by [`gm`] instead of unzipping a
/// tree. Returns `Err("cancelled")` when the user stopped it mid-extract.
fn extract_and_index(
    job: &JobHandle,
    sink: &Arc<dyn EventSink>,
    pack: &Pack,
    dir: &Path,
    archive_path: &Path,
    pack_id: &str,
) -> Result<(u64, usize, String), String> {
    // GM: convert the `.sf2` directly (it writes its own `registry.toml`).
    if matches!(pack.layout, Layout::Sf2) {
        let cancel_job = job.clone_handle();
        let is_cancelled = move || cancel_job.is_cancelled();
        let sink_cb = Arc::clone(sink);
        let job_cb = job.clone_handle();
        let pack_id_owned = pack_id.to_string();
        let emit = move |phase: &str, done: u64, total: u64| {
            emit_pack_progress(&sink_cb, &job_cb, &pack_id_owned, phase, done, total);
        };
        return gm::convert(dir, archive_path, pack, &is_cancelled, &emit);
    }

    let file = std::fs::File::open(archive_path).map_err(|e| format!("open archive: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("read archive: {e}"))?;

    let count = zip.len();
    let mut extracted_bytes: u64 = 0;
    let mut root: Option<PathBuf> = None;
    let mut throttle = ProgressThrottle::default();

    for i in 0..count {
        if job.is_cancelled() {
            return Err("cancelled".to_string());
        }
        let mut entry = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out_path = dir.join(&rel);
        // Track the archive's top-level dir (`<repo>-<ref>/`).
        if root.is_none() {
            if let Some(first) = rel.components().next() {
                root = Some(dir.join(first.as_os_str()));
            }
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| format!("mkdir: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            let mut out =
                std::fs::File::create(&out_path).map_err(|e| format!("create file: {e}"))?;
            extracted_bytes +=
                std::io::copy(&mut entry, &mut out).map_err(|e| format!("extract: {e}"))?;
        }
        emit_throttled(sink, job, pack_id, "extracting", i as u64 + 1, count as u64, &mut throttle);
    }

    let root = root.ok_or_else(|| "empty archive".to_string())?;
    let (toml, instrument_count) = layout::generate(&root, pack.layout);
    let registry_path = root.join("registry.toml");
    std::fs::write(&registry_path, toml).map_err(|e| format!("write registry: {e}"))?;
    let registry_rel = registry_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| "registry.toml".to_string());

    Ok((extracted_bytes, instrument_count, registry_rel))
}

/// Re-index an installed pack from its extracted tree (no network). Regenerates
/// `registry.toml` via the pack's [`Layout`] and rewrites the install marker's
/// instrument count, leaving the downloaded samples untouched.
///
/// `Err` when the pack isn't installed, its extracted tree is gone, or its layout
/// can't rebuild from the tree (the GM `.sf2` is deleted post-install, so GM can only
/// be rebuilt by re-downloading).
pub fn reindex(cfg: &MerulaConfig, pack_id: &str) -> Result<packs::PackStatus, String> {
    let pack = packs::pack(pack_id).ok_or_else(|| format!("unknown sample pack `{pack_id}`"))?;
    let dir = packs::pack_dir(cfg, pack_id);
    let manifest =
        packs::read_manifest(&dir).ok_or_else(|| format!("pack `{pack_id}` is not installed"))?;
    if matches!(pack.layout, Layout::Sf2) {
        return Err(
            "re-indexing isn't supported for General MIDI — re-download it to rebuild".to_string(),
        );
    }
    // The extracted tree's root is the parent of the registry path (`<repo>-<ref>/`).
    let registry_path = dir.join(&manifest.registry_rel);
    let root = registry_path
        .parent()
        .ok_or_else(|| "malformed install marker (no registry parent)".to_string())?
        .to_path_buf();
    if !root.exists() {
        return Err("the pack's extracted files are missing — re-download it".to_string());
    }

    let (toml, instrument_count) = layout::generate(&root, pack.layout);
    std::fs::write(&registry_path, toml).map_err(|e| format!("write registry: {e}"))?;
    write_manifest(&dir, &packs::InstallManifest { instrument_count, ..manifest })?;
    let active = packs::active_packs::is_active(&packs::active_packs::active_set(), pack_id);
    Ok(packs::status_of(cfg, pack, active))
}

/// Delete an installed pack's files (its whole install dir; for VSCO with a custom
/// `vsco_dir`, that directory). `Ok` when already absent; `Err` only on a filesystem
/// failure or an unknown id.
pub fn delete(cfg: &MerulaConfig, pack_id: &str) -> Result<(), String> {
    if packs::pack(pack_id).is_none() {
        return Err(format!("unknown sample pack `{pack_id}`"));
    }
    let dir = packs::pack_dir(cfg, pack_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

// ── Install marker (write half; the read half lives in `crate::packs`) ─────────

/// Write the install marker (`install.json`) the `packs` read surface reads back via
/// `packs::read_manifest`. Reuses `packs::InstallManifest` so the wire shape lives in
/// exactly one place across the read/job seam.
fn write_manifest(dir: &Path, manifest: &packs::InstallManifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(dir.join("install.json"), text).map_err(|e| format!("write marker: {e}"))
}

/// Stream `url` to `archive_path`, hashing with SHA-256 and emitting throttled
/// `merula:pack_progress` (phase `downloading`). Returns `Some(sha)` on success,
/// `None` when the user cancelled (the partial archive is removed). Polls the job's
/// cancel flag per chunk over the reverse channel.
async fn stream_archive(
    job: &JobHandle,
    sink: &Arc<dyn EventSink>,
    pack_id: &str,
    url: &str,
    archive_path: &Path,
) -> Result<Option<String>, String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    // `download_client` (not `client`): the API client's 30s TOTAL deadline spans the
    // body too, so it aborts every multi-hundred-MB pack mid-stream.
    let resp = arbor_core::prelude::download_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("server error: {e}"))?;
    let total = resp.content_length().unwrap_or(0);

    let mut file =
        std::fs::File::create(archive_path).map_err(|e| format!("create archive: {e}"))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut throttle = ProgressThrottle::default();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if job.is_cancelled() {
            drop(file);
            let _ = std::fs::remove_file(archive_path);
            return Ok(None);
        }
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| format!("write archive: {e}"))?;
        received += chunk.len() as u64;
        emit_throttled(sink, job, pack_id, "downloading", received, total, &mut throttle);
    }
    file.flush().map_err(|e| format!("flush archive: {e}"))?;
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(Some(sha256))
}

/// Emit a `merula:pack_progress` event + append a coarse line into the job output,
/// throttled by [`ProgressThrottle`] (the per-chunk download loop would otherwise
/// emit thousands of events).
fn emit_throttled(
    sink: &Arc<dyn EventSink>,
    job: &JobHandle,
    pack_id: &str,
    phase: &str,
    done: u64,
    total: u64,
    throttle: &mut ProgressThrottle,
) {
    if !throttle.should_emit(done, total) {
        return;
    }
    let pct = percent_of(done, total);
    events::emit(
        &**sink,
        EVT_PACK_PROGRESS,
        PackProgress {
            job_id: job.id.clone(),
            pack_id: pack_id.to_string(),
            phase: phase.to_string(),
            done,
            total,
            pct,
        },
    );
    let line = if pct >= 0 {
        format!("[{phase}] {pct}% ({done}/{total})")
    } else {
        format!("[{phase}] {done} bytes")
    };
    job.append(&line);
}

/// Emit a single (un-throttled) `merula:pack_progress` — used by the extract phase,
/// whose per-step granularity is already coarse enough not to need throttling.
fn emit_pack_progress(
    sink: &Arc<dyn EventSink>,
    job: &JobHandle,
    pack_id: &str,
    phase: &str,
    done: u64,
    total: u64,
) {
    events::emit(
        &**sink,
        EVT_PACK_PROGRESS,
        PackProgress {
            job_id: job.id.clone(),
            pack_id: pack_id.to_string(),
            phase: phase.to_string(),
            done,
            total,
            pct: percent_of(done, total),
        },
    );
}
