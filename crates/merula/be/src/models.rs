//! `models` domain — downloadable ONNX **transcription models** (basic-pitch,
//! Demucs), fetched on-demand so the base bundle stays light.
//!
//! This module owns the **descriptor table** + the read surface: where models
//! live on disk, whether each is installed, the resolved download URL (config
//! override → built-in default), and the `merula_models` listing handler. The
//! **download / delete** plumbing (job-tracked single-file stream) lives in
//! [`crate::models_download`] — it reuses [`MODELS`], [`models_dir`],
//! [`model_path`], and [`url_for`], kept `pub(crate)` here. Ported from the
//! shell's `src-tauri/src/merula/models.rs`, split along the read/job seam.
//!
//! ⚠️ The default URLs are best-effort and may move — they're overridable in the
//! merula config (`basic_pitch_url` / `demucs_url`) so the artifact source can be
//! pointed anywhere without a rebuild.

use std::path::PathBuf;

use serde::Serialize;

use crate::config_cmds::{self, MerulaConfig};
use crate::state::MerulaState;

/// Stable model ids (used in commands + filenames).
pub const BASIC_PITCH_ID: &str = "basic-pitch";
pub const DEMUCS_ID: &str = "demucs";

/// Default download URLs, **overridable** via the merula config (`basic_pitch_url`
/// / `demucs_url`).
///
/// basic-pitch's ONNX (`nmp.onnx`) ships in Spotify's own GitHub repo — the raw
/// path below is verified. Demucs has **no official ONNX** export (the project
/// ships PyTorch weights); we use the community HT-Demucs FT *drums specialist*
/// (StemSplitio), whose I/O matches `demucs.rs`. We need only drums (pitch runs on
/// `mix − drums`), so the single drums model is enough. If the URL moves, set
/// `demucs_url` to a compatible `htdemucs` ONNX — or skip Demucs entirely (the DSP
/// onset detector still handles drums on the mix).
const BASIC_PITCH_DEFAULT_URL: &str =
    "https://raw.githubusercontent.com/spotify/basic-pitch/main/basic_pitch/saved_models/icassp_2022/nmp.onnx";
const DEMUCS_DEFAULT_URL: &str =
    "https://huggingface.co/StemSplitio/htdemucs-ft-onnx/resolve/main/htdemucs_ft_drums.onnx";

/// A declarative downloadable model.
pub struct ModelDesc {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub filename: &'static str,
    pub approx_bytes: u64,
}

pub const MODELS: &[ModelDesc] = &[
    ModelDesc {
        id: BASIC_PITCH_ID,
        name: "basic-pitch (polyphonic pitch)",
        description: "Spotify's lightweight polyphonic note transcription model. \
            Far better than the built-in DSP pitch on real, polyphonic audio. Small.",
        filename: "basic-pitch.onnx",
        approx_bytes: 17_000_000,
    },
    ModelDesc {
        id: DEMUCS_ID,
        name: "Demucs (stem separation)",
        description: "HT-Demucs drums separator — removes percussion before pitch \
            detection (and isolates the kit) for cleaner notes. Large; optional.",
        filename: "htdemucs.onnx",
        approx_bytes: 316_000_000,
    },
];

pub fn desc(id: &str) -> Option<&'static ModelDesc> {
    MODELS.iter().find(|m| m.id == id)
}

/// Directory holding the downloaded models (`<merula-data>/models` by default).
pub fn models_dir(cfg: &MerulaConfig) -> PathBuf {
    match &cfg.models_dir {
        Some(d) => PathBuf::from(d),
        None => arbor_core::prelude::merula_data_dir().join("models"),
    }
}

/// On-disk path of a model's file (whether or not it's downloaded yet).
pub fn model_path(cfg: &MerulaConfig, id: &str) -> Option<PathBuf> {
    desc(id).map(|m| models_dir(cfg).join(m.filename))
}

/// Whether a model has been downloaded. Only the `onnx` feature's transcriber
/// path reads this (to default stem-splitting on), so it is gated to match its
/// sole caller and avoid a dead-code warning in the default (DSP-only) build.
#[cfg(feature = "onnx")]
pub fn is_installed(cfg: &MerulaConfig, id: &str) -> bool {
    model_path(cfg, id).map(|p| p.exists()).unwrap_or(false)
}

/// The download URL for a model — the config override, else the built-in default.
pub fn url_for(cfg: &MerulaConfig, id: &str) -> Option<String> {
    match id {
        BASIC_PITCH_ID => {
            Some(cfg.basic_pitch_url.clone().unwrap_or_else(|| BASIC_PITCH_DEFAULT_URL.to_string()))
        }
        DEMUCS_ID => Some(cfg.demucs_url.clone().unwrap_or_else(|| DEMUCS_DEFAULT_URL.to_string())),
        _ => None,
    }
}

/// The reported state of one model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub approx_bytes: u64,
    pub installed: bool,
    pub path: String,
    pub size_bytes: u64,
}

fn status(cfg: &MerulaConfig, m: &ModelDesc) -> ModelStatus {
    let path = models_dir(cfg).join(m.filename);
    let size_bytes = std::fs::metadata(&path).map(|md| md.len()).unwrap_or(0);
    ModelStatus {
        id: m.id.to_string(),
        name: m.name.to_string(),
        description: m.description.to_string(),
        approx_bytes: m.approx_bytes,
        installed: path.exists(),
        path: path.display().to_string(),
        size_bytes,
    }
}

/// List every transcription model with its install state.
#[arbor_rpc::handler]
fn merula_models(_ctx: &MerulaState) -> Result<Vec<ModelStatus>, String> {
    let cfg = config_cmds::load();
    Ok(MODELS.iter().map(|m| status(&cfg, m)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default URL resolves to the built-in when no override is set, and to
    /// the override when present — the precedence the downloader relies on.
    #[test]
    fn url_override_precedence() {
        let mut cfg = MerulaConfig::default();
        assert_eq!(url_for(&cfg, BASIC_PITCH_ID).as_deref(), Some(BASIC_PITCH_DEFAULT_URL));
        cfg.basic_pitch_url = Some("https://example.com/m.onnx".to_string());
        assert_eq!(url_for(&cfg, BASIC_PITCH_ID).as_deref(), Some("https://example.com/m.onnx"));
        assert!(url_for(&cfg, "unknown").is_none());
    }

    /// Every descriptor has a distinct id resolvable via `desc`.
    #[test]
    fn descriptors_are_addressable() {
        assert!(desc(BASIC_PITCH_ID).is_some());
        assert!(desc(DEMUCS_ID).is_some());
        assert!(desc("nope").is_none());
    }
}
