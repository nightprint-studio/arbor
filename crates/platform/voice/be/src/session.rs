//! The listening session: one microphone stream feeding either a dictation (push-to-talk,
//! toggle, test, wake-initiated) or the always-on wake detector.
//!
//! ## Cost model
//!
//! The capture callback hands 16 kHz chunks to a pump thread. In the armed-but-silent case the
//! pump does one RMS per 20 ms frame and a few comparisons; the speech model is touched only
//! when the energy gate sees an utterance, only for its first ~2.5 s, and never more than the
//! [`WakeBudget`] allows. Nothing is written to disk and no audio leaves this process.
//!
//! ## Threads
//!
//! capture thread (owns the cpal stream) -> pump thread (this module, holds the `INNER` lock only
//! for cheap work) -> short-lived transcription threads (below-normal priority), or, for live
//! dictation, one worker per dictation (`live`). Nothing here runs on a runtime worker.
//!
//! ## Never stuck
//!
//! Every transcription is cancellable (`voice_cancel`), bounded by a deadline derived from the
//! audio length, and runs under `catch_unwind`; whatever happens, a final `voice://transcript`
//! (with an `error` if need be) and a `voice://state` follow, and every waiter is answered.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use voice_core::prelude::*;

use crate::capture::{self, Capture};
use crate::live::{self, Live};
use crate::{engine, jobs, models_cmds, priority};

const FRAME: usize = 320; // 20 ms @ 16 kHz
const WAKE_DETECT_SECONDS: usize = 3;
const WAKE_TRIGGER_SAMPLES: usize = 5 * SAMPLE_RATE as usize / 2; // 2.5 s
const WAKE_UTTERANCE_CAP: usize = 12 * SAMPLE_RATE as usize;
const PREROLL_FRAMES: usize = 15;
const WAKE_PATIENCE: Duration = Duration::from_secs(10);
const MIN_VOICED_MS: u32 = live::MIN_VOICED_MS;
/// Slack on top of the engine deadline before a waiter gives up on the thread itself.
const WAIT_SLACK: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    PushToTalk,
    Toggle,
    Wake,
    Test,
}

impl Reason {
    pub fn parse(s: &str) -> Reason {
        match s {
            "toggle" => Reason::Toggle,
            "wake" => Reason::Wake,
            "test" => Reason::Test,
            _ => Reason::PushToTalk,
        }
    }

    /// Dictations the user watches grow as they speak.
    fn can_stream(self) -> bool {
        matches!(self, Reason::PushToTalk | Reason::Toggle)
    }
}

/// The finished result of one dictation.
#[derive(Debug, Clone, Serialize)]
pub struct Transcript {
    pub id: u64,
    pub reason: Reason,
    /// Everything that was said.
    pub text: String,
    /// The part not already announced as live `voice://partial` text (equals `text` when nothing
    /// was streamed). A consumer that inserted the live pieces inserts only this.
    pub tail: String,
    /// Live pieces were announced while the user spoke.
    pub streamed: bool,
    /// Catalogue id of the model that ran (empty for a custom file).
    pub model: String,
    pub audio_secs: f32,
    pub took_secs: f32,
    pub rtf: f32,
    pub comfort: Option<Comfort>,
    /// No speech was heard: nothing was transcribed (and nothing hallucinated).
    pub silent: bool,
    /// `"cancelled"` when the user stopped it.
    pub error: Option<String>,
}

impl Transcript {
    pub fn base(id: u64, reason: Reason, audio_secs: f32) -> Self {
        Self {
            id,
            reason,
            text: String::new(),
            tail: String::new(),
            streamed: false,
            model: String::new(),
            audio_secs,
            took_secs: 0.0,
            rtf: 0.0,
            comfort: None,
            silent: false,
            error: None,
        }
    }
}

struct Active {
    id: u64,
    reason: Reason,
    buf: Vec<f32>,
    started: Instant,
    endpoint: Option<Endpointer>,
    heard_speech: bool,
    max_samples: usize,
    frames: u32,
    model: PathBuf,
    model_id: String,
    language: String,
    prompt: String,
    /// Raised by `voice_cancel`; the engine polls it.
    cancel: Arc<AtomicBool>,
    /// Present when text is announced while the user speaks.
    live: Option<Live>,
    /// Why live text is off although it was asked for (shown to the user).
    note: Option<String>,
}

