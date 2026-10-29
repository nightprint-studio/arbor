//! Diagnostics: transcribe a WAV file through the very same engine path dictation uses and report
//! the timings. Hidden (not wired to any UI); it is how the real-time factor of a model on a given
//! machine is measured without a microphone, and how the build's CPU features are read back.

use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use voice_core::prelude::*;

use crate::{engine, session};

#[derive(Deserialize)]
pub struct FileRequest {
    pub path: String,
    /// Catalogue id; empty = the configured model.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub threads: Option<usize>,
    /// Default true; false = whisper's full 30 s window.
    #[serde(default)]
    pub short_context: Option<bool>,
    #[serde(default)]
    pub prompt: String,
    /// Cap on tokens (benchmarks of random-weight models, whose output never ends by itself).
    #[serde(default)]
    pub max_tokens: Option<i32>,
    /// Cancel after this many milliseconds (exercises the cancel path).
    #[serde(default)]
    pub cancel_after_ms: Option<u64>,
    /// Deadline in milliseconds (exercises the timeout path).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Serialize)]
pub struct FileReport {
    system_info: String,
    threads: usize,
    short_context: bool,
    load_secs: f32,
    audio_secs: f32,
    took_secs: f32,
    rtf: f32,
    text: String,
}

/// PCM16 WAV -> mono f32 at 16 kHz.
pub fn read_wav_16k(path: &str) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("could not read {path}: {e}"))?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let (mut channels, mut rate, mut bits, mut data) = (1usize, 16_000u32, 16u16, None);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let body = at + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]) as usize;
            rate = u32::from_le_bytes([bytes[body + 4], bytes[body + 5], bytes[body + 6], bytes[body + 7]]);
            bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
        } else if id == b"data" {
            data = Some(&bytes[body..(body + len).min(bytes.len())]);
            break;
        }
        at = body + len + (len & 1);
    }
    if bits != 16 {
        return Err("only 16-bit PCM WAV is supported".into());
    }
    let data = data.ok_or("the WAV has no data chunk")?;
    let samples: Vec<f32> = data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    Ok(resample_to_16k(&downmix_to_mono(&samples, channels.max(1)), rate))
}

#[arbor_rpc::handler]
fn voice_transcribe_file(state: &VoiceState, request: FileRequest) -> Result<FileReport, String> {
    state.touch();
    let cfg = state.config();
    // A path to a ggml file, or a catalogue id.
    let model: PathBuf = if request.model.ends_with(".bin") {
        PathBuf::from(&request.model)
    } else {
        session::model_path(&cfg, if request.model.is_empty() { None } else { Some(request.model.as_str()) })?
    };
    let samples = read_wav_16k(&request.path)?;
    let audio_secs = samples.len() as f32 / SAMPLE_RATE as f32;
    let t = Instant::now();
    engine::preload(&model)?;
    let load_secs = t.elapsed().as_secs_f32();
    let language = request.language.unwrap_or(cfg.language);
    let mut opts = engine::Options::new(&language, &request.prompt, audio_secs);
    if let Some(n) = request.threads {
        opts.threads = n.max(1);
    }
    opts.short_context = request.short_context.unwrap_or(true);
    opts.max_tokens = request.max_tokens;
    if let Some(ms) = request.timeout_ms {
        opts.timeout = std::time::Duration::from_millis(ms);
    }
    if let Some(ms) = request.cancel_after_ms {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        opts.cancel = Some(std::sync::Arc::clone(&flag));
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }
    let t = Instant::now();
    let text = engine::transcribe(&model, &samples, &opts)?;
    let took_secs = t.elapsed().as_secs_f32();
    Ok(FileReport {
        system_info: engine::system_info(),
        threads: opts.threads,
        short_context: opts.short_context,
        load_secs,
        audio_secs,
        took_secs,
        rtf: took_secs / audio_secs.max(0.1),
        text,
    })
}

#[derive(Serialize)]
pub struct SystemReport {
    engine: bool,
    system_info: String,
    threads: usize,
    physical_cores: usize,
    logical_cores: usize,
}

/// What the linked whisper.cpp was compiled with and how many threads a run would use.
#[arbor_rpc::handler]
fn voice_system_info(_state: &VoiceState) -> Result<SystemReport, String> {
    Ok(SystemReport {
        engine: engine::compiled_in(),
        system_info: engine::system_info(),
        threads: engine::default_threads(),
        physical_cores: engine::physical_cores(),
        logical_cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
    })
}

#[derive(Serialize)]
pub struct MatmulReport {
    threads: usize,
    /// Giga-multiply-adds per second of a 1500-row quantised (q5_0) matmul, the shape the Whisper
    /// encoder spends its time in.
    gmacs_q5_0: f32,
    gmacs_f16: f32,
}

