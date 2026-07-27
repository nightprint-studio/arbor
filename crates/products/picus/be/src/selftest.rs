//! Self-test handlers — prove the framed-stdio handshake + request/response.
//!
//! Plain `#[arbor_rpc::handler]`s with a `&PicusState` context (downcast from
//! `&dyn Any` by the generated thunk), exactly like bennu-be / tyto-be. They
//! register via `inventory`, so `arbor_rpc::registry()` collects them and `Hello`
//! advertises them by name. The database and script domains land in per-wave
//! modules the same way.

use picus_core::prelude::PicusState;

/// Liveness round-trip: `rpc("picus", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &PicusState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &PicusState, message: String) -> Result<String, String> {
    Ok(message)
}