struct Armed {
    endpoint: Endpointer,
    utterance: Vec<f32>,
    preroll: VecDeque<[f32; FRAME]>,
    in_utt: bool,
    utt_id: u64,
    detector_running: bool,
    detector_done: bool,
    wake_hit: Option<WakeMatch>,
    budget: WakeBudget,
    since: Instant,
    clock: Instant,
}

struct Inner {
    capture: Option<Capture>,
    active: Option<Active>,
    armed: Option<Armed>,
    next_id: u64,
    transcribing: usize,
    /// Cancel flags of the transcriptions in flight, by dictation id.
    running: Vec<(u64, Arc<AtomicBool>)>,
    vocab: VocabInput,
    carry: Vec<f32>,
    waiters: Vec<(u64, Sender<Transcript>)>,
}

static INNER: LazyLock<Mutex<Inner>> = LazyLock::new(|| {
    Mutex::new(Inner {
        capture: None,
        active: None,
        armed: None,
        next_id: 1,
        transcribing: 0,
        running: Vec::new(),
        vocab: VocabInput::default(),
        carry: Vec::new(),
        waiters: Vec::new(),
    })
});

fn lock() -> MutexGuard<'static, Inner> {
    INNER.lock().unwrap_or_else(|p| p.into_inner())
}

/// Public snapshot for `voice_status`.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub state: &'static str,
    pub armed: bool,
    pub listening_id: Option<u64>,
    pub mic_open: bool,
    pub model_loaded: usize,
    /// The current dictation shows text while the user speaks.
    pub streaming: bool,
    /// Why it does not, when it was asked to.
    pub streaming_note: Option<String>,
}

pub fn status() -> Status {
    snapshot(&lock())
}

fn snapshot(g: &Inner) -> Status {
    let state = if g.active.is_some() {
        "listening"
    } else if g.transcribing > 0 {
        "transcribing"
    } else {
        "idle"
    };
    Status {
        state,
        armed: g.armed.is_some(),
        listening_id: g.active.as_ref().map(|a| a.id),
        mic_open: g.capture.is_some(),
        model_loaded: engine::loaded(),
        streaming: g.active.as_ref().is_some_and(|a| a.live.is_some()),
        streaming_note: g.active.as_ref().and_then(|a| a.note.clone()),
    }
}

/// True while the process must not exit for idleness.
pub fn busy() -> bool {
    let g = lock();
    g.capture.is_some() || g.active.is_some() || g.armed.is_some() || g.transcribing > 0
}

pub fn set_vocab(v: VocabInput) {
    lock().vocab = v;
}

// ── model resolution ──────────────────────────────────────────────────────────

pub fn model_path(cfg: &VoiceConfig, id: Option<&str>) -> Result<PathBuf, String> {
    if id.is_none() && !cfg.custom_model_path.trim().is_empty() {
        let p = PathBuf::from(cfg.custom_model_path.trim());
        return if p.is_file() { Ok(p) } else { Err(format!("The model file {} does not exist.", p.display())) };
    }
    let wanted = id.unwrap_or(cfg.model.as_str());
    let spec = find_model(wanted).ok_or("No speech model is selected yet. Choose one in Settings > Voice.")?;
    let p = voice_models_dir().join(spec.file);
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!("The {} model is not downloaded yet. Install it in Settings > Voice.", spec.label))
    }
}

/// Catalogue id of the model a request runs with (empty for the user's own file).
fn model_id_for(cfg: &VoiceConfig, id: Option<&str>) -> String {
    match id {
        Some(id) => id.to_string(),
        None if !cfg.custom_model_path.trim().is_empty() => String::new(),
        None => cfg.model.clone(),
    }
}

