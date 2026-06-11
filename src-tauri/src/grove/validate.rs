//! Semantic validation of an evaluated arrangement: surface sound/instrument
//! references the registry can't resolve as editor diagnostics.
//!
//! The language layer accepts any name (it can't know the registry) and the
//! audio renderer silently falls back to the default synth for an unresolved one
//! — convenient for "grove always makes a sound", but it hides typos like
//! `.inst("snyth.lead")`. This pass cross-checks every `sound`/`inst` name an
//! evaluated [`ControlMap`] carries against the names the live registry would
//! resolve (built-in `synth.*` presets + the installed VSCO manifest) and emits
//! an `error` diagnostic, located at the offending leaf, for each one it can't.

use std::collections::HashSet;

use arbor_grove::prelude::{ControlMap, Registry, Time, TimeSpan, Tracks};

use super::config::GroveConfig;
use super::events::Diagnostic;
use super::packs;

/// Cycles probed for instrument references. A handful catches leaves that only
/// appear on later cycles (`arrange`/`cat`/cycle-seeded choice) while staying
/// cheap for the per-eval hot path.
const PROBE_CYCLES: i64 = 8;

/// The names the live registry can resolve: the built-in `synth.*` presets
/// (always present, no pack) plus every entry of each installed sample pack
/// (read by name only — no sample decode, so it stays cheap to call per eval).
pub fn known_instruments(cfg: &GroveConfig) -> HashSet<String> {
    let mut known: HashSet<String> = HashSet::new();
    let mut builtins = Registry::new();
    builtins.install_builtin_synths();
    known.extend(builtins.instruments_list().into_iter().map(|i| i.name));
    known.extend(packs::installed_instrument_names(cfg));
    known
}

/// Diagnose every `sound`/`inst` reference `known` can't resolve, located at the
/// source span of the offending leaf. Mirrors the renderer's precedence (`inst`
/// over `sound`) so the diagnosed name is the one that would actually be looked
/// up. Dedup'd by span (a leaf repeats every cycle).
pub fn validate_instruments(
    tracks: &Tracks<ControlMap>,
    known: &HashSet<String>,
) -> Vec<Diagnostic> {
    let span = TimeSpan::new(Time::int(0), Time::int(PROBE_CYCLES));
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    for track in &tracks.tracks {
        for hap in track.pattern.query(span) {
            let Some(name) = hap.value.inst.as_deref().or(hap.value.sound.as_deref()) else {
                continue;
            };
            if known.contains(name) {
                continue;
            }
            let Some(s) = hap.span else { continue };
            if !seen.insert((s.start, s.end)) {
                continue;
            }
            diags.push(Diagnostic {
                message: format!("unknown instrument `{name}` — not in the sound registry"),
                severity: "error",
                start: Some(s.start),
                end: Some(s.end),
            });
        }
    }
    diags
}
