//! Changing the running game: mutate a field, insert/remove a component or resource, spawn,
//! despawn, reparent.
//!
//! Every write goes through [`call`], so the method name is whatever the connected game's Bevy
//! version calls it (`world.despawn_entity` vs. `bevy/destroy`), and a game that lacks a verb says
//! so in words instead of the UI guessing. The game's own error message is passed through
//! untouched — "Unknown component type" or a reflect path error is exactly what the user needs.
//!
//! **Component or resource** is decided by the presence of `entity`: a resource is a singleton and
//! has none. One argument shape per verb keeps the frontend's editor code identical for both.
//!
//! None of these touch the snapshot; after a write that changes the hierarchy the frontend asks for
//! a refresh, which is also when `changed` would flip.

use arbor_brp::prelude::{parse_entity_ref, Op};
use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::call;

/// Args for [`bennu_brp_mutate`].
#[derive(Deserialize)]
pub struct MutateArgs {
    /// The entity owning the component; absent = `type_path` names a resource.
    #[serde(default)]
    pub entity: Option<u64>,
    pub type_path: String,
    /// Reflect path inside the value: `.translation.x`, `.0`, `.items[2]`.
    pub path: String,
    pub value: Value,
}

/// Args for [`bennu_brp_insert`] and [`bennu_brp_remove`].
#[derive(Deserialize)]
pub struct TypedArgs {
    #[serde(default)]
    pub entity: Option<u64>,
    pub type_path: String,
    /// Insert only: the whole value.
    #[serde(default)]
    pub value: Option<Value>,
}

/// Args for [`bennu_brp_spawn`].
#[derive(Deserialize)]
pub struct SpawnArgs {
    /// `type path → value`; empty spawns a bare entity.
    #[serde(default)]
    pub components: Map<String, Value>,
}

/// Args for [`bennu_brp_despawn`].
#[derive(Deserialize)]
pub struct DespawnArgs {
    pub entity: u64,
}

/// Args for [`bennu_brp_reparent`].
#[derive(Deserialize)]
pub struct ReparentArgs {
    pub entity: u64,
    /// The new parent; absent detaches the entity.
    #[serde(default)]
    pub parent: Option<u64>,
}

/// Set one field inside a component or resource.
#[arbor_rpc::handler]
async fn bennu_brp_mutate(_ctx: &BennuState, args: MutateArgs) -> Result<(), String> {
    let (op, params) = match args.entity {
        Some(entity) => (
            Op::Mutate,
            json!({ "entity": entity, "component": args.type_path, "path": args.path, "value": args.value }),
        ),
        None => (
            Op::MutateResource,
            json!({ "resource": args.type_path, "path": args.path, "value": args.value }),
        ),
    };
    call(op, Some(params)).await.map(drop)
}

/// Insert a component (or replace it whole), or insert a resource.
#[arbor_rpc::handler]
async fn bennu_brp_insert(_ctx: &BennuState, args: TypedArgs) -> Result<(), String> {
    let value = args.value.ok_or_else(|| "Nothing to insert: no value given.".to_string())?;
    let (op, params) = match args.entity {
        Some(entity) => {
            let mut components = Map::new();
            components.insert(args.type_path, value);
            (Op::Insert, json!({ "entity": entity, "components": components }))
        }
        None => (Op::InsertResource, json!({ "resource": args.type_path, "value": value })),
    };
    call(op, Some(params)).await.map(drop)
}

/// Remove a component from an entity, or a resource from the world.
#[arbor_rpc::handler]
async fn bennu_brp_remove(_ctx: &BennuState, args: TypedArgs) -> Result<(), String> {
    let (op, params) = match args.entity {
        Some(entity) => (Op::Remove, json!({ "entity": entity, "components": [args.type_path] })),
        None => (Op::RemoveResource, json!({ "resource": args.type_path })),
    };
    call(op, Some(params)).await.map(drop)
}

/// Spawn an entity with the given components. Returns the new entity's id.
#[arbor_rpc::handler]
async fn bennu_brp_spawn(_ctx: &BennuState, args: SpawnArgs) -> Result<u64, String> {
    let raw = call(Op::Spawn, Some(json!({ "components": args.components }))).await?;
    raw.get("entity")
        .and_then(parse_entity_ref)
        .ok_or_else(|| "The game spawned an entity but did not say which.".to_string())
}

/// Despawn an entity.
#[arbor_rpc::handler]
async fn bennu_brp_despawn(_ctx: &BennuState, args: DespawnArgs) -> Result<(), String> {
    call(Op::Despawn, Some(json!({ "entity": args.entity }))).await.map(drop)
}

/// Move an entity under another, or detach it.
#[arbor_rpc::handler]
async fn bennu_brp_reparent(_ctx: &BennuState, args: ReparentArgs) -> Result<(), String> {
    let mut params = json!({ "entities": [args.entity] });
    if let Some(parent) = args.parent {
        params["parent"] = json!(parent);
    }
    call(Op::Reparent, Some(params)).await.map(drop)
}
