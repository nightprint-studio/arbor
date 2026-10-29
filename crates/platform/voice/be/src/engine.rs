//! The speech engine seam. With the `whisper` feature this drives whisper.cpp; without it every
//! transcription answers with a clear error and nothing else in the backend changes.
//!
//! Loaded contexts are cached by model path (the wake detector's small model and the dictation
//! model can both be resident) and dropped by [`unload_all`] when the process goes idle, which is
//! what gives the memory back.
//!
//! Every transcription is **bounded and cancellable**: whisper.cpp polls an abort callback inside
//! its compute loops, and ours answers `true` once the caller's cancel flag is raised or the
//! deadline has passed. A transcription therefore can never run forever.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// The error text of a transcription the caller cancelled (not shown to the user as a failure).
pub const CANCELLED: &str = "cancelled";

/// What the caller wants from one transcription.
#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "whisper"), allow(dead_code))]
pub struct Options {
    /// `auto` or an ISO 639-1 code.
    pub language: String,
    /// Vocabulary hint / previous text (may be empty).
    pub initial_prompt: String,
    pub threads: usize,
    /// Shrink the encoder window to the clip length (whisper always encodes 30 s otherwise).
    pub short_context: bool,
    /// Raised by the caller to stop the run between compute steps.
    pub cancel: Option<Arc<AtomicBool>>,
    /// Hard ceiling on the run.
    pub timeout: Duration,
    /// Cap on tokens per window (diagnostics only; `None` = whisper default).
    pub max_tokens: Option<i32>,
}

impl Options {
    pub fn new(language: &str, initial_prompt: &str, audio_secs: f32) -> Self {
        Self {
            language: language.to_string(),
            initial_prompt: initial_prompt.to_string(),
            threads: default_threads(),
            short_context: true,
            cancel: None,
            timeout: voice_core::prelude::transcription_timeout(audio_secs),
            max_tokens: None,
        }
    }
}

/// Whether this build contains the speech engine.
pub const fn compiled_in() -> bool {
    cfg!(feature = "whisper")
}

/// Whether a GPU back end was compiled in (what the model recommendation may count on).
pub const fn gpu_compiled_in() -> bool {
    cfg!(any(feature = "metal", feature = "vulkan", feature = "cuda"))
}

/// Threads for the matrix work: the physical cores (ggml scales with them and degrades when it
/// oversubscribes hyper-threads), leaving one core free on bigger machines so the editor stays
/// responsive. `VOICE_THREADS` overrides it for diagnosis.
pub fn default_threads() -> usize {
    if let Some(n) = std::env::var("VOICE_THREADS").ok().and_then(|v| v.parse::<usize>().ok()) {
        return n.clamp(1, 64);
    }
    let physical = physical_cores();
    voice_core::prelude::threads_for_cores(physical)
}

/// Physical cores (not hyper-threads); half the logical count when the OS will not say.
pub fn physical_cores() -> usize {
    sysinfo::System::new()
        .physical_core_count()
        .unwrap_or_else(|| (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).max(1))
}

/// CPU features the linked whisper.cpp was built with (`AVX2 = 1 | FMA = 1 | ...`).
pub fn system_info() -> String {
    imp::system_info()
}

