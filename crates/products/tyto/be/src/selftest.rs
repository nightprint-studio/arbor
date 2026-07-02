//! Self-test handlers — prove the framed-stdio handshake + request/response.
//!
//! Plain `#[arbor_rpc::handler]`s with a `&TytoState` context (downcast from
//! `&dyn Any` by the generated thunk), exactly like sitta-be / merula-be. They
//! register via `inventory`, so `arbor_rpc::registry()` collects them and `Hello`
//! advertises them by name. The recorder domains land in per-wave modules the same
//! way.

use tyto_core::prelude::TytoState;

/// Liveness round-trip: `rpc("tyto", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &TytoState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &TytoState, message: String) -> Result<String, String> {
    Ok(message)
}
