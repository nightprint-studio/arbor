//! The real-time output: a cpal stream whose callback owns a [`Renderer`] and
//! drains a lock-free ring buffer of [`AudioCommand`]s produced by the engine.
//!
//! [`StreamSink`] is the engine-facing half (the ring **producer** + a shared
//! playhead atomic); it is the production [`AudioSink`]. The cpal stream and the
//! consuming callback are stood up by [`open_output_stream`] (Stage A).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::AudioError;
use crate::meters::{MeterSnapshot, MeterTap};
use crate::registry::Registry;
use crate::renderer::Renderer;
use crate::seam::{AudioCommand, AudioSink, Frame, TrackConfig};

/// Output-meter ballistics: each device-buffer callback decays the held peak by
/// this factor before taking the new block max, so the meter falls smoothly
/// (~150 ms to silence at a 512-frame / 48 kHz buffer) instead of latching.
const METER_DECAY: f32 = 0.85;

/// Target output sample rate (design: 48 kHz); falls back to the device default
/// if 48 kHz isn't offered. The canonical value lives in [`crate::defaults`].
const TARGET_SAMPLE_RATE: u32 = crate::defaults::DEFAULT_SAMPLE_RATE;

/// Target device buffer size in frames (design: ~512). Advisory — the host may
/// pick its own; the renderer copes with any block length. Canonical value in
/// [`crate::defaults`].
const TARGET_BUFFER_FRAMES: u32 = crate::defaults::DEFAULT_BLOCK_FRAMES as u32;

/// Command ring capacity. Generous: one block of look-ahead is a few hundred
/// events at most, and a full ring just makes `send` return the command back.
const RING_CAPACITY: usize = 4096;

/// Engine-facing handle to the live audio backend: pushes commands into the
/// ring and reads the callback's sample clock. One producer (the scheduler
/// thread), one consumer (the cpal callback) — SPSC, lock-free.
pub struct StreamSink {
    tx: rtrb::Producer<AudioCommand>,
    playhead: Arc<AtomicU64>,
    /// Shared audio telemetry (master + per-track peak, voice count, DSP load),
    /// written by the callback and read non-RT by the shell (out-of-band, like
    /// `playhead` — not part of the engine↔audio command flow).
    tap: Arc<MeterTap>,
    sample_rate: u32,
}

impl StreamSink {
    /// Construct from the ring producer, the shared playhead the callback
    /// advances, and the shared telemetry [`MeterTap`] it writes. Used by
    /// [`open_output_stream`]; exposed so an alternate backend can reuse the same
    /// engine-facing type.
    pub fn new(
        tx: rtrb::Producer<AudioCommand>,
        playhead: Arc<AtomicU64>,
        tap: Arc<MeterTap>,
        sample_rate: u32,
    ) -> Self {
        StreamSink {
            tx,
            playhead,
            tap,
            sample_rate,
        }
    }

    /// The most recent master output peak `[left, right]` (`0.0..~1.0`,
    /// post-limiter). Decays smoothly between buffers ([`METER_DECAY`]).
    /// Lock-free, non-RT read.
    pub fn peak(&self) -> [f32; 2] {
        self.tap.load_master()
    }

    /// Snapshot the full audio telemetry (master + per-track peak, voice count,
    /// DSP load) for one front-end frame. Lock-free, non-RT read; the per-track
    /// `Vec` allocates here, never in the callback.
    pub fn meters(&self) -> MeterSnapshot {
        self.tap.snapshot()
    }
}

impl AudioSink for StreamSink {
    fn send(&mut self, cmd: AudioCommand) -> Result<(), AudioCommand> {
        match self.tx.push(cmd) {
            Ok(()) => Ok(()),
            Err(rtrb::PushError::Full(cmd)) => Err(cmd),
        }
    }

