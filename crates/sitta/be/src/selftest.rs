//! Self-test handlers — prove the framed-stdio handshake + request/response.
//!
//! Plain `#[arbor_rpc::handler]`s with a `&SittaState` context (downcast from
//! `&dyn Any` by the generated thunk), exactly like corvus-be / merula-be. They
//! register via `inventory`, so `arbor_rpc::registry()` collects them and `Hello`
//! advertises them by name. The explorer domains land in per-wave modules the same
//! way.

use sitta_core::prelude::SittaState;

/// Liveness round-trip: `rpc("sitta", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &SittaState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &SittaState, message: String) -> Result<String, String> {
    Ok(message)
}
