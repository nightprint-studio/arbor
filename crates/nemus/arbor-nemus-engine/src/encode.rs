//! Output encoders for the offline render.
//!
//! A small sink abstraction so the render loop writes blocks without caring
//! whether the target is a lossless **WAV** (via `hound`) or a lossy **Ogg
//! Vorbis** (via `vorbis_rs`). The Vorbis backend is deliberately quarantined
//! behind [`RenderSink`] / [`OggSink`] so it can be swapped for another encoder
//! (the crate is the only third-party Vorbis option and isn't heavily
//! maintained) without touching the render driver.

use std::fs::File;
use std::io::BufWriter;
use std::num::{NonZeroU32, NonZeroU8};
use std::path::Path;

use arbor_nemus_audio::prelude::Frame;

use crate::error::{EngineError, Result};
use crate::render::{BitDepth, RenderConfig};

/// Container / codec of a render's output file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Format {
    /// Lossless WAV (PCM int24 / float32, per [`RenderConfig::bit_depth`]).
    #[default]
    Wav,
    /// Lossy Ogg Vorbis (VBR; `bit_depth` does not apply).
    Ogg,
}

impl Format {
    /// Canonical file extension (no leading dot).
    pub fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Ogg => "ogg",
        }
    }

    /// Format implied by a path's extension (defaults to WAV).
    pub fn from_path(path: &Path) -> Format {
        match path.extension().and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("ogg") => Format::Ogg,
            _ => Format::Wav,
        }
    }
}

/// A block-oriented stereo audio sink the render loop drives, codec-agnostic.
pub enum RenderSink {
    Wav {
        writer: hound::WavWriter<BufWriter<File>>,
        bit_depth: BitDepth,
    },
    Ogg(OggSink),
}

impl std::fmt::Debug for RenderSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderSink::Wav { .. } => f.write_str("RenderSink::Wav"),
            RenderSink::Ogg(_) => f.write_str("RenderSink::Ogg"),
        }
    }
}

impl RenderSink {
    /// Open the sink for `format` at `out_path` (stereo, `cfg.sample_rate`).
    pub fn open(format: Format, cfg: &RenderConfig, out_path: &Path) -> Result<RenderSink> {
        match format {
            Format::Wav => Ok(RenderSink::Wav {
                writer: open_wav(cfg, out_path)?,
                bit_depth: cfg.bit_depth,
            }),
            Format::Ogg => Ok(RenderSink::Ogg(OggSink::open(cfg.sample_rate, out_path)?)),
        }
    }

    /// Write one rendered stereo block (`[L, R]` frames).
    pub fn write_block(&mut self, block: &[Frame]) -> Result<()> {
        match self {
            RenderSink::Wav { writer, bit_depth } => write_wav_block(writer, *bit_depth, block),
            RenderSink::Ogg(ogg) => ogg.write_block(block),
        }
    }

    /// Finalize the file — write the WAV header back / flush the trailing Ogg
    /// pages. A render that skips this leaves an unplayable file.
    pub fn finalize(self) -> Result<()> {
        match self {
            RenderSink::Wav { writer, .. } => writer
                .finalize()
                .map_err(|e| EngineError::Render(format!("finalizing WAV: {e}"))),
            RenderSink::Ogg(ogg) => ogg.finalize(),
        }
    }
}

// ── WAV (hound) ────────────────────────────────────────────────────────────────