    fn now_frame(&self) -> u64 {
        self.playhead.load(Ordering::Acquire)
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

impl fmt::Debug for StreamSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamSink")
            .field("sample_rate", &self.sample_rate)
            .field("now_frame", &self.playhead.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// Keeps the live output stream alive. Dropping it tears the stream down and
/// stops audio. Opaque on purpose — the cpal stream + the consuming callback
/// (which owns the [`Renderer`]) live behind it.
pub struct OutputStream {
    // The audio agent stores the cpal `Stream` (and anything it must keep alive)
    // here in Stage A. Boxed-opaque so the frozen signature carries no cpal type.
    //
    // NOT `+ Send`: a `cpal::Stream` is `!Send` (WASAPI on Windows holds a
    // `PhantomData<*mut ()>`), so the stream — and thus `OutputStream` — stays on
    // the thread that opened it. That's correct: the audio-owning thread holds it
    // for the session. The engine never touches `OutputStream`, only `StreamSink`
    // (which IS the engine-facing seam and is `Send`), so this carries no
    // cross-thread requirement.
    _keep_alive: Box<dyn std::any::Any>,
}

impl fmt::Debug for OutputStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutputStream").finish_non_exhaustive()
    }
}

/// Open the default output device and start streaming.
///
/// Picks the default output device and a stereo config at (or nearest to) 48 kHz,
/// builds the `rtrb` command ring, constructs a [`Renderer`] backed by the given
/// sound [`Registry`], and starts a cpal output stream whose callback drains the
/// ring into the renderer, calls [`Renderer::process`], writes the device buffer,
/// advances the shared playhead, and updates the output meter. The returned
/// [`OutputStream`] keeps the cpal `Stream` alive; drop it to stop audio.
///
/// `registry` resolves symbolic sound names (`bd`, `strings.violin`) to concrete
/// voices; pass [`Registry::new`] for the default synth bank, or a loaded VSCO
/// manifest. It must be set here because the [`Renderer`] then lives inside the
/// real-time callback, unreachable for a later swap.
pub fn open_output_stream(
    tracks: Vec<TrackConfig>,
    registry: Registry,
) -> Result<(StreamSink, OutputStream), AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| AudioError::Device("no default output device".to_string()))?;

    let config = choose_output_config(&device)?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let sample_format = config.sample_format();

    // A stream config with our advisory buffer size.
    let mut stream_config: cpal::StreamConfig = config.into();
    stream_config.buffer_size = cpal::BufferSize::Fixed(TARGET_BUFFER_FRAMES);

    // The lock-free command ring (engine → callback), the shared playhead, and
    // the shared telemetry tap (master/per-track peak, voices, DSP load).
    let (tx, rx) = rtrb::RingBuffer::<AudioCommand>::new(RING_CAPACITY);
    let playhead = Arc::new(AtomicU64::new(0));
    let tap = MeterTap::new();

    let mut renderer = Renderer::new(sample_rate, &tracks);
    renderer.set_registry(registry);
    let stream = build_stream(
        &device,
        &stream_config,
        sample_format,
        channels,
        sample_rate,
        rx,
        renderer,
        Arc::clone(&playhead),
        Arc::clone(&tap),
    )?;

    stream
        .play()
        .map_err(|e| AudioError::Device(format!("failed to start stream: {e}")))?;

    let sink = StreamSink::new(tx, playhead, tap, sample_rate);
    let output = OutputStream {
        _keep_alive: Box::new(stream),
    };
    Ok((sink, output))
}

/// Pick a supported output config: prefer stereo @ 48 kHz, else the device
/// default. cpal's supported-config ranges are queried, not assumed.
fn choose_output_config(
    device: &cpal::Device,
) -> Result<cpal::SupportedStreamConfig, AudioError> {
    let supported: Vec<_> = device
        .supported_output_configs()
        .map_err(|e| AudioError::Device(format!("query configs: {e}")))?
        .collect();

    // Prefer a range that covers 48 kHz with 2 channels, and — among equally
    // valid ranges — the highest-fidelity sample format the callback can write
    // (so a device that *also* exposes U8/U16 isn't picked at 8-bit). Falls back
    // to any stereo range, then any range at all.
    let target = cpal::SampleRate(TARGET_SAMPLE_RATE);
    let covers_target = |c: &&cpal::SupportedStreamConfigRange| {
        c.channels() == 2 && c.min_sample_rate() <= target && c.max_sample_rate() >= target
    };
    let pick = supported
        .iter()
        .filter(covers_target)
        .min_by_key(|c| format_rank(c.sample_format()))
        .or_else(|| {
            supported
                .iter()
                .filter(|c| c.channels() == 2)
                .min_by_key(|c| format_rank(c.sample_format()))
        })
        .or_else(|| supported.iter().min_by_key(|c| format_rank(c.sample_format())));

    match pick {
        Some(range) => {
            let clamped = clamp_rate(range, target);
            Ok(range.clone().with_sample_rate(clamped))
        }
        None => device
            .default_output_config()
            .map_err(|e| AudioError::Device(format!("default config: {e}"))),
    }
}

