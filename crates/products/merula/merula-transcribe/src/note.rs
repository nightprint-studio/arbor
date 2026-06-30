//! The internal detected-note model produced by a backend, before it is written
//! out as MIDI. Times are in **seconds** (a backend works against the audio
//! clock); [`crate::midi_out`] turns these into ticks.

/// A note a backend detected in the audio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetNote {
    /// Onset in seconds from the start of the audio.
    pub start_sec: f64,
    /// Duration in seconds (always `> 0`).
    pub dur_sec: f64,
    /// MIDI note number (`60` = C4); for drums, the GM percussion key.
    pub pitch: u8,
    /// Velocity `0..=127`.
    pub vel: u8,
    /// MIDI channel. Channel 9 (GM "channel 10") marks drums so the downstream
    /// converter splits them into their own part.
    pub channel: u8,
}

/// GM drum channel, zero-based.
pub const DRUM_CHANNEL: u8 = 9;
