//! `packs` domain — downloadable sample **packs** (VSCO 2, VCSL, Dirt-Samples,
//! drum machines, General MIDI): the **read surface**.
//!
//! Each pack is a declarative [`Pack`] descriptor (id, name, archive URL, registry
//! [`Layout`]). This module owns the descriptor table plus everything the *read*
//! paths need — install status, on-disk layout, the cheap (no-decode) instrument
//! listing for the sound bank + eval validator, and the per-profile **active**
//! allow-list ([`active_packs`]). The two read handlers live here:
//! `merula_packs` (list) and `merula_pack_set_active` (toggle the allow-list).
//!
//! The job-tracked **download / reindex / delete** plumbing — and the
//! `Layout::generate` tree-walkers (gm / versilian / folder-of-wavs) — live in
//! `crate::packs_download`, a later wave. They reuse [`PACKS`], [`pack_dir`],
//! [`PackStatus`], and [`active_packs`], kept `pub` here. Ported from the shell's
//! `src-tauri/src/merula/packs/*`, split along the read/job seam.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use merula::prelude::{list_manifest_instruments, InstrumentInfo, Registry};

use self::active_packs::is_active;
use crate::config_cmds::{self, MerulaConfig};
use crate::state::MerulaState;

/// How a pack's extracted tree becomes registry entries. Only the **data** lives
/// here (the variants the [`Pack`] table needs); the tree-walking `generate(...)`
/// that consumes them is implemented by `crate::packs_download` (install-time).
#[derive(Debug, Clone, Copy)]
pub enum Layout {
    /// A tree of `.sfz` instruments (VSCO 2).
    SfzTree,
    /// Folders of `.wav` variants (Dirt-Samples, drum machines).
    ///
    /// * `strip_segments` — leading path components (below the archive root) to
    ///   drop from the **name**.
    /// * `joiner` — string joining the remaining name components.
    FolderOfWavs { strip_segments: usize, joiner: &'static str },
    /// A single General MIDI SoundFont (`.sf2`), converted to wav+SFZ at install.
    Sf2,
    /// A Versilian wav tree (VSCO 2 CE **or** VCSL — same on-disk format), with no
    /// shipped `.sfz`; the layer is built from the filenames at install time.
    VersilianWavTree,
}

/// The General MIDI SoundFont (`.sf2`) download URL — descriptor data for the
/// `gm` pack. Owned by the read surface (it's part of the [`Pack`] table);
/// [`crate::packs_download`]'s GM converter reads it from here.
pub const GM_SF2_URL: &str = "https://musical-artifacts.com/artifacts/738/FluidR3_GM.sf2";

/// A declarative downloadable sample pack.
pub struct Pack {
    /// Stable id used in commands / install paths (`vsco`, `dirt-samples`, …).
    pub id: &'static str,
    /// Human label for the sound-bank UI.
    pub name: &'static str,
    /// One-line description shown in the sound bank (what the pack contains).
    pub description: &'static str,
    /// Rough **download** size, for a pre-install "how big is this" estimate.
    pub approx_bytes: u64,
    /// GitHub archive (`.zip`) of the pack's source repo.
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
        layout: Layout::FolderOfWavs { strip_segments: 1, joiner: "_" },
    },
    Pack {
        id: "gm",
        name: "General MIDI (soundfont)",
        description: "The FluidR3 General MIDI soundfont — the 128 standard GM \
            instruments (pianos, organs, guitars, synths, ethnic and more), \
            converted to playable multisamples at install time.",
        approx_bytes: 148_000_000,
        // A single `.sf2`, converted to wav+SFZ at install time (see packs_download).
        archive_url: GM_SF2_URL,
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
    /// Whether this pack is in the per-profile **active** allow-list (its
    /// instruments are usable for playback / eval / the sound bank). When no
    /// allow-list file exists yet, every pack reports `true` (all-active).
    pub active: bool,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub instrument_count: usize,
}

/// Look up a pack descriptor by id.
pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

/// Iterate only the packs that are **active** for the given allow-list. The single
/// chokepoint every *consumption* path (playback, eval validation, sound-bank
/// enumeration) funnels through, so an inactive pack is uniformly invisible to
/// those paths while pack management still sees them all.
fn active_packs_iter(active: &Option<HashSet<String>>) -> impl Iterator<Item = &'static Pack> + '_ {
    PACKS.iter().filter(move |p| is_active(active, p.id))
}

/// The ids of every currently-**installed** pack (used to seed / prune the
/// active-pack allow-list).
pub fn installed_ids(cfg: &MerulaConfig) -> Vec<String> {
    PACKS.iter().filter(|p| status_of(cfg, p, true).installed).map(|p| p.id.to_string()).collect()
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

/// The install status of every known pack (display order). Pack management lists
/// **every** pack regardless of the active allow-list; the per-pack `active` flag
/// reflects allow-list membership for the UI's active/inactive toggle.
pub fn list(cfg: &MerulaConfig) -> Vec<PackStatus> {
    let active = active_packs::active_set();
    PACKS.iter().map(|p| status_of(cfg, p, is_active(&active, p.id))).collect()
}

/// The instrument names declared by every installed pack (union), for the eval
/// validator. Cheap: header scan only, no sample decode. Only **active** packs
/// count — a script referencing a voice from an inactive pack must fail the eval
/// validator loudly, not render silently.
pub fn installed_instrument_names(cfg: &MerulaConfig) -> Vec<String> {
    let active = active_packs::active_set();
    let mut names = Vec::new();
    for p in active_packs_iter(&active) {
        names.extend(installed_names(cfg, p));
    }
    names
}

/// Map each installed pack's instrument names to its `(pack_id, pack_name)`, for
/// the sound-bank's per-pack grouping. Cheap header scan (no sample decode). On a
/// name claimed by two packs, the first in [`PACKS`] order wins.
pub fn instrument_pack_map(cfg: &MerulaConfig) -> std::collections::HashMap<String, (String, String)> {
    let active = active_packs::active_set();
    let mut map = std::collections::HashMap::new();
    for p in active_packs_iter(&active) {
        for name in installed_names(cfg, p) {
            map.entry(name).or_insert_with(|| (p.id.to_string(), p.name.to_string()));
        }
    }
    map
}

/// Append every installed pack's instruments (name / kind / articulations) to
/// `out` **without decoding samples** — the sound-bank listing path.
pub fn list_instruments_into(cfg: &MerulaConfig, out: &mut Vec<InstrumentInfo>) {
    let active = active_packs::active_set();
    for p in active_packs_iter(&active) {
        list_into(cfg, p, out);
    }
}

// ── Install marker + read helpers (the read half of the shell's download.rs) ───

/// Install marker, written after a successful extract+index by the install path.
/// Only the fields the read surface needs are required; the rest are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallManifest {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub instrument_count: usize,
    /// Registry TOML path, relative to the install dir.
    pub registry_rel: String,
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("install.json")
}

pub fn read_manifest(dir: &Path) -> Option<InstallManifest> {
    let text = std::fs::read_to_string(manifest_path(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The install status of one pack (installed marker, or a not-installed stub).
pub fn status_of(cfg: &MerulaConfig, pack: &Pack, active: bool) -> PackStatus {
    let dir = pack_dir(cfg, pack.id);
    let path = dir.display().to_string();
    match read_manifest(&dir) {
        Some(m) => PackStatus {
            id: pack.id.to_string(),
            name: pack.name.to_string(),
            description: pack.description.to_string(),
            approx_bytes: pack.approx_bytes,
            installed: true,
            active,
            path,
            size_bytes: m.size_bytes,
            sha256: Some(m.sha256),
            instrument_count: m.instrument_count,
        },
        None => PackStatus {
            id: pack.id.to_string(),
            name: pack.name.to_string(),
            description: pack.description.to_string(),
            approx_bytes: pack.approx_bytes,
            installed: false,
            active,
            path,
            size_bytes: 0,
            sha256: None,
            instrument_count: 0,
        },
    }
}

/// The instrument names declared by an installed pack's registry (cheap header
/// scan — no sample decode). Empty when the pack isn't installed.
pub fn installed_names(cfg: &MerulaConfig, pack: &Pack) -> Vec<String> {
    let dir = pack_dir(cfg, pack.id);
    let Some(manifest) = read_manifest(&dir) else {
        return Vec::new();
    };
    let registry_path = dir.join(&manifest.registry_rel);
    let Ok(text) = std::fs::read_to_string(&registry_path) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some(inner) = line.strip_prefix('[') else { continue };
        let Some(header) = inner.strip_suffix(']') else { continue };
        let name = header.trim().trim_matches('"').to_string();
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

/// Append an installed pack's instruments — name / kind / articulations — to
/// `out` **without decoding samples** (no-op if not installed).
pub fn list_into(cfg: &MerulaConfig, pack: &Pack, out: &mut Vec<InstrumentInfo>) {
    let dir = pack_dir(cfg, pack.id);
    let Some(manifest) = read_manifest(&dir) else {
        return;
    };
    let registry_path = dir.join(&manifest.registry_rel);
    out.extend(list_manifest_instruments(&registry_path));
}

/// Merge **only** the entries named in `needed` from every installed pack into
/// `reg`. The lazy playback path: the live session decodes just the instruments
/// the arrangement references, not every sample of every installed pack. (Read
/// surface; the W3 audio session calls this.)
pub fn load_subset_into(
    cfg: &MerulaConfig,
    reg: &mut Registry,
    needed: &HashSet<String>,
) {
    let active = active_packs::active_set();
    for p in active_packs_iter(&active) {
        let dir = pack_dir(cfg, p.id);
        let Some(manifest) = read_manifest(&dir) else {
            continue;
        };
        let registry_path = dir.join(&manifest.registry_rel);
        if let Err(e) = reg.load_manifest_subset_into(&registry_path, needed) {
            eprintln!("merula-be: pack `{}` subset load failed ({e}); skipping", p.id);
        }
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// List every downloadable sample pack with its current install status.
#[arbor_rpc::handler]
fn merula_packs(_ctx: &MerulaState) -> Result<Vec<PackStatus>, String> {
    Ok(list(&config_cmds::load()))
}

/// Toggle a pack's **active** state in the per-profile allow-list. Inactive packs
/// stay installed (pack management still sees them) but their instruments are
/// hidden from playback, the eval validator, and the sound bank. Seeds the
/// allow-list from the currently-installed packs on the first toggle, so turning
/// one pack off keeps every other installed pack on.
#[arbor_rpc::handler]
fn merula_pack_set_active(_ctx: &MerulaState, pack_id: String, active: bool) -> Result<(), String> {
    let cfg = config_cmds::load();
    let installed_ids = installed_ids(&cfg);
    active_packs::set_active(&pack_id, active, &installed_ids)
}

// ── Per-profile active-pack allow-list ─────────────────────────────────────────
//
// A small allow-list that filters which *installed* sample packs are actually
// usable by the audio session, the eval validator, and the sound bank. Pack
// management always sees every pack — only the *consumption* paths honour this
// list. An **absent or unparseable** file means *all packs active* (the
// migration-safe sentinel). Ported verbatim from the shell's `active_packs.rs`.

pub mod active_packs {
    use std::collections::HashSet;

    use serde::{Deserialize, Serialize};

    /// The per-profile active-pack allow-list, persisted as `active_packs.toml`.
    #[derive(Debug, Default, Clone, Serialize, Deserialize)]
    struct ActivePacks {
        /// Pack ids the user has marked active. Absent file = all active.
        #[serde(default)]
        ids: Vec<String>,
    }

    /// Per-profile path of the active-packs file.
    fn config_path() -> std::path::PathBuf {
        arbor_core::prelude::merula_config_path("active_packs.toml")
    }

    /// The set of active pack ids, or `None` when the file is absent / unparseable.
    /// `None` is the migration-safe "all packs active" sentinel — never an error.
    pub fn active_set() -> Option<HashSet<String>> {
        let text = std::fs::read_to_string(config_path()).ok()?;
        let parsed: ActivePacks = toml::from_str(&text).ok()?;
        Some(parsed.ids.into_iter().collect())
    }

    /// Whether `id` is active given an [`active_set`] result. `None` (no file)
    /// means every pack is active.
    pub fn is_active(active: &Option<HashSet<String>>, id: &str) -> bool {
        match active {
            None => true,
            Some(s) => s.contains(id),
        }
    }

    /// Persist the active-pack allow-list (no BOM). Creates the parent dir.
    pub fn save(ids: &[String]) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        let payload = ActivePacks { ids: ids.to_vec() };
        let text = toml::to_string_pretty(&payload).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
    }

    /// Toggle one pack's active state and persist the result.
    ///
    /// When the file is absent (all-active), this **seeds** the list from
    /// `installed_ids` first, so toggling a single pack off keeps every other
    /// installed pack on.
    pub fn set_active(id: &str, on: bool, installed_ids: &[String]) -> Result<(), String> {
        let mut ids: Vec<String> = match active_set() {
            Some(set) => set.into_iter().collect(),
            None => installed_ids.to_vec(),
        };
        let present = ids.iter().any(|x| x == id);
        if on && !present {
            ids.push(id.to_string());
        } else if !on && present {
            ids.retain(|x| x != id);
        }
        save(&ids)
    }

    /// Auto-activate a freshly-installed pack so the user never loses access to a
    /// pack they just downloaded. Only mutates an **existing** allow-list. (Used by
    /// the install path in `packs_download`.)
    pub fn on_pack_installed(id: &str, installed_ids: &[String]) {
        let Some(set) = active_set() else {
            return; // all-active: a new install is already usable.
        };
        if set.contains(id) {
            return;
        }
        let mut ids: Vec<String> = set.into_iter().collect();
        ids.retain(|x| installed_ids.iter().any(|i| i == x));
        ids.push(id.to_string());
        if let Err(e) = save(&ids) {
            eprintln!("merula-be: failed to auto-activate pack `{id}`: {e}");
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// `None` (no file) is all-active; an explicit set gates by membership.
        #[test]
        fn is_active_semantics() {
            assert!(is_active(&None, "vsco"), "no file → all active");
            let set: Option<HashSet<String>> = Some(["vsco".to_string()].into_iter().collect());
            assert!(is_active(&set, "vsco"));
            assert!(!is_active(&set, "gm"), "absent from explicit set → inactive");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every descriptor is addressable by its stable id, and the table is in a
    /// stable display order (vsco first, gm last) the sound bank relies on.
    #[test]
    fn descriptor_table_is_addressable() {
        assert!(pack("vsco").is_some());
        assert!(pack("gm").is_some());
        assert!(pack("nope").is_none());
        assert_eq!(PACKS.first().map(|p| p.id), Some("vsco"));
        assert_eq!(PACKS.last().map(|p| p.id), Some("gm"));
    }

    /// `pack_dir` routes VSCO to its own dir and every other pack under `packs/<id>`,
    /// honouring the config overrides.
    #[test]
    fn pack_dir_routing() {
        let mut cfg = MerulaConfig::default();
        cfg.vsco_dir = Some("/custom/vsco".into());
        cfg.packs_dir = Some("/custom/packs".into());
        assert_eq!(pack_dir(&cfg, "vsco"), PathBuf::from("/custom/vsco"));
        assert_eq!(pack_dir(&cfg, "dirt-samples"), PathBuf::from("/custom/packs").join("dirt-samples"));
    }
}
