//! Liveness handlers — prove the framed-stdio handshake.

use voice_core::prelude::VoiceState;

#[arbor_rpc::handler]
fn be_ping(_state: &VoiceState) -> Result<String, String> {
    Ok("pong".to_string())
}
