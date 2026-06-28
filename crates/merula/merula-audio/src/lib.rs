//! # merula-audio
//!
//! The real-time / DSP layer of merula (Arbor's live-coding music engine): the
//! only crate that touches the audio hardware. It turns the engine's
//! sample-accurate [`VoiceEvent`](prelude::VoiceEvent)s into sound.
//!
//! ## Two halves
//!
//! - **The contract** ([`seam`]) — [`VoiceEvent`](prelude::VoiceEvent),
//!   [`AudioCommand`](prelude::AudioCommand), the [`AudioSink`](prelude::AudioSink)
//!   trait. Frozen so `merula-engine` builds against it in parallel.
//! - **The implementation** — the [`Renderer`](prelude::Renderer) DSP core
//!   (voices → mixer → effects → master), the cpal real-time
//!   [`stream`], the hand-written SFZ sampler + default synth + sound registry
//!   (Stage A).
//!
//! The same [`Renderer`](prelude::Renderer) drives both real-time playback and
//! the engine's offline render — only who calls `process` differs.
//!
//! ## Entry point
//!
//! Reach the public API through [`prelude`] (workspace convention).

pub mod defaults;
pub mod error;
pub mod meters;
pub mod prelude;
pub mod registry;
pub mod renderer;
pub mod seam;
pub mod speech;
pub mod stream;
pub mod testing;

// Internal DSP modules — implementation detail behind `Renderer`. Kept
// crate-private (not part of the engine-facing surface); the public types they
// expose are re-exported through `prelude` where consumers need them.
pub(crate) mod decode;
pub(crate) mod effects;
pub(crate) mod sampler;
pub(crate) mod sfz;
pub(crate) mod synth;
pub(crate) mod voice;
