//! The voice RPC surface. Thin: every handler delegates to `session`, `capture` or the config.

use serde::Serialize;
use serde_json::{json, Value};
use voice_core::prelude::*;

use crate::capture::{self, DeviceInfo};
use crate::session::{self, Reason, Status, Transcript};
use crate::{engine, models_cmds, shared};

#[derive(Serialize)]
pub struct Overview {
    #[serde(flatten)]
    status: Status,
    /// This build contains the speech engine.
    engine: bool,
    /// Dictation can start now: a model is selected and its file is there.
    model_ready: bool,
    /// The model in use (catalogue id), `""` for none or the user's own file.
    selected: String,
    pid: u32,
}

#[arbor_rpc::handler]
fn voice_status(state: &VoiceState) -> Result<Overview, String> {
    state.touch();
    let cfg = state.config();
    Ok(Overview {
        status: session::status(),
        engine: engine::compiled_in(),
        model_ready: session::model_path(&cfg, None).is_ok(),
        selected: models_cmds::selection(&cfg).model_id().to_string(),
        pid: std::process::id(),
    })
}

#[arbor_rpc::handler]
fn voice_get_config(state: &VoiceState) -> Result<VoiceConfig, String> {
    state.touch();
    Ok(state.config())
}

/// Change some settings. `patch` holds only the fields to change (`wake` merges field by field),
/// so a page with an out-of-date copy cannot undo a newer change such as the selected model.
#[arbor_rpc::handler]
fn voice_set_config(state: &VoiceState, patch: Value) -> Result<VoiceConfig, String> {
    state.touch();
    let saved = state.patch_config(&patch)?;
    if !saved.wake.enabled {
        session::disarm(&shared::get(), "setting");
    }
    state.emit("voice://config", serde_json::to_value(&saved).unwrap_or_default());
    Ok(saved)
}

#[arbor_rpc::handler]
fn voice_list_devices(state: &VoiceState) -> Result<Vec<DeviceInfo>, String> {
    state.touch();
    capture::list_devices()
}

/// Vocabulary the speaker is likely to say; used as the recogniser's `initial_prompt`.
#[arbor_rpc::handler]
fn voice_set_vocabulary(state: &VoiceState, vocab: VocabInput) -> Result<(), String> {
    state.touch();
    session::set_vocab(vocab);
    Ok(())
}

/// The abstract trigger. `reason` is `push_to_talk`, `toggle` or `wake`; returns the request id.
#[arbor_rpc::handler]
fn voice_start_listening(state: &VoiceState, reason: Option<String>) -> Result<u64, String> {
    state.touch();
    session::start_listening(&shared::get(), Reason::parse(reason.as_deref().unwrap_or("push_to_talk")), None, None)
}

#[arbor_rpc::handler]
fn voice_stop_listening(state: &VoiceState) -> Result<Transcript, String> {
    state.touch();
    session::stop_listening(&shared::get())
}

/// Stop everything: the recording is dropped and any transcription in flight gives up.
#[arbor_rpc::handler]
fn voice_cancel(state: &VoiceState) -> Result<(), String> {
    state.touch();
    session::cancel(&shared::get());
    Ok(())
}

/// "Test it": record `seconds` with `model`, transcribe, report the speed. The phases arrive as
/// `voice://test` events (`recording`, `transcribing`); `voice_cancel` stops either one. The speed
/// is remembered, so the recommendation learns what this PC can really do.
#[arbor_rpc::handler]
fn voice_test_model(state: &VoiceState, model: String, seconds: Option<u32>) -> Result<Transcript, String> {
    state.touch();
    let st = shared::get();
    let secs = seconds.unwrap_or(5).clamp(2, 15);
    let id = session::start_listening(&st, Reason::Test, Some(&model), Some(secs))?;
    st.emit("voice://test", json!({ "id": id, "phase": "recording", "seconds": secs }));
    // The recording ends by itself at the cap; wait for it, then collect the transcript.
    let result = session::await_transcript(id, secs as f32);
    st.emit("voice://test", json!({ "id": id, "phase": "done" }));
    result
}

#[arbor_rpc::handler]
fn voice_arm(state: &VoiceState) -> Result<(), String> {
    state.touch();
    session::arm(&shared::get())
}

#[arbor_rpc::handler]
fn voice_disarm(state: &VoiceState) -> Result<(), String> {
    state.touch();
    session::disarm(&shared::get(), "user");
    Ok(())
}

/// The process monitor's Stop button: drop everything and exit.
#[arbor_rpc::handler]
fn voice_shutdown(_state: &VoiceState) -> Result<(), String> {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(150)); // let the reply go out first
        engine::unload_all();
        std::process::exit(0);
    });
    Ok(())
}
