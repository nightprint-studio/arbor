//! The **frozen contract** between `merula-engine` (timing) and
//! `merula-audio` (sound). This is the seam the two crates are developed
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

use merula_pattern::prelude::SourceKind;

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
/// mapping (`design/merula/architecture.md`, clock & tempo).
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceEvent {
    /// Identity, for any later command that needs to address this voice.
    pub id: VoiceId,
    /// When the voice starts, in absolute frames.
    pub start_frame: u64,
    /// Voice lifetime in frames (from the hap's `whole` × frames-per-cycle).
    /// `None` lets the source ring to its natural end (one-shots / decays / a
    /// `.hold()` drone).
    pub dur_frames: Option<u64>,
    /// Monophonic **connected** voicing: re-pitch the track's current voice rather
    /// than stack a fresh one (no envelope re-attack). Set by `art("legato")` and
    /// by any `.hold(...)` (drone / pad). The held lifetime itself rides on
    /// `dur_frames` (`None` = ring until the next note). Additive seam extension.
    pub legato: bool,
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
    /// Delay line time in **fractions of a cycle** (e.g. `0.25` = a quarter-cycle);
    /// `None` leaves the destination track's delay bus at its current setting.
    /// Configures the per-track delay bus (additive seam extension, Onda 2).
    pub delay: Option<f32>,
    /// Delay feedback `0..1` — how much of the echo feeds back into the line.
    /// `None` leaves the bus's feedback unchanged.
    pub feedback: Option<f32>,
    /// Per-event delay **send** `0..1` — how much of this voice feeds the track's
    /// delay bus. `0`/`None` → no send (the echo bus rings independently of the
    /// voice's lifetime, distinct from `off`).
    pub delay_mix: Option<f32>,
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
            delay: None,
            feedback: None,
            delay_mix: None,
        }
    }
}

/// One parametric-EQ band: a single biquad section. Strip processors are driven
/// by mixer [`AudioCommand`]s, never by the language — these are not `ControlMap`
/// fields. Additive seam extension (Onda 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqBand {
    /// Filter shape this band realises.
    pub kind: EqBandKind,
    /// Centre / corner frequency in Hz.
    pub freq: f32,
    /// Gain in dB (peak / shelf bands only; ignored for hpf/lpf).
    pub gain_db: f32,
    /// Quality factor (bandwidth for peak, slope for shelf, resonance for hpf/lpf).
    pub q: f32,
}

impl Default for EqBand {
    fn default() -> Self {
        EqBand {
            kind: EqBandKind::Peak,
            freq: 1_000.0,
            gain_db: 0.0,
            q: 0.707,
        }
    }
}

/// The shape of a parametric-EQ [`EqBand`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqBandKind {
    /// Bell / peaking boost-cut around `freq`.
    Peak,
    /// Low shelf below `freq`.
    LowShelf,
    /// High shelf above `freq`.
    HighShelf,
    /// High-pass (rumble removal).
    Hpf,
    /// Low-pass (top-end taming).
    Lpf,
}

/// Standard feed-forward compressor settings for a strip / master processor.
/// Additive seam extension (Onda 2); driven by mixer [`AudioCommand`]s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompSettings {
    /// Threshold in dBFS below which no reduction is applied.
    pub threshold_db: f32,
    /// Compression ratio (e.g. `4.0` = 4:1). `1.0` is a no-op.
    pub ratio: f32,
    /// Attack time in seconds.
    pub attack: f32,
    /// Release time in seconds.
    pub release: f32,
    /// Make-up gain in dB applied after compression.
    pub makeup_db: f32,
    /// Soft-knee width in dB (0 = hard knee).
    pub knee_db: f32,
}

impl Default for CompSettings {
    fn default() -> Self {
        CompSettings {
            threshold_db: -18.0,
            ratio: 4.0,
            attack: 0.005,
            release: 0.10,
            makeup_db: 0.0,
            knee_db: 6.0,
        }
    }
}

/// Configuration of a per-track delay bus. The line time is given in **frames**
/// (the engine converts `VoiceParams::delay` cycle-fractions → frames via the
/// epoch). Additive seam extension (Onda 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelayConfig {
    /// Delay time in output frames.
    pub time_frames: u32,
    /// Feedback `0..1`.
    pub feedback: f32,
}

