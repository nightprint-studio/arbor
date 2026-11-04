//! The console: call any method the connected game answers to, by name.
//!
//! The inspector covers the verbs every Bevy game has; a game's own remote methods (`my_game.spawn_wave`)
//! and the ones a newer Bevy added have no screen, and this is how they are reached. The method list is
//! the game's own `rpc.discover`, so the console offers exactly what that game speaks.

use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::Value;

use super::{err_string, live, touch};

/// Args for [`bennu_brp_call`].
#[derive(Deserialize)]
pub struct CallArgs {
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// The methods the connected game lists in `rpc.discover`, sorted.
#[arbor_rpc::handler]
async fn bennu_brp_methods(_ctx: &BennuState) -> Result<Vec<String>, String> {
    let mut methods = live()?.caps.methods.clone();
    methods.sort();
    methods.dedup();
    Ok(methods)
}

/// Call `method` with `params` as they are and return the game's `result`. The game's own error
/// message is the error: it names the offending type or path.
#[arbor_rpc::handler]
async fn bennu_brp_call(_ctx: &BennuState, args: CallArgs) -> Result<Value, String> {
    let method = args.method.trim();
    if method.is_empty() {
        return Err("Name the method to call.".to_string());
    }
    let live = live()?;
    let result = live.client.call(method, args.params).await.map_err(err_string)?;
    touch();
    Ok(result)
}
