//! Resources: the game's singleton data (`Time`, `Score`, a `State<GameState>`, …).
//!
//! Listing is one cheap call; a value is read on demand for the one resource that is open. Nothing
//! here polls — the frontend re-reads the open resource when asked to, and the world's resource set
//! rarely changes.

use arbor_brp::prelude::{short_type_name, type_paths, unwrap_resource, Op};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{call, live};

#[derive(Serialize)]
pub struct ResourceEntry {
    pub type_path: String,
    pub short_path: String,
}

/// Args for [`bennu_brp_resource`].
#[derive(Deserialize)]
pub struct ResourceArgs {
    /// The resource's full type path.
    pub type_path: String,
}

#[derive(Serialize)]
pub struct ResourceDto {
    pub type_path: String,
    pub value: Value,
}

/// Every resource currently in the world, sorted by path.
#[arbor_rpc::handler]
async fn bennu_brp_resources(_ctx: &BennuState) -> Result<Vec<ResourceEntry>, String> {
    let raw = call(Op::ListResources, None).await?;
    Ok(type_paths(&raw)
        .into_iter()
        .map(|type_path| ResourceEntry {
            short_path: short_type_name(&type_path),
            type_path,
        })
        .collect())
}

/// One resource's current value. A resource the game cannot serialise comes back as the game's own
/// error, which is the useful message.
#[arbor_rpc::handler]
async fn bennu_brp_resource(_ctx: &BennuState, args: ResourceArgs) -> Result<ResourceDto, String> {
    // Fails fast on "not connected" before building a request.
    let _ = live()?;
    let raw = call(Op::GetResource, Some(json!({ "resource": args.type_path }))).await?;
    Ok(ResourceDto { type_path: args.type_path, value: unwrap_resource(&raw) })
}
