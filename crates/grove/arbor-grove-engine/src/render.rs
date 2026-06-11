//! Offline render: drive the audio [`Renderer`](arbor_grove_audio::prelude::Renderer)
//! block by block in **non-real-time** and write the result to a WAV file.
//!
//! Reuses [`schedule_span`](crate::schedule::schedule_span) and the same
//! `Renderer` as live playback — only the driver differs (a synchronous pull
//! loop instead of the cpal callback). Non-real-time work, so it is fine to run
//! under Arbor's job system (hard rule: never the *real-time* path).
//!
//! Length = one pass of the arrangement + a tail until silence (capped by
//! [`RenderConfig::tail_max_secs`]). Output is WAV via `hound`.

use std::path::Path;

use arbor_grove_audio::prelude::{
    AudioCommand, Frame, Renderer, SourceKind, TrackConfig, VoiceSource,
};
use arbor_grove_pattern::prelude::{ControlMap, Tracks};
use std::collections::HashSet;

use crate::clock::Epoch;
use crate::error::{EngineError, Result};
use crate::schedule::schedule_span;

/// Frames per render block. Small enough that `start_frame`s land in the right
/// block, large enough to keep the per-block overhead negligible offline.
const BLOCK_FRAMES: usize = 512;

/// Sample format of the rendered WAV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitDepth {
    /// 24-bit signed PCM (the default).
    Int24,
    /// 32-bit float.
    Float32,
}

/// Options for an offline render.
#[derive(Clone, Copy, Debug)]
pub struct RenderConfig {
    /// Output sample rate (frames/s). Default `48_000`.
    pub sample_rate: u32,
    /// Sample format. Default [`BitDepth::Int24`].
    pub bit_depth: BitDepth,
    /// Max trailing silence/tail to capture after the arrangement, in seconds
    /// (release/reverb). Default `4.0`.
    pub tail_max_secs: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            sample_rate: 48_000,
            bit_depth: BitDepth::Int24,
            tail_max_secs: 4.0,
        }
    }
}

/// Render `cycles` cycles of `tracks` (at `cps`) to a stereo WAV at `out_path`,
/// plus a trailing tail up to [`RenderConfig::tail_max_secs`].
///
/// The render length is an **explicit cycle count**, not auto-detected: a
/// `Pattern` doesn't expose its `arrange` period, so the caller (the shell, or
/// the user) decides how many cycles to bounce.
///
/// Drives the same scheduling + `Renderer` as live playback in a synchronous
/// block loop. Sustained file sources are deduped across the whole render with a
/// single started-set (mirroring the transport's cross-tick dedup), so a stem
/// starts once and rings rather than retriggering each cycle.
pub fn render_offline(
    tracks: &Tracks<ControlMap>,
    cps: f64,
    cycles: u32,
    cfg: &RenderConfig,
    out_path: &Path,
) -> Result<()> {
    let sr = cfg.sample_rate;
    let epoch = Epoch::start(cps);

    let track_configs: Vec<TrackConfig> = tracks
        .tracks
        .iter()
        .map(|t| TrackConfig {
            name: t.name.clone(),
        })
        .collect();
    let mut renderer = Renderer::new(sr, &track_configs);

    // Total length: the requested cycles + a tail for releases / reverb.
    let arrangement_frames = (cycles as f64 * epoch.frames_per_cycle(sr)).round() as u64;
    let tail_frames = (cfg.tail_max_secs.max(0.0) as f64 * sr as f64).round() as u64;
    let total_frames = arrangement_frames + tail_frames;

    let mut writer = wav_writer(cfg, out_path)?;

    // Voice-id counter and cross-render sustained dedup, threaded across blocks
    // (the schedule core is pure, so this state lives here — like the transport).
    let mut next_id: u64 = 0;
    let mut sustained_started: HashSet<(u32, String)> = HashSet::new();

    let mut block: Vec<Frame> = vec![[0.0, 0.0]; BLOCK_FRAMES];
    let mut frame_cursor: u64 = 0;

    // Mixer layout up front, then one Voice-bearing block at a time.
    let mut initial = Some(AudioCommand::ConfigureTracks(track_configs.clone()));

    while frame_cursor < total_frames {
        let block_len = ((total_frames - frame_cursor) as usize).min(BLOCK_FRAMES);
        let block_end = frame_cursor + block_len as u64;

        // Schedule voices whose onset lands in this block. Past the arrangement we
        // stop scheduling new onsets (tail is pure decay), but keep rendering so
        // already-sounding voices ring out.
        let mut voice_cmds: Vec<AudioCommand> = Vec::new();
        if frame_cursor < arrangement_frames {
            let span_end = block_end.min(arrangement_frames);
            let events = schedule_span(tracks, &epoch, sr, frame_cursor..span_end, &mut next_id);
            for ev in events {
                if let VoiceSource::File {
                    path,
                    kind: SourceKind::Sustained,
                } = &ev.source
                {
                    if !sustained_started.insert((ev.track, path.clone())) {
                        continue;
                    }
                }
                voice_cmds.push(AudioCommand::Voice(ev));
            }
        }

        // The first block also carries the initial ConfigureTracks, ahead of voices.
        let mut cmds = initial.take().into_iter().chain(voice_cmds);

        let out = &mut block[..block_len];
        renderer.process(&mut cmds, out);
        write_block(&mut writer, cfg, out)?;

        frame_cursor = block_end;
    }

    writer
        .finalize()
        .map_err(|e| EngineError::Render(format!("finalizing WAV: {e}")))?;
    Ok(())
}

/// Open a `hound` WAV writer matching `cfg`'s format (stereo, `cfg.sample_rate`).
fn wav_writer(cfg: &RenderConfig, out_path: &Path) -> Result<hound::WavWriter<std::io::BufWriter<std::fs::File>>> {
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

/// Write one rendered block, interleaving L/R and converting to the WAV format.
fn write_block(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    cfg: &RenderConfig,
    block: &[Frame],
) -> Result<()> {
    match cfg.bit_depth {
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

/// Convert a `-1.0..=1.0` float sample to a 24-bit signed integer (carried in an
/// `i32`, which is how `hound` writes 24-bit PCM). Out-of-range values are
/// hard-clipped to the 24-bit full-scale.
fn to_i24(sample: f32) -> i32 {
    const MAX: f32 = 8_388_607.0; // 2^23 - 1
    let scaled = (sample.clamp(-1.0, 1.0) * MAX).round();
    scaled as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_match_design() {
        let c = RenderConfig::default();
        assert_eq!(c.sample_rate, 48_000);
        assert_eq!(c.bit_depth, BitDepth::Int24);
        assert_eq!(c.tail_max_secs, 4.0);
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
