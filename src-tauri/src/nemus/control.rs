//! Messages from the command layer to the dedicated audio thread.
//!
//! The audio thread owns the `Transport` (and the cpal stream); commands never
//! touch it directly. Instead they post a [`NemusControl`] down an `mpsc` channel
//! the thread drains each tick — so the transport needs no lock and the real-time
//! path is never blocked by IPC.

use std::collections::HashSet;

use arbor_nemus::prelude::{ControlMap, Registry, TempoMap, Tracks};

/// A registry decoded off the real-time thread (by the command layer) and handed
/// to the audio thread ready to use. Carried by [`NemusControl::SetTracks`] when
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
pub enum NemusControl {
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

    /// Play a one-off **instrument preview** on the dedicated audition bus (bypasses
    /// the song mixer). `tracks` is a tiny arrangement the command already evaluated
    /// from a generated `.nemus` snippet (a note + the panel's chain): the audio
    /// thread schedules one cycle of it at `cps`, anchored at the current frame, and
    /// routes the resulting voices to the preview bus — so the full language (notes,
    /// chords, scales, any effect) drives the preview without per-param plumbing.
    /// `prepared` carries a registry decoded off-thread when a referenced instrument
    /// isn't resident yet (same path as [`NemusControl::SetTracks`]).
    Audition {
        tracks: Tracks<ControlMap>,
        cps: f64,
        prepared: Option<Prepared>,
    },

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

    /// Tear the session down (drop the cpal stream on this thread) and exit.
    Shutdown,
}
