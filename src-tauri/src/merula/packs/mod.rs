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

use merula::prelude::{InstrumentInfo, Registry};

use super::config::MerulaConfig;

mod download;
mod gm;
mod layout;
mod versilian;

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

/// Every pack merula knows how to install. Order is the sound-bank display order.
pub const PACKS: &[Pack] = &[
    Pack {
        id: "vsco",
        name: "VSCO 2 — orchestral",
        description: "The Versilian Studios Chamber Orchestra 2 — a full set of \
            multisampled orchestral instruments (strings, brass, woodwinds, \
            percussion). Large; the richest sound source merula ships. Indexed into \
            playable SFZ instruments from its raw wavs at install time.",
        approx_bytes: 2_900_000_000,
        archive_url: "https://github.com/sgossner/VSCO-2-CE/archive/refs/heads/master.zip",
        // The CE archive ships raw wavs (no `.sfz`); merula builds the layer from
        // the filenames — see `versilian`.
        layout: Layout::VersilianWavTree,
    },
    Pack {
        id: "vcsl",
        name: "VCSL — community orchestra",
        description: "The Versilian Community Sample Library (sibling of VSCO 2, same \
            authors) — a huge CC0 set of orchestral, world and folk instruments \
            plus a deep percussion/idiophone collection (anvil, claps, woodblocks, \
            mallets, hand percussion). Indexed into playable instruments + one-shots \
            from its raw wavs at install time.",
        approx_bytes: 4_000_000_000,
        archive_url: "https://github.com/sgossner/VCSL/archive/refs/heads/master.zip",
        // Same raw-wav format as VSCO (no `.sfz`), deeper Hornbostel-Sachs nesting
        // and a mix of pitched + unpitched instruments — both handled by `versilian`.
        layout: Layout::VersilianWavTree,
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

/// Install directory for a pack. VSCO lives at `<merula-data>/vsco` (or the
/// `vsco_dir` override); every other pack lives under `<merula-data>/packs/<id>`
/// (or the `packs_dir` override).
pub fn pack_dir(cfg: &MerulaConfig, id: &str) -> PathBuf {
    if id == "vsco" {
        if let Some(dir) = &cfg.vsco_dir {
            return PathBuf::from(dir);
        }
        return arbor_core::prelude::merula_data_dir().join("vsco");
    }
    let base = match &cfg.packs_dir {
        Some(d) => PathBuf::from(d),
        None => arbor_core::prelude::merula_data_dir().join("packs"),
    };
    base.join(id)
}

/// Delete an installed pack's files (its whole install dir; for VSCO with a
/// custom `vsco_dir`, that directory). `Ok` when already absent; `Err` only on
/// a filesystem failure or an unknown id.
pub fn delete(cfg: &MerulaConfig, id: &str) -> Result<(), String> {
    if pack(id).is_none() {
        return Err(format!("unknown sample pack `{id}`"));
    }
    let dir = pack_dir(cfg, id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

/// The install status of every known pack (display order).
pub fn list(cfg: &MerulaConfig) -> Vec<PackStatus> {
    PACKS.iter().map(|p| download::status(cfg, p)).collect()
}

/// The install status of one pack by id (`None` for an unknown id).
pub fn status(cfg: &MerulaConfig, id: &str) -> Option<PackStatus> {
    pack(id).map(|p| download::status(cfg, p))
}

/// Re-index an already-installed pack: regenerate its `registry.toml` from the
/// **extracted tree on disk** (no re-download) and refresh the install marker's
/// instrument count. The fix for a pack whose index is stale or empty — e.g. an
/// older VSCO install that produced zero instruments. `Err` for an unknown /
/// not-installed id, a missing extracted tree, or a layout that can't re-index
/// from the tree (General MIDI, whose source `.sf2` is deleted after install).
pub fn reindex(cfg: &MerulaConfig, id: &str) -> Result<PackStatus, String> {
    let pack = pack(id).ok_or_else(|| format!("unknown sample pack `{id}`"))?;
    download::reindex(cfg, pack)
}

/// The instrument names declared by every installed pack (union), for the eval
/// validator. Cheap: header scan only, no sample decode.
pub fn installed_instrument_names(cfg: &MerulaConfig) -> Vec<String> {
    let mut names = Vec::new();
    for p in PACKS {
        names.extend(download::installed_names(cfg, p));
    }
    names
}

/// Map each installed pack's instrument names to its `(pack_id, pack_name)`, for
/// the sound-bank's per-pack grouping. Cheap header scan (no sample decode). On
/// a name claimed by two packs, the first in [`PACKS`] order wins.
pub fn instrument_pack_map(cfg: &MerulaConfig) -> std::collections::HashMap<String, (String, String)> {
    let mut map = std::collections::HashMap::new();
    for p in PACKS {
        for name in download::installed_names(cfg, p) {
            map.entry(name)
                .or_insert_with(|| (p.id.to_string(), p.name.to_string()));
        }
    }
    map
}

/// Append every installed pack's instruments (name / kind / articulations) to
/// `out` **without decoding samples** — the sound-bank listing path. Pairs with
/// [`load_subset_into`], which decodes (for playback) only the referenced
/// instruments (a pack like VSCO/Dirt is gigabytes, never loaded wholesale).
pub fn list_instruments_into(cfg: &MerulaConfig, out: &mut Vec<InstrumentInfo>) {
    for p in PACKS {
        download::list_into(cfg, p, out);
    }
}

/// Merge **only** the entries named in `needed` from every installed pack into
/// `reg`. The lazy playback path: the live session decodes just the instruments
/// the arrangement references, not every sample of every installed pack.
pub fn load_subset_into(
    cfg: &MerulaConfig,
    reg: &mut Registry,
    needed: &std::collections::HashSet<String>,
) {
    for p in PACKS {
        download::load_subset_into(cfg, p, reg, needed);
    }
}

/// Start a background download+install for pack `id` (job-tracked). Returns the
/// job id; `Err` for an unknown id. Cancel via the standard `cancel_job`.
pub fn start_download(app: &AppHandle, cfg: &MerulaConfig, id: &str) -> Result<String, String> {
    let pack = pack(id).ok_or_else(|| format!("unknown sample pack `{id}`"))?;
    Ok(download::start(app, cfg, pack))
}