#[cfg(feature = "whisper")]
mod imp {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, LazyLock, Mutex};
    use std::time::Instant;

    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    use super::{Options, CANCELLED};

    /// What the engine polls between compute steps to know whether to give up.
    struct AbortCheck {
        cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
        deadline: Instant,
    }

    unsafe extern "C" fn should_abort(data: *mut std::ffi::c_void) -> bool {
        // SAFETY: `data` is the `AbortCheck` that `transcribe` keeps alive for the whole run.
        let check = unsafe { &*(data as *const AbortCheck) };
        check.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) || Instant::now() > check.deadline
    }

    static CONTEXTS: LazyLock<Mutex<HashMap<PathBuf, Arc<WhisperContext>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

    pub fn system_info() -> String {
        whisper_rs::print_system_info().to_string()
    }

    fn context(model: &Path) -> Result<Arc<WhisperContext>, String> {
        let mut map = CONTEXTS.lock().map_err(|_| "the speech engine is unavailable".to_string())?;
        if let Some(c) = map.get(model) {
            return Ok(Arc::clone(c));
        }
        let path = model.to_str().ok_or("the model path is not valid UTF-8")?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| format!("could not load the model {}: {e}", model.display()))?;
        let ctx = Arc::new(ctx);
        // Keep at most two resident (dictation + wake detector).
        if map.len() >= 2 {
            map.clear();
        }
        map.insert(model.to_path_buf(), Arc::clone(&ctx));
        Ok(ctx)
    }

    pub fn context_for_bench(model: &Path) -> Result<Arc<WhisperContext>, String> {
        context(model)
    }

    /// Load a model without transcribing (warm-up; also what a benchmark times separately).
    pub fn preload(model: &Path) -> Result<(), String> {
        context(model).map(|_| ())
    }

    pub fn transcribe(model: &Path, samples: &[f32], opts: &Options) -> Result<String, String> {
        let ctx = context(model)?;
        let mut state = ctx.create_state().map_err(|e| format!("speech engine error: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(opts.threads.max(1) as i32);
        params.set_language(Some(if opts.language.is_empty() { "auto" } else { opts.language.as_str() }));
        // Dictation only: never translate, never carry decoder state between calls (the caller
        // passes the previous text as `initial_prompt` when it wants continuity).
        params.set_translate(false);
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_no_speech_thold(0.6);
        // No temperature fallback: on a hard clip whisper otherwise re-decodes the whole window up
        // to five times at rising temperatures, which on a CPU looks like a hang.
        params.set_temperature_inc(0.0);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        if opts.short_context {
            if let Some(frames) = voice_core::prelude::audio_ctx_for(samples.len() as f32 / 16_000.0) {
                params.set_audio_ctx(frames);
            }
        }
        if let Some(n) = opts.max_tokens {
            params.set_max_tokens(n);
        }
        if !opts.initial_prompt.is_empty() {
            params.set_initial_prompt(&opts.initial_prompt);
        }
        let started = Instant::now();
        // The safe wrapper `set_abort_callback_safe` of whisper-rs 0.16 reads its boxed closure back
        // as the wrong type (undefined behaviour: it aborted every run at random), so the raw
        // callback is used with a context that lives until `full` returns.
        let abort = AbortCheck { cancel: opts.cancel.clone(), deadline: started + opts.timeout };
        // SAFETY: `abort` outlives the `full` call below, and the callback only reads it.
        unsafe {
            params.set_abort_callback(Some(should_abort));
            params.set_abort_callback_user_data(&abort as *const AbortCheck as *mut std::ffi::c_void);
        }
        let outcome = state.full(params, samples);
        if opts.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(CANCELLED.into());
        }
        if started.elapsed() > opts.timeout {
            return Err(format!("The transcription took longer than {} s and was stopped.", opts.timeout.as_secs()));
        }
        outcome.map_err(|e| format!("transcription failed: {e}"))?;
        let mut text = String::new();
        for i in 0..state.full_n_segments() {
            if let Some(segment) = state.get_segment(i) {
                if let Ok(piece) = segment.to_str_lossy() {
                    text.push_str(&piece);
                }
            }
        }
        Ok(text.trim().to_string())
    }

    pub fn unload_all() {
        if let Ok(mut map) = CONTEXTS.lock() {
            map.clear();
        }
    }

    pub fn loaded() -> usize {
        CONTEXTS.lock().map(|m| m.len()).unwrap_or(0)
    }
}

#[cfg(not(feature = "whisper"))]
mod imp {
    use std::path::Path;

    use super::Options;

    fn no_engine() -> String {
        "This build of Arbor does not include the speech engine (voice-be was built without the \
         `whisper` feature)."
            .into()
    }

    pub fn system_info() -> String {
        "built without the speech engine".into()
    }

    pub fn preload(_model: &Path) -> Result<(), String> {
        Err(no_engine())
    }

    pub fn transcribe(_model: &Path, _samples: &[f32], _opts: &Options) -> Result<String, String> {
        Err(no_engine())
    }

    pub fn unload_all() {}

    pub fn loaded() -> usize {
        0
    }
}

pub fn transcribe(model: &Path, samples: &[f32], opts: &Options) -> Result<String, String> {
    imp::transcribe(model, samples, opts)
}

pub fn preload(model: &Path) -> Result<(), String> {
    imp::preload(model)
}

pub fn unload_all() {
    imp::unload_all();
}

pub fn loaded() -> usize {
    imp::loaded()
}

/// Diagnostics: time the mel + encoder pass alone (full 30 s window), in seconds.
#[cfg(feature = "whisper")]
pub fn bench_encode(model: &Path, samples: &[f32], threads: usize) -> Result<(f32, f32), String> {
    let ctx = imp::context_for_bench(model)?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    state.pcm_to_mel(samples, threads).map_err(|e| e.to_string())?;
    let mel = t.elapsed().as_secs_f32();
    let t = std::time::Instant::now();
    state.encode(0, threads).map_err(|e| e.to_string())?;
    Ok((mel, t.elapsed().as_secs_f32()))
}

#[cfg(not(feature = "whisper"))]
pub fn bench_encode(_model: &Path, _samples: &[f32], _threads: usize) -> Result<(f32, f32), String> {
    Err("built without the speech engine".into())
}
