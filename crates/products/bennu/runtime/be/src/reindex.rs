//! `reindex` domain — `bennu_reindex` (manual index invalidation + full rebuild).
//!
//! The escape hatch behind the Index Inspector's "Rebuild" button (and the "Rebuild
//! index" palette verb): drop the whole semantic index for the open project and rebuild
//! it from scratch. It re-runs [`IndexService::open`] for `root`, which allocates a FRESH
//! generation dir, re-reads every `.java` source, and rebuilds the symbol index, the Go-to
//! Class cache, the config-graph resolver, the semantic engine, and the completion provider
//! off-thread — emitting `arbor://bennu/index-progress` exactly like an open (so the FE
//! index store re-arms its "Indexing…" job and invalidates its class cache on `ready`).
//!
//! No compilation happens (that's `bennu_build`); this is a pure re-scan of the sources
//! on disk. The JDK level and encoding are detected again from the poms. Errs when no open
//! project owns `root`: a rebuild that silently did nothing read as one that found nothing to fix.

use bennu_core::prelude::BennuState;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_reindex`].
#[derive(Deserialize)]
pub struct ReindexArgs {
    /// Absolute path to the open project's root to invalidate + rebuild.
    pub root: String,
}

/// Re-read the sources of the project at `root` after something outside Bennu **wrote** some: a Maven
/// lifecycle run from the Maven tool window, whose annotation processors put `Order_` (the JPA metamodel),
/// `*MapperImpl` or `QOrder` under `target/generated-sources`. The index only reads a file when it opens
/// the project, so until it is told, every reference to a generated class stays "cannot find symbol" —
/// whatever the build just produced.
///
/// The cheap tier, [`IndexService::refresh_after_compile`] — what `bennu_build` does after its own compile —
/// and not [`bennu_reindex`]: no pom changed, so the dependency list is still true and Maven is not asked again.
#[arbor_rpc::handler]
fn bennu_refresh_index(ctx: &BennuState, args: ReindexArgs) -> Result<(), String> {
    IndexService::global().set_host(ctx.host_caller());
    IndexService::global().refresh_after_compile(&args.root, ctx.event_sink());
    Ok(())
}

/// Invalidate + rebuild the whole semantic index for the project at `root`. Returns
/// immediately; the rebuild runs off-thread and reports progress on the index-progress
/// event stream. Errs when no open project owns `root`.
#[arbor_rpc::handler]
fn bennu_reindex(ctx: &BennuState, args: ReindexArgs) -> Result<(), String> {
    // Keep the reverse channel current so the rebuild's warm-up job still tracks.
    IndexService::global().set_host(ctx.host_caller());
    // Test discovery and entry-point discovery are cached scans of the same sources, so they
    // go stale in exactly the same circumstances. Rebuilding the index and not these is how a
    // newly written test class — or a newly written `main` — ends up needing a restart to
    // appear.
    crate::tests::forget_discovery(&args.root);
    crate::cargo_tests::forget_discovery(&args.root);
    crate::main_classes::forget_main_classes(&args.root);
    // And the "nothing has changed since the last compile" stamp: a re-index is the user
    // saying they no longer trust what we remember about this project.
    crate::build::forget_build_stamp(&args.root);
    // And a broken reactor already announced this episode: the rebuild is the user asking to be
    // told the current state again.
    crate::project_health::forget_module_episode(&args.root);
    IndexService::global().reindex(&args.root, ctx.event_sink())
}
