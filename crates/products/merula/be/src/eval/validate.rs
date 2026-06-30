//! Semantic validation of an evaluated arrangement: surface sound/instrument
//! references the registry can't resolve as editor diagnostics.
//!
//! The language layer accepts any name (it can't know the registry) and the
//! audio renderer silently falls back to the default synth for an unresolved one
//! — convenient for "merula always makes a sound", but it hides typos like
//! `.inst("snyth.lead")`. This pass cross-checks every `sound`/`inst` name an
//! evaluated [`ControlMap`] carries against the names the live registry would
//! resolve (built-in `synth.*` presets + the installed VSCO manifest) and emits
//! an `error` diagnostic, located at the offending leaf, for each one it can't.
//!
//! Ported verbatim from the shell's `src-tauri/src/merula/validate.rs` (Tauri-free
//! already); only the module paths change (`config` -> `config_cmds`, `state` ->
//! `fstate`).

use std::collections::HashSet;

use merula::prelude::{
    parse, ControlMap, Expr, ExprKind, IslandKind, Item, Registry, SpeechSpec, Time, TimeSpan,
    Tracks,
};

use merula_core::config::MerulaConfig;
use merula_core::events::Diagnostic;
use crate::packs;

/// Cycles probed for instrument references. A handful catches leaves that only
/// appear on later cycles (`arrange`/`cat`/cycle-seeded choice) while staying
/// cheap for the per-eval hot path.
const PROBE_CYCLES: i64 = 8;

/// The names the live registry can resolve: the built-in `synth.*` presets
/// (always present, no pack) plus every entry of each installed sample pack
/// (read by name only — no sample decode, so it stays cheap to call per eval).
pub fn known_instruments(cfg: &MerulaConfig) -> HashSet<String> {
    let mut known: HashSet<String> = HashSet::new();
    let mut builtins = Registry::new();
    builtins.install_builtin_synths();
    known.extend(builtins.instruments_list().into_iter().map(|i| i.name));
    known.extend(packs::installed_instrument_names(cfg));
    // User aliases resolve to a real voice, so `s("kick")` is a known name — never
    // flag it as an unknown instrument.
    known.extend(crate::fstate::load_aliases().into_keys());
    known
}

/// Every distinct `sound`/`inst` name the arrangement references over the probe
/// window — the renderer's precedence (`inst` over `sound`). Drives lazy sample
/// loading: the live session decodes only these instead of every installed pack
/// (VSCO/Dirt are gigabytes). Same probe window as validation, so the same blind
/// spot for a name that first appears only after `PROBE_CYCLES` (it falls back to
/// the synth until referenced within the window).
pub fn referenced_instruments(tracks: &Tracks<ControlMap>) -> HashSet<String> {
    let span = TimeSpan::new(Time::int(0), Time::int(PROBE_CYCLES));
    let mut names: HashSet<String> = HashSet::new();
    for track in &tracks.tracks {
        for hap in track.pattern.query(span) {
            if let Some(name) = hap.value.inst.as_deref().or(hap.value.sound.as_deref()) {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Every distinct `speech(...)` request the arrangement references over the probe
/// window (dedup'd by content-addressed key). The shell synthesizes each offline
/// and registers it under that key; the engine then resolves the source to it.
/// The keys also feed the lazy-load delta (so a new speech request triggers a
/// registry rebuild) — see `referenced_instruments`.
pub fn referenced_speech(tracks: &Tracks<ControlMap>) -> Vec<SpeechSpec> {
    let span = TimeSpan::new(Time::int(0), Time::int(PROBE_CYCLES));
    let mut seen: HashSet<String> = HashSet::new();
    let mut specs: Vec<SpeechSpec> = Vec::new();
    for track in &tracks.tracks {
        for hap in track.pattern.query(span) {
            if let Some(spec) = &hap.value.speech {
                if seen.insert(spec.registry_key()) {
                    specs.push(spec.clone());
                }
            }
        }
    }
    specs
}

/// Speech-only transform names. They refine a `speech(...)` source and silently
/// no-op on anything else, so chaining one onto a plain sound/note/sample is
/// almost always a mistake worth flagging.
const SPEECH_KNOBS: &[&str] = &["engine", "voice", "lang", "pitch", "rate", "mouth", "throat"];

/// Lint: warn when a speech control (`.pitch`/`.rate`/`.engine`/…) is chained
/// onto a pattern whose root is a **non-speech** source (`s`/`n`/`sample`/
/// `audio`). The builder no-ops there, so it's almost certainly unintended
/// (`.shift`/`.speed` are the sample equivalents). Works on the AST (the dropped
/// control leaves no trace in the evaluated `ControlMap`), and is **conservative**:
/// a chain rooted in a variable or a non-source call is left alone, since its
/// speech-ness isn't knowable locally — no false positives.
pub fn lint_speech_knobs(source: &str) -> Vec<Diagnostic> {
    let Ok(program) = parse(source) else {
        return Vec::new();
    };
    let mut diags = Vec::new();
    for item in &program.items {
        match item {
            Item::Let(b) => walk_expr(&b.value, &mut diags),
            Item::Fn(f) => walk_expr(&f.body, &mut diags),
            Item::Expr(e) => walk_expr(e, &mut diags),
            _ => {}
        }
    }
    diags
}

/// Recurse through an expression, flagging speech-knob methods on non-speech
/// roots and descending into every sub-expression.
fn walk_expr(e: &Expr, diags: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Method { recv, name, args } => {
            if SPEECH_KNOBS.contains(&name.name.as_str()) {
                if let Some(root) = non_speech_source_root(recv) {
                    diags.push(Diagnostic {
                        message: format!(
                            "`.{}(…)` is a speech control and has no effect on {root} — use `speech(…)`, or `.shift` / `.speed` for a normal sample",
                            name.name
                        ),
                        severity: "warning",
                        start: Some(name.span.start),
                        end: Some(name.span.end),
                    });
                }
            }
            walk_expr(recv, diags);
            for a in args {
                walk_expr(a, diags);
            }
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                walk_expr(a, diags);
            }
        }
        ExprKind::Unary { rhs, .. } => walk_expr(rhs, diags),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, diags);
            walk_expr(rhs, diags);
        }
        ExprKind::Range { lo, hi, .. } => {
            walk_expr(lo, diags);
            walk_expr(hi, diags);
        }
        ExprKind::Lambda { body, .. } => walk_expr(body, diags),
        // Leaves + islands carry no host sub-expressions.
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Note(_)
        | ExprKind::Var(_)
        | ExprKind::Island(_) => {}
    }
}

/// If the method chain rooted at `recv` bottoms out in a **known non-speech
/// source**, describe it (for the warning). `None` for a `speech(...)` root
/// (correct usage) or any root whose speech-ness isn't locally known.
fn non_speech_source_root(recv: &Expr) -> Option<&'static str> {
    // Descend through the method chain to its base expression.
    let mut cur: &Expr = recv;
    while let ExprKind::Method { recv, .. } = &cur.kind {
        cur = &**recv;
    }
    match &cur.kind {
        ExprKind::Call { name, .. } => match name.name.as_str() {
            "speech" => None, // the correct root
            "sample" => Some("a `sample(…)` source"),
            "audio" => Some("an `audio(…)` source"),
            _ => None, // a combinator / function call → can't be sure, skip
        },
        ExprKind::Island(isl) => Some(match isl.kind {
            IslandKind::Sound => "a sound pattern (`s(…)`)",
            IslandKind::Note => "a note pattern (`n(…)`)",
        }),
        _ => None, // variable / literal / etc. → conservative
    }
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
