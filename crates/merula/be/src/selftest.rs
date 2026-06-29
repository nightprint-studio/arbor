//! Self-test handlers — prove the framed-stdio handshake + request/response.
//!
//! Plain `#[arbor_rpc::handler]`s with a `&MerulaState` context (downcast from
//! `&dyn Any` by the generated thunk), exactly like corvus-be's. They register via
//! `inventory`, so `arbor_rpc::registry()` collects them and `Hello` advertises
//! them by name. The git/audio domains land in the per-wave modules the same way.

use merula_core::prelude::MerulaState;

/// Liveness round-trip: `rpc("merula", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &MerulaState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &MerulaState, message: String) -> Result<String, String> {
    Ok(message)
}
