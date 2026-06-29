//! `sounds` domain — registry introspection for the sound-bank UI.
//!
//! Builds the same sound registry the audio thread would (built-in synths plus
//! every installed sample pack) and enumerates its resolvable instruments. Reflects
//! the *real* registry, not a static list, so it tracks exactly what's installed.
//! The built-in default synth (`synth`) — merula's universal fallback for any
//! unresolved name — is always present.
//!
//! Ported from the shell's `src-tauri/src/merula/sounds.rs`. The human-readable
//! sound-bank copy (the shell's `sound_catalog.rs`, not a be module) is inlined
//! here as the private [`catalog`] module — its only consumer.

use serde::Serialize;

use merula::prelude::{InstrumentKind, Registry};

use crate::config_cmds;
use crate::packs;
use crate::state::MerulaState;

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
    /// A short one-line description for the sound bank; `None` when the catalogue
    /// has no match.
    pub description: Option<&'static str>,
    /// Stable id of the sample pack this voice comes from (`dirt-samples`, …),
    /// for the sound bank's per-pack grouping. `None` for built-in synths.
    pub pack: Option<String>,
    /// Human label of that pack (`Dirt-Samples`, …); `None` for built-in synths.
    pub pack_name: Option<String>,
}

/// The `merula_sounds` result. Always includes the built-in default synth.
#[derive(Debug, Clone, Serialize)]
pub struct SoundList {
    pub instruments: Vec<Instrument>,
}

