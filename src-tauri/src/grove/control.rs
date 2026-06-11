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
    /// Tear the session down (drop the cpal stream on this thread) and exit.
    Shutdown,
}
