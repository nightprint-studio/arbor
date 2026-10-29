//! Model management: catalogue with install state, hardware recommendation, verified download
//! (consent is collected by the frontend), removal.
//!
//! The digest of every catalogue file is pinned in `voice-core` (taken from the official
//! `ggerganov/whisper.cpp` repository's LFS metadata), so verification never trusts a checksum
//! served next to the file. A download is hashed as it streams and only renamed into place when
//! the digest matches.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use voice_core::prelude::*;

use crate::{engine, jobs, shared};

static INSTALLING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Hosts a model download may touch: the Hugging Face front door and the CDN hosts it redirects
/// to. Everything else is refused, redirects included.
pub fn host_allowed(scheme: &str, host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    scheme == "https" && (host == "huggingface.co" || host == "hf.co" || host.ends_with(".huggingface.co") || host.ends_with(".hf.co"))
}

/// Free bytes on the disk that holds `target`: the mount with the longest matching prefix.
pub fn free_bytes_for(mounts: &[(PathBuf, u64)], target: &Path) -> Option<u64> {
    mounts.iter().filter(|(m, _)| target.starts_with(m)).max_by_key(|(m, _)| m.as_os_str().len()).map(|(_, free)| *free)
}

fn free_disk_bytes(target: &Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mounts: Vec<(PathBuf, u64)> = disks.list().iter().map(|d| (d.mount_point().to_path_buf(), d.available_space())).collect();
    // The models folder may not exist yet: walk up to something that does.
    let mut probe = target.to_path_buf();
    while !probe.exists() && probe.pop() {}
    free_bytes_for(&mounts, &probe)
}

/// The machine, probed once (the answer cannot change while the process lives).
pub fn hardware() -> HardwareProfile {
    static HW: OnceLock<HardwareProfile> = OnceLock::new();
    *HW.get_or_init(|| {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        HardwareProfile {
            cores: engine::physical_cores(),
            logical: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2),
            ram_gb: sys.total_memory() as f32 / 1_073_741_824.0,
            gpu: engine::gpu_compiled_in(),
        }
    })
}

fn installed(spec: &ModelSpec) -> bool {
    std::fs::metadata(voice_models_dir().join(spec.file)).map(|m| m.len() == spec.size_bytes).unwrap_or(false)
}

/// Ids of the catalogue models that are on disk.
pub fn installed_ids() -> Vec<&'static str> {
    CATALOGUE.iter().filter(|m| installed(m)).map(|m| m.id).collect()
}

/// What dictation will run, from the persisted config and the files on disk.
pub fn selection(cfg: &VoiceConfig) -> Selection {
    effective_selection(&cfg.model, &cfg.custom_model_path, &installed_ids())
}

/// Persist a new `model`, apply it to the running process at once and tell every window. The
/// engine loads the file lazily at the next dictation, so there is nothing to restart.
fn apply_model(state: &VoiceState, model: &str) -> Result<VoiceConfig, String> {
    let saved = state.patch_config(&json!({ "model": model, "custom_model_path": "" }))?;
    state.emit("voice://config", serde_json::to_value(&saved).unwrap_or_default());
    Ok(saved)
}

/// Models that are installed, best first for this machine: the ones that keep up with speech
/// (most accurate first), then the rest by speed.
fn ranked_installed(language: &str) -> Vec<&'static str> {
    let (hw, measured) = (hardware(), load_measured());
    let mut ids: Vec<&ModelSpec> = installed_ids().into_iter().filter_map(find_model).collect();
    ids.sort_by(|a, b| {
        let (ra, rb) = (expected_rtf(a, &hw, &measured).0, expected_rtf(b, &hw, &measured).0);
        let (ca, cb) = (ra <= COMFORT_RTF, rb <= COMFORT_RTF);
        cb.cmp(&ca)
            .then_with(|| if ca { b.accuracy.cmp(&a.accuracy) } else { std::cmp::Ordering::Equal })
            .then_with(|| ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| (language == "en" || !a.english_only).cmp(&(language == "en" || !b.english_only)).reverse())
    });
    ids.into_iter().map(|m| m.id).collect()
}

/// A download finished: if nothing usable is selected yet, this model becomes the one in use.
fn auto_select_after_install(state: &VoiceState, id: &str) {
    let cfg = state.config();
    if let Some(model) = after_install(&cfg.model, &cfg.custom_model_path, &installed_ids(), id) {
        let _ = apply_model(state, &model);
    }
}

