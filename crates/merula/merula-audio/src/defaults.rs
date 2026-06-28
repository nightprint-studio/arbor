//! Shared audio defaults — one source of truth for the numbers the real-time
//! output, the offline render driver, and the shell config all need to agree on.
//!
//! These live in the audio crate because they describe the **audio backend**
//! (its target rate and processing block size); the engine's offline render and
//! the Tauri shell's persisted `[merula]` config reference them rather than
//! re-stating the literals.

/// Target output sample rate in frames per second (design: 48 kHz). The cpal
/// stream opens at (or nearest to) this; the offline render and the shell config
/// default to it.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Processing block size in frames (design: ~512). The cpal stream requests this
/// as its advisory device-buffer size; the offline render driver pulls the
/// `Renderer` one block of this length at a time. Advisory for the live path —
/// the renderer copes with any block length the host actually hands it.
pub const DEFAULT_BLOCK_FRAMES: usize = 512;
