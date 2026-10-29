//! The world's entities: the light snapshot a tree is served from, and one entity's components.
//!
//! BRP cannot paginate and cannot filter by name, so the strategy is: ask the game for the cheapest
//! thing it can answer — every entity's `Name` and parent, nothing else — **once per refresh**, keep
//! that as a [`Snapshot`], and serve every expand, search and scroll from memory. Component *values*
//! are only ever fetched for the one entity that is selected, and kept fresh by a `+watch` stream
//! (see `watch.rs`) rather than by re-reading.
//!
//! A refresh reports whether anything changed (`changed` / `revision`), which is what lets the
//! frontend back its polling off while the world is quiet.

use std::collections::HashSet;
use std::sync::Arc;

use arbor_brp::prelude::{
    entity_ids, type_paths, unwrap_components, Op, PageQuery, RowView, Snapshot,
};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{call, live, lock, FilterCache};

const DEFAULT_PAGE: usize = 200;
const MAX_PAGE: usize = 2_000;

// ── The wire ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RefreshDto {
    /// Opaque; compare for equality only. A string because it is a 64-bit hash and a JS number
    /// cannot hold one.
    pub revision: String,
    pub total: usize,
    /// The world differs from the previous refresh (an entity appeared, vanished, was renamed or
    /// reparented). Component values changing does not count — those are not in the snapshot.
    pub changed: bool,
    pub generation: u64,
}

