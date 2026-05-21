//! Bevy Remote Protocol client.
//!
//! Phase 1 — read-only HTTP JSON-RPC. Phase 2 adds SSE watch streams; the
//! registry now also tracks live subscriptions so they can be aborted when
//! the session ends or the user calls `arbor.brp.unwatch`. Editing arrives
//! in later phases (see `project_bevy_brp_client.md` memory).
//!
//! The plan calls for a **singleton global session** — at most one game at a
//! time. That maps to `AppState.brp = Mutex<Option<BrpSession>>` and is what
//! `BrpRegistry` wraps so callers don't reach into the option directly.
//!
//! BRP 0.18 method names are namespaced under `world.*` / `registry.*` /
//! `rpc.*`. See `methods` below — these are the only strings we need to
//! hard-code here since the rest is just JSON pass-through.

pub mod client;
pub mod sse;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tokio::task::AbortHandle;

pub use client::{BrpClient, BrpError};
pub use sse::{WatchEvent, run_watch_stream};

/// Active connection to a single Bevy game's BRP HTTP endpoint.
///
/// One per app at most (singleton — see plan decision #2). Cloned cheaply
/// via the inner `Arc<BrpClient>` so async tasks can hold a reference past
/// the registry lock.
#[derive(Clone)]
pub struct BrpSession {
    pub endpoint: String,
    pub connected_at: SystemTime,
    pub client: Arc<BrpClient>,
    /// Result of the Phase 1.2 capability probe. `default()` until the
    /// connect step finishes its `rpc.discover` + `registry.schema` pass
    /// (or if either probe failed soft — we still let the session live
    /// since `world.query` may still work).
    pub capabilities: BrpCapabilities,
}

impl BrpSession {
    pub fn new(endpoint: String, client: BrpClient) -> Self {
        Self {
            endpoint,
            connected_at: SystemTime::now(),
            client: Arc::new(client),
            capabilities: BrpCapabilities::default(),
        }
    }

    pub fn with_capabilities(mut self, caps: BrpCapabilities) -> Self {
        self.capabilities = caps;
        self
    }
}

/// Per-session capability matrix produced once at connect time.
///
/// Phase 1.2 replaces the plugin's suffix-match heuristic
/// (`type_name_matches(full, "Name")`) with the canonical paths discovered
/// here. `registry.schema` walks every type the game has registered with
/// `App::register_type`, and we pluck the ones we care about out of the
/// result so the plugin doesn't have to guess between e.g.
/// `bevy_core::name::Name` and `bevy_ecs::name::Name` across Bevy versions.
///
/// Lists rather than `Option<String>` because a single game may have legacy
/// duplicates registered (e.g. both `bevy_hierarchy::Parent` and
/// `bevy_ecs::hierarchy::ChildOf` in transitional builds). Plugins should
/// pass the whole list into `world.query`'s `option` array — BRP tolerates
/// types that aren't present on any entity.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BrpCapabilities {
    /// Method names returned by `rpc.discover`. Empty on probe failure.
    pub methods: Vec<String>,
    /// Every type path found in `registry.schema`. Used by plugins to
    /// drive add-component pickers and `mutate_components` validation
    /// without re-fetching the registry.
    pub all_types: Vec<String>,
    /// Type paths whose short name is `Name` (entity-display label).
    pub name_types: Vec<String>,
    /// Type paths whose short name is `ChildOf` (Bevy ≥ 0.16) or
    /// `Parent` (legacy `bevy_hierarchy::components::parent::Parent`).
    pub parent_types: Vec<String>,
    /// Type paths whose short name is `Children`.
    pub children_types: Vec<String>,
}

impl BrpCapabilities {
    /// True when `rpc.discover` exposes `method`. Soft-true when the probe
    /// failed (methods empty) — we don't want to false-negative a feature
    /// just because the discovery call hiccuped. Plugins that need a hard
    /// guard should check `!methods.is_empty() && contains(method)`.
    pub fn has_method(&self, method: &str) -> bool {
        self.methods.is_empty() || self.methods.iter().any(|m| m == method)
    }

    fn push_unique(list: &mut Vec<String>, value: String) {
        if !list.iter().any(|s| s == &value) {
            list.push(value);
        }
    }