/// Output-format preference (lower = better): float first, then the wider
/// integer formats, with 8-bit last. Drives `choose_output_config` so we open
/// the best format the device offers and the callback can write. The catch-all
/// keeps any future (cpal is `#[non_exhaustive]`) format selectable but least
/// preferred.
fn format_rank(f: cpal::SampleFormat) -> u8 {
    match f {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::F64 => 1,
        cpal::SampleFormat::I16 => 2,
        cpal::SampleFormat::I32 => 3,
        cpal::SampleFormat::I64 => 4,
        cpal::SampleFormat::U16 => 5,
        cpal::SampleFormat::U32 => 6,
        cpal::SampleFormat::U64 => 7,
        cpal::SampleFormat::I8 => 8,
        cpal::SampleFormat::U8 => 9,
        _ => 10,
    }
}

/// Clamp a target rate into a supported range.
fn clamp_rate(
    range: &cpal::SupportedStreamConfigRange,
    target: cpal::SampleRate,
) -> cpal::SampleRate {
    cpal::SampleRate(target.0.clamp(range.min_sample_rate().0, range.max_sample_rate().0))
}

/// Build the cpal output stream for the chosen sample format. The callback is the
/// only real-time code path: drain ring → `Renderer::process` → write device
/// buffer → advance playhead. It never allocates, locks, or does IO.
fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    channels: usize,
    sample_rate: u32,
    rx: rtrb::Consumer<AudioCommand>,
    renderer: Renderer,
    playhead: Arc<AtomicU64>,
    tap: Arc<MeterTap>,
) -> Result<cpal::Stream, AudioError> {
    fn err_fn(e: cpal::StreamError) {
        eprintln!("grove audio stream error: {e}");
    }

    macro_rules! build {
        ($sample:ty) => {{
            let mut state =
                CallbackState::new(renderer, rx, playhead, Arc::clone(&tap), channels, sample_rate);
            device.build_output_stream(
                config,
                move |data: &mut [$sample], _| state.fill::<$sample>(data),
                err_fn,
                None,
            )
        }};
    }

    // Every primitive cpal sample type implements `SizedSample + FromSample<f32>`,
    // so the callback (which renders f32 and converts per channel) can target any
    // of them — cover them all so no real device's default format is rejected.
    let stream = match sample_format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::F64 => build!(f64),
        cpal::SampleFormat::I8 => build!(i8),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::I64 => build!(i64),
        cpal::SampleFormat::U8 => build!(u8),
        cpal::SampleFormat::U16 => build!(u16),
        cpal::SampleFormat::U32 => build!(u32),
        cpal::SampleFormat::U64 => build!(u64),
        other => {
            return Err(AudioError::Device(format!(
                "unsupported sample format: {other:?}"
            )))
        }
    };
    stream.map_err(|e| AudioError::Device(format!("build stream: {e}")))
}

/// Real-time callback state: the renderer, the ring consumer, the playhead, and
/// a pre-sized scratch buffer of stereo [`Frame`]s so `process` never allocates.
struct CallbackState {
    renderer: Renderer,
    rx: rtrb::Consumer<AudioCommand>,
    playhead: Arc<AtomicU64>,
    /// Shared telemetry (master/per-track peak, voices, DSP load), updated each
    /// buffer for the meters.
    tap: Arc<MeterTap>,
    channels: usize,
    /// Output rate, for the DSP-load budget (block wall-time = frames / rate).
    sample_rate: u32,
    /// Pre-sized stereo scratch; grown only on the (cold) path where the host
    /// hands a bigger buffer than we provisioned.
    scratch: Vec<Frame>,
}