/// The wake detector's model: the configured one, else the smallest installed.
fn detector_model(cfg: &VoiceConfig) -> Result<PathBuf, String> {
    if !cfg.wake.detector_model.is_empty() {
        return model_path(cfg, Some(&cfg.wake.detector_model));
    }
    let mut installed: Vec<&ModelSpec> =
        CATALOGUE.iter().filter(|m| voice_models_dir().join(m.file).is_file()).collect();
    installed.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap_or(std::cmp::Ordering::Equal));
    installed
        .first()
        .map(|m| voice_models_dir().join(m.file))
        .ok_or_else(|| "The wake word needs a small speech model. Install one in Settings > Voice.".to_string())
}

fn prompt_for(cfg: &VoiceConfig, vocab: &VocabInput) -> String {
    if cfg.vocabulary_hints {
        build_initial_prompt(vocab, &cfg.language)
    } else {
        String::new()
    }
}

/// Whether to show text while the user speaks, judged by the model's real-time factor (measured
/// when we have a measurement, estimated from the hardware otherwise). `(live, why-not)`.
fn live_plan(cfg: &VoiceConfig, reason: Reason, model_id: &str) -> (bool, Option<String>) {
    if !cfg.live_text || !reason.can_stream() || model_id.is_empty() {
        return (false, None);
    }
    let Some(spec) = find_model(model_id) else { return (false, None) };
    let (rtf, measured) = expected_rtf(spec, &models_cmds::hardware(), &load_measured());
    if streaming_comfortable(rtf) {
        (true, None)
    } else {
        let basis = if measured { "measured" } else { "estimated" };
        (false, Some(format!(
            "{} is too slow to show text while you speak on this PC ({basis}: {rtf:.1} s of work per second of speech). The text appears when you finish. A smaller model shows it live.",
            spec.label
        )))
    }
}

// ── capture lifecycle ─────────────────────────────────────────────────────────

fn ensure_capture(g: &mut Inner, state: &Arc<VoiceState>) -> Result<(), String> {
    if g.capture.is_some() {
        return Ok(());
    }
    let cfg = state.config();
    let (cap, rx) = capture::start(&cfg.microphone)?;
    g.capture = Some(cap);
    g.carry.clear();
    let st = Arc::clone(state);
    std::thread::Builder::new()
        .name("voice-pump".into())
        .spawn(move || pump(rx, st))
        .map_err(|e| format!("could not start the audio pump: {e}"))?;
    Ok(())
}

fn release_capture_if_unused(g: &mut Inner) {
    if g.active.is_none() && g.armed.is_none() {
        g.capture = None; // Drop stops the stream; the pump ends when the channel closes.
    }
}

fn emit_state(state: &VoiceState) {
    emit_snapshot(&status(), state);
}

/// For callers that already hold the lock (re-locking would deadlock).
fn emit_state_locked(g: &Inner, state: &VoiceState) {
    emit_snapshot(&snapshot(g), state);
}

fn emit_snapshot(s: &Status, state: &VoiceState) {
    state.emit(
        "voice://state",
        json!({ "state": s.state, "armed": s.armed, "id": s.listening_id, "streaming": s.streaming, "note": s.streaming_note }),
    );
}

// ── dictation ─────────────────────────────────────────────────────────────────

/// Everything that distinguishes one dictation from another; the rest comes from the config.
struct Dictation {
    reason: Reason,
    model: PathBuf,
    model_id: String,
    buf: Vec<f32>,
    endpoint: Option<Endpointer>,
    heard_speech: bool,
    max_samples: usize,
}

/// Build the active dictation, starting its live worker when the model can keep up.
fn new_active(g: &mut Inner, state: &Arc<VoiceState>, cfg: &VoiceConfig, d: Dictation) -> Active {
    let id = g.next_id;
    g.next_id += 1;
    let prompt = prompt_for(cfg, &g.vocab);
    let cancel = Arc::new(AtomicBool::new(false));
    let (live_on, note) = live_plan(cfg, d.reason, &d.model_id);
    let live = live_on
        .then(|| {
            live::start(
                state,
                live::Ctx {
                    id,
                    reason: d.reason,
                    model: d.model.clone(),
                    model_id: d.model_id.clone(),
                    language: cfg.language.clone(),
                    base_prompt: prompt.clone(),
                    cancel: Arc::clone(&cancel),
                },
            )
        })
        .flatten();
    Active {
        id,
        reason: d.reason,
        buf: d.buf,
        started: Instant::now(),
        endpoint: d.endpoint,
        heard_speech: d.heard_speech,
        max_samples: d.max_samples,
        frames: 0,
        model: d.model,
        model_id: d.model_id,
        language: cfg.language.clone(),
        prompt,
        cancel,
        live,
        note,
    }
}

