//! `materialize` domain — **freeze** a pattern to its literal source.
//!
//! `merula_materialize` evaluates a self-contained snippet (the front end prepends
//! the file's constants/imports) and materializes the first track's pattern over
//! one cycle to canonical literal source (`n(c4 e4 g4)` / `s(bd sn)`), the unit the
//! editor splices back in to replace a generative expression with the concrete
//! notes it produces.
//!
//! Pure: **no audio, no live state**. It runs on the dispatcher's `spawn_blocking`
//! worker, so it's lowered to a sync `#[handler]`. Ported from the shell's
//! `src-tauri/src/merula/mod.rs::merula_materialize`, with the live-staging path
//! dropped (materialize never touches the session) and the eval lowered to a
//! standalone parse+evaluate that uses a [`SilentLog`] sink instead of the shell's
//! app-emitting log sink — so it needs no `AppHandle` and emits nothing. A bad
//! snippet (parse / eval error) → empty string (lints live elsewhere), exactly as
//! the shell did.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use merula::prelude::{
    evaluate, materialize_source, parse, EvalConfig, EvalOutput, IslandKind, LogSink, SilentLog,
    SourceLoader, Time, TimeSpan,
};

use crate::config_cmds;
use crate::state::MerulaState;

/// A filesystem [`SourceLoader`] for the standalone (silent) evaluation. A
/// `$lib/<name>/<file>` import resolves against the synced external-library cache
/// (`libs`, name → cache dir); every other path resolves against the project
/// directory (`base`). Lives only for one evaluation, so `Rc` is fine.
///
/// Local to materialise for W1 (the only pure handler that evaluates); the W3 eval
/// domain lifts the identical loader into `eval.rs` alongside the app-emitting log
/// sink. Byte-faithful to the shell's `eval::FsLoader`.
struct FsLoader {
    base: PathBuf,
    libs: BTreeMap<String, PathBuf>,
}

impl SourceLoader for FsLoader {
    fn load(&self, path: &str) -> Result<String, String> {
        if let Some(rest) = path.strip_prefix("$lib/") {
            let (name, file) = rest
                .split_once('/')
                .ok_or_else(|| format!("import {path}: expected $lib/<name>/<file>"))?;
            let dir = self.libs.get(name).ok_or_else(|| {
                format!("library `{name}` is not synced — run Sync libraries (import {path})")
            })?;
            let resolved = dir.join(file);
            return std::fs::read_to_string(&resolved)
                .map_err(|e| format!("cannot read import {path}: {e}"));
        }
        let resolved = self.base.join(path);
        std::fs::read_to_string(&resolved)
            .map_err(|e| format!("cannot read import {}: {e}", resolved.display()))
    }
}

/// **Freeze** a pattern: evaluate `source` and materialize the first track's
/// pattern over one cycle to canonical literal source. Returns an empty string
/// when the snippet doesn't evaluate or yields no onsets (the caller leaves the
/// source untouched).
#[arbor_rpc::handler]
fn merula_materialize(
    _ctx: &MerulaState,
    source: String,
    project_dir: Option<String>,
) -> Result<String, String> {
    let cfg = config_cmds::load();
    let base = project_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let output = match evaluate_silent(&source, base, cfg.eval_config()) {
        Some(o) => o,
        None => return Ok(String::new()), // bad snippet → no-op (lints live elsewhere)
    };
    let Some(track) = output.tracks.tracks.first() else {
        return Ok(String::new());
    };
    // Freeze one cycle — the common case (euclid / random / chord generators are
    // per-cycle). A multi-cycle pattern captures its first cycle.
    let haps = track.pattern.query(TimeSpan::new(Time::int(0), Time::int(1)));
    if haps.is_empty() {
        return Ok(String::new());
    }
    // Note island when any onset carries a pitch; a sound island only when there
    // are sounds and no notes; default to notes (covers scale-degree patterns).
    let any_note = haps.iter().any(|h| h.value.note.is_some());
    let any_sound = haps.iter().any(|h| h.value.sound.is_some());
    let kind = if any_note || !any_sound { IslandKind::Note } else { IslandKind::Sound };
    Ok(materialize_source(kind, &haps))
}

/// Parse + evaluate a snippet **silently** (no log emission), resolving imports
/// against `base_dir` + the project's synced `$lib` cache. Returns `None` on any
/// parse / eval error — the materialise caller treats a bad snippet as a no-op.
/// The log sink is [`SilentLog`] (materialise surfaces nothing), so this needs no
/// event egress; the W3 eval domain adds the app-emitting log path.
fn evaluate_silent(source: &str, base_dir: PathBuf, cfg: EvalConfig) -> Option<EvalOutput> {
    let program = parse(source).ok()?;
    let libs = crate::libraries::resolve_dirs(&base_dir);
    let loader: Rc<dyn SourceLoader> = Rc::new(FsLoader { base: base_dir, libs });
    let log: Arc<dyn LogSink> = Arc::new(SilentLog);
    evaluate(&program, loader, log, cfg).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Materialising a concrete note pattern round-trips to canonical literal
    /// source the editor can splice back in. A self-contained `tracks(...)` snippet
    /// → the first track's one-cycle freeze.
    #[test]
    fn materialize_round_trip_notes() {
        let src = "tracks(track(\"lead\", n(c4 e4 g4)))";
        let cfg = config_cmds::MerulaConfig::default().eval_config();
        let out = evaluate_silent(src, PathBuf::from("."), cfg).expect("evaluates");
        let track = out.tracks.tracks.first().expect("one track");
        let haps = track.pattern.query(TimeSpan::new(Time::int(0), Time::int(1)));
        assert!(!haps.is_empty(), "the pattern produces onsets");
        let literal = materialize_source(IslandKind::Note, &haps);
        // Re-evaluating the materialised literal yields the same set of onsets:
        // freezing is an identity on already-concrete patterns.
        let again = evaluate_silent(
            &format!("tracks(track(\"lead\", {literal}))"),
            PathBuf::from("."),
            config_cmds::MerulaConfig::default().eval_config(),
        )
        .expect("re-evaluates");
        let again_haps = again
            .tracks
            .tracks
            .first()
            .expect("one track")
            .pattern
            .query(TimeSpan::new(Time::int(0), Time::int(1)));
        assert_eq!(again_haps.len(), haps.len(), "freeze preserves the onset count");
    }

    /// A syntactically invalid snippet evaluates to `None` (→ the handler's empty
    /// string), never a panic.
    #[test]
    fn bad_snippet_is_none() {
        let cfg = config_cmds::MerulaConfig::default().eval_config();
        assert!(evaluate_silent("tracks(track(\"x\",", PathBuf::from("."), cfg).is_none());
    }
}
