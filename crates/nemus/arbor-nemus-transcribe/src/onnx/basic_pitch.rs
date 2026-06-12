//! basic-pitch (Spotify, ICASSP 2022) polyphonic note transcription via ONNX.
//!
//! The model takes ~2 s mono windows at 22.05 kHz and emits per-frame
//! posteriorgrams; we threshold the *onset* + *note* maps into note events. This
//! is the real quality path (polyphonic, unlike the DSP YIN), at the cost of a
//! downloaded model + the onnxruntime linked into the binary.
//!
//! The input tensor name is read **from the model at load time** (it differs
//! between exports — Spotify's `nmp.onnx` uses `serving_default_input_2:0`), so
//! only the *output* names are pinned ([`OUT_NOTE`] / [`OUT_ONSET`], the values
//! Spotify's own inference code requests). Those + the `ort` 2.x run/extract API
//! are the centralised places to touch if a different export disagrees.

use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::oerr;
use crate::error::{Result, TranscribeError};
use crate::note::DetNote;

// ── Model geometry (documented basic-pitch constants) ────────────────────────
const SR: u32 = 22_050; // model input sample rate
const AUDIO_N_SAMPLES: usize = 43_844; // samples per inference window (~2 s)
const ANNOT_N_FRAMES: usize = 172; // output frames per window
const N_PITCHES: usize = 88; // note/onset bins
const MIDI_OFFSET: i32 = 21; // bin 0 = A0 (MIDI 21)
const HOP: usize = AUDIO_N_SAMPLES / ANNOT_N_FRAMES; // ≈255 samples/frame

// ── Output tensor names (as requested by Spotify's own ONNX inference) ───────
// The input name is read from the model at load time (see `Model::input_name`).
const OUT_NOTE: &str = "StatefulPartitionedCall:1";
const OUT_ONSET: &str = "StatefulPartitionedCall:2";

// ── Decode thresholds ────────────────────────────────────────────────────────
const ONSET_THRESH: f32 = 0.5;
const FRAME_THRESH: f32 = 0.3;
const MIN_FRAMES: usize = 2; // ≥ ~23 ms, filters spurious one-frame blips

/// A loaded basic-pitch session.
pub struct Model {
    session: Mutex<Session>,
    /// The model's input tensor name, read at load (export-dependent).
    input_name: String,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("basic_pitch::Model")
    }
}

/// Load the model from `path` (DirectML GPU when available, else CPU).
pub fn load(path: &Path) -> Result<Model> {
    let session = super::open_session(path)?;
    let input_name = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| TranscribeError::Backend("onnx: model has no inputs".into()))?;
    Ok(Model {
        session: Mutex::new(session),
        input_name,
    })
}

/// Transcribe `samples` (mono, `in_rate`) to pitched note events on channel 0.
pub fn infer(
    model: &Model,
    samples: &[f32],
    in_rate: u32,
    mut on_progress: impl FnMut(f32),
) -> Result<Vec<DetNote>> {
    let audio = resample_linear(samples, in_rate, SR);
    if audio.len() < HOP {
        return Ok(Vec::new());
    }

    // Non-overlapping windows: simple and robust (a small artefact every ~2 s is
    // acceptable for a baseline; overlap-add stitching is a later refinement).
    let mut note_rows: Vec<[f32; N_PITCHES]> = Vec::new();
    let mut onset_rows: Vec<[f32; N_PITCHES]> = Vec::new();
    let total = audio.len() as f32;

    let mut sess = model
        .session
        .lock()
        .map_err(|_| TranscribeError::Backend("onnx session poisoned".into()))?;

    let mut start = 0usize;
    while start < audio.len() {
        let end = (start + AUDIO_N_SAMPLES).min(audio.len());
        let mut window = vec![0.0f32; AUDIO_N_SAMPLES];
        window[..end - start].copy_from_slice(&audio[start..end]);

        let (note, onset) = run(&mut sess, &model.input_name, window)?;
        for fr in 0..ANNOT_N_FRAMES {
            let base = fr * N_PITCHES;
            let mut nrow = [0.0f32; N_PITCHES];
            let mut orow = [0.0f32; N_PITCHES];
            if base + N_PITCHES <= note.len() && base + N_PITCHES <= onset.len() {
                nrow.copy_from_slice(&note[base..base + N_PITCHES]);
                orow.copy_from_slice(&onset[base..base + N_PITCHES]);
            }
            note_rows.push(nrow);
            onset_rows.push(orow);
        }

        start += AUDIO_N_SAMPLES;
        on_progress((start as f32 / total).min(1.0));
        if end >= audio.len() {
            break;
        }
    }
    drop(sess);

    Ok(decode(&note_rows, &onset_rows))
}

/// Run one window → `(note, onset)` flattened `[ANNOT_N_FRAMES × N_PITCHES]`.
///
/// ⚠️ The two `ort` calls here — the `inputs!` macro and `try_extract_tensor` —
/// are the most likely first-run break point if the crate rc's API differs.
fn run(sess: &mut Session, input_name: &str, window: Vec<f32>) -> Result<(Vec<f32>, Vec<f32>)> {
    let input = Tensor::from_array(([1usize, AUDIO_N_SAMPLES, 1usize], window)).map_err(oerr)?;
    let outputs = sess
        .run(ort::inputs![input_name => input])
        .map_err(oerr)?;
    let (_, note) = outputs[OUT_NOTE].try_extract_tensor::<f32>().map_err(oerr)?;
    let (_, onset) = outputs[OUT_ONSET].try_extract_tensor::<f32>().map_err(oerr)?;
    Ok((note.to_vec(), onset.to_vec()))
}

/// Threshold the posteriorgrams into note events: a note starts where onset and
/// note both cross their thresholds, and runs while the note map stays above its
/// threshold (a fresh onset ends the current note and starts the next).
fn decode(note: &[[f32; N_PITCHES]], onset: &[[f32; N_PITCHES]]) -> Vec<DetNote> {
    let dt = HOP as f64 / SR as f64;
    let n = note.len();
    let mut notes = Vec::new();
    for p in 0..N_PITCHES {
        let mut f = 0;
        while f < n {
            let is_onset = onset[f][p] >= ONSET_THRESH && note[f][p] >= FRAME_THRESH;
            if !is_onset {
                f += 1;
                continue;
            }
            let start_f = f;
            let mut g = f + 1;
            let mut peak = note[f][p];
            while g < n && note[g][p] >= FRAME_THRESH && onset[g][p] < ONSET_THRESH {
                peak = peak.max(note[g][p]);
                g += 1;
            }
            let len = g - start_f;
            if len >= MIN_FRAMES {
                let vel = (peak.clamp(0.0, 1.0) * 96.0 + 31.0) as u8; // 31..127
                notes.push(DetNote {
                    start_sec: start_f as f64 * dt,
                    dur_sec: len as f64 * dt,
                    pitch: (MIDI_OFFSET + p as i32).clamp(0, 127) as u8,
                    vel,
                    channel: 0,
                });
            }
            f = g;
        }
    }
    notes
}

/// Linear-interpolation resample. Good enough for ML preprocessing and dependency
/// -free; the model is robust to the mild aliasing this introduces.
fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if input.is_empty() || from == to {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let last = input.len() - 1;
    (0..out_len)
        .map(|i| {
            let src = i as f64 / ratio;
            let i0 = (src.floor() as usize).min(last);
            let i1 = (i0 + 1).min(last);
            let frac = (src - i0 as f64) as f32;
            input[i0] + (input[i1] - input[i0]) * frac
        })
        .collect()
}