/// The abstract trigger: anything that wants the user's speech (a shortcut, a button, the wake
/// word, a future trigger) goes through here. Returns the request id.
pub fn start_listening(state: &Arc<VoiceState>, reason: Reason, model_override: Option<&str>, max_secs: Option<u32>) -> Result<u64, String> {
    state.touch();
    let cfg = state.config();
    let model = model_path(&cfg, model_override)?;
    let mut g = lock();
    if let Some(existing) = &g.active {
        return Ok(existing.id); // already listening: the trigger is idempotent
    }
    ensure_capture(&mut g, state)?;
    let secs = max_secs.unwrap_or(cfg.max_seconds).min(cfg.max_seconds.max(1)) as usize;
    let endpoint = (reason == Reason::Wake)
        .then(|| Endpointer::new(EndpointerConfig { end_silence_ms: cfg.wake.silence_ms, ..Default::default() }));
    let active = new_active(
        &mut g,
        state,
        &cfg,
        Dictation {
            reason,
            model,
            model_id: model_id_for(&cfg, model_override),
            buf: Vec::new(),
            endpoint,
            heard_speech: false,
            max_samples: secs * SAMPLE_RATE as usize,
        },
    );
    let id = active.id;
    g.active = Some(active);
    drop(g);
    emit_state(state);
    Ok(id)
}

/// How long a caller waits for a transcript of `audio_secs` before declaring the thread lost.
fn wait_limit(audio_secs: f32) -> Duration {
    transcription_timeout(audio_secs) + WAIT_SLACK
}

/// Stop the current dictation and return its transcript (blocks this handler thread, never a
/// runtime worker, until the engine is done, is cancelled or its deadline passes).
pub fn stop_listening(state: &Arc<VoiceState>) -> Result<Transcript, String> {
    state.touch();
    let (rx, limit) = {
        let mut g = lock();
        let Some(active) = g.active.take() else { return Err("Nothing is being recorded.".into()) };
        let limit = wait_limit(active.buf.len() as f32 / SAMPLE_RATE as f32);
        let (tx, rx) = mpsc::channel();
        g.waiters.push((active.id, tx));
        release_capture_if_unused(&mut g);
        finish_active(&mut g, state, active);
        (rx, limit)
    };
    emit_state(state);
    rx.recv_timeout(limit).map_err(|_| "The transcription did not finish in time.".to_string())
}

/// Wait for a recording that ends on its own (a test, a wake request) to finish.
pub fn await_transcript(id: u64, audio_secs: f32) -> Result<Transcript, String> {
    let rx = {
        let mut g = lock();
        let (tx, rx) = mpsc::channel();
        g.waiters.push((id, tx));
        rx
    };
    rx.recv_timeout(wait_limit(audio_secs)).map_err(|_| "The transcription did not finish in time.".to_string())
}

/// Stop whatever is happening: drop the recording and make every transcription in flight give up
/// at its next compute step (their waiters are answered with a `cancelled` transcript).
pub fn cancel(state: &Arc<VoiceState>) {
    {
        let mut g = lock();
        if let Some(a) = g.active.take() {
            a.cancel.store(true, Ordering::Relaxed);
        }
        for (_, flag) in &g.running {
            flag.store(true, Ordering::Relaxed);
        }
        release_capture_if_unused(&mut g);
    }
    emit_state(state);
}

