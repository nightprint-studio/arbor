//! Proves the seam first: `rpc("hirundo", "be_ping")` → `"pong"`.

use hirundo_core::prelude::HirundoState;

#[arbor_rpc::handler]
fn be_ping(_state: &HirundoState) -> Result<String, String> {
    Ok("pong".to_string())
}

#[arbor_rpc::handler]
fn be_echo(_state: &HirundoState, message: String) -> Result<String, String> {
    Ok(message)
}
