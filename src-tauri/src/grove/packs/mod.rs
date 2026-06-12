//! Downloadable sample **packs**: the generalisation of the original VSCO 2
//! bank into an N-pack system. Each pack is a declarative [`Pack`] descriptor
//! (id, name, archive URL, registry [`Layout`]); the shared [`download`]
//! plumbing fetches/extracts/indexes it the same way, and the audio engine loads
//! every installed pack's `registry.toml` into one merged registry.
//!
//! Adding a pack is one `Pack { … }` entry in [`PACKS`] — no new download or UI
//! code. The audio crate only ever sees `kind=sample` / `kind=sfz` entries, so
//! its (frozen) dependency set is untouched.

use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;

use arbor_grove::prelude::Registry;

use super::config::GroveConfig;

mod download;
mod gm;
mod layout;

pub use layout::Layout;

/// A declarative downloadable sample pack.
pub struct Pack {
    /// Stable id used in commands / install paths (`vsco`, `dirt-samples`, …).
    pub id: &'static str,
    /// Human label for the sound-bank UI.
    pub name: &'static str,
    /// One-line description shown in the sound bank (what the pack contains).
    pub description: &'static str,
    /// Rough **download** size, for a pre-install "how big is this" estimate.
    /// Approximate (the GitHub archive size varies); shown as `~N MB`.
    pub approx_bytes: u64,
    /// GitHub archive (`.zip`) of the pack's source repo. `HEAD.zip` resolves the
    /// default branch regardless of its name.
    pub archive_url: &'static str,
    /// How the extracted tree maps to registry entries.
    pub layout: Layout,
}

/// Every pack grove knows how to install. Order is the sound-bank display order.
pub const PACKS: &[Pack] = &[
    Pack {
        id: "vsco",
        name: "VSCO 2 — orchestral",
        description: "The Versilian Studios Chamber Orchestra 2 — a full set of \
            multisampled orchestral instruments (strings, brass, woodwinds, \
            percussion) as SFZ. Large; the richest sound source grove ships.",
        approx_bytes: 2_900_000_000,
        archive_url: "https://github.com/sgossner/VSCO-2-CE/archive/refs/heads/master.zip",
        layout: Layout::SfzTree,
    },
    Pack {
        id: "dirt-samples",
        name: "Dirt-Samples",
        description: "The TidalCycles / SuperDirt sample library — hundreds of \
            short one-shots and loops (drums, blips, vocals, textures) addressed \
            by name with `:n` variants. The classic live-coding sound set.",
        approx_bytes: 230_000_000,
        archive_url: "https://github.com/tidalcycles/Dirt-Samples/archive/HEAD.zip",
        // Flat folders of variant wavs (`bd`, `casio`, …).
        layout: Layout::FolderOfWavs { strip_segments: 0, joiner: "_" },
    },
    Pack {
        id: "drum-machines",
        name: "Drum machines",
        description: "Sampled classic drum machines (Roland TR-808/909, LinnDrum, \
            and many more), one voice per drum as `<Machine>_<drum>`. Punchy, \
            ready-made kits for beats.",
        approx_bytes: 55_000_000,
        archive_url: "https://github.com/ritchse/tidal-drum-machines/archive/HEAD.zip",
        // `machines/<Machine>/<drum>/*.wav` → `<Machine>_<drum>` (drop `machines`).
        layout: Layout::FolderOfWavs { strip_segments: 1, joiner: "_" },
    },
    Pack {
        id: "gm",
        name: "General MIDI (soundfont)",
        description: "The FluidR3 General MIDI soundfont — the 128 standard GM \
            instruments (pianos, organs, guitars, synths, ethnic and more), \
            converted to playable multisamples at install time.",
        approx_bytes: 148_000_000,
        // A single `.sf2`, converted to wav+SFZ at install time (see `gm`).
        archive_url: gm::SF2_URL,
        layout: Layout::Sf2,
    },
];

/// The reported install state of one pack.
#[derive(Debug, Clone, Serialize)]
pub struct PackStatus {
    pub id: String,
    pub name: String,
    /// One-line description of the pack's contents (from the [`Pack`] descriptor).
    pub description: String,
    /// Rough download size for the pre-install estimate (from the descriptor).
    pub approx_bytes: u64,
    pub installed: bool,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub instrument_count: usize,
}

/// Look up a pack descriptor by id.
pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

/// Install directory for a pack. VSCO keeps its legacy location (and the
/// `[grove].vsco_dir` override) for back-compat with existing installs; every
/// other pack lives under `<data>/grove/packs/<id>` (or the `packs_dir` override).
pub fn pack_dir(cfg: &GroveConfig, id: &str) -> PathBuf {
    if id == "vsco" {
        if let Some(dir) = &cfg.vsco_dir {
            return PathBuf::from(dir);
        }
        return arbor_core::prelude::arbor_data_dir().join("grove").join("vsco");
    }
    let base = match &cfg.packs_dir {
        Some(d) => PathBuf::from(d),
        None => arbor_core::prelude::arbor_data_dir().join("grove").join("packs"),
    };
    base.join(id)
}

/// The install status of every known pack (display order).
pub fn list(cfg: &GroveConfig) -> Vec<PackStatus> {
    PACKS.iter().map(|p| download::status(cfg, p)).collect()
}

/// The install status of one pack by id (`None` for an unknown id).
pub fn status(cfg: &GroveConfig, id: &str) -> Option<PackStatus> {
    pack(id).map(|p| download::status(cfg, p))
}

/// The instrument names declared by every installed pack (union), for the eval
/// validator. Cheap: header scan only, no sample decode.
pub fn installed_instrument_names(cfg: &GroveConfig) -> Vec<String> {
    let mut names = Vec::new();
    for p in PACKS {
        names.extend(download::installed_names(cfg, p));
    }
    names
}

/// Merge every installed pack's registry into `reg` (additive). The engine calls
/// this after [`Registry::install_builtin_synths`] so built-ins + all packs
/// resolve from one registry.
pub fn load_into(cfg: &GroveConfig, reg: &mut Registry) {
    for p in PACKS {
        download::load_into(cfg, p, reg);
    }
}

/// Start a background download+install for pack `id` (job-tracked). Returns the
/// job id; `Err` for an unknown id. Cancel via the standard `cancel_job`.
pub fn start_download(app: &AppHandle, cfg: &GroveConfig, id: &str) -> Result<String, String> {
    let pack = pack(id).ok_or_else(|| format!("unknown sample pack `{id}`"))?;
    Ok(download::start(app, cfg, pack))
}
