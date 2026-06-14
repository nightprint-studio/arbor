//! Offline render: drive the audio [`Renderer`](arbor_nemus_audio::prelude::Renderer)
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

use arbor_nemus_audio::prelude::{
    AudioCommand, DelayConfig, Frame, Renderer, SourceKind, TrackConfig, VoiceSource,
    DEFAULT_BLOCK_FRAMES, DEFAULT_SAMPLE_RATE,
};
use arbor_nemus_pattern::prelude::{ControlMap, Time, TimeSpan, Tracks};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::clock::Epoch;
use crate::encode::{Format, RenderSink};
use crate::error::{EngineError, Result};
use crate::schedule::{delay_config_for, schedule_span, track_fx_commands};

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
    /// Sample format (WAV only). Default [`DEFAULT_BIT_DEPTH`].
    pub bit_depth: BitDepth,
    /// Max trailing silence/tail to capture after the arrangement, in seconds
    /// (release/reverb). Default [`DEFAULT_TAIL_MAX_SECS`].
    pub tail_max_secs: f32,
    /// Output container/codec (WAV vs Ogg Vorbis). Default [`Format::Wav`].
    /// This is a per-export choice, not a persisted preference.
    pub format: Format,
    /// Target **integrated loudness** (LUFS, ITU-R BS.1770) to normalize the
    /// bounce to, or `None` to leave levels untouched (the default). A typical
    /// streaming target is `-14.0`. Applied as a single broadband gain after the
    /// render, peak-limited so it never clips.
    pub normalize: Option<f32>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            sample_rate: DEFAULT_SAMPLE_RATE,
            bit_depth: DEFAULT_BIT_DEPTH,
            tail_max_secs: DEFAULT_TAIL_MAX_SECS,
            format: Format::Wav,
            normalize: None,
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

/// How an offline render ended: it ran to completion, or the caller's
/// `should_cancel` requested an early stop. On `Cancelled` the file on disk is
/// still finalized (a valid, if partial, WAV) — it's the caller's choice to keep
/// or delete it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderOutcome {
    /// The full `cycles` (+ tail) were rendered.
    Completed,
    /// `should_cancel` returned true mid-bounce; the partial file was finalized.
    Cancelled,
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
    render_offline_with_progress(tracks, cps, 0, cycles, cfg, out_path, |_| {}, || false).map(|_| ())
}