/// Args for [`bennu_brp_entities`].
#[derive(Deserialize)]
pub struct EntitiesArgs {
    /// Children of this entity; absent = the roots. Ignored while searching or filtering.
    #[serde(default)]
    pub parent: Option<u64>,
    /// Name substring, or digits matching an entity's id.
    #[serde(default)]
    pub search: Option<String>,
    /// Only entities having every one of these components (type paths).
    #[serde(default)]
    pub with: Vec<String>,
    /// Only entities having none of these components.
    #[serde(default)]
    pub without: Vec<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct EntitiesDto {
    pub revision: String,
    pub total: usize,
    pub rows: Vec<RowView>,
    pub generation: u64,
}

/// Args for the handlers that address one entity.
#[derive(Deserialize)]
pub struct EntityArgs {
    pub id: u64,
}

#[derive(Serialize)]
pub struct EntityDto {
    pub id: u64,
    pub name: Option<String>,
    pub parent: Option<u64>,
    /// `type path → value`, as the game serialises it.
    pub components: Map<String, Value>,
    /// Components the game could not serialise: `type path → { code, message }`.
    pub errors: Map<String, Value>,
    pub generation: u64,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// Re-read the world's entity names and hierarchy. The only call that costs the game O(entities).
#[arbor_rpc::handler]
async fn bennu_brp_refresh(_ctx: &BennuState) -> Result<RefreshDto, String> {
    let (snapshot, changed, generation) = refresh_snapshot().await?;
    Ok(RefreshDto {
        revision: snapshot.revision().to_string(),
        total: snapshot.len(),
        changed,
        generation,
    })
}

/// A page of entities, from the last refresh. Tree mode lists one level (`parent`); a `search` or a
/// component filter lists every match flat.
#[arbor_rpc::handler]
async fn bennu_brp_entities(_ctx: &BennuState, args: EntitiesArgs) -> Result<EntitiesDto, String> {
    let snapshot = current_snapshot().await?;
    let only = match (args.with.is_empty(), args.without.is_empty()) {
        (true, true) => None,
        _ => Some(ids_matching(&snapshot, &args.with, &args.without).await?),
    };
    let search = args.search.unwrap_or_default();
    let limit = args.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let page = {
        let snapshot = Arc::clone(&snapshot);
        tokio::task::spawn_blocking(move || {
            snapshot.page(&PageQuery {
                parent: args.parent,
                search: &search,
                only: only.as_deref(),
                offset: args.offset,
                limit,
            })
        })
        .await
        .map_err(|e| format!("entity page failed: {e}"))?
    };
    Ok(EntitiesDto {
        revision: snapshot.revision().to_string(),
        total: page.total,
        rows: page.rows,
        generation: lock().generation,
    })
}

/// The chain of ancestors of an entity, outermost first — what to expand to reveal a search hit.
#[arbor_rpc::handler]
async fn bennu_brp_ancestors(_ctx: &BennuState, args: EntityArgs) -> Result<Vec<u64>, String> {
    Ok(current_snapshot().await?.ancestors(args.id))
}

/// Every component on one entity, with values. For the selected entity only; its live updates come
/// from `bennu_brp_watch_entity`.
#[arbor_rpc::handler]
async fn bennu_brp_entity(_ctx: &BennuState, args: EntityArgs) -> Result<EntityDto, String> {
    let paths = component_paths(args.id).await?;
    let reply = if paths.is_empty() {
        Default::default()
    } else {
        let raw = call(
            Op::GetComponents,
            Some(json!({ "entity": args.id, "components": paths, "strict": false })),
        )
        .await?;
        unwrap_components(&raw)
    };
    let known = lock().snapshot.as_ref().and_then(|s| s.get(args.id).cloned());
    Ok(EntityDto {
        id: args.id,
        name: known.as_ref().and_then(|r| r.name.clone()),
        parent: known.as_ref().and_then(|r| r.parent),
        components: reply.components,
        errors: reply.errors,
        generation: lock().generation,
    })
}

// ── Shared with siblings ─────────────────────────────────────────────────────

/// The type paths of the components currently on `entity`.
pub(super) async fn component_paths(entity: u64) -> Result<Vec<String>, String> {
    let raw = call(Op::ListComponents, Some(json!({ "entity": entity }))).await?;
    Ok(type_paths(&raw))
}

// ── The snapshot ─────────────────────────────────────────────────────────────

/// Query the game and swap the result in. Returns `(snapshot, changed, generation)`.
async fn refresh_snapshot() -> Result<(Arc<Snapshot>, bool, u64), String> {
    let live = live()?;
    let generation = lock().generation;

    let option: Vec<&String> = live.name_types.iter().chain(&live.parent_types).collect();
    let data = if option.is_empty() { json!({}) } else { json!({ "option": option }) };
    let raw = call(Op::Query, Some(json!({ "data": data, "strict": false }))).await?;

    // Turning 100k query rows into an index is real CPU work.
    let (names, parents) = (live.name_types.clone(), live.parent_types.clone());
    let fresh = Arc::new(
        tokio::task::spawn_blocking(move || Snapshot::from_query(&raw, &names, &parents))
            .await
            .map_err(|e| format!("snapshot indexing failed: {e}"))?,
    );

    let mut conn = lock();
    if conn.generation != generation {
        // The game was swapped under this refresh; its answer describes a world that is gone.
        return Err("The connection changed while the world was being read.".to_string());
    }
    let changed = conn.snapshot.as_ref().map_or(true, |s| s.revision() != fresh.revision());
    if changed {
        conn.snapshot = Some(Arc::clone(&fresh));
        conn.filter_cache = None;
    }
    Ok((fresh, changed, generation))
}

/// The last snapshot, taking one first if there is none yet.
async fn current_snapshot() -> Result<Arc<Snapshot>, String> {
    if let Some(s) = lock().snapshot.clone() {
        return Ok(s);
    }
    Ok(refresh_snapshot().await?.0)
}

/// The entities having all of `with` and none of `without` — one cheap id-only query, remembered
/// for as long as the snapshot (and therefore the set of entities) stays the same.
async fn ids_matching(
    snapshot: &Snapshot,
    with: &[String],
    without: &[String],
) -> Result<Arc<HashSet<u64>>, String> {
    let revision = snapshot.revision();
    let key = format!("{}|{}", with.join(","), without.join(","));
    if let Some(c) = lock().filter_cache.as_ref() {
        if c.revision == revision && c.key == key {
            return Ok(Arc::clone(&c.ids));
        }
    }
    let mut filter = Map::new();
    if !with.is_empty() {
        filter.insert("with".into(), json!(with));
    }
    if !without.is_empty() {
        filter.insert("without".into(), json!(without));
    }
    let raw = call(Op::Query, Some(json!({ "data": {}, "filter": filter, "strict": false }))).await?;
    let ids = Arc::new(
        tokio::task::spawn_blocking(move || entity_ids(&raw))
            .await
            .map_err(|e| format!("filter failed: {e}"))?,
    );
    lock().filter_cache = Some(FilterCache { revision, key, ids: Arc::clone(&ids) });
    Ok(ids)
}