#[derive(Serialize)]
pub struct ModelRow {
    #[serde(flatten)]
    spec: ModelSpec,
    installed: bool,
    installing: bool,
    recommended: bool,
    /// The catalogue's source, for the consent dialog.
    url: String,
    /// Expected real-time factor on this PC (seconds of work per second of speech).
    rtf: f32,
    /// `rtf` comes from a real run on this PC, not from the hardware estimate.
    rtf_measured: bool,
    /// Text can appear while the user is still speaking with this model.
    live_ok: bool,
}

#[derive(Serialize)]
pub struct ModelList {
    models: Vec<ModelRow>,
    hardware: HardwareProfile,
    recommendation: Recommendation,
    engine: bool,
    free_disk_bytes: Option<u64>,
    models_dir: String,
    /// The model in use (a catalogue id), `""` for none or the user's own file. The single answer
    /// the "In use" badge and the "no model selected" notice are both derived from.
    selected: String,
    /// The user's own model file is in use.
    custom: bool,
}

#[arbor_rpc::handler]
fn voice_list_models(state: &VoiceState, language: Option<String>) -> Result<ModelList, String> {
    state.touch();
    let cfg = state.config();
    let language = language.unwrap_or_else(|| cfg.language.clone());
    let (hw, measured) = (hardware(), load_measured());
    let recommendation = recommend(&hw, &language, &measured);
    let busy = INSTALLING.lock().map(|s| s.clone()).unwrap_or_default();
    let models = CATALOGUE
        .iter()
        .map(|m| {
            let (rtf, rtf_measured) = expected_rtf(m, &hw, &measured);
            ModelRow {
                spec: *m,
                installed: installed(m),
                installing: busy.contains(m.id),
                recommended: m.id == recommendation.id,
                url: asset_url(m),
                rtf,
                rtf_measured,
                live_ok: streaming_comfortable(rtf),
            }
        })
        .collect();
    let selection = selection(&cfg);
    Ok(ModelList {
        models,
        hardware: hw,
        recommendation,
        engine: engine::compiled_in(),
        free_disk_bytes: free_disk_bytes(&voice_models_dir()),
        models_dir: voice_models_dir().display().to_string(),
        selected: selection.model_id().to_string(),
        custom: selection == Selection::Custom,
    })
}

/// Make `id` the model in use, at once and without a restart.
#[arbor_rpc::handler]
fn voice_select_model(state: &VoiceState, id: String) -> Result<VoiceConfig, String> {
    state.touch();
    let spec = find_model(&id).ok_or("Unknown model.")?;
    if !installed(spec) {
        return Err(format!("The {} model is not downloaded yet.", spec.label));
    }
    apply_model(state, &id)
}

#[arbor_rpc::handler]
fn voice_remove_model(state: &VoiceState, id: String) -> Result<(), String> {
    state.touch();
    let spec = find_model(&id).ok_or("Unknown model.")?;
    engine::unload_all(); // the file may be the one in memory (Windows refuses to delete it)
    let path = voice_models_dir().join(spec.file);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("could not remove {}: {e}", path.display())),
    }
    let cfg = state.config();
    let ranked = ranked_installed(&cfg.language);
    if let Some(next) = after_remove(&cfg.model, &id, &ranked) {
        apply_model(state, &next)?;
    }
    Ok(())
}

