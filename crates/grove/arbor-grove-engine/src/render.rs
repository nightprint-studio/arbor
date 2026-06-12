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
    AudioCommand, DelayConfig, Frame, Renderer, SourceKind, TrackConfig, VoiceSource,
    DEFAULT_BLOCK_FRAMES, DEFAULT_SAMPLE_RATE,
};
use arbor_grove_pattern::prelude::{ControlMap, Time, TimeSpan, Tracks};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::clock::Epoch;
use crate::error::{EngineError, Result};
use crate::schedule::{delay_config_for, schedule_span};

/// Frames per render block. Small enough that `start_frame`s land in the right
/// block, large enough to keep the per-block overhead negligible offline. Shares
/// the audio backend's processing block size ([`DEFAULT_BLOCK_FRAMES`]).
const BLOCK_FRAMES: usize = DEFAULT_BLOCK_FRAMES;

/// Default offline-render bit depth ([`BitDepth::Int24`]) — the canonical value
/// the shell config mirrors.
pub const DEFAULT_BIT_DEPTH: BitDepth = BitDepth::Int24;

/// Default trailing tail (release/reverb) captured after the arrangement, in
/// seconds. The canonical value the shell config mirrors.
pub const DEFAULT_TAIL_MAX_SECS: f32 = 4.0;

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
    /// Output sample rate (frames/s). Default [`DEFAULT_SAMPLE_RATE`].
    pub sample_rate: u32,
    /// Sample format. Default [`DEFAULT_BIT_DEPTH`].
    pub bit_depth: BitDepth,
    /// Max trailing silence/tail to capture after the arrangement, in seconds
    /// (release/reverb). Default [`DEFAULT_TAIL_MAX_SECS`].
    pub tail_max_secs: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            sample_rate: DEFAULT_SAMPLE_RATE,
            bit_depth: DEFAULT_BIT_DEPTH,
            tail_max_secs: DEFAULT_TAIL_MAX_SECS,
        }
    }
}

/// How far an offline render has progressed, reported to the optional progress
/// callback of [`render_offline_with_progress`]. `total_frames` is the whole
/// bounce (arrangement + tail); `done_frames` rises to it as blocks are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderProgress {
    /// Frames written so far.
    pub done_frames: u64,
    /// Total frames the render will write (`0` only for an empty bounce).
    pub total_frames: u64,
}

impl RenderProgress {
    /// Completion as a `0.0..=1.0` fraction (`1.0` for an empty render).
    pub fn fraction(self) -> f32 {
        if self.total_frames == 0 {
            return 1.0;
        }
        (self.done_frames as f64 / self.total_frames as f64).clamp(0.0, 1.0) as f32
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
    render_offline_with_progress(tracks, cps, cycles, cfg, out_path, |_| {})
}

/// Like [`render_offline`], but reports progress: `on_progress` is invoked after
/// each block is written (and once at the start) with the running
/// [`RenderProgress`]. The callback runs on the render thread — keep it cheap
/// (the shell throttles + forwards it as an event). Anything else is identical.
pub fn render_offline_with_progress(
    tracks: &Tracks<ControlMap>,
    cps: f64,
    cycles: u32,
    cfg: &RenderConfig,
    out_path: &Path,
    mut on_progress: impl FnMut(RenderProgress),
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
    // Built-in `synth.*` presets, same as live playback, so `.inst("synth.lead")`
    // and friends render as intended instead of the default fallback voice.
    renderer.registry_mut().install_builtin_synths();

    // Preload every file source up front. The real-time path can't decode in the
    // callback; offline we have no such constraint, but the `Renderer` still only
    // plays a `File` voice if its path was preloaded (otherwise it falls back to
    // the synth). So scan the whole arrangement for `sample`/`audio` markers and
    // preload them before the block loop — without this, file sources render as
    // synth. Best-effort: a missing/undecodable file is left to the synth
    // fallback rather than aborting the bounce.
    preload_file_sources(&mut renderer, tracks, cycles);

    // Total length: the requested cycles + a tail for releases / reverb.
    let arrangement_frames = (cycles as f64 * epoch.frames_per_cycle(sr)).round() as u64;
    let tail_frames = (cfg.tail_max_secs.max(0.0) as f64 * sr as f64).round() as u64;
    let total_frames = arrangement_frames + tail_frames;

    let mut writer = wav_writer(cfg, out_path)?;

    // Voice-id counter and cross-render sustained dedup, threaded across blocks
    // (the schedule core is pure, so this state lives here — like the transport).
    let mut next_id: u64 = 0;
    let mut sustained_started: HashSet<(u32, String)> = HashSet::new();
    // Last delay-bus config applied per track, so we only re-emit `SetTrackDelay`
    // when a track's delay line actually changes (avoids spamming the command
    // stream and reconfiguring the line — which would reset its tail — each onset).
    let mut delay_state: HashMap<u32, DelayConfig> = HashMap::new();

    let mut block: Vec<Frame> = vec![[0.0, 0.0]; BLOCK_FRAMES];
    let mut frame_cursor: u64 = 0;

    // Mixer layout up front, then one Voice-bearing block at a time.
    let mut initial = Some(AudioCommand::ConfigureTracks(track_configs.clone()));

    // Capture (don't `?`) a mid-loop write error so we can still finalize: a WAV
    // whose header is never written back (RIFF/`data` chunk sizes) is unplayable
    // even though megabytes of samples reached disk. A short, *valid* file (and a
    // surfaced error) beats a large corrupt one.
    let mut write_err: Option<EngineError> = None;
    on_progress(RenderProgress { done_frames: 0, total_frames });
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
                // Reconfigure the track's delay bus only when its config changes.
                if let Some(AudioCommand::SetTrackDelay(track, cfg)) =
                    delay_config_for(&ev, &epoch, sr)
                {
                    if delay_state.get(&track) != Some(&cfg) {
                        delay_state.insert(track, cfg);
                        voice_cmds.push(AudioCommand::SetTrackDelay(track, cfg));
                    }
                }
                voice_cmds.push(AudioCommand::Voice(ev));
            }
        }

        // The first block also carries the initial ConfigureTracks, ahead of voices.
        let mut cmds = initial.take().into_iter().chain(voice_cmds);

        let out = &mut block[..block_len];
        renderer.process(&mut cmds, out);
        if let Err(e) = write_block(&mut writer, cfg, out) {
            write_err = Some(e);
            break;
        }

        frame_cursor = block_end;
        on_progress(RenderProgress { done_frames: frame_cursor, total_frames });
    }

    // Always finalize, even after a write error, so the file is a valid WAV.
    let finalized = writer
        .finalize()
        .map_err(|e| EngineError::Render(format!("finalizing WAV: {e}")));
    match write_err {
        Some(e) => Err(e),
        None => finalized,
    }
}