/// Like [`render_offline`], but renders an **arbitrary cycle window** with
/// progress + cancellation. The bounce covers `[start_cycle, start_cycle + cycles)`
/// (a region export), plus the usual trailing tail; pass `start_cycle = 0` for the
/// whole arrangement. The output file always starts at frame 0 — voices in the
/// window are re-based onto the output's local timeline, so the region plays from
/// the top of the file. Onsets that fall *before* `start_cycle` are not captured
/// (a region starts clean), matching a DAW's loop-region bounce.
///
/// `on_progress` is invoked after each block is written (and once at the start)
/// with the running [`RenderProgress`], and `should_cancel` is polled before each
/// block — when it returns true the bounce stops early, the partial file is
/// finalized, and [`RenderOutcome::Cancelled`] is returned. Both callbacks run on
/// the render thread — keep them cheap (the shell throttles progress + forwards it
/// as an event, and checks a job flag for cancellation).
pub fn render_offline_with_progress(
    tracks: &Tracks<ControlMap>,
    cps: f64,
    start_cycle: u32,
    cycles: u32,
    cfg: &RenderConfig,
    out_path: &Path,
    mut on_progress: impl FnMut(RenderProgress),
    should_cancel: impl Fn() -> bool,
) -> Result<RenderOutcome> {
    let sr = cfg.sample_rate;
    let epoch = Epoch::start(cps);
    // Frame offset of the region's first cycle. Voices are queried at absolute
    // frames (from the epoch) but written at output-local frames, so we subtract
    // this from every scheduled onset to re-base the region onto `[0, …)`.
    let fpc = epoch.frames_per_cycle(sr);
    let start_frame = (start_cycle as f64 * fpc).round() as u64;

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
    preload_file_sources(&mut renderer, tracks, start_cycle, cycles);

    // Total length: the requested cycles + a tail for releases / reverb.
    let arrangement_frames = (cycles as f64 * fpc).round() as u64;
    let tail_frames = (cfg.tail_max_secs.max(0.0) as f64 * sr as f64).round() as u64;
    let total_frames = arrangement_frames + tail_frames;

    let mut sink = RenderSink::open(cfg.format, cfg, out_path)?;

    // Optional LUFS normalization: when a target is set we meter the whole bounce
    // (ITU-R BS.1770 integrated loudness) and buffer the frames, then apply a
    // single peak-limited gain and write at the end — a one-pass measurement that
    // sets the level without re-rendering. `None` streams straight to disk. A
    // failed meter init silently falls back to no normalization.
    let mut meter = match cfg.normalize {
        Some(_) => ebur128::EbuR128::new(2, sr, ebur128::Mode::I).ok(),
        None => None,
    };
    let mut norm_buffer: Vec<Frame> = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    if meter.is_some() {
        norm_buffer.reserve(total_frames as usize);
        interleaved.reserve(BLOCK_FRAMES * 2);
    }

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

    // Mixer layout up front (plus the per-track FX inserts implied by the source),
    // then one Voice-bearing block at a time.
    let mut initial: Option<Vec<AudioCommand>> = Some({
        let mut v = vec![AudioCommand::ConfigureTracks(track_configs.clone())];
        v.extend(track_fx_commands(tracks));
        v
    });

    // Capture (don't `?`) a mid-loop write error so we can still finalize: a WAV
    // whose header is never written back (RIFF/`data` chunk sizes) is unplayable
    // even though megabytes of samples reached disk. A short, *valid* file (and a
    // surfaced error) beats a large corrupt one.
    let mut write_err: Option<EngineError> = None;
    let mut cancelled = false;
    on_progress(RenderProgress { done_frames: 0, total_frames });
    while frame_cursor < total_frames {
        // Cooperative cancellation: bail before doing this block's work. The file
        // is still finalized below (valid, partial), and the caller decides whether
        // to keep or delete it.
        if should_cancel() {
            cancelled = true;
            break;
        }
        let block_len = ((total_frames - frame_cursor) as usize).min(BLOCK_FRAMES);
        let block_end = frame_cursor + block_len as u64;

        // Schedule voices whose onset lands in this block. Past the arrangement we
        // stop scheduling new onsets (tail is pure decay), but keep rendering so
        // already-sounding voices ring out.
        let mut voice_cmds: Vec<AudioCommand> = Vec::new();
        if frame_cursor < arrangement_frames {
            let span_end = block_end.min(arrangement_frames);
            voice_cmds = schedule_block(
                tracks,
                &epoch,
                sr,
                start_frame,
                (start_frame + frame_cursor)..(start_frame + span_end),
                &mut next_id,
                &mut sustained_started,
                &mut delay_state,
            );
        }

        // The first block also carries the initial ConfigureTracks + FX inserts,
        // ahead of voices.
        let mut cmds = initial.take().into_iter().flatten().chain(voice_cmds);

        let out = &mut block[..block_len];
        renderer.process(&mut cmds, out);
        if let Some(m) = meter.as_mut() {
            // Normalizing: meter this block, then buffer it for the deferred pass.
            interleaved.clear();
            for &[l, r] in out.iter() {
                interleaved.push(l);
                interleaved.push(r);
            }
            let _ = m.add_frames_f32(&interleaved);
            norm_buffer.extend_from_slice(out);
        } else if let Err(e) = sink.write_block(out) {
            write_err = Some(e);
            break;
        }

        frame_cursor = block_end;
        on_progress(RenderProgress { done_frames: frame_cursor, total_frames });
    }

    // Deferred normalization pass: scale the buffered bounce to the target LUFS
    // (peak-limited) and write it. Skipped on cancel (the partial buffer is
    // discarded — the caller drops the file anyway) or after a prior write error.
    if let (Some(m), Some(target)) = (meter.as_ref(), cfg.normalize) {
        if !cancelled && write_err.is_none() {
            let measured = m.loudness_global().unwrap_or(f64::NEG_INFINITY);
            // Silence / un-measurable loudness → leave the level untouched.
            let mut gain: f32 = if measured.is_finite() && measured > -70.0 {
                10f64.powf((f64::from(target) - measured) / 20.0) as f32
            } else {
                1.0
            };
            // Peak-limit so the gain never pushes a sample past full scale.
            let peak = norm_buffer
                .iter()
                .fold(0.0f32, |mx, &[l, r]| mx.max(l.abs()).max(r.abs()));
            const CEILING: f32 = 0.999;
            if peak > 0.0 && peak * gain > CEILING {
                gain = CEILING / peak;
            }
            for f in norm_buffer.iter_mut() {
                f[0] *= gain;
                f[1] *= gain;
            }
            for chunk in norm_buffer.chunks(BLOCK_FRAMES) {
                if let Err(e) = sink.write_block(chunk) {
                    write_err = Some(e);
                    break;
                }
            }
        }
    }

    // Always finalize, even after a write error or a cancel, so the file is
    // valid/playable (a WAV whose header is never written back is unplayable).
    let finalized = sink.finalize();
    match write_err {
        Some(e) => Err(e),
        None => finalized.map(|()| if cancelled { RenderOutcome::Cancelled } else { RenderOutcome::Completed }),
    }
}