    /// Walk a `registry.schema` response. We don't assume a fixed envelope —
    /// some BRP builds wrap the type map under `components` or `registry`,
    /// others put it at the root. We recurse one level looking for the
    /// first object whose keys look like Rust paths (contain `::`), and
    /// treat that as the type map.
    pub fn ingest_schema(&mut self, schema: &serde_json::Value) {
        let map = locate_type_map(schema);
        let Some(map) = map else { return };
        for key in map.keys() {
            Self::push_unique(&mut self.all_types, key.clone());
            let short = key.rsplit("::").next().unwrap_or("");
            match short {
                "Name" => Self::push_unique(&mut self.name_types, key.clone()),
                "ChildOf" | "Parent" => {
                    Self::push_unique(&mut self.parent_types, key.clone())
                }
                "Children" => Self::push_unique(&mut self.children_types, key.clone()),
                _ => {}
            }
        }
        // Stable order makes the Lua-side query reproducible across runs.
        self.all_types.sort();
        self.name_types.sort();
        self.parent_types.sort();
        self.children_types.sort();
    }

    pub fn ingest_discover(&mut self, discover: &serde_json::Value) {
        // `rpc.discover` returns an OpenRPC-ish document; we only need the
        // method names. Try a few common shapes and gracefully degrade.
        let methods = discover
            .get("methods")
            .or_else(|| discover.get("result"))
            .and_then(|v| v.as_array());
        if let Some(arr) = methods {
            for item in arr {
                let name = item
                    .as_str()
                    .map(|s| s.to_string())
                    .or_else(|| item.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()));
                if let Some(name) = name {
                    Self::push_unique(&mut self.methods, name);
                }
            }
        }
        self.methods.sort();
    }
}

fn locate_type_map(value: &serde_json::Value) -> Option<&serde_json::Map<String, serde_json::Value>> {
    fn looks_like_type_map(map: &serde_json::Map<String, serde_json::Value>) -> bool {
        map.keys().any(|k| k.contains("::"))
    }
    let obj = value.as_object()?;
    if looks_like_type_map(obj) {
        return Some(obj);
    }
    for v in obj.values() {
        if let Some(child) = v.as_object() {
            if looks_like_type_map(child) {
                return Some(child);
            }
        }
    }
    None
}

/// Drive the connect-time probe sequence. Both the Tauri command and the
/// Lua plugin namespace want exactly the same behaviour here, so the logic
/// lives next to `BrpSession` rather than being inlined twice.
///
/// Soft-failure policy: `rpc.discover` failing fatally aborts the connect
/// (no session is created — same as before Phase 1.2). `registry.schema`
/// failing is *not* fatal — some BRP custom configurations gate registry
/// access behind a permission, and we'd rather hand the plugin a partial
/// capability matrix than refuse the connection outright.
pub async fn probe_capabilities(client: &BrpClient) -> Result<BrpCapabilities, BrpError> {
    let discover = client.call(methods::RPC_DISCOVER, None).await?;
    let mut caps = BrpCapabilities::default();
    caps.ingest_discover(&discover);
    match client.call(methods::REGISTRY_SCHEMA, None).await {
        Ok(schema) => caps.ingest_schema(&schema),
        Err(e) => {
            tracing::warn!("brp: registry.schema probe failed (continuing): {e}");
        }
    }
    Ok(caps)
}

/// One live SSE watch — `world.*+watch` subscription pinned to a plugin.
///
/// `aborter` is the handle on the tokio task running `run_watch_stream`.
/// Calling `.abort()` drops the task mid-poll; the streaming task does not
/// fire a final `Close` in that case (the unwatch caller already knows the
/// stream is gone).
pub struct WatchSub {
    pub id: u64,
    pub plugin: String,
    pub method: String,
    pub hook_name: String,
    pub aborter: AbortHandle,
}

/// Singleton holder. Wrapped in a Mutex on AppState so commands can swap
/// the active session atomically. Also owns the active watch subscriptions
/// so a manual disconnect can tear them all down with a single `clear()`.
#[derive(Default)]
pub struct BrpRegistry {
    session: Option<BrpSession>,
    watches: HashMap<u64, WatchSub>,
    next_watch_id: u64,
}

impl BrpRegistry {
    pub fn session(&self) -> Option<&BrpSession> {
        self.session.as_ref()
    }

    pub fn set(&mut self, session: BrpSession) {
        // Replacing the session invalidates every existing watch — they
        // were bound to the previous endpoint's HTTP client. Drop them
        // here so the plugin can re-subscribe against the new session.
        self.abort_all_watches();
        self.session = Some(session);
    }

    pub fn clear(&mut self) {
        self.abort_all_watches();
        self.session = None;
    }