/// Hand a finished recording to the engine: the live worker's last job, or a one-shot thread.
fn finish_active(g: &mut Inner, state: &Arc<VoiceState>, mut active: Active) {
    let Some(mut live) = active.live.take() else {
        spawn_transcription(g, state, active);
        return;
    };
    let audio_secs = active.buf.len() as f32 / SAMPLE_RATE as f32;
    let tail = live.cutter.finish(active.buf.len()).map(|r| active.buf[r.start.min(active.buf.len())..r.end.min(active.buf.len())].to_vec());
    g.transcribing += 1;
    g.running.push((active.id, Arc::clone(&active.cancel)));
    if live.tx.send(live::Job::Finish { tail, audio_secs }).is_err() {
        // The worker is gone (it panicked): answer the waiter ourselves rather than leave it hanging.
        let mut out = Transcript::base(active.id, active.reason, audio_secs);
        out.error = Some("The live transcription stopped unexpectedly.".into());
        let st = Arc::clone(state);
        std::thread::spawn(move || publish(&st, out, true));
    }
}

fn rms_frames(buf: &[f32]) -> Vec<f32> {
    buf.chunks(FRAME).map(frame_rms).collect()
}

/// Make a finished transcript visible: update the counters, tell the frontend, answer the waiter.
/// `counted` = the dictation was counted in `transcribing`.
pub fn publish(state: &Arc<VoiceState>, transcript: Transcript, counted: bool) {
    let mut g = lock();
    if counted {
        g.transcribing = g.transcribing.saturating_sub(1);
    }
    g.running.retain(|(id, _)| *id != transcript.id);
    let waiter = g.waiters.iter().position(|(id, _)| *id == transcript.id).map(|i| g.waiters.remove(i).1);
    drop(g);
    state.touch();
    state.emit("voice://transcript", serde_json::to_value(&transcript).unwrap_or_default());
    emit_state(state);
    if let Some(tx) = waiter {
        let _ = tx.send(transcript);
    }
}

/// Hand a finished recording to a transcription thread. Called with the lock held; the thread
/// takes it again only to publish the result.
fn spawn_transcription(g: &mut Inner, state: &Arc<VoiceState>, active: Active) {
    g.transcribing += 1;
    g.running.push((active.id, Arc::clone(&active.cancel)));
    let (id, reason) = (active.id, active.reason);
    let audio_secs = active.buf.len() as f32 / SAMPLE_RATE as f32;
    let st = Arc::clone(state);
    let spawned = std::thread::Builder::new().name("voice-transcribe".into()).spawn(move || {
        priority::lower();
        let transcript = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| transcribe_active(&st, &active))).unwrap_or_else(|_| {
            let mut t = Transcript::base(id, reason, audio_secs);
            t.error = Some("The speech engine stopped unexpectedly.".into());
            t
        });
        publish(&st, transcript, true);
    });
    if spawned.is_err() {
        g.transcribing = g.transcribing.saturating_sub(1);
        g.running.retain(|(i, _)| *i != id);
        let mut t = Transcript::base(id, reason, audio_secs);
        t.error = Some("could not start the transcription thread".into());
        if let Some(i) = g.waiters.iter().position(|(w, _)| *w == id) {
            let _ = g.waiters.remove(i).1.send(t);
        }
    }
}

fn transcribe_active(state: &Arc<VoiceState>, a: &Active) -> Transcript {
    let audio_secs = a.buf.len() as f32 / SAMPLE_RATE as f32;
    let mut base = Transcript::base(a.id, a.reason, audio_secs);
    base.model = a.model_id.clone();
    if !contains_speech(&rms_frames(&a.buf), MIN_VOICED_MS) {
        return Transcript { silent: true, ..base };
    }
    let job = jobs::Job::start(&state.event_sink(), "Transcribe speech", "whisper", "voice", false);
    let started = Instant::now();
    let result = live::run_guarded(&a.model, &a.buf, &a.language, &a.prompt, &a.cancel);
    let took = started.elapsed().as_secs_f32();
    let out = match result {
        Ok(mut text) => {
            if a.reason == Reason::Wake {
                if let Some(m) = match_wake(&text, &state.config().wake.phrases) {
                    text = m.remainder;
                }
            }
            let text = clean_segment(&text).unwrap_or_default();
            let rtf = took / audio_secs.max(0.1);
            if !text.is_empty() {
                record_measurement(&a.model_id, audio_secs, rtf);
            }
            Transcript { tail: text.clone(), text, took_secs: took, rtf, comfort: Some(comfort(audio_secs, took)), ..base }
        }
        Err(e) => Transcript { error: Some(e), took_secs: took, ..base },
    };
    if let Some(job) = job {
        job.finish(out.error.clone().filter(|e| e != engine::CANCELLED));
    }
    out
}

