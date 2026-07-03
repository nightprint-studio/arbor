//! `jdk_status` domain — `bennu_jdk_status`.
//!
//! Reports how the project's JDK resolved (exact match / fallback / none) so the FE can
//! warn: a titlebar badge when NO JDK is installed at all (completion + navigation can't
//! resolve the standard library), and a Problems entry when a fallback JDK was used (the
//! exact level the project targets isn't installed). Read-only, off the project slot.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::JdkStatus;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_jdk_status`].
#[derive(Deserialize)]
pub struct JdkStatusArgs {
    /// Absolute path to the open project's root.
    pub root: String,
}

/// Return the JDK resolution status for the project at `root`, or `None` when no project
/// owns `root`. Never errors.
#[arbor_rpc::handler]
fn bennu_jdk_status(_ctx: &BennuState, args: JdkStatusArgs) -> Result<Option<JdkStatus>, String> {
    Ok(IndexService::global().jdk_report(&args.root))
}
