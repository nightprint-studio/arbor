//! The background push driver — a plain thread (the `arbor-scheduler` engine is
//! owned by the plugin host, not exposed to product domains; `security.rs` sets
//! the precedent for a domain-local tokio/thread loop).
//!
//! It wakes on a fixed granularity, rebuilds + fingerprints the bundle, and
//! pushes when the fingerprint differs from the last push — debounced by the
//! configured `interval_secs`, unless an in-process writer flagged a change
//! (then it pushes on the next tick). All disabled/unconfigured/no-dir states
//! are self-healing no-ops, so the driver can start before the shell has pushed
//! the corvus dir or the user has enabled sync.

use std::sync::Arc;
use std::time::{Duration, Instant};

use corvus_core::prelude::CorvusState;

/// How often the driver wakes to consider a push. Small so a flagged change
/// (`mark_dirty`) is picked up promptly; the actual push cadence is gated by
/// `interval_secs`.
const TICK: Duration = Duration::from_secs(15);

/// Spawn the driver thread. Cheap no-op until sync is enabled + resolvable.
pub(crate) fn start(state: Arc<CorvusState>, rt: tokio::runtime::Handle) {
    let _ = std::thread::Builder::new()
        .name("corvus-sync".to_string())
        .spawn(move || run(state, rt));
}

fn run(state: Arc<CorvusState>, rt: tokio::runtime::Handle) {
    let mut last_push: Option<Instant> = None;
    loop {
        std::thread::sleep(TICK);

        let cfg = crate::corvus_config::load(&state).sync;
        if !cfg.enabled {
            continue;
        }
        let Some(remote) = crate::sync::remote::from_config(&cfg) else { continue };
        let files = match crate::sync::sources::build(&state, &cfg) {
            Ok(f) => f,
            Err(_) => continue, // corvus dir not pushed yet — retry next tick
        };

        let fp = crate::sync::sources::fingerprint(&files);
        if !crate::sync::is_dirty(fp) {
            crate::sync::take_dirty(); // in sync already; drop any stale fast-path flag
            continue;
        }

        // Debounce: respect the min interval between pushes unless a writer
        // explicitly flagged a change since the last push.
        let user_dirty = crate::sync::take_dirty();
        let interval_ok = last_push
            .map(|t| t.elapsed().as_secs() >= cfg.interval_secs)
            .unwrap_or(true);
        if !user_dirty && !interval_ok {
            continue;
        }

        match rt.block_on(crate::sync::engine::push(&remote, &files)) {
            Ok(()) => {
                crate::sync::record_pushed(fp);
                last_push = Some(Instant::now());
                let _ = crate::corvus_config::update_sync(&state, |s| {
                    s.last_push_at = Some(crate::sync::now_epoch());
                    s.last_machine = Some(crate::sync::machine_id());
                });
                state.emit(
                    "arbor://corvus-sync-pushed",
                    serde_json::json!({ "at": crate::sync::now_epoch() }),
                );
            }
            Err(e) => eprintln!("corvus-be: sync push failed: {e}"),
        }
    }
}