// ── wake word ─────────────────────────────────────────────────────────────────

pub fn arm(state: &Arc<VoiceState>) -> Result<(), String> {
    state.touch();
    let cfg = state.config();
    detector_model(&cfg)?; // fail early with the actionable message
    let mut g = lock();
    if g.armed.is_some() {
        return Ok(());
    }
    g.armed = Some(new_armed());
    if let Err(e) = ensure_capture(&mut g, state) {
        g.armed = None;
        return Err(e);
    }
    drop(g);
    spawn_supervisor(Arc::clone(state));
    emit_state(state);
    Ok(())
}

fn new_armed() -> Armed {
    Armed {
        endpoint: Endpointer::new(EndpointerConfig { end_silence_ms: 600, ..Default::default() }),
        utterance: Vec::new(),
        preroll: VecDeque::new(),
        in_utt: false,
        utt_id: 0,
        detector_running: false,
        detector_done: false,
        wake_hit: None,
        budget: WakeBudget::default(),
        since: Instant::now(),
        clock: Instant::now(),
    }
}

pub fn disarm(state: &Arc<VoiceState>, why: &str) {
    let was = {
        let mut g = lock();
        let was = g.armed.take().is_some();
        release_capture_if_unused(&mut g);
        was
    };
    if was {
        state.emit("voice://disarmed", json!({ "why": why }));
        emit_state(state);
    }
}

/// Once a second while armed: honour the focus rule, the auto-disarm timer, and re-open the
/// microphone when the app comes back to the front. Exits on its own when disarmed.
fn spawn_supervisor(state: Arc<VoiceState>) {
    let _ = std::thread::Builder::new().name("voice-supervisor".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let cfg = state.config();
        let mut g = lock();
        let Some(armed) = g.armed.as_ref() else { return };
        let minutes = cfg.wake.auto_disarm_minutes as u64;
        if minutes > 0 && armed.since.elapsed() >= Duration::from_secs(minutes * 60) {
            drop(g);
            disarm(&state, "timeout");
            return;
        }
        let listening = arbor_be::app_focused() || cfg.wake.in_background;
        if listening && g.capture.is_none() {
            if let Err(e) = ensure_capture(&mut g, &state) {
                g.armed = None;
                drop(g);
                state.emit("voice://error", json!({ "message": e }));
                state.emit("voice://disarmed", json!({ "why": "error" }));
                return;
            }
        } else if !listening && g.active.is_none() && g.capture.is_some() {
            g.capture = None;
            if let Some(a) = g.armed.as_mut() {
                a.in_utt = false;
                a.utterance.clear();
            }
        }
    });
}

fn spawn_detector(state: &Arc<VoiceState>, samples: Vec<f32>, utt_id: u64, ended: bool) {
    let st = Arc::clone(state);
    let _ = std::thread::Builder::new().name("voice-wake-detect".into()).spawn(move || {
        priority::lower();
        let cfg = st.config();
        let hit = detector_model(&cfg).ok().and_then(|model| {
            let language = if cfg.language == "auto" { String::new() } else { cfg.language.clone() };
            let opts = engine::Options {
                threads: 1,
                ..engine::Options::new(&language, "Ehi Imber, hey Imber.", WAKE_DETECT_SECONDS as f32)
            };
            let head = &samples[..samples.len().min(WAKE_DETECT_SECONDS * SAMPLE_RATE as usize)];
            let heard = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine::transcribe(&model, head, &opts)));
            heard.ok().and_then(Result::ok).and_then(|text| match_wake(&text, &cfg.wake.phrases))
        });
        on_detector_result(&st, samples, utt_id, ended, hit);
    });
}

