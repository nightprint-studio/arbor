//! control — messages from the command layer to the dedicated audio thread.
//!
//! The audio thread owns the `Transport` (and the cpal stream); commands never
//! touch it directly. Instead they post a [`MerulaControl`] down an `mpsc` channel
//! the thread drains each tick — so the transport needs no lock and the real-time
//! path is never blocked by IPC.
//!
//! Ported verbatim from the shell's `src-tauri/src/merula/control.rs`: the audio
//! thread is Tauri-free, so the variant set + [`Prepared`] cross the process
//! boundary unchanged.

use std::collections::HashSet;

use merula::prelude::{ControlMap, Registry, TempoMap, Tracks};

/// A registry decoded off the real-time thread (by the command layer) and handed
/// to the audio thread ready to use. Carried by [`MerulaControl::SetTracks`] when
/// the new arrangement references sample instruments not yet in the live stream:
/// the sample decode (`packs::load_subset_into`, seconds for a big pack) has
/// already run on a blocking worker, so the audio thread only has to reopen the
/// cpal stream (cheap) — it never decodes on the RT driver loop.
pub struct Prepared {
    /// The built registry: built-in synths + the decoded sample voices.
    pub registry: Registry,
    /// The instrument names this registry resolves (synths + decoded samples),
    /// used to refresh the session's shared `loaded` set on a successful swap.
    pub names: HashSet<String>,
}

/// A control message for the audio thread.
pub enum MerulaControl {
    /// Replace the playing arrangement (a re-eval). The transport applies it
    /// quantized at the next cycle boundary. `cps` carries an optional constant
    /// tempo from the script's `cps(...)`; `tempo` carries a piecewise-constant
    /// tempo automation from `tempo(...)` (empty = none, in which case `cps`
    /// applies). Both are applied (quantized) alongside the tracks. `prepared` is
    /// `Some` only when the arrangement pulls in a sample instrument not yet
    /// loaded: the command pre-decoded its registry off-thread, so the swap costs
    /// no RT decode. `None` = no new voices, just restage the tracks.
    SetTracks {
        tracks: Tracks<ControlMap>,
        cps: Option<f64>,
        tempo: TempoMap,
        prepared: Option<Prepared>,
    },
    /// Start the scheduler.
    Play,
    /// Stop and release all voices.
    Stop,
    /// Jump the cycle clock so `cycle` aligns with the current frame.
    Seek { cycle: f64 },
    /// Change tempo (quantized at the next cycle boundary).
    SetCps { cps: f64 },
    /// Switch the output device live (`None` = host default). Reopens the cpal
    /// stream on this thread, preserving the playhead + play state.
    SetOutputDevice { device: Option<String> },

    /// Play a one-off **preview / snippet** on the dedicated audition bus (bypasses
    /// the song mixer). `tracks` is a small arrangement the command already evaluated
    /// — either an instrument-preview snippet (a note + the panel's chain) or an
    /// arbitrary user-selected `.merula` chunk. The audio thread schedules `cycles`
    /// cycles of it at `cps`, anchored at the current frame, and routes the resulting
    /// voices to the preview bus — so the full language (notes, chords, scales, any
    /// effect, multiple tracks) drives the preview without per-param plumbing. Each
    /// voice self-releases via its own duration, so the one-shot stops on its own.
    /// `prepared` carries a registry decoded off-thread when a referenced instrument
    /// isn't resident yet (same path as [`MerulaControl::SetTracks`]).
    Audition {
        tracks: Tracks<ControlMap>,
        cps: f64,
        cycles: u32,
        prepared: Option<Prepared>,
    },
    /// Clear the audition bus only (stop an in-flight snippet preview early) without
    /// touching the song's voices — unlike [`MerulaControl::Stop`], the main
    /// transport keeps playing.
    StopSnippet,

    // ── Live mixer overrides ───────────────────────────────────────────────
    // Ephemeral session tweaks on top of the source-derived baseline: applied
    // to the running transport in real time (smooth knob drag), and released the
    // next time `SetTracks` re-baselines from the script. The source stays
    // authoritative; these never persist.
    /// Override a strip's gain (linear).
    SetTrackGain { track: u32, gain: f32 },
    /// Override a strip's stereo pan (`0` left … `1` right).
    SetTrackPan { track: u32, pan: f32 },
    /// Mute / unmute a strip.
    SetTrackMute { track: u32, mute: bool },
    /// Solo / un-solo a strip (any soloed strip mutes the non-soloed ones).
    SetTrackSolo { track: u32, solo: bool },
    /// Override the master-strip gain (linear).
    SetMasterGain { gain: f32 },
    /// Set the shared reverb-return decay (procedural IR length, in seconds). A
    /// global mix control like the master gain — session-only, not in the source.
    SetReverb { seconds: f32 },
    /// Enable / disable the audible metronome click track (a monitoring aid; clicks
    /// ride the audition bus, bypassing the song mixer).
    SetMetronome { on: bool },
    /// Set the count-in length in whole bars (`0` = off). On the next play the song is
    /// delayed by this many bars while the metronome clicks the pre-roll.
    SetCountIn { bars: u32 },

    /// Tear the session down (drop the cpal stream on this thread) and exit.
    Shutdown,
}
