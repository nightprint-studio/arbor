//! Canonical entry point for `voice-core`'s public API.

pub use crate::audio::{downmix_to_mono, frame_rms, level_from_rms, resample_to_16k, SAMPLE_RATE};
pub use crate::config::{voice_config_path, voice_models_dir, VoiceConfig, WakeConfig};
pub use crate::plan::{audio_ctx_for, threads_for_cores, transcription_timeout};
pub use crate::models::{
    asset_url, calibration, comfort, estimated_rtf, expected_rtf, find_model, free_space_ok, recommend, streaming_comfortable, Comfort,
    HardwareProfile, Measured, ModelSpec, COMFORT_RTF, RECOMMEND_MAX_RTF, STREAMING_MAX_RTF,
    Recommendation, CATALOGUE,
};
pub use crate::speech::{contains_speech, EndpointEvent, Endpointer, EndpointerConfig, SpeechGate, WakeBudget, FRAME_MS};
pub use crate::state::VoiceState;
pub use crate::vocab::{build_initial_prompt, VocabInput};
pub use crate::wake::{edit_distance, match_wake, normalize, WakeMatch, DEFAULT_PHRASES};
pub use crate::bench::{bench_path, load_measured, record_measurement};
pub use crate::selection::{after_install, after_remove, effective as effective_selection, Selection};
pub use crate::stream::{clean_segment, Assembler, SegmentCutter, MAX_SEGMENT_SECS, SEGMENT_PAUSE_MS};
