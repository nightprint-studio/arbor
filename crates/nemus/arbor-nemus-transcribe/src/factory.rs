//! Backend selection — the single place callers choose a [`Transcriber`].
//!
//! Today every selection resolves to the built-in DSP backend; the ONNX backends
//! (basic-pitch, Demucs) branch here once their models are installed, so callers
//! never change.

use crate::dsp::DspTranscriber;
use crate::transcriber::Transcriber;

/// Which transcription backend to use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backend {
    /// The zero-dependency built-in DSP backend — always available.
    #[default]
    BuiltinDsp,
    // Onnx — basic-pitch (+ optional Demucs), models downloaded on demand. Added
    // here when the inference path lands; the trait is the seam, nothing else moves.
}

/// Build a transcriber for `backend`.
pub fn transcriber_for(backend: Backend) -> Box<dyn Transcriber> {
    match backend {
        Backend::BuiltinDsp => Box::new(DspTranscriber),
    }
}