/// Raw matrix-multiply throughput of this machine through ggml, to turn a model's encoder size
/// into an expected time without needing the model file.
#[arbor_rpc::handler]
fn voice_bench_matmul(_state: &VoiceState, threads: Option<usize>) -> Result<MatmulReport, String> {
    let threads = threads.unwrap_or_else(engine::default_threads);
    Ok(MatmulReport { threads, gmacs_q5_0: matmul::gmacs(threads, matmul::Kind::Q5_0)?, gmacs_f16: matmul::gmacs(threads, matmul::Kind::F16)? })
}

#[cfg(feature = "whisper")]
mod matmul {
    use std::time::Instant;
    use whisper_rs::whisper_rs_sys as sys;

    #[derive(Clone, Copy)]
    pub enum Kind {
        Q5_0,
        F16,
    }

    pub fn gmacs(threads: usize, kind: Kind) -> Result<f32, String> {
        // 1280 -> 5120 (the encoder MLP of large-v3), 1500 frames.
        let (k, n, t) = (1280i64, 5120i64, 1500i64);
        // SAFETY: a private ggml context that lives for this call; tensors are only touched inside it.
        unsafe {
            let ctx = sys::ggml_init(sys::ggml_init_params { mem_size: 512 << 20, mem_buffer: std::ptr::null_mut(), no_alloc: false });
            if ctx.is_null() {
                return Err("ggml_init failed".into());
            }
            let ty = match kind {
                Kind::Q5_0 => sys::ggml_type_GGML_TYPE_Q5_0,
                Kind::F16 => sys::ggml_type_GGML_TYPE_F16,
            };
            let w = sys::ggml_new_tensor_2d(ctx, ty, k, n);
            let x = sys::ggml_new_tensor_2d(ctx, sys::ggml_type_GGML_TYPE_F32, k, t);
            let y = sys::ggml_mul_mat(ctx, w, x);
            let g = sys::ggml_new_graph(ctx);
            sys::ggml_build_forward_expand(g, y);
            let bytes = sys::ggml_nbytes(w);
            let wp = std::slice::from_raw_parts_mut((*w).data as *mut u8, bytes);
            for (i, b) in wp.iter_mut().enumerate() {
                let random = ((i as u32).wrapping_mul(2654435761) >> 24) as u8;
                // Normal (not denormal) f16 values: the scale of a q5_0 block, or every f16 weight.
                *b = match kind {
                    Kind::Q5_0 if i % 22 == 0 => 0,
                    Kind::Q5_0 if i % 22 == 1 => 0x2e,
                    Kind::F16 if i % 2 == 1 => 0x2e,
                    _ => random,
                };
            }
            let xp = std::slice::from_raw_parts_mut((*x).data as *mut f32, (k * t) as usize);
            xp.iter_mut().enumerate().for_each(|(i, v)| *v = ((i % 97) as f32 - 48.0) * 0.001);
            sys::ggml_graph_compute_with_ctx(ctx, g, threads as i32);
            let reps = 3;
            let start = Instant::now();
            for _ in 0..reps {
                sys::ggml_graph_compute_with_ctx(ctx, g, threads as i32);
            }
            let secs = start.elapsed().as_secs_f32() / reps as f32;
            sys::ggml_free(ctx);
            Ok((k * n * t) as f32 / secs / 1e9)
        }
    }
}

#[cfg(not(feature = "whisper"))]
mod matmul {
    #[derive(Clone, Copy)]
    pub enum Kind {
        Q5_0,
        F16,
    }
    pub fn gmacs(_threads: usize, _kind: Kind) -> Result<f32, String> {
        Err("built without the speech engine".into())
    }
}

#[derive(Serialize)]
pub struct EncodeReport {
    threads: usize,
    mel_secs: f32,
    /// Encoder pass over a full 30 s window (what a run without `audio_ctx` pays).
    encode_secs: f32,
}

/// Mel + encoder time for a WAV, on the full window.
#[arbor_rpc::handler]
fn voice_bench_encode(state: &VoiceState, path: String, model: String, threads: Option<usize>) -> Result<EncodeReport, String> {
    state.touch();
    let model_path: PathBuf = if model.ends_with(".bin") { PathBuf::from(&model) } else { session::model_path(&state.config(), Some(&model))? };
    let samples = read_wav_16k(&path)?;
    let threads = threads.unwrap_or_else(engine::default_threads);
    let (mel_secs, encode_secs) = engine::bench_encode(&model_path, &samples, threads)?;
    Ok(EncodeReport { threads, mel_secs, encode_secs })
}
