//! Messages from the command layer to the dedicated audio thread.
//!
//! The audio thread owns the `Transport` (and the cpal stream); commands never
//! touch it directly. Instead they post a [`GroveControl`] down an `mpsc` channel
//! the thread drains each tick — so the transport needs no lock and the real-time
//! path is never blocked by IPC.

use arbor_grove::prelude::{ControlMap, Tracks};

/// A control message for the audio thread.
pub enum GroveControl {
    /// Replace the playing arrangement (a re-eval). The transport applies it
    /// quantized at the next cycle boundary. `cps` carries an optional tempo from
    /// the script's `cps(...)`, applied (also quantized) alongside.
    SetTracks {
        tracks: Tracks<ControlMap>,
        cps: Option<f64>,
    },
    /// Start the scheduler.
    Play,
    /// Stop and release all voices.
    Stop,
    /// Jump the cycle clock so `cycle` aligns with the current frame.
    Seek { cycle: f64 },
    /// Change tempo (quantized at the next cycle boundary).
    SetCps { cps: f64 },

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
