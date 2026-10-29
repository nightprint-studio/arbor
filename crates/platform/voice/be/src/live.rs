//! Live dictation: text while the user is still speaking.
//!
//! The pump (see `session`) cuts the recording at pauses with a [`SegmentCutter`] and sends every
//! finished piece here. One worker thread per dictation transcribes the pieces in order, in the
//! background and at below-normal priority, and announces each as a `voice://partial` event. When
//! the user stops, the unfinished tail is the last piece and the worker publishes the final
//! transcript (`text` = everything, `tail` = only what was not already announced).
//!
//! Every piece goes through [`run_guarded`]: cancellable, bounded by a deadline derived from its
//! length, and panic-safe, so the worker can never hang or die silently.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use serde_json::json;
use voice_core::prelude::*;

use crate::session::{self, Reason, Transcript};
use crate::{engine, jobs, priority};

/// Context handed to the next piece (characters of the text so far).
const PROMPT_TAIL_CHARS: usize = 200;
/// Minimum voiced time worth transcribing in one piece.
pub const MIN_VOICED_MS: u32 = 300;

/// Work for the worker.
pub enum Job {
    /// A finished piece, to transcribe now.
    Segment(Vec<f32>),
    /// The recording ended: transcribe the unfinished tail (if any) and publish the result.
    Finish { tail: Option<Vec<f32>>, audio_secs: f32 },
}

/// The pump's side of a live dictation.
pub struct Live {
    pub cutter: SegmentCutter,
    pub tx: Sender<Job>,
}

/// What the worker needs to know about its dictation.
pub struct Ctx {
    pub id: u64,
    pub reason: Reason,
    pub model: PathBuf,
    pub model_id: String,
    pub language: String,
    /// Vocabulary hint; the previous text is appended to it.
    pub base_prompt: String,
    pub cancel: Arc<AtomicBool>,
}

fn rms_frames(buf: &[f32]) -> Vec<f32> {
    buf.chunks(320).map(frame_rms).collect()
}

/// Transcribe `samples` with every safety net: speech check, cancel flag, deadline, panic guard.
/// `Ok("")` for a piece without speech.
pub fn run_guarded(model: &Path, samples: &[f32], language: &str, prompt: &str, cancel: &Arc<AtomicBool>) -> Result<String, String> {
    if !contains_speech(&rms_frames(samples), MIN_VOICED_MS) {
        return Ok(String::new());
    }
    let audio_secs = samples.len() as f32 / SAMPLE_RATE as f32;
    let mut opts = engine::Options::new(language, prompt, audio_secs);
    opts.cancel = Some(Arc::clone(cancel));
    catch_unwind(AssertUnwindSafe(|| engine::transcribe(model, samples, &opts)))
        .unwrap_or_else(|_| Err("The speech engine stopped unexpectedly.".to_string()))
}

fn joined_prompt(base: &str, previous: &str) -> String {
    match (base.is_empty(), previous.is_empty()) {
        (_, true) => base.to_string(),
        (true, false) => previous.to_string(),
        (false, false) => format!("{base} {previous}"),
    }
}

/// Start the worker for one dictation. `None` when the thread could not be spawned (the dictation
/// then simply transcribes on release).
pub fn start(state: &Arc<VoiceState>, ctx: Ctx) -> Option<Live> {
    let (tx, rx) = mpsc::channel();
    let st = Arc::clone(state);
    std::thread::Builder::new()
        .name("voice-live".into())
        .spawn(move || worker(st, rx, ctx))
        .ok()
        .map(|_| Live { cutter: SegmentCutter::new(), tx })
}

struct Totals {
    assembler: Assembler,
    audio_secs: f32,
    took_secs: f32,
    seq: u32,
    first_error: Option<String>,
    job: Option<jobs::Job>,
}

fn worker(state: Arc<VoiceState>, rx: Receiver<Job>, ctx: Ctx) {
    priority::lower();
    let mut t = Totals { assembler: Assembler::new(), audio_secs: 0.0, took_secs: 0.0, seq: 0, first_error: None, job: None };
    while let Ok(job) = rx.recv() {
        match job {
            Job::Segment(samples) => {
                if run_piece(&state, &ctx, &mut t, &samples) {
                    t.assembler.mark_shown();
                    let so_far = t.assembler.text();
                    state.emit("voice://partial", json!({ "id": ctx.id, "seq": t.seq, "text": so_far, "reason": ctx.reason }));
                }
            }
            Job::Finish { tail, audio_secs } => {
                if let Some(samples) = tail {
                    run_piece(&state, &ctx, &mut t, &samples);
                }
                finish(&state, &ctx, t, audio_secs);
                return;
            }
        }
    }
    // The channel closed without a Finish: the dictation was cancelled while recording.
    if let Some(job) = t.job.take() {
        job.finish(Some("cancelled".into()));
    }
}

/// Transcribe one piece into the assembler; `true` when it added text.
fn run_piece(state: &Arc<VoiceState>, ctx: &Ctx, t: &mut Totals, samples: &[f32]) -> bool {
    if ctx.cancel.load(Ordering::Relaxed) {
        return false;
    }
    if t.job.is_none() {
        t.job = jobs::Job::start(&state.event_sink(), "Transcribe speech", "whisper", "voice", false);
    }
    let prompt = joined_prompt(&ctx.base_prompt, &t.assembler.prompt_tail(PROMPT_TAIL_CHARS));
    let started = Instant::now();
    let result = run_guarded(&ctx.model, samples, &ctx.language, &prompt, &ctx.cancel);
    let took = started.elapsed().as_secs_f32();
    match result {
        Ok(raw) => {
            if !raw.is_empty() {
                t.audio_secs += samples.len() as f32 / SAMPLE_RATE as f32;
                t.took_secs += took;
            }
            t.seq += 1;
            t.assembler.push(&raw).is_some()
        }
        Err(e) => {
            if e != engine::CANCELLED && t.first_error.is_none() {
                t.first_error = Some(e);
            }
            false
        }
    }
}

fn finish(state: &Arc<VoiceState>, ctx: &Ctx, mut t: Totals, audio_secs: f32) {
    let cancelled = ctx.cancel.load(Ordering::Relaxed);
    let text = t.assembler.text();
    let mut out = Transcript::base(ctx.id, ctx.reason, audio_secs);
    out.model = ctx.model_id.clone();
    out.took_secs = t.took_secs;
    if cancelled {
        out.error = Some(engine::CANCELLED.into());
    } else if text.is_empty() {
        match t.first_error.clone() {
            Some(e) => out.error = Some(e),
            None => out.silent = true,
        }
    } else {
        out.tail = t.assembler.undelivered();
        out.streamed = t.assembler.streamed();
        out.text = text;
        if t.audio_secs > 0.0 {
            out.rtf = t.took_secs / t.audio_secs;
            out.comfort = Some(comfort(t.audio_secs, t.took_secs));
            record_measurement(&ctx.model_id, t.audio_secs, out.rtf);
        }
    }
    if let Some(job) = t.job.take() {
        job.finish(out.error.clone().filter(|e| e != engine::CANCELLED));
    }
    session::publish(state, out, true);
}