fn open_wav(cfg: &RenderConfig, out_path: &Path) -> Result<hound::WavWriter<BufWriter<File>>> {
    let spec = match cfg.bit_depth {
        BitDepth::Int24 => hound::WavSpec {
            channels: 2,
            sample_rate: cfg.sample_rate,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        },
        BitDepth::Float32 => hound::WavSpec {
            channels: 2,
            sample_rate: cfg.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    };
    hound::WavWriter::create(out_path, spec)
        .map_err(|e| EngineError::Io(format!("creating {}: {e}", out_path.display())))
}

fn write_wav_block(
    writer: &mut hound::WavWriter<BufWriter<File>>,
    bit_depth: BitDepth,
    block: &[Frame],
) -> Result<()> {
    match bit_depth {
        BitDepth::Int24 => {
            for &[l, r] in block {
                writer
                    .write_sample(to_i24(l))
                    .and_then(|()| writer.write_sample(to_i24(r)))
                    .map_err(|e| EngineError::Render(format!("writing sample: {e}")))?;
            }
        }
        BitDepth::Float32 => {
            for &[l, r] in block {
                writer
                    .write_sample(l)
                    .and_then(|()| writer.write_sample(r))
                    .map_err(|e| EngineError::Render(format!("writing sample: {e}")))?;
            }
        }
    }
    Ok(())
}

/// Convert a `-1.0..=1.0` float to a 24-bit signed integer (carried in an `i32`,
/// how `hound` writes 24-bit PCM). Out-of-range values hard-clip to full-scale.
fn to_i24(sample: f32) -> i32 {
    const MAX: f32 = 8_388_607.0; // 2^23 - 1
    (sample.clamp(-1.0, 1.0) * MAX).round() as i32
}

// ── Ogg Vorbis (vorbis_rs) ──────────────────────────────────────────────────────

/// Stereo Ogg Vorbis sink. Deinterleaves each render block into per-channel
/// buffers and feeds the VBR encoder; [`finalize`](OggSink::finalize) flushes
/// the trailing Ogg pages.
pub struct OggSink {
    encoder: vorbis_rs::VorbisEncoder<BufWriter<File>>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl std::fmt::Debug for OggSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OggSink").finish_non_exhaustive()
    }
}

impl OggSink {
    fn open(sample_rate: u32, out_path: &Path) -> Result<OggSink> {
        let file = File::create(out_path)
            .map_err(|e| EngineError::Io(format!("creating {}: {e}", out_path.display())))?;
        let writer = BufWriter::new(file);
        let freq = NonZeroU32::new(sample_rate.max(1)).expect("sample rate is non-zero");
        let channels = NonZeroU8::new(2).expect("stereo");
        let encoder = vorbis_rs::VorbisEncoderBuilder::new(freq, channels, writer)
            .map_err(|e| EngineError::Render(format!("ogg encoder: {e}")))?
            .build()
            .map_err(|e| EngineError::Render(format!("ogg encoder: {e}")))?;
        Ok(OggSink {
            encoder,
            left: Vec::new(),
            right: Vec::new(),
        })
    }

    fn write_block(&mut self, block: &[Frame]) -> Result<()> {
        self.left.clear();
        self.right.clear();
        self.left.reserve(block.len());
        self.right.reserve(block.len());
        for &[l, r] in block {
            self.left.push(l);
            self.right.push(r);
        }
        self.encoder
            .encode_audio_block([self.left.as_slice(), self.right.as_slice()])
            .map_err(|e| EngineError::Render(format!("ogg encode: {e}")))
    }

    fn finalize(self) -> Result<()> {
        self.encoder
            .finish()
            .map(|_writer| ())
            .map_err(|e| EngineError::Render(format!("finalizing OGG: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_extension_round_trips() {
        assert_eq!(Format::Wav.extension(), "wav");
        assert_eq!(Format::Ogg.extension(), "ogg");
        assert_eq!(Format::from_path(Path::new("a/b/song.ogg")), Format::Ogg);
        assert_eq!(Format::from_path(Path::new("a/b/song.OGG")), Format::Ogg);
        assert_eq!(Format::from_path(Path::new("a/b/song.wav")), Format::Wav);
        assert_eq!(Format::from_path(Path::new("a/b/song")), Format::Wav);
    }

    #[test]
    fn i24_conversion_clamps_and_scales() {
        assert_eq!(to_i24(0.0), 0);
        assert_eq!(to_i24(1.0), 8_388_607);
        assert_eq!(to_i24(-1.0), -8_388_607);
        assert_eq!(to_i24(2.0), 8_388_607); // clipped
        assert_eq!(to_i24(-2.0), -8_388_607);
    }
}