/// List the instruments the engine can currently resolve (default synth + any
/// installed VSCO/manifest entries).
#[arbor_rpc::handler]
fn merula_sounds(_ctx: &MerulaState) -> Result<SoundList, String> {
    let cfg = config_cmds::load();
    // The sound bank only needs the *names* the engine can resolve, never the
    // audio. Built-in synths are cheap in-memory presets; sample packs are
    // enumerated WITHOUT decoding (listing VSCO/Dirt by building a real registry
    // would eagerly read gigabytes of WAV into RAM).
    let mut synths = Registry::new();
    synths.install_builtin_synths();
    let mut infos = synths.instruments_list();
    packs::list_instruments_into(&cfg, &mut infos);
    // name → (pack_id, pack_name), so each sampler voice carries its origin for
    // the sound bank's per-pack grouping (built-in synths stay unmapped).
    let pack_map = packs::instrument_pack_map(&cfg);

    let mut instruments: Vec<Instrument> = infos
        .into_iter()
        .map(|i| {
            let (pack, pack_name) = match pack_map.get(&i.name) {
                Some((id, name)) => (Some(id.clone()), Some(name.clone())),
                None => (None, None),
            };
            Instrument {
                description: catalog::describe(&i.name, i.kind),
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
                description: catalog::describe("synth", InstrumentKind::Synth),
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

// ── Sound-bank copy ────────────────────────────────────────────────────────────
//
// Human-readable, one-line descriptions of the resolvable instruments (synth
// presets, common drum leaves, orchestral families). Authored here — the registry
// stays about *sound*, this stays about *what the user reads*. Inlined from the
// shell's `sound_catalog.rs` (its only consumer is this `sounds` domain).

mod catalog {
    use merula::prelude::InstrumentKind;

    /// A short, one-line description for a resolvable instrument `name`, or `None`
    /// when nothing in the catalogue matches (the UI then shows a kind-based label).
    pub fn describe(name: &str, kind: InstrumentKind) -> Option<&'static str> {
        // 1. Exact built-in synth / oscillator names.
        if let Some(d) = synth_description(name) {
            return Some(d);
        }
        // 2. A `<Machine>_<drum>` drum-machine leaf, or a bare drum abbreviation.
        let leaf = name.rsplit(['_', '.']).next().unwrap_or(name);
        if let Some(d) = drum_description(leaf) {
            return Some(d);
        }
        // 3. A dotted orchestral name (`strings.violin`) → describe by family.
        if let Some((family, _)) = name.split_once('.') {
            if let Some(d) = family_description(family) {
                return Some(d);
            }
        }
        // 4. Kind-based fallbacks for the rest (still better than nothing for SFZ).
        match kind {
            InstrumentKind::Sfz => {
                Some("Multisampled instrument — velocity layers mapped across the keyboard.")
            }
            InstrumentKind::Sample | InstrumentKind::Synth => None,
        }
    }

    /// Descriptions for the built-in `synth.*` presets, bare oscillator names and
    /// their aliases, and the noise colours.
    fn synth_description(name: &str) -> Option<&'static str> {
        Some(match name {
            "synth" => "The default voice — a soft triangle with a gentle pluck envelope. Any unresolved name falls back here.",
            "synth.bass" => "Saw bass — punchy, focused low-end for basslines.",
            "synth.sub" => "Sine sub — a clean fundamental with almost no harmonics; deep low end.",
            "synth.pad" => "Triangle pad — soft and slow-swelling, for sustained background harmony.",
            "synth.pluck" => "Square pluck — a short, percussive stab with no sustain.",
            "synth.lead" => "Saw lead — bright and cutting, for melodic top lines.",
            "synth.supersaw" => "Supersaw — seven detuned saws stacked into a wide, lush ensemble.",
            "synth.noise" => "Noise burst — a white-noise transient for percussive textures.",
            "synth.hat" => "Noise hat — a tight pink-noise tick for hi-hat-like parts.",
            "sine" | "sin" => "Sine wave — the purest tone, a single harmonic.",
            "sawtooth" | "saw" => "Sawtooth — bright and buzzy, rich in every harmonic.",
            "square" | "sqr" | "pulse" => "Square wave — hollow and woody, odd harmonics only.",
            "triangle" | "tri" => "Triangle — soft and mellow, only faint upper harmonics.",
            "supersaw" => "Supersaw — stacked detuned saws; big, wide and shimmering.",
            "white" => "White noise — equal energy at every frequency; a bright hiss.",
            "pink" => "Pink noise — equal energy per octave; a warmer, fuller hiss.",
            "brown" => "Brown noise — energy weighted to the lows; a deep rumble.",
            "crackle" => "Crackle — sparse random impulses, like vinyl surface noise.",
            _ => return None,
        })
    }

    /// Descriptions for the common drum / percussion leaves shared across
    /// Dirt-Samples and the drum-machine packs (matched on the leaf after any
    /// `<Machine>_` prefix). Covers the standard kit abbreviations.
    fn drum_description(leaf: &str) -> Option<&'static str> {
        Some(match leaf {
            "bd" | "kick" => "Bass drum (kick) — the low, punchy downbeat.",
            "sd" | "sn" | "snare" => "Snare drum — the sharp, cracking backbeat.",
            "rim" | "rs" => "Rimshot — a thin, clicky snare-rim hit.",
            "cp" | "clap" => "Hand clap — a layered, slappy transient.",
            "hh" | "ch" => "Closed hi-hat — a short, crisp metallic tick.",
            "oh" => "Open hi-hat — a longer, ringing hi-hat.",
            "cr" | "crash" => "Crash cymbal — a bright, explosive accent.",
            "rd" | "ride" => "Ride cymbal — a sustained, shimmering ping.",
            "lt" => "Low tom — a deep, round drum fill voice.",
            "mt" => "Mid tom — a mid-pitched tom fill voice.",
            "ht" => "High tom — a high-pitched tom fill voice.",
            "cb" => "Cowbell — a bright, cutting metallic accent.",
            "perc" => "Percussion — an assorted one-shot percussion hit.",
            "tb" => "Tambourine — a jingly, shaken accent.",
            "sh" | "shaker" => "Shaker — a sustained granular percussion texture.",
            "cl" | "click" => "Click — a sharp metronomic tick.",
            _ => return None,
        })
    }

    /// Descriptions for the VSCO 2 orchestral families (the head of a dotted name).
    fn family_description(family: &str) -> Option<&'static str> {
        Some(match family {
            "strings" => "Orchestral strings — bowed sustains and articulations (VSCO 2).",
            "brass" => "Orchestral brass — horns, trumpets and trombones (VSCO 2).",
            "ww" | "winds" | "woodwinds" => "Orchestral woodwinds — flutes, clarinets, oboes (VSCO 2).",
            "keys" | "keyboard" | "piano" => "Keyboard instrument — sampled piano / mallet voice (VSCO 2).",
            "perc" | "percussion" => "Orchestral percussion — timpani, mallets and more (VSCO 2).",
            "guitar" => "Guitar — plucked / strummed multisample.",
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::catalog::describe;
    use merula::prelude::InstrumentKind;

    /// The default synth and common drum leaves resolve a description; a `<Machine>_`
    /// prefix is stripped before the leaf match; an unknown name falls through.
    #[test]
    fn catalogue_describes_known_voices() {
        assert!(describe("synth", InstrumentKind::Synth).is_some());
        assert!(describe("bd", InstrumentKind::Sample).is_some());
        // Leaf match through a drum-machine prefix.
        assert!(describe("RolandTR808_bd", InstrumentKind::Sample).is_some());
        // Dotted orchestral family.
        assert!(describe("strings.violin", InstrumentKind::Sfz).is_some());
        // Unknown sample → no copy.
        assert!(describe("totally-unknown", InstrumentKind::Sample).is_none());
        // Unknown SFZ → the kind-based fallback line.
        assert!(describe("mystery.thing", InstrumentKind::Sfz).is_some());
    }
}