/// Schedule the voices whose onset lands in `abs_range` (absolute frames) into the
/// command list for one render block: re-bases each onset onto the output-local
/// timeline (subtracting `start_frame`), dedups sustained file sources across the
/// render, and emits a `SetTrackDelay` only when a track's delay bus actually
/// changes. Shared by the offline bounce and the level analyzer so both drive the
/// `Renderer` through identical scheduling.
#[allow(clippy::too_many_arguments)]
fn schedule_block(
    tracks: &Tracks<ControlMap>,
    epoch: &Epoch,
    sr: u32,
    start_frame: u64,
    abs_range: std::ops::Range<u64>,
    next_id: &mut u64,
    sustained_started: &mut HashSet<(u32, String)>,
    delay_state: &mut HashMap<u32, DelayConfig>,
) -> Vec<AudioCommand> {
    let mut voice_cmds: Vec<AudioCommand> = Vec::new();
    for ev in schedule_span(tracks, epoch, sr, abs_range, next_id) {
        // Re-base the absolute onset onto the output's local timeline (a no-op for
        // a whole-arrangement run, where `start_frame == 0`).
        let mut ev = ev;
        ev.start_frame -= start_frame;
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
        if let Some(AudioCommand::SetTrackDelay(track, cfg)) = delay_config_for(&ev, epoch, sr) {
            if delay_state.get(&track) != Some(&cfg) {
                delay_state.insert(track, cfg);
                voice_cmds.push(AudioCommand::SetTrackDelay(track, cfg));
            }
        }
        voice_cmds.push(AudioCommand::Voice(ev));
    }
    voice_cmds
}

/// A contiguous stretch where one track's post-fader level exceeds full scale —
/// an overload that clips (or forces the master limiter). Times are in cycles on
/// the arrangement timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipWindow {
    /// Mixer-strip / arrangement-lane index.
    pub track: u32,
    /// Window start (cycles, inclusive).
    pub start_cycle: f64,
    /// Window end (cycles, exclusive).
    pub end_cycle: f64,
    /// Deepest post-fader peak in the window, linear (`1.0` = 0 dBFS).
    pub peak: f32,
}

/// The result of an offline level analysis: per-track peak over the bounce plus the
/// clip windows. Peaks are **post-fader, pre-limiter** — the level that actually
/// overloads — so a track over `1.0` would clip even though the master limiter
/// would mask it on playback.
#[derive(Clone, Debug, Default)]
pub struct LevelAnalysis {
    /// Peak `max(|L|, |R|)` per track over the whole analysis, linear.
    pub track_peaks: Vec<f32>,
    /// Contiguous overload windows (adjacent clipping blocks merged).
    pub clips: Vec<ClipWindow>,
}

/// Post-fader peak at/above which a track is overloading (0 dBFS).
const CLIP_LEVEL: f32 = 1.0;

