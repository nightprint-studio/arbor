//! The **frozen contract** between `arbor-grove-engine` (timing) and
//! `arbor-grove-audio` (sound). This is the seam the two crates are developed
//! against in parallel; changing it is a coordinated change to both.
//!
//! Flow:
//! - the engine queries `Pattern<ControlMap>` over a look-ahead window, turns
//!   each onset into a [`VoiceEvent`] (cycle-time → absolute sample frame), and
//!   pushes it through an [`AudioSink`] as [`AudioCommand::Voice`];
//! - the audio backend (real-time cpal callback, or the offline render driver)
//!   pulls due commands and feeds them to the [`Renderer`](crate::renderer::Renderer);
//! - the audio backend owns the **sample clock**; the engine reads "now" back
//!   through [`AudioSink::now_frame`].
//!
//! Resolution of *what* a [`VoiceSource::Named`] actually sounds like (synth
//! preset vs. SFZ region vs. fallback) lives entirely in the audio registry —
//! the engine only forwards symbolic names.

use arbor_grove_pattern::prelude::SourceKind;

/// A stereo output sample (left, right). The unit the [`Renderer`](crate::renderer::Renderer)
/// renders into and the offline driver writes out.
pub type Frame = [f32; 2];

/// Engine-assigned, monotonically increasing voice identifier. Lets a later
/// command target a specific sounding voice (e.g. live note-off); for Fase 2
/// voices self-release via [`VoiceEvent::dur_frames`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(pub u64);

/// One sample-accurate trigger handed engine → audio for a single hap onset.
///
/// Times are in **absolute output frames** against the audio backend's sample
/// clock — never cycle-time. The engine has already done the cycle→frame
/// mapping (`design/grove/architecture.md`, clock & tempo).
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceEvent {
    /// Identity, for any later command that needs to address this voice.
    pub id: VoiceId,
    /// When the voice starts, in absolute frames.
    pub start_frame: u64,
    /// Voice lifetime in frames (from the hap's `whole` × frames-per-cycle).
    /// `None` lets the source ring to its natural end (one-shots / decays).
    pub dur_frames: Option<u64>,
    /// What produces the sound.
    pub source: VoiceSource,
    /// Pitch as a MIDI-style semitone (`C4 = 60`). `None` plays at native pitch
    /// (an unpitched drum hit / one-shot).
    pub note: Option<f32>,
    /// Per-voice DSP / mix parameters, already resolved from the `ControlMap`.
    pub params: VoiceParams,
    /// Destination mixer strip (index into the [`TrackConfig`] list).
    pub track: u32,
    /// Source byte range `(start, end)` carried through for the live
    /// active-hap highlight feedback path; `None` for generated patterns.
    pub span: Option<(u32, u32)>,
}

/// What a voice plays.
#[derive(Clone, Debug, PartialEq)]
pub enum VoiceSource {
    /// A symbolic sound / instrument resolved by the audio **registry**: a drum
    /// or sound-bank leaf (`s("bd")`), and/or a melodic instrument (`.inst(...)`)
    /// with an optional articulation. Resolution order and synth↔sampler
    /// fallback are the audio crate's concern.
    Named {
        /// Sound-bank leaf name (`"bd"`), if any.
        sound: Option<String>,
        /// Sample variant index (`:n`), only with `sound`.
        variant: Option<u32>,
        /// Instrument / voice name (`"strings.violin"`, `"synth.pad"`), if any.
        inst: Option<String>,
        /// Articulation (`"legato"`/`"staccato"`/…), resolved by the instrument.
        art: Option<String>,
    },
    /// A user file (`sample`/`audio`); `kind` decides one-shot vs. sustained.
    File {
        /// Path as written in the source (resolved against the project by the shell).
        path: String,
        /// One-shot vs. sustained playback.
        kind: SourceKind,
    },
}

/// Per-voice DSP / mix parameters. Numeric, already sampled at the hap onset by
/// the engine (patternised controls collapse to a value here). Defaults match
/// the design's "unset → engine default".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceParams {
    /// Output amplitude (multiplicative). Default `1.0`.
    pub gain: f32,
    /// Stereo position `0` left … `1` right. Default `0.5`.
    pub pan: f32,
    /// Reverb-bus send amount `0..1`. Default `0.0`.
    pub room: f32,
    /// Low-pass cutoff in Hz; `None` = open.
    pub lpf: Option<f32>,
    /// High-pass cutoff in Hz; `None` = open.
    pub hpf: Option<f32>,
    /// Pitch shift in semitones (resampling). Default `0.0`.
    pub shift: f32,
    /// Playback speed factor (resampling; couples pitch + duration). Default `1.0`.
    pub speed: f32,
    /// Bitcrush resolution in bits; `None` = off.
    pub crush: Option<f32>,
    /// Waveshaper distortion `0..1`. Default `0.0`.
    pub shape: f32,
    /// Velocity `0..1`: selects the sampled velocity-layer + dynamics. Default `0.8`.
    pub vel: f32,
}

impl Default for VoiceParams {
    fn default() -> Self {
        VoiceParams {
            gain: 1.0,
            pan: 0.5,
            room: 0.0,
            lpf: None,
            hpf: None,
            shift: 0.0,
            speed: 1.0,
            crush: None,
            shape: 0.0,
            vel: 0.8,
        }
    }
}

/// Static description of one mixer strip, sent once (and on track-set swaps) so
/// the audio mixer can lay out its strips before voices arrive.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackConfig {
    /// Display / addressing name (the `track("name", …)` label).
    pub name: String,
}

/// Everything the engine can push to the audio backend, in time order. Voice
/// triggers plus the small set of transport / mixer controls. Anything that
/// would allocate or block is forbidden on the receiving (real-time) side — the
/// payloads here are all plain data.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioCommand {
    /// Start a voice.
    Voice(VoiceEvent),
    /// (Re)configure the mixer strips. Sent at startup and on a track-set swap.
    ConfigureTracks(Vec<TrackConfig>),
    /// Set a strip's gain (linear).
    SetTrackGain(u32, f32),
    /// Mute / unmute a strip.
    SetTrackMute(u32, bool),
    /// Release every sounding voice (transport stop / panic).
    StopAll,
}

/// The engine's view of the audio backend: a place to push timed commands and a
/// read of the backend's sample clock. The seam that makes the engine testable
/// headless — the real impl is a ring-buffer producer over a live cpal stream
/// (`crate::stream`), the test/offline impl is a recorder
/// (`crate::testing::RecordingSink`).
pub trait AudioSink {
    /// Enqueue a command. **Non-blocking.** On a full queue the command is
    /// handed back so the caller can decide to drop or retry — never blocks the
    /// scheduler.
    fn send(&mut self, cmd: AudioCommand) -> std::result::Result<(), AudioCommand>;

    /// The backend's current output frame — its sample-clock "now". Lock-free.
    fn now_frame(&self) -> u64;

    /// Output sample rate in frames per second.
    fn sample_rate(&self) -> u32;
}
