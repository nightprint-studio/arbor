//! `reindex` domain — `bennu_reindex` (manual index invalidation + full rebuild).
//!
//! The escape hatch behind the Index Inspector's "Rebuild" button (and the "Rebuild
//! index" palette verb): drop the whole semantic index for the open project and rebuild
//! it from scratch. It re-runs [`IndexService::open`] for `root`, which allocates a FRESH
//! generation dir, re-reads every `.java` source, and rebuilds the symbol index, the Go-to
//! Class cache, the config-graph resolver, the rename engine, and the completion provider
//! off-thread — emitting `arbor://bennu/index-progress` exactly like an open (so the FE
//! index store re-arms its "Indexing…" job and invalidates its class cache on `ready`).
//!
//! No compilation happens (that's `bennu_build`); this is a pure re-scan of the sources
//! on disk. A no-op (still `Ok`) when no open project owns `root` — [`IndexService::reindex`]
//! reads the JDK level off the existing slot, so there's nothing to rebuild without one.

use bennu_core::prelude::BennuState;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_reindex`].
#[derive(Deserialize)]
pub struct ReindexArgs {
    /// Absolute path to the open project's root to invalidate + rebuild.
    pub root: String,
}

/// Invalidate + rebuild the whole semantic index for the project at `root`. Returns
/// immediately; the rebuild runs off-thread and reports progress on the index-progress
/// event stream. No-op when no open project owns `root`.
#[arbor_rpc::handler]
fn bennu_reindex(ctx: &BennuState, args: ReindexArgs) -> Result<(), String> {
    // Keep the reverse channel current so the rebuild's warm-up job still tracks.
    IndexService::global().set_host(ctx.host_caller());
    // Test discovery and entry-point discovery are cached scans of the same sources, so they
    // go stale in exactly the same circumstances. Rebuilding the index and not these is how a
    // newly written test class — or a newly written `main` — ends up needing a restart to
    // appear.
    crate::tests::forget_discovery(&args.root);
    crate::main_classes::forget_main_classes(&args.root);
    // And the "nothing has changed since the last compile" stamp: a re-index is the user
    // saying they no longer trust what we remember about this project.
    crate::build::forget_build_stamp(&args.root);
    IndexService::global().reindex(&args.root, ctx.event_sink());
    Ok(())
}