/// Scan `cycles` cycles of `tracks` for distinct file-source paths and preload
/// each into the `Renderer`, so `sample`/`audio` voices decode at trigger time
/// instead of falling back to the synth.
///
/// One query over the whole `[0, cycles)` window catches sources that only appear
/// on some cycles (`cat`, `arrange`, cycle-seeded choice). Paths are deduped, and
/// preload is **best-effort**: the `Renderer` keys files by the exact path string
/// `resolve_source` stamps on `VoiceSource::File`, so a successful preload always
/// hits, and a failure (missing/undecodable file) is left to the synth fallback.
fn preload_file_sources(renderer: &mut Renderer, tracks: &Tracks<ControlMap>, cycles: u32) {
    if cycles == 0 {
        return;
    }
    let span = TimeSpan::new(Time::int(0), Time::int(cycles as i64));
    let mut seen: HashSet<String> = HashSet::new();
    for t in &tracks.tracks {
        for hap in t.pattern.query(span) {
            if let Some(path) = &hap.value.source_file {
                if seen.insert(path.clone()) {
                    let _ = renderer.preload_file(Path::new(path));
                }
            }
        }
    }
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
        assert_eq!(c.sample_rate, DEFAULT_SAMPLE_RATE);
        assert_eq!(c.bit_depth, DEFAULT_BIT_DEPTH);
        assert_eq!(c.tail_max_secs, DEFAULT_TAIL_MAX_SECS);
    }

    #[test]
    fn i24_conversion_clamps_and_scales() {
        assert_eq!(to_i24(0.0), 0);
        assert_eq!(to_i24(1.0), 8_388_607);
        assert_eq!(to_i24(-1.0), -8_388_607);
        assert_eq!(to_i24(2.0), 8_388_607); // clipped
        assert_eq!(to_i24(-2.0), -8_388_607);
    }

    #[test]
    fn render_with_missing_file_source_falls_back_and_succeeds() {
        use arbor_grove_pattern::prelude::{sample, track, tracks};

        // A `sample(...)` pointing at a non-existent file: preload fails, and the
        // best-effort scan must not abort the bounce — the voice falls back to the
        // synth and the render still writes a valid WAV.
        let t = tracks(vec![track("chop", sample("does-not-exist.wav"))]);
        let out = std::env::temp_dir().join("grove_render_missing_source.wav");
        let res = render_offline(&t, 1.0, 1, &RenderConfig::default(), &out);
        let cleanup = std::fs::remove_file(&out);
        assert!(res.is_ok(), "missing file source must not abort the render");
        assert!(cleanup.is_ok(), "render should have produced the WAV file");
    }
}
