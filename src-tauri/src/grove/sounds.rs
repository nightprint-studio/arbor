//! Registry introspection for the sound-bank UI.
//!
//! Builds the same sound registry the audio thread would (built-in synths plus
//! every installed sample pack) and enumerates its resolvable
//! instruments. Reflects the *real* registry, not a static list, so it tracks
//! exactly what's installed. The built-in default synth (`synth`) — grove's
//! universal fallback for any unresolved name — is always present.

use serde::Serialize;
use tauri::State;

use arbor_grove::prelude::{InstrumentKind, Registry};

use crate::error::AppError;
use crate::AppState;

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
}

/// The `grove_sounds` result. Always includes the built-in default synth.
#[derive(Debug, Clone, Serialize)]
pub struct SoundList {
    pub instruments: Vec<Instrument>,
}

/// List the instruments the engine can currently resolve (default synth + any
/// installed VSCO/manifest entries).
#[tauri::command]
pub async fn grove_sounds(state: State<'_, AppState>) -> Result<SoundList, AppError> {
    let cfg = super::grove_config(&state)?;
    // Match the audio thread exactly: built-in synths, then every installed pack.
    let mut registry = Registry::new();
    registry.install_builtin_synths();
    super::packs::load_into(&cfg, &mut registry);

    let mut instruments: Vec<Instrument> = registry
        .instruments_list()
        .into_iter()
        .map(|i| Instrument {
            description: super::sound_catalog::describe(&i.name, i.kind),
            name: i.name,
            kind: kind_str(i.kind),
            articulations: i.articulations,
        })
        .collect();
    instruments.sort_by(|a, b| a.name.cmp(&b.name));

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