/// Render `cycles` cycles of `tracks` (at `cps`) **silently**, off the real-time
/// path, and report where it would clip.
///
/// Drives the same `Renderer` + scheduling as [`render_offline`], but instead of
/// writing samples it taps the per-track post-fader peak each block
/// ([`Renderer::track_peaks`](arbor_nemus_audio::prelude::Renderer::track_peaks))
/// and merges the over-0 dBFS stretches into per-track [`ClipWindow`]s. No file, no
/// audio output — a pure measurement, so the editor / mixer can warn about clipping
/// without the user starting playback. No tail (overloads only happen while voices
/// sound) and no normalization (we want the *raw* levels).
pub fn analyze_levels(
    tracks: &Tracks<ControlMap>,
    cps: f64,
    cycles: u32,
    sample_rate: u32,
) -> LevelAnalysis {
    let track_count = tracks.tracks.len();
    if cycles == 0 || track_count == 0 {
        return LevelAnalysis::default();
    }
    let sr = sample_rate;
    let epoch = Epoch::start(cps);
    let fpc = epoch.frames_per_cycle(sr);

    let track_configs: Vec<TrackConfig> = tracks
        .tracks
        .iter()
        .map(|t| TrackConfig { name: t.name.clone() })
        .collect();
    let mut renderer = Renderer::new(sr, &track_configs);
    renderer.registry_mut().install_builtin_synths();
    preload_file_sources(&mut renderer, tracks, 0, cycles);

    let arrangement_frames = (cycles as f64 * fpc).round() as u64;

    let mut next_id: u64 = 0;
    let mut sustained_started: HashSet<(u32, String)> = HashSet::new();
    let mut delay_state: HashMap<u32, DelayConfig> = HashMap::new();

    let mut block: Vec<Frame> = vec![[0.0, 0.0]; BLOCK_FRAMES];
    let mut frame_cursor: u64 = 0;

    let mut initial: Option<Vec<AudioCommand>> = Some({
        let mut v = vec![AudioCommand::ConfigureTracks(track_configs.clone())];
        v.extend(track_fx_commands(tracks));
        v
    });

    let mut peaks = vec![0.0f32; track_count];
    // Open clip window per track: (start cycle, running peak), merged across blocks.
    let mut open: Vec<Option<(f64, f32)>> = vec![None; track_count];
    let mut clips: Vec<ClipWindow> = Vec::new();

    while frame_cursor < arrangement_frames {
        let block_len = ((arrangement_frames - frame_cursor) as usize).min(BLOCK_FRAMES);
        let block_end = frame_cursor + block_len as u64;

        let voice_cmds = schedule_block(
            tracks,
            &epoch,
            sr,
            0,
            frame_cursor..block_end,
            &mut next_id,
            &mut sustained_started,
            &mut delay_state,
        );
        let mut cmds = initial.take().into_iter().flatten().chain(voice_cmds);
        let out = &mut block[..block_len];
        renderer.process(&mut cmds, out);

        let block_start_cycle = frame_cursor as f64 / fpc;
        for (i, peak_frame) in renderer.track_peaks().iter().enumerate() {
            if i >= track_count {
                break;
            }
            let p = peak_frame[0].max(peak_frame[1]);
            if p > peaks[i] {
                peaks[i] = p;
            }
            if p >= CLIP_LEVEL {
                match &mut open[i] {
                    Some((_, wp)) => {
                        if p > *wp {
                            *wp = p;
                        }
                    }
                    None => open[i] = Some((block_start_cycle, p)),
                }
            } else if let Some((start, wp)) = open[i].take() {
                clips.push(ClipWindow { track: i as u32, start_cycle: start, end_cycle: block_start_cycle, peak: wp });
            }
        }

        frame_cursor = block_end;
    }
    // Close any window still open at the end of the bounce.
    let end_cycle = arrangement_frames as f64 / fpc;
    for (i, slot) in open.into_iter().enumerate() {
        if let Some((start, wp)) = slot {
            clips.push(ClipWindow { track: i as u32, start_cycle: start, end_cycle, peak: wp });
        }
    }

    LevelAnalysis { track_peaks: peaks, clips }
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
fn preload_file_sources(
    renderer: &mut Renderer,
    tracks: &Tracks<ControlMap>,
    start_cycle: u32,
    cycles: u32,
) {
    if cycles == 0 {
        return;
    }
    let span = TimeSpan::new(
        Time::int(start_cycle as i64),
        Time::int((start_cycle + cycles) as i64),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_match_design() {
        let c = RenderConfig::default();
        assert_eq!(c.sample_rate, DEFAULT_SAMPLE_RATE);
        assert_eq!(c.bit_depth, DEFAULT_BIT_DEPTH);
        assert_eq!(c.tail_max_secs, DEFAULT_TAIL_MAX_SECS);
        assert_eq!(c.format, Format::Wav);
        assert_eq!(c.normalize, None);
    }

    #[test]
    fn region_render_offset_succeeds_and_writes_file() {
        use arbor_nemus_pattern::prelude::{fastcat, pure, track, tracks};
        use arbor_nemus_pattern::prelude::ControlMap;

        // A four-step melody; render only the window starting at cycle 1 (length 1).
        let melody = fastcat(vec![
            pure(ControlMap::note(60.0)),
            pure(ControlMap::note(62.0)),
            pure(ControlMap::note(64.0)),
            pure(ControlMap::note(65.0)),
        ]);
        let t = tracks(vec![track("lead", melody)]);
        let out = std::env::temp_dir().join("nemus_render_region.wav");
        let res = render_offline_with_progress(
            &t, 1.0, 1, 1, &RenderConfig::default(), &out, |_| {}, || false,
        );
        let cleanup = std::fs::remove_file(&out);
        assert_eq!(res.expect("region render"), RenderOutcome::Completed);
        assert!(cleanup.is_ok(), "region render should have produced the WAV file");
    }

    #[test]
    fn analyze_levels_flags_a_hot_track() {
        use arbor_nemus_pattern::prelude::{pure, track, tracks, ControlMap};
        // A note boosted far past unity must overload (peak > 0 dBFS) and report a
        // clip window for its track.
        let loud = pure(ControlMap::note(60.0)).gain(50.0);
        let t = tracks(vec![track("loud", loud)]);
        let a = analyze_levels(&t, 1.0, 1, DEFAULT_SAMPLE_RATE);
        assert_eq!(a.track_peaks.len(), 1);
        assert!(a.track_peaks[0] > 1.0, "hot track should peak over full scale, got {}", a.track_peaks[0]);
        assert!(a.clips.iter().any(|c| c.track == 0), "expected a clip window for the loud track");
    }

    #[test]
    fn analyze_levels_quiet_track_has_no_clips() {
        use arbor_nemus_pattern::prelude::{pure, track, tracks, ControlMap};
        let quiet = pure(ControlMap::note(60.0)).gain(0.1);
        let t = tracks(vec![track("quiet", quiet)]);
        let a = analyze_levels(&t, 1.0, 1, DEFAULT_SAMPLE_RATE);
        assert!(a.clips.is_empty(), "a quiet track should not clip");
    }

    #[test]
    fn analyze_levels_zero_cycles_is_inert() {
        use arbor_nemus_pattern::prelude::{pure, track, tracks, ControlMap};
        let t = tracks(vec![track("t", pure(ControlMap::note(60.0)))]);
        let a = analyze_levels(&t, 1.0, 0, DEFAULT_SAMPLE_RATE);
        assert!(a.track_peaks.is_empty() && a.clips.is_empty());
    }

    #[test]
    fn render_with_missing_file_source_falls_back_and_succeeds() {
        use arbor_nemus_pattern::prelude::{sample, track, tracks};

        // A `sample(...)` pointing at a non-existent file: preload fails, and the
        // best-effort scan must not abort the bounce — the voice falls back to the
        // synth and the render still writes a valid WAV.
        let t = tracks(vec![track("chop", sample("does-not-exist.wav"))]);
        let out = std::env::temp_dir().join("nemus_render_missing_source.wav");
        let res = render_offline(&t, 1.0, 1, &RenderConfig::default(), &out);
        let cleanup = std::fs::remove_file(&out);
        assert!(res.is_ok(), "missing file source must not abort the render");
        assert!(cleanup.is_ok(), "render should have produced the WAV file");
    }
}