impl CallbackState {
    fn new(
        renderer: Renderer,
        rx: rtrb::Consumer<AudioCommand>,
        playhead: Arc<AtomicU64>,
        tap: Arc<MeterTap>,
        channels: usize,
        sample_rate: u32,
    ) -> Self {
        CallbackState {
            renderer,
            rx,
            playhead,
            tap,
            channels,
            sample_rate,
            scratch: vec![[0.0; 2]; TARGET_BUFFER_FRAMES as usize],
        }
    }

    /// Fill one device buffer. `T` is the device sample type; we render f32
    /// stereo frames and convert per channel on write-out.
    fn fill<T: cpal::SizedSample + cpal::FromSample<f32>>(&mut self, data: &mut [T]) {
        let frames = data.len() / self.channels.max(1);
        if self.scratch.len() < frames {
            // Cold path: host asked for more than provisioned. This *can* alloc,
            // but only once until the buffer size settles — acceptable vs. a
            // hard cap that would underrun.
            self.scratch.resize(frames, [0.0; 2]);
        }
        let out = &mut self.scratch[..frames];

        // Drain due commands + render, timing the call for the DSP-load meter.
        // `Instant::now` is a monotonic clock read — not an alloc/lock/IO, so it's
        // RT-safe; this is the standard way to meter callback utilisation.
        let mut drained = RingDrain { rx: &mut self.rx };
        let t0 = Instant::now();
        self.renderer.process(&mut drained, out);
        let elapsed = t0.elapsed();

        // Telemetry tap (out-of-band; never feeds back into rendering). Master +
        // per-track peaks are this block's max |sample|, floored by the decayed
        // previous value so the meters fall smoothly; voices/DSP-load reflect the
        // block just rendered.
        let mut master_peak = [0.0f32; 2];
        for frame in out.iter() {
            master_peak[0] = master_peak[0].max(frame[0].abs());
            master_peak[1] = master_peak[1].max(frame[1].abs());
        }
        let prev_master = self.tap.load_master();
        self.tap.store_master([
            master_peak[0].max(prev_master[0] * METER_DECAY),
            master_peak[1].max(prev_master[1] * METER_DECAY),
        ]);

        let track_peaks = self.renderer.track_peaks();
        for (i, block) in track_peaks.iter().enumerate() {
            let prev = self.tap.load_track(i);
            self.tap.store_track(
                i,
                [
                    block[0].max(prev[0] * METER_DECAY),
                    block[1].max(prev[1] * METER_DECAY),
                ],
            );
        }
        self.tap.store_track_count(track_peaks.len());
        self.tap.store_voices(self.renderer.active_voices() as u32);

        // DSP load = compute time / buffer wall-time, EMA-smoothed (a utilisation,
        // not a peak, so a gentle filter rather than peak-hold-with-decay).
        let budget = frames.max(1) as f32 / self.sample_rate.max(1) as f32;
        let instant_load = if budget > 0.0 { elapsed.as_secs_f32() / budget } else { 0.0 };
        let prev_load = self.tap.load_dsp_load();
        self.tap.store_dsp_load(prev_load * 0.9 + instant_load * 0.1);

        // Interleave the stereo frames into the device buffer.
        for (i, frame) in out.iter().enumerate() {
            let base = i * self.channels;
            for ch in 0..self.channels {
                // Mono-downmix beyond stereo by duplicating L/R alternately; for
                // the common stereo case this is just L,R.
                let v = frame[ch % 2];
                data[base + ch] = T::from_sample(v);
            }
        }

        // Advance the shared playhead by the frames we produced.
        self.playhead
            .fetch_add(frames as u64, Ordering::Release);
    }
}

/// An iterator that yields commands off the ring until it's momentarily empty.
struct RingDrain<'a> {
    rx: &'a mut rtrb::Consumer<AudioCommand>,
}

impl Iterator for RingDrain<'_> {
    type Item = AudioCommand;
    fn next(&mut self) -> Option<AudioCommand> {
        self.rx.pop().ok()
    }
}
