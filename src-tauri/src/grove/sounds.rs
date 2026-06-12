//! Registry introspection for the sound-bank UI.
//!
//! Builds the same sound registry the audio thread would (built-in synths plus
//! every installed sample pack) and enumerates its resolvable
//! instruments. Reflects the *real* registry, not a static list, so it tracks
//! exactly what's installed. The built-in default synth (`synth`) — grove's
//! universal fallback for any unresolved name — is always present.

use serde::Serialize;

use arbor_grove::prelude::{InstrumentKind, Registry};

use crate::error::AppError;

/// One resolvable voice in the sound registry.
#[derive(Debug, Clone, Serialize)]
pub struct Instrument {
    /// Dotted registry name (`strings.violin`) or a short bank name (`bd`).
    pub name: String,
    /// `synth` | `sample` | `sfz`.
    pub kind: &'static str,
    /// Named articulations the instrument exposes (`.art("…")`), sorted; empty
    /// for synth / sample voices.
    pub articulations: Vec<String>,
    /// A short one-line description for the sound bank (authored in
    /// [`super::sound_catalog`]); `None` when the catalogue has no match.
    pub description: Option<&'static str>,
    /// Stable id of the sample pack this voice comes from (`dirt-samples`, …),
    /// for the sound bank's per-pack grouping. `None` for built-in synths.
    pub pack: Option<String>,
    /// Human label of that pack (`Dirt-Samples`, …); `None` for built-in synths.
    pub pack_name: Option<String>,
}

/// The `grove_sounds` result. Always includes the built-in default synth.
#[derive(Debug, Clone, Serialize)]
pub struct SoundList {
    pub instruments: Vec<Instrument>,
}

/// List the instruments the engine can currently resolve (default synth + any
/// installed VSCO/manifest entries).
#[tauri::command]
pub async fn grove_sounds() -> Result<SoundList, AppError> {
    let cfg = super::grove_config();
    // The sound bank only needs the *names* the engine can resolve, never the
    // audio. Built-in synths are cheap in-memory presets; sample packs are
    // enumerated WITHOUT decoding (listing VSCO/Dirt by building a real registry
    // would eagerly read gigabytes of WAV into RAM — see `list_manifest_instruments`).
    let mut synths = Registry::new();
    synths.install_builtin_synths();
    let mut infos = synths.instruments_list();
    super::packs::list_instruments_into(&cfg, &mut infos);
    // name → (pack_id, pack_name), so each sampler voice carries its origin for
    // the sound bank's per-pack grouping (built-in synths stay unmapped).
    let pack_map = super::packs::instrument_pack_map(&cfg);

    let mut instruments: Vec<Instrument> = infos
        .into_iter()
        .map(|i| {
            let (pack, pack_name) = match pack_map.get(&i.name) {
                Some((id, name)) => (Some(id.clone()), Some(name.clone())),
                None => (None, None),
            };
            Instrument {
                description: super::sound_catalog::describe(&i.name, i.kind),
                name: i.name,
                kind: kind_str(i.kind),
                articulations: i.articulations,
                pack,
                pack_name,
            }
        })
        .collect();
    // One row per name (a real registry dedups by name; the listing path is a
    // flat Vec, so collapse any same-named entry — overrides, cross-pack clashes).
    instruments.sort_by(|a, b| a.name.cmp(&b.name));
    instruments.dedup_by(|a, b| a.name == b.name);

    // The universal fallback is always resolvable, even with no manifest; surface
    // it first so the UI can always offer it.
    if !instruments.iter().any(|i| i.name == "synth") {
        instruments.insert(
            0,
            Instrument {
                name: "synth".to_string(),
                kind: "synth",
                articulations: Vec::new(),
                description: super::sound_catalog::describe("synth", InstrumentKind::Synth),
                pack: None,
                pack_name: None,
            },
        );
    }

    Ok(SoundList { instruments })
}

/// Map the registry's instrument kind to the wire string.
fn kind_str(kind: InstrumentKind) -> &'static str {
    match kind {
        InstrumentKind::Synth => "synth",
        InstrumentKind::Sample => "sample",
        InstrumentKind::Sfz => "sfz",
    }
}