fn on_detector_result(state: &Arc<VoiceState>, samples: Vec<f32>, utt_id: u64, ended: bool, hit: Option<WakeMatch>) {
    let mut g = lock();
    let current = g.armed.as_ref().map(|a| a.utt_id);
    if let Some(a) = g.armed.as_mut() {
        if a.utt_id == utt_id {
            a.detector_running = false;
            a.detector_done = true;
        }
    }
    let (Some(m), Some(_)) = (hit, current) else { return };
    if g.active.is_some() {
        return;
    }
    state.emit("voice://wake", json!({ "phrase": m.phrase, "similarity": m.similarity }));
    if !ended {
        if current == Some(utt_id) {
            if let Some(a) = g.armed.as_mut() {
                a.wake_hit = Some(m);
            }
        }
        return;
    }
    // The utterance already ended: either the phrase was the whole utterance (wait for the
    // request) or the request came with it (transcribe now).
    let cfg = state.config();
    let Ok(model) = model_path(&cfg, None) else { return };
    let mut active = new_active(
        &mut g,
        state,
        &cfg,
        Dictation {
            reason: Reason::Wake,
            model,
            model_id: model_id_for(&cfg, None),
            buf: samples,
            endpoint: Some(Endpointer::new(EndpointerConfig { end_silence_ms: cfg.wake.silence_ms, ..Default::default() })),
            heard_speech: false,
            max_samples: cfg.max_seconds as usize * SAMPLE_RATE as usize,
        },
    );
    if m.remainder.trim().is_empty() {
        active.buf.clear();
        g.active = Some(active);
    } else {
        spawn_transcription(&mut g, state, active);
    }
    drop(g);
    emit_state(state);
}

// ── the pump ──────────────────────────────────────────────────────────────────

fn pump(rx: Receiver<Vec<f32>>, state: Arc<VoiceState>) {
    let mut since_level = 0u32;
    while let Ok(chunk) = rx.recv() {
        let mut g = lock();
        let mut carry = std::mem::take(&mut g.carry);
        carry.extend_from_slice(&chunk);
        let whole = carry.len() / FRAME * FRAME;
        for frame in carry[..whole].chunks_exact(FRAME) {
            process_frame(&mut g, &state, frame, &mut since_level);
        }
        carry.drain(..whole);
        g.carry = carry;
    }
}

fn process_frame(g: &mut Inner, state: &Arc<VoiceState>, frame: &[f32], since_level: &mut u32) {
    let rms = frame_rms(frame);
    if g.active.is_some() {
        process_active(g, state, frame, rms, since_level);
    } else if g.armed.is_some() {
        process_armed(g, state, frame, rms);
    }
}

fn process_active(g: &mut Inner, state: &Arc<VoiceState>, frame: &[f32], rms: f32, since_level: &mut u32) {
    let Some(a) = g.active.as_mut() else { return };
    a.buf.extend_from_slice(frame);
    a.frames += 1;
    *since_level += 1;
    if *since_level >= 3 {
        *since_level = 0;
        state.emit("voice://level", json!({ "id": a.id, "level": level_from_rms(rms) }));
    }
    if let Some(live) = a.live.as_mut() {
        if let Some(range) = live.cutter.push_frame(rms, FRAME) {
            let end = range.end.min(a.buf.len());
            let _ = live.tx.send(live::Job::Segment(a.buf[range.start.min(end)..end].to_vec()));
        }
    }
    let mut finish = a.buf.len() >= a.max_samples;
    if let Some(ep) = a.endpoint.as_mut() {
        match ep.push(rms) {
            EndpointEvent::SpeechStart => a.heard_speech = true,
            EndpointEvent::SpeechEnd { .. } => finish = true,
            EndpointEvent::None => {}
        }
        if !a.heard_speech && a.started.elapsed() > WAKE_PATIENCE {
            // Woken but never asked anything: give up quietly.
            g.active = None;
            release_capture_if_unused(g);
            state.emit("voice://transcript", json!({ "silent": true, "reason": "wake", "text": "" }));
            emit_state_locked(g, state);
            return;
        }
    }
    if finish {
        if let Some(active) = g.active.take() {
            release_capture_if_unused(g);
            finish_active(g, state, active);
            emit_state_locked(g, state);
        }
    }
}

