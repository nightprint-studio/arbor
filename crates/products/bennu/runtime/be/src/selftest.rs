//! Self-test handlers — prove the framed-stdio handshake + request/response.
//!
//! Plain `#[arbor_rpc::handler]`s with a `&BennuState` context (downcast from
//! `&dyn Any` by the generated thunk), exactly like tyto-be / merula-be. They
//! register via `inventory`, so `arbor_rpc::registry()` collects them and `Hello`
//! advertises them by name. The analysis domains land in per-wave modules the same
//! way.

use bennu_core::prelude::BennuState;

/// Liveness round-trip: `rpc("bennu", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &BennuState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &BennuState, message: String) -> Result<String, String> {
    Ok(message)
}
