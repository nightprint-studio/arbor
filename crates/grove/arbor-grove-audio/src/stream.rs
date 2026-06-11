//! The real-time output: a cpal stream whose callback owns a [`Renderer`] and
//! drains a lock-free ring buffer of [`AudioCommand`]s produced by the engine.
//!
//! [`StreamSink`] is the engine-facing half (the ring **producer** + a shared
//! playhead atomic); it is the production [`AudioSink`]. The cpal stream and the
//! consuming callback are stood up by [`open_output_stream`] (Stage A).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::AudioError;
use crate::renderer::Renderer;
use crate::seam::{AudioCommand, AudioSink, Frame, TrackConfig};

/// Target output sample rate (design: 48 kHz); falls back to the device default
/// if 48 kHz isn't offered.
const TARGET_SAMPLE_RATE: u32 = 48_000;

/// Target device buffer size in frames (design: ~512). Advisory — the host may
/// pick its own; the renderer copes with any block length.
const TARGET_BUFFER_FRAMES: u32 = 512;

/// Command ring capacity. Generous: one block of look-ahead is a few hundred
/// events at most, and a full ring just makes `send` return the command back.
const RING_CAPACITY: usize = 4096;

/// Engine-facing handle to the live audio backend: pushes commands into the
/// ring and reads the callback's sample clock. One producer (the scheduler
/// thread), one consumer (the cpal callback) — SPSC, lock-free.
pub struct StreamSink {
    tx: rtrb::Producer<AudioCommand>,
    playhead: Arc<AtomicU64>,
    sample_rate: u32,
}

impl StreamSink {
    /// Construct from the ring producer + the shared playhead the callback
    /// advances. Used by [`open_output_stream`]; exposed so an alternate backend
    /// can reuse the same engine-facing type.
    pub fn new(tx: rtrb::Producer<AudioCommand>, playhead: Arc<AtomicU64>, sample_rate: u32) -> Self {
        StreamSink {
            tx,
            playhead,
            sample_rate,
        }
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
/// builds the `rtrb` command ring, constructs a [`Renderer`], and starts a cpal
/// output stream whose callback drains the ring into the renderer, calls
/// [`Renderer::process`], writes the device buffer, and advances the shared
/// playhead. The returned [`OutputStream`] keeps the cpal `Stream` alive; drop it
/// to stop audio.
pub fn open_output_stream(
    tracks: Vec<TrackConfig>,
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

    // The lock-free command ring (engine → callback) and the shared playhead.
    let (tx, rx) = rtrb::RingBuffer::<AudioCommand>::new(RING_CAPACITY);
    let playhead = Arc::new(AtomicU64::new(0));

    let renderer = Renderer::new(sample_rate, &tracks);
    let stream = build_stream(
        &device,
        &stream_config,
        sample_format,
        channels,
        rx,
        renderer,
        Arc::clone(&playhead),
    )?;

    stream
        .play()
        .map_err(|e| AudioError::Device(format!("failed to start stream: {e}")))?;

    let sink = StreamSink::new(tx, playhead, sample_rate);
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

    // Prefer a range that covers 48 kHz with 2 channels.
    let target = cpal::SampleRate(TARGET_SAMPLE_RATE);
    let pick = supported
        .iter()
        .find(|c| {
            c.channels() == 2
                && c.min_sample_rate() <= target
                && c.max_sample_rate() >= target
        })
        .or_else(|| supported.iter().find(|c| c.channels() == 2))
        .or_else(|| supported.first());

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
    rx: rtrb::Consumer<AudioCommand>,
    renderer: Renderer,
    playhead: Arc<AtomicU64>,
) -> Result<cpal::Stream, AudioError> {
    fn err_fn(e: cpal::StreamError) {
        eprintln!("grove audio stream error: {e}");
    }

    macro_rules! build {
        ($sample:ty) => {{
            let mut state = CallbackState::new(renderer, rx, playhead, channels);
            device.build_output_stream(
                config,
                move |data: &mut [$sample], _| state.fill::<$sample>(data),
                err_fn,
                None,
            )
        }};
    }

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::U16 => build!(u16),
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
    channels: usize,
    /// Pre-sized stereo scratch; grown only on the (cold) path where the host
    /// hands a bigger buffer than we provisioned.
    scratch: Vec<Frame>,
}

impl CallbackState {
    fn new(
        renderer: Renderer,
        rx: rtrb::Consumer<AudioCommand>,
        playhead: Arc<AtomicU64>,
        channels: usize,
    ) -> Self {
        CallbackState {
            renderer,
            rx,
            playhead,
            channels,
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

        // Drain due commands. The ring is SPSC; `pop` is lock-free.
        let mut drained = RingDrain { rx: &mut self.rx };
        self.renderer.process(&mut drained, out);

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
