//! Live updates for the selected entity, pushed by the game instead of polled for.
//!
//! BRP's `+watch` methods answer a POST with a Server-Sent-Events stream that carries only what
//! changed. Two streams run for the selected entity:
//!
//! * `get_components+watch` over the entity's current component list — the values;
//! * `list_components+watch` — notices a component being added or removed, which a fixed-list value
//!   watch can never see. When the set changes the frontend is told (`kind: "shape"`) and asks to
//!   watch again, which re-lists. That keeps this file free of any merge logic.
//!
//! Only one entity is watched at a time; watching another replaces the streams. A game that does not
//! offer `+watch` (older Bevy) reports an error here and the frontend falls back to slow polling.

use std::collections::HashSet;

use arbor_brp::prelude::{
    components_set_changed, run_watch_stream, unwrap_components, Op, WatchEvent,
};
use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
use serde_json::{json, Value};
use std::sync::Arc;

use super::entities::{component_paths, EntityArgs};
use super::{abort_watches, live, lock, TOPIC_ENTITY};

/// Start watching `args.id`, replacing any previous watch. Resolves once the streams are spawned;
/// frames arrive as `arbor://bennu/brp-entity` events.
#[arbor_rpc::handler]
async fn bennu_brp_watch_entity(ctx: &BennuState, args: EntityArgs) -> Result<(), String> {
    let live = live()?;
    let (Some(get_watch), list_watch) =
        (live.methods.get(Op::GetComponentsWatch), live.methods.get(Op::ListComponentsWatch))
    else {
        return Err("This game does not offer watch streams.".to_string());
    };
    let paths = component_paths(args.id).await?;
    let generation = lock().generation;
    let sink = ctx.event_sink();
    let entity = args.id;

    let mut handles = Vec::new();
    if !paths.is_empty() {
        let params = json!({ "entity": entity, "components": paths, "strict": false });
        let sink = Arc::clone(&sink);
        let task = tokio::spawn(run_watch_stream(
            live.endpoint.clone(),
            get_watch.to_string(),
            Some(params),
            move |ev| forward_values(&sink, entity, generation, ev),
        ));
        handles.push(task.abort_handle());
    }
    if let Some(list_watch) = list_watch {
        let known: HashSet<String> = paths.iter().cloned().collect();
        let sink = Arc::clone(&sink);
        let task = tokio::spawn(run_watch_stream(
            live.endpoint.clone(),
            list_watch.to_string(),
            Some(json!({ "entity": entity })),
            move |ev| forward_shape(&sink, entity, generation, &known, ev),
        ));
        handles.push(task.abort_handle());
    }

    let mut conn = lock();
    if conn.generation != generation {
        // Reconnected while the component list was being read: these streams point at a dead game.
        for h in handles {
            h.abort();
        }
        return Err("The connection changed.".to_string());
    }
    abort_watches(&mut conn);
    conn.watches = handles;
    Ok(())
}

/// Stop watching (the selection was cleared, or the panel closed).
#[arbor_rpc::handler]
async fn bennu_brp_unwatch(_ctx: &BennuState) -> Result<(), String> {
    abort_watches(&mut lock());
    Ok(())
}

// ── Stream → event ───────────────────────────────────────────────────────────

fn emit(sink: &Arc<dyn EventSink>, entity: u64, generation: u64, kind: &str, extra: Value) {
    let mut payload = json!({ "entity": entity, "generation": generation, "kind": kind });
    if let (Some(p), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    sink.emit(TOPIC_ENTITY, payload);
}

/// A value-stream event → `values` (a frame of changed components) / `error` / `closed`.
fn forward_values(sink: &Arc<dyn EventSink>, entity: u64, generation: u64, ev: WatchEvent) {
    match ev {
        WatchEvent::Open => {}
        WatchEvent::Data(v) => {
            let reply = unwrap_components(&v);
            emit(
                sink,
                entity,
                generation,
                "values",
                json!({ "components": reply.components, "errors": reply.errors, "removed": reply.removed }),
            );
        }
        // The usual cause is the entity having been despawned mid-watch.
        WatchEvent::RpcError { message, .. } => emit(sink, entity, generation, "error", json!({ "message": message })),
        WatchEvent::Error(message) => emit(sink, entity, generation, "error", json!({ "message": message })),
        WatchEvent::Close => emit(sink, entity, generation, "closed", json!({})),
    }
}

/// A list-stream event → `shape` when the component set really changed. Everything else on this
/// stream is noise to the frontend: errors and closes are already reported by the value stream.
fn forward_shape(
    sink: &Arc<dyn EventSink>,
    entity: u64,
    generation: u64,
    known: &HashSet<String>,
    ev: WatchEvent,
) {
    if let WatchEvent::Data(frame) = ev {
        if components_set_changed(known, &frame) {
            emit(sink, entity, generation, "shape", json!({}));
        }
    }
}
