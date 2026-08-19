//! `index_stats` domain — `bennu_index_stats` (index inspector).
//!
//! A cheap snapshot of the per-project index for an inspector panel: symbol counts (types
//! / members) from the last full build, the resolved JDK level, the config-graph counts
//! (actions / beans / relations), and whether the build has finished (`ready`).
//!
//! Never errors just because the index isn't built yet — an unbuilt (or unknown) project
//! reports zeros + `ready = false`, so the FE can poll it while the background build runs.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::IndexStats;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_index_stats`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct IndexStatsArgs {
    /// Absolute path to the project root to report on.
    pub root: String,
}

/// Report how much of the project's semantic index is built: symbol and config counts,
/// the JDK level in use, and whether the build model resolved.
///
/// Worth checking when a navigation or diagnostic call comes back empty — an empty
/// answer from a warming index means "not yet", not "nothing there".
#[arbor_rpc::handler(mcp(
    title = "Check the index state",
    safety = read,
))]
fn bennu_index_stats(_ctx: &BennuState, args: IndexStatsArgs) -> Result<IndexStats, String> {
    Ok(IndexService::global().index_stats(&args.root))
}
