//! Human-readable copy for the sound-bank UI: short descriptions of the
//! resolvable instruments (synth presets, common drum leaves, orchestral
//! families). Authored once here — the registry stays about *sound*, this stays
//! about *what the user reads*. Surfaced through [`super::sounds`] (the IPC seam).
//!
//! Coverage is best-effort "per-voice where possible": every built-in synth gets
//! a line; the common Dirt-Samples / drum-machine leaves and the VSCO orchestral
//! families are matched by name or by their `<Machine>_<drum>` / `family.inst`
//! shape; anything unmatched returns `None` and the UI falls back to the kind.

use arbor_nemus::prelude::InstrumentKind;

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
        InstrumentKind::Sfz => Some("Multisampled instrument — velocity layers mapped across the keyboard."),
        InstrumentKind::Sample | InstrumentKind::Synth => None,
    }
}

/// Descriptions for the built-in `synth.*` presets, bare oscillator names and
/// their aliases, and the noise colours (every name from the registry's built-in
/// install set).
fn synth_description(name: &str) -> Option<&'static str> {
    Some(match name {
        // Fallback / generic.
        "synth" => "The default voice — a soft triangle with a gentle pluck envelope. Any unresolved name falls back here.",
        // Named presets.
        "synth.bass" => "Saw bass — punchy, focused low-end for basslines.",
        "synth.sub" => "Sine sub — a clean fundamental with almost no harmonics; deep low end.",
        "synth.pad" => "Triangle pad — soft and slow-swelling, for sustained background harmony.",
        "synth.pluck" => "Square pluck — a short, percussive stab with no sustain.",
        "synth.lead" => "Saw lead — bright and cutting, for melodic top lines.",
        "synth.supersaw" => "Supersaw — seven detuned saws stacked into a wide, lush ensemble.",
        "synth.noise" => "Noise burst — a white-noise transient for percussive textures.",
        "synth.hat" => "Noise hat — a tight pink-noise tick for hi-hat-like parts.",
        // Bare oscillators (+ aliases).
        "sine" | "sin" => "Sine wave — the purest tone, a single harmonic.",
        "sawtooth" | "saw" => "Sawtooth — bright and buzzy, rich in every harmonic.",
        "square" | "sqr" | "pulse" => "Square wave — hollow and woody, odd harmonics only.",
        "triangle" | "tri" => "Triangle — soft and mellow, only faint upper harmonics.",
        // Bare shapes / noise colours.
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