/// Start a download and return at once; progress, completion and failure arrive as
/// `voice://install` events and the work appears in the Jobs panel.
#[arbor_rpc::handler]
fn voice_install_model(state: &VoiceState, id: String) -> Result<(), String> {
    state.touch();
    let spec = *find_model(&id).ok_or("Unknown model.")?;
    if installed(&spec) {
        return Ok(());
    }
    if let Some(free) = free_disk_bytes(&voice_models_dir()) {
        if !free_space_ok(free, &spec) {
            return Err(format!(
                "Not enough free disk space: {} needs about {} MB and {} MB are free.",
                spec.label,
                spec.size_bytes / 1_048_576,
                free / 1_048_576
            ));
        }
    }
    {
        let mut busy = INSTALLING.lock().map_err(|_| "install state poisoned".to_string())?;
        if !busy.insert(spec.id.to_string()) {
            return Err("This model is already being downloaded.".into());
        }
    }
    let sink = state.event_sink();
    std::thread::Builder::new()
        .name("voice-model-download".into())
        .spawn(move || {
            let job = jobs::Job::start(&sink, &format!("Download Whisper model: {}", spec.label), &asset_url(&spec), "downloads", false);
            let result = download_verified(&spec, &|done, total| {
                sink.emit("voice://install", json!({ "id": spec.id, "kind": "progress", "done": done, "total": total }));
            });
            if let Ok(mut busy) = INSTALLING.lock() {
                busy.remove(spec.id);
            }
            match &result {
                Ok(()) => {
                    // Select it before announcing: the page that reacts to "done" must already see it in use.
                    auto_select_after_install(&shared::get(), spec.id);
                    sink.emit("voice://install", json!({ "id": spec.id, "kind": "done" }));
                }
                Err(e) => sink.emit("voice://install", json!({ "id": spec.id, "kind": "error", "error": e })),
            }
            if let Some(job) = job {
                job.finish(result.err());
            }
            shared::get().touch();
        })
        .map(|_| ())
        .map_err(|e| format!("could not start the download: {e}"))
}

fn download_verified(spec: &ModelSpec, progress: &(dyn Fn(u64, Option<u64>) + Send + Sync)) -> Result<(), String> {
    let dir = voice_models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let part = dir.join(format!("{}.part", spec.file));
    let final_path = dir.join(spec.file);
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let digest = rt.block_on(stream_to_file(&asset_url(spec), &part, spec.size_bytes + (1 << 20), progress));
    let digest = match digest {
        Ok(d) => d,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    };
    if !digest.eq_ignore_ascii_case(spec.sha256) {
        let _ = std::fs::remove_file(&part);
        return Err("The downloaded file does not match the published checksum and was discarded.".into());
    }
    std::fs::rename(&part, &final_path).map_err(|e| format!("could not install the model: {e}"))
}

async fn stream_to_file(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .user_agent(concat!("Arbor-Voice/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 6 && host_allowed(attempt.url().scheme(), attempt.url().host_str()) {
                attempt.follow()
            } else {
                attempt.error("redirected to a host that is not allowed")
            }
        }))
        .build()
        .map_err(|e| format!("could not build the HTTP client: {e}"))?;
    let mut resp = client.get(url).send().await.map_err(|e| format!("Could not reach the model repository: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("The model repository answered with status {}.", resp.status()));
    }
    let total = resp.content_length();
    let mut file = std::fs::File::create(dest).map_err(|e| format!("could not write the download: {e}"))?;
    let (mut hasher, mut done, mut last) = (Sha256::new(), 0u64, Instant::now());
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("The download was interrupted: {e}"))? {
        done += chunk.len() as u64;
        if done > max_bytes {
            return Err("The server sent more data than expected - stopped.".into());
        }
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| format!("could not write the download: {e}"))?;
        if last.elapsed() >= Duration::from_millis(150) {
            progress(done, total);
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    progress(done, total);
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_model_host_is_contacted() {
        assert!(host_allowed("https", Some("huggingface.co")));
        assert!(host_allowed("https", Some("cdn-lfs.huggingface.co")));
        assert!(host_allowed("https", Some("cas-bridge.xethub.hf.co")));
        for bad in [("http", "huggingface.co"), ("https", "evilhuggingface.co"), ("https", "huggingface.co.evil.example"), ("https", "example.com")] {
            assert!(!host_allowed(bad.0, Some(bad.1)), "{bad:?}");
        }
        assert!(!host_allowed("https", None));
    }

    #[test]
    fn free_space_uses_the_deepest_mount() {
        let mounts = vec![(PathBuf::from("/"), 10), (PathBuf::from("/home"), 99)];
        assert_eq!(free_bytes_for(&mounts, Path::new("/home/u/models")), Some(99));
        assert_eq!(free_bytes_for(&mounts, Path::new("/opt")), Some(10));
        assert_eq!(free_bytes_for(&[], Path::new("/opt")), None);
    }
}

/// Whether a download is in flight (the watchdog must not exit under it).
pub fn downloading() -> bool {
    INSTALLING.lock().map(|s| !s.is_empty()).unwrap_or(false)
}