fn process_armed(g: &mut Inner, state: &Arc<VoiceState>, frame: &[f32], rms: f32) {
    let cfg = state.config();
    let mut promote = false;
    let mut detect: Option<(Vec<f32>, u64, bool)> = None;
    {
        let Some(armed) = g.armed.as_mut() else { return };
        let mut f = [0f32; FRAME];
        f.copy_from_slice(frame);
        armed.preroll.push_back(f);
        if armed.preroll.len() > PREROLL_FRAMES {
            armed.preroll.pop_front();
        }
        match armed.endpoint.push(rms) {
            EndpointEvent::SpeechStart => {
                armed.in_utt = true;
                armed.utt_id += 1;
                armed.detector_running = false;
                armed.detector_done = false;
                armed.wake_hit = None;
                armed.utterance = armed.preroll.iter().flatten().copied().collect();
            }
            EndpointEvent::SpeechEnd { voiced_ms } => {
                if armed.in_utt && !armed.detector_done && !armed.detector_running && voiced_ms >= MIN_VOICED_MS {
                    let now = armed.clock.elapsed().as_millis() as u64;
                    if armed.budget.allow(now) {
                        armed.detector_running = true;
                        detect = Some((std::mem::take(&mut armed.utterance), armed.utt_id, true));
                    }
                }
                armed.in_utt = false;
                armed.utterance.clear();
            }
            EndpointEvent::None => {
                if armed.in_utt {
                    armed.utterance.extend_from_slice(frame);
                    if armed.wake_hit.is_some() {
                        promote = true;
                    } else if !armed.detector_done && !armed.detector_running && armed.utterance.len() >= WAKE_TRIGGER_SAMPLES {
                        let now = armed.clock.elapsed().as_millis() as u64;
                        if armed.budget.allow(now) {
                            armed.detector_running = true;
                            detect = Some((armed.utterance[..WAKE_TRIGGER_SAMPLES].to_vec(), armed.utt_id, false));
                        } else {
                            armed.detector_done = true;
                        }
                    }
                    if armed.utterance.len() > WAKE_UTTERANCE_CAP && armed.wake_hit.is_none() {
                        // Long speech that is not for us: stop buffering it.
                        armed.utterance.clear();
                        armed.detector_done = true;
                    }
                }
            }
        }
    }
    if let Some((samples, utt_id, ended)) = detect {
        spawn_detector(state, samples, utt_id, ended);
    }
    if promote {
        promote_wake(g, state, &cfg);
    }
}

/// The detector heard the phrase mid-utterance: the utterance so far becomes the request, and
/// the endpointer that is already inside it decides where it ends.
fn promote_wake(g: &mut Inner, state: &Arc<VoiceState>, cfg: &VoiceConfig) {
    let Ok(model) = model_path(cfg, None) else { return };
    let Some(armed) = g.armed.as_mut() else { return };
    armed.wake_hit = None;
    armed.endpoint = Endpointer::new(EndpointerConfig { end_silence_ms: 600, ..Default::default() });
    armed.since = Instant::now(); // a wake counts as use: it restarts the auto-disarm timer
    let buf = std::mem::take(&mut armed.utterance);
    armed.in_utt = false;
    let active = new_active(
        g,
        state,
        cfg,
        Dictation {
            reason: Reason::Wake,
            model,
            model_id: model_id_for(cfg, None),
            buf,
            endpoint: Some(Endpointer::new_in_speech(EndpointerConfig { end_silence_ms: cfg.wake.silence_ms, ..Default::default() })),
            heard_speech: true,
            max_samples: cfg.max_seconds as usize * SAMPLE_RATE as usize,
        },
    );
    g.active = Some(active);
    emit_state_locked(g, state);
}
