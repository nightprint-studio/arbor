//! `Session` — a running merula audio session, addressed by `MerulaState`.
//!
//! Holds the dedicated audio thread's `JoinHandle`, the control `Sender` the
//! command layer posts [`MerulaControl`](crate::control::MerulaControl) down, and
//! the shared `loaded` instrument set that gates off-thread sample decode. The
//! thread owns the real-time path; the command layer never touches the transport
//! directly.
//!
//! Ported from the shell's `src-tauri/src/merula/mod.rs` (`Session` +
//! `ensure_session` / `send_if_live` / `shutdown`), with the egress changed from
//! the Tauri `AppHandle` to an `Arc<dyn EventSink>`. Because `MerulaState` (W0)
//! exposes only the session **slot** (`MutexGuard<Option<Session>>`), the lazy
//! start / send / teardown are free functions here that the audio-command handlers
//! drive against that guard — keeping `state.rs` frozen.

use std::collections::HashSet;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use arbor_ipc::prelude::EventSink;
use merula::prelude::{ControlMap, Scene, TempoMap, Tracks};

use crate::audio_thread;
use crate::config_cmds::MerulaConfig;
use crate::control::MerulaControl;

/// A live audio session: the thread driving the cpal stream + `Transport`, and the
/// channel to post control messages to it. The thread owns the real-time path; the
/// command layer never touches the transport directly.
pub struct Session {
    /// Control channel into the audio thread (staging, transport, mixer overrides,
    /// shutdown).
    pub tx: Sender<MerulaControl>,
    /// The audio thread. Checked for `is_finished()` before reuse so a crashed /
    /// exited session is restarted rather than written to.
    pub handle: JoinHandle<()>,
    /// Instruments decoded into the live stream's registry (built-in synths +
    /// sample voices). Shared with the audio thread: the command reads it to tell
    /// whether an eval pulls in a *new* voice (which it then decodes off-thread,
    /// handing the result over in `SetTracks`); the audio thread updates it after
    /// a successful stream swap.
    pub loaded: Arc<Mutex<HashSet<String>>>,
}

/// Return a sender to the audio thread, starting it if needed. Opening the session
/// opens the audio device — done lazily here (on play), not on eval. Drives the
/// session **slot** (`MerulaState::session()`), so the caller holds the guard.
///
/// `sink` is cloned into the spawned thread for its event egress (the shell
/// re-emits to the merula window). The shared `loaded` set is seeded with the
/// always-present built-in synth names so a synth-only patch never triggers a
/// pointless decode; pack voices join it as they're first referenced.
pub fn ensure(
    slot: &mut Option<Session>,
    sink: Arc<dyn EventSink>,
    cfg: &MerulaConfig,
) -> Sender<MerulaControl> {
    if let Some(s) = slot.as_ref() {
        if !s.handle.is_finished() {
            return s.tx.clone();
        }
    }
    let (tx, rx) = mpsc::channel();
    let cfg2 = cfg.clone();
    let loaded = Arc::new(Mutex::new(audio_thread::builtin_synth_names()));
    let loaded2 = Arc::clone(&loaded);
    let handle = std::thread::Builder::new()
        .name("merula-audio".to_string())
        .spawn(move || audio_thread::run(sink, rx, cfg2, loaded2))
        .expect("spawn merula-audio thread");
    *slot = Some(Session {
        tx: tx.clone(),
        handle,
        loaded,
    });
    tx
}

/// Send to the live session, if any. No-op when nothing is running.
pub fn send_if_live(slot: &Option<Session>, msg: MerulaControl) {
    if let Some(s) = slot.as_ref() {
        let _ = s.tx.send(msg);
    }
}

/// Snapshot the control sender + shared `loaded` set of the live session, or `None`
/// when no (still-running) session exists. Used by the off-thread staging path
/// (`stage` / `audition` in `audio_cmds`) to release the session lock before any
/// `.await` — the `MutexGuard` is not `Send`.
pub fn live_handles(
    slot: &Option<Session>,
) -> Option<(Sender<MerulaControl>, Arc<Mutex<HashSet<String>>>)> {
    match slot.as_ref() {
        Some(s) if !s.handle.is_finished() => Some((s.tx.clone(), Arc::clone(&s.loaded))),
        _ => None,
    }
}

/// Tear the session down (drop the cpal stream on its thread) and join. Takes the
/// session out of the slot so a subsequent play opens a fresh one. Called on
/// merula-window close.
pub fn shutdown(slot: &mut Option<Session>) {
    if let Some(s) = slot.take() {
        let _ = s.tx.send(MerulaControl::Shutdown);
        let _ = s.handle.join();
    }
}

// ── Last-good evaluation (typed) ────────────────────────────────────────────────
//
// The most recent successful evaluation, stashed so a `play` replays it and the
// `query` / `scenes` domains can read it without re-evaluating. It holds the
// engine `Tracks<ControlMap>` — whose patterns are `Arc<dyn Fn>` closures, NOT
// serializable — so it CANNOT live in `MerulaState::latest` (a
// `Mutex<Option<serde_json::Value>>`, frozen by W0). merula-be serves exactly one
// audio session per process (one device, one transport), so a single
// process-global typed slot is the faithful home: `eval` (W3) writes it, `query` /
// `scenes` (later waves) read it. See the W3 summary's cross-wave concern.

/// The last successfully evaluated arrangement (typed; not the `Value` of
/// `MerulaState::latest`). Mirrors the shell's `merula::Latest`.
pub struct Latest {
    pub tracks: Tracks<ControlMap>,
    pub cps: Option<f64>,
    pub tempo: TempoMap,
    /// Launchable `scene(...)` declarations from the same evaluation, read by the
    /// clip launcher (`merula_scenes`) and substituted into `tracks` when fired.
    pub scenes: Vec<Scene>,
}

/// Process-global typed last-evaluation slot (see the note above).
fn latest_slot() -> &'static Mutex<Option<Latest>> {
    static SLOT: OnceLock<Mutex<Option<Latest>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Stash the latest evaluation, replacing any prior one. Called from `merula_eval`.
pub fn set_latest(latest: Latest) {
    *latest_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(latest);
}

/// Read from the latest evaluation under the lock, mapping it to `R` (the `Tracks`
/// are not `Clone`-cheap, so callers extract only what they need — e.g. the
/// transport's `(tracks, cps, tempo)` snapshot, or the scene list). `None` when no
/// evaluation has landed yet.
pub fn with_latest<R>(f: impl FnOnce(&Latest) -> R) -> Option<R> {
    latest_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(f)
}
