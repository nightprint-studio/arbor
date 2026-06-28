//! A non-real-time [`AudioSink`] that records every command and exposes a
//! manually-driven clock.
//!
//! This is the seam that lets `merula-engine` be tested headless: the
//! scheduler runs against a `RecordingSink`, advancing the fake clock and
//! asserting on the [`VoiceEvent`]s it emitted — no device, no real time. It
//! lives in the audio crate (not behind `#[cfg(test)]`) so both crates share
//! one recorder.

use crate::seam::{AudioCommand, AudioSink, VoiceEvent};

/// Records all sent commands in order; the clock is whatever you set it to.
#[derive(Debug, Default)]
pub struct RecordingSink {
    sent: Vec<AudioCommand>,
    now: u64,
    sample_rate: u32,
}

impl RecordingSink {
    /// A recorder reporting `sample_rate`, clock at frame 0.
    pub fn new(sample_rate: u32) -> Self {
        RecordingSink {
            sent: Vec::new(),
            now: 0,
            sample_rate,
        }
    }

    /// Advance the fake sample clock by `frames`.
    pub fn advance(&mut self, frames: u64) {
        self.now += frames;
    }

    /// Set the fake sample clock to an absolute frame.
    pub fn set_now(&mut self, frame: u64) {
        self.now = frame;
    }

    /// Every command recorded, in send order.
    pub fn commands(&self) -> &[AudioCommand] {
        &self.sent
    }

    /// Just the voice triggers, in send order.
    pub fn voices(&self) -> impl Iterator<Item = &VoiceEvent> {
        self.sent.iter().filter_map(|c| match c {
            AudioCommand::Voice(v) => Some(v),
            _ => None,
        })
    }

    /// Drop all recorded commands (keeps the clock).
    pub fn clear(&mut self) {
        self.sent.clear();
    }
}

impl AudioSink for RecordingSink {
    fn send(&mut self, cmd: AudioCommand) -> Result<(), AudioCommand> {
        self.sent.push(cmd);
        Ok(())
    }

    fn now_frame(&self) -> u64 {
        self.now
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
