//! Audio file decoding — the **non-real-time** path.
//!
//! Everything here allocates and does file IO; it runs on a worker thread (or at
//! load time), never in the cpal callback. The product is a [`DecodedAudio`]:
//! interleaved-to-mono `f32` samples plus the source sample rate, which the
//! sampler keeps resident behind an `Arc` for the RT path to read.
//!
//! WAV goes through `hound` (fast, ubiquitous for SFZ packs); compressed formats
//! (mp3/ogg/flac) go through `symphonia`.

use std::path::Path;

use crate::error::{AudioError, Result};

/// A fully decoded audio file: mono `f32` samples and the rate they were
/// recorded at. The sampler resamples from `sample_rate` to the device rate (and
/// to the target pitch) on the fly.
#[derive(Clone, Debug)]
pub struct DecodedAudio {
    /// Mono samples (channels averaged), normalised to roughly `[-1, 1]`.
    pub samples: Vec<f32>,
    /// The file's native sample rate.
    pub sample_rate: u32,
}

impl DecodedAudio {
    /// Decode any supported file, dispatching on extension (WAV → `hound`,
    /// everything else → `symphonia`).
    pub fn load(path: &Path) -> Result<DecodedAudio> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "wav" | "wave" => load_wav(path),
            _ => load_symphonia(path),
        }
    }
}

/// Decode a WAV file to mono `f32` via `hound`.
fn load_wav(path: &Path) -> Result<DecodedAudio> {
    let reader = hound::WavReader::open(path).map_err(|e| AudioError::Decode {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let sample_rate = spec.sample_rate;

    let mono = match spec.sample_format {
        hound::SampleFormat::Float => {
            let samples: std::result::Result<Vec<f32>, _> =
                reader.into_samples::<f32>().collect();
            interleaved_to_mono(
                &samples.map_err(|e| decode_err(path, e))?,
                channels,
            )
        }
        hound::SampleFormat::Int => {
            // Normalise by the format's full-scale magnitude.
            let bits = spec.bits_per_sample;
            let scale = 1.0 / (1i64 << (bits - 1)) as f32;
            let samples: std::result::Result<Vec<i32>, _> =
                reader.into_samples::<i32>().collect();
            let floats: Vec<f32> = samples
                .map_err(|e| decode_err(path, e))?
                .into_iter()
                .map(|s| s as f32 * scale)
                .collect();
            interleaved_to_mono(&floats, channels)
        }
    };

    Ok(DecodedAudio {
        samples: mono,
        sample_rate,
    })
}

fn decode_err(path: &Path, e: impl std::fmt::Display) -> AudioError {
    AudioError::Decode {
        path: path.display().to_string(),
        reason: e.to_string(),
    }
}

/// Average interleaved channels down to a mono track.
fn interleaved_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let frames = interleaved.len() / channels;
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut sum = 0.0;
        for c in 0..channels {
            sum += interleaved[f * channels + c];
        }
        mono.push(sum / channels as f32);
    }
    mono
}

/// Decode mp3/ogg/flac (and WAV fallback) to mono `f32` via `symphonia`.
fn load_symphonia(path: &Path) -> Result<DecodedAudio> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| decode_err(path, e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| decode_err(path, e))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| AudioError::Decode {
            path: path.display().to_string(),
            reason: "no audio track".to_string(),
        })?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| decode_err(path, e))?;

    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);

    let mut mono: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                sample_rate = spec.rate;
                // Channel count is read off each decoded frame's own spec.
                let channels = spec.channels.count().max(1);
                if sample_buf.is_none() {
                    sample_buf =
                        Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                if let Some(buf) = sample_buf.as_mut() {
                    buf.copy_interleaved_ref(decoded);
                    append_mono(&mut mono, buf.samples(), channels);
                }
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(decode_err(path, e)),
        }
    }

    if mono.is_empty() {
        return Err(AudioError::Decode {
            path: path.display().to_string(),
            reason: "decoded zero samples".to_string(),
        });
    }

    Ok(DecodedAudio {
        samples: mono,
        sample_rate,
    })
}

/// Append an interleaved chunk to `mono`, averaging channels.
fn append_mono(mono: &mut Vec<f32>, interleaved: &[f32], channels: usize) {
    if channels <= 1 {
        mono.extend_from_slice(interleaved);
        return;
    }
    let frames = interleaved.len() / channels;
    mono.reserve(frames);
    for f in 0..frames {
        let mut sum = 0.0;
        for c in 0..channels {
            sum += interleaved[f * channels + c];
        }
        mono.push(sum / channels as f32);
    }
}