impl Default for DelayConfig {
    fn default() -> Self {
        DelayConfig {
            time_frames: 0,
            feedback: 0.0,
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
    /// Trigger a **preview / audition** voice: a one-off note for the instrument
    /// browser, rendered through a dedicated bus that **bypasses the song mixer
    /// strips** (no per-strip gain / mute / solo / insert) and folds straight into
    /// the master — so a preview sounds identical whether or not a song is loaded,
    /// playing, or muted. Self-releases via [`VoiceEvent::dur_frames`]; the event's
    /// `track` is ignored (the audition bus is separate). The engine never emits
    /// this — only the shell's instrument-preview path does. Additive seam extension.
    Audition(VoiceEvent),
    /// (Re)configure the mixer strips. Sent at startup and on a track-set swap.
    ConfigureTracks(Vec<TrackConfig>),
    /// Set a strip's gain (linear).
    SetTrackGain(u32, f32),
    /// Set a strip's stereo pan `0` left … `1` right. (Additive, Onda 2.)
    SetTrackPan(u32, f32),
    /// Mute / unmute a strip.
    SetTrackMute(u32, bool),
    /// Solo / un-solo a strip. Any soloed strip mutes every non-soloed one.
    /// (Additive, Onda 2.)
    SetTrackSolo(u32, bool),
    /// Set the master-strip gain (linear), applied after the strip sum.
    /// (Additive, Onda 2.)
    SetMasterGain(f32),
    /// Replace a strip's parametric-EQ band list (empty = bypass). (Additive, Onda 2.)
    SetTrackEq(u32, Vec<EqBand>),
    /// Replace the master parametric-EQ band list. (Additive, Onda 2.)
    SetMasterEq(Vec<EqBand>),
    /// Set / clear a strip's compressor (`None` = bypass). (Additive, Onda 2.)
    SetTrackComp(u32, Option<CompSettings>),
    /// Set / clear the master compressor (`None` = bypass). (Additive, Onda 2.)
    SetMasterComp(Option<CompSettings>),
    /// Configure a strip's delay bus (time + feedback). The per-event send amount
    /// rides on [`VoiceParams::delay_mix`]. (Additive, Onda 2.)
    SetTrackDelay(u32, DelayConfig),
    /// Install / replace the convolution-reverb impulse response. An empty buffer
    /// regenerates the default procedural IR. (Additive, Onda 2.)
    SetReverbIr(ReverbIr),
    /// Release every sounding voice (transport stop / panic).
    StopAll,
    /// Clear the **audition / preview bus only** (stop an in-flight snippet preview
    /// early), leaving the song's voices sounding. Unlike [`AudioCommand::StopAll`]
    /// this never touches the main voice pool. Additive seam extension.
    StopAudition,
}

/// An impulse response for the convolution reverb send bus, carried on
/// [`AudioCommand::SetReverbIr`]. Either an explicit stereo buffer or a request
/// to (re)synthesise the default procedural IR. Additive seam extension (Onda 2).
#[derive(Clone, Debug, PartialEq)]
pub enum ReverbIr {
    /// Synthesise a procedural IR of `seconds` decay at the renderer's rate.
    Procedural { seconds: f32 },
    /// An explicit stereo IR: interleaved-free, one `Frame` (L/R) per tap.
    Buffer(Vec<Frame>),
}

/// The engine's view of the audio backend: a place to push timed commands and a
/// read of the backend's sample clock. The seam that makes the engine testable
/// headless — the real impl is a ring-buffer producer over a live cpal stream
/// (`crate::stream`), the test/offline impl is a recorder
/// (`crate::testing::RecordingSink`).
pub trait AudioSink {
    /// Enqueue a command. **Non-blocking.** On a full queue the command is
    /// handed back (boxed, since it is a large enum) so the caller can decide to
    /// drop or retry — never blocks the scheduler.
    fn send(&mut self, cmd: AudioCommand) -> std::result::Result<(), Box<AudioCommand>>;

    /// The backend's current output frame — its sample-clock "now". Lock-free.
    fn now_frame(&self) -> u64;

    /// Output sample rate in frames per second.
    fn sample_rate(&self) -> u32;
}
