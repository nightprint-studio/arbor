//! Demucs (HT-Demucs FT) source separation via ONNX.
//!
//! We only need two things from separation: the **isolated drums** (for the onset
//! detector) and **everything-but-drums** (a percussion-free signal for the pitch
//! model). So a single model — the HT-Demucs FT *drums specialist* — suffices: it
//! outputs the four-stem tensor, we keep its accurate drums row, and derive the
//! melodic signal by subtraction (`mix − drums`, since the stems sum to the mix).
//! Waveform-in / waveform-out, processed in fixed [`SEGMENT_SAMPLES`] windows with
//! triangular overlap-add and a stereo→mono downmix.
//!
//! I/O matches the StemSplitio HT-Demucs FT ONNX export (verified on its model
//! card): input `mix` `(1, 2, 343980)` f32 @ 44.1 kHz, output `stems`
//! `(1, 4, 2, 343980)` f32 in `[drums, bass, other, vocals]` order. A different
//! export may rename the tensors or change the segment length — those constants
//! are centralised here (and the `ort` run/extract surface in [`run`]) so a
//! mismatch is a localised fix.

use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::oerr;
use crate::error::{Result, TranscribeError};

// ── Model geometry (StemSplitio HT-Demucs FT export) ─────────────────────────
/// Demucs operating sample rate.
pub const SR: u32 = 44_100;
/// Stem rows in the output tensor, and the index of the one we read.
const NUM_SOURCES: usize = 4;
const IDX_DRUMS: usize = 0; // output order: [drums, bass, other, vocals]
/// Samples per inference window (~7.8 s).
const SEGMENT_SAMPLES: usize = 343_980;
/// Overlap between consecutive windows for seam-free overlap-add (25%).
const OVERLAP: usize = SEGMENT_SAMPLES / 4;
/// Input/output channels (stereo in, per-stem stereo out).
const CHANNELS: usize = 2;

// ── Output tensor name (StemSplitio export). Input name is read at load. ─────
const OUTPUT_NAME: &str = "stems";

/// The two signals separation gives us, mono, at [`SR`].
#[derive(Debug)]
pub struct Stems {
    /// Isolated drums (the specialist's target row).
    pub drums: Vec<f32>,
    /// Everything but drums (`mix − drums`) — the ideal input for a polyphonic
    /// pitch model (no percussion to confuse it).
    pub melodic: Vec<f32>,
}

/// A loaded Demucs session.
pub struct Model {
    session: Mutex<Session>,
    /// The model's input tensor name, read at load (export-dependent).
    input_name: String,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("demucs::Model")
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

/// Separate `samples` (mono, `in_rate`) into drums + everything-but-drums at [`SR`].
pub fn separate(
    model: &Model,
    samples: &[f32],
    in_rate: u32,
    mut on_progress: impl FnMut(f32),
) -> Result<Stems> {
    let mono = resample_linear(samples, in_rate, SR);
    let n = mono.len();
    if n == 0 {
        return Ok(Stems { drums: Vec::new(), melodic: Vec::new() });
    }

    // Overlap-add the drums stem; the weight sum normalises the crossfades.
    let mut drums = vec![0.0f32; n];
    let mut wsum = vec![0.0f32; n];
    let weights = triangular_window(SEGMENT_SAMPLES);
    let hop = SEGMENT_SAMPLES - OVERLAP;

    let mut sess = model
        .session
        .lock()
        .map_err(|_| TranscribeError::Backend("onnx session poisoned".into()))?;

    let mut start = 0usize;
    loop {
        let end = (start + SEGMENT_SAMPLES).min(n);
        let mut window = vec![0.0f32; SEGMENT_SAMPLES];
        window[..end - start].copy_from_slice(&mono[start..end]);

        let seg_drums = run(&mut sess, &model.input_name, &window)?; // [SEGMENT_SAMPLES] mono drums
        for i in 0..(end - start) {
            let w = weights[i];
            drums[start + i] += seg_drums[i] * w;
            wsum[start + i] += w;
        }

        on_progress((end as f32 / n as f32).min(1.0));
        if end >= n {
            break;
        }
        start += hop;
    }
    drop(sess);

    // Normalise by the accumulated weights, then derive the melodic signal.
    let mut melodic = vec![0.0f32; n];
    for i in 0..n {
        if wsum[i] > 1e-6 {
            drums[i] /= wsum[i];
        }
        melodic[i] = mono[i] - drums[i];
    }
    Ok(Stems { drums, melodic })
}

/// Run one window → the drums stem, `[SEGMENT_SAMPLES]` mono (stereo averaged).
///
/// ⚠️ Same `ort` 2.x surface as `basic_pitch::run` (`inputs!`, `try_extract_tensor`)
/// — the most likely first-run break point if the crate rc's API differs.
fn run(sess: &mut Session, input_name: &str, window: &[f32]) -> Result<Vec<f32>> {
    // Stereo, channel-major: duplicate the mono mix into both channels.
    let mut input = Vec::with_capacity(CHANNELS * SEGMENT_SAMPLES);
    for _ in 0..CHANNELS {
        input.extend_from_slice(window);
    }
    let tensor =
        Tensor::from_array(([1usize, CHANNELS, SEGMENT_SAMPLES], input)).map_err(oerr)?;
    let outputs = sess.run(ort::inputs![input_name => tensor]).map_err(oerr)?;
    let (_, data) = outputs[OUTPUT_NAME].try_extract_tensor::<f32>().map_err(oerr)?;

    // Output layout [1, NUM_SOURCES, CHANNELS, SEGMENT_SAMPLES]; keep the drums row.
    const _: () = assert!(IDX_DRUMS < NUM_SOURCES);
    let mut drums = vec![0.0f32; SEGMENT_SAMPLES];
    for (i, drum) in drums.iter_mut().enumerate().take(SEGMENT_SAMPLES) {
        let mut sum = 0.0f32;
        for c in 0..CHANNELS {
            let idx = (IDX_DRUMS * CHANNELS + c) * SEGMENT_SAMPLES + i;
            if idx < data.len() {
                sum += data[idx];
            }
        }
        *drum = sum / CHANNELS as f32;
    }
    Ok(drums)
}

/// Triangular overlap-add window (1 at the centre, tapering to a small floor at
/// the edges so no sample gets zero weight).
fn triangular_window(len: usize) -> Vec<f32> {
    let center = (len as f32 - 1.0) / 2.0;
    (0..len)
        .map(|i| (1.0 - (i as f32 - center).abs() / center).max(1e-3))
        .collect()
}

/// Linear-interpolation resample (dependency-free; demucs is robust to the mild
/// aliasing). Identical in spirit to `basic_pitch`'s.
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