    /// Allocate the next subscription id. Monotonic across the lifetime of
    /// the registry; restarting Arbor resets it. Plugins shouldn't persist
    /// ids across launches.
    pub fn next_watch_id(&mut self) -> u64 {
        self.next_watch_id += 1;
        self.next_watch_id
    }

    pub fn insert_watch(&mut self, sub: WatchSub) {
        self.watches.insert(sub.id, sub);
    }

    /// Take a watch out of the registry by id. Returns the handle so the
    /// caller can `.abort()` it without holding the registry lock during
    /// the abort.
    pub fn take_watch(&mut self, id: u64) -> Option<WatchSub> {
        self.watches.remove(&id)
    }

    /// Abort + drop every active watch. Called on `clear()` and `set()` —
    /// see the comments there for why we bind subscription lifetimes to
    /// the session.
    pub fn abort_all_watches(&mut self) {
        for (_, sub) in self.watches.drain() {
            sub.aborter.abort();
        }
    }

    /// Drop the watches owned by a given plugin (used on plugin unload).
    /// Returns the number aborted so the caller can log it.
    pub fn abort_watches_for_plugin(&mut self, plugin: &str) -> usize {
        let ids: Vec<u64> = self
            .watches
            .iter()
            .filter(|(_, s)| s.plugin == plugin)
            .map(|(id, _)| *id)
            .collect();
        let n = ids.len();
        for id in ids {
            if let Some(sub) = self.watches.remove(&id) {
                sub.aborter.abort();
            }
        }
        n
    }
}

/// Status payload returned by `brp_status` — a compact, serialisable view
/// that frontend + Lua both consume verbatim. Kept deliberately flat so
/// the same `BrpStatus` shape moves over Tauri IPC and Lua tables without
/// transformation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrpStatus {
    pub connected: bool,
    pub endpoint: Option<String>,
    /// Unix seconds since the active session connected. `None` when
    /// disconnected.
    pub connected_at: Option<u64>,
    /// Phase 1.2 capability matrix. `None` when disconnected; on connect
    /// it's always present but its fields may be empty if the probe
    /// returned soft-failure (e.g. `registry.schema` denied by the game's
    /// BRP permissions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<BrpCapabilities>,
}

impl BrpStatus {
    pub fn from_session(session: Option<&BrpSession>) -> Self {
        match session {
            Some(s) => BrpStatus {
                connected: true,
                endpoint: Some(s.endpoint.clone()),
                connected_at: s
                    .connected_at
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs()),
                capabilities: Some(s.capabilities.clone()),
            },
            None => BrpStatus {
                connected: false,
                endpoint: None,
                connected_at: None,
                capabilities: None,
            },
        }
    }
}

/// BRP 0.18 method-name constants. Mirrors `bevy_remote::builtin_methods::*`
/// — we duplicate the strings here so the host doesn't need a compile-time
/// dep on `bevy_remote` (it would drag the whole Bevy graph in).
#[allow(dead_code)] // Most constants only see use from Phase 2+.
pub mod methods {
    pub const WORLD_LIST_COMPONENTS: &str = "world.list_components";
    pub const WORLD_QUERY: &str = "world.query";
    pub const WORLD_GET_COMPONENTS: &str = "world.get_components";
    pub const WORLD_SPAWN_ENTITY: &str = "world.spawn_entity";
    pub const WORLD_DESPAWN_ENTITIES: &str = "world.despawn_entities";
    pub const WORLD_INSERT_COMPONENTS: &str = "world.insert_components";
    pub const WORLD_REMOVE_COMPONENTS: &str = "world.remove_components";
    pub const WORLD_MUTATE_COMPONENTS: &str = "world.mutate_components";
    pub const WORLD_REPARENT_ENTITIES: &str = "world.reparent_entities";
    pub const WORLD_GET_COMPONENTS_WATCH: &str = "world.get_components+watch";
    pub const WORLD_LIST_COMPONENTS_WATCH: &str = "world.list_components+watch";
    pub const WORLD_LIST_RESOURCES: &str = "world.list_resources";
    pub const WORLD_GET_RESOURCES: &str = "world.get_resources";
    pub const WORLD_INSERT_RESOURCES: &str = "world.insert_resources";
    pub const WORLD_REMOVE_RESOURCES: &str = "world.remove_resources";
    pub const WORLD_MUTATE_RESOURCES: &str = "world.mutate_resources";
    pub const WORLD_TRIGGER_EVENT: &str = "world.trigger_event";
    pub const REGISTRY_SCHEMA: &str = "registry.schema";
    pub const RPC_DISCOVER: &str = "rpc.discover";
}

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:15702";
