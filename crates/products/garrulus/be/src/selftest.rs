//! Self-test handlers — prove the framed-stdio handshake + request/response.
//!
//! Plain `#[arbor_rpc::handler]`s with a `&GarrulusState` context (downcast from
//! `&dyn Any` by the generated thunk), exactly like sitta-be / picus-be. They
//! register via `inventory`, so `arbor_rpc::registry()` collects them and `Hello`
//! advertises them by name.

use garrulus_core::prelude::GarrulusState;

/// Liveness round-trip: `rpc("garrulus", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &GarrulusState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &GarrulusState, message: String) -> Result<String, String> {
    Ok(message)
}
