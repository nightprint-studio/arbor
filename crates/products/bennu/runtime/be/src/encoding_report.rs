//! `encoding_report` domain — `bennu_encoding_report`.
//!
//! Serves the list of source files whose bytes weren't valid in the project's declared
//! (Maven `sourceEncoding`) encoding. Those files were recovered (via `encoding_rs`) and
//! indexed anyway — their classes are never lost — but each is recorded on the project slot
//! during the build so a future UI can list the files that need their real encoding sorted.
//!
//! Read-only, off the slot the index build populates. Returns `[]` when no project owns
//! `root`, the build hasn't landed, or every file was compliant.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::EncodingIssue;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_encoding_report`].
#[derive(Deserialize)]
pub struct EncodingReportArgs {
    /// Absolute path to the open project's root.
    pub root: String,
}

/// Return the non-compliant source files (declared vs. recovered encoding) for the project at
/// `root`. Never errors — an unbuilt / unknown project yields `[]`.
#[arbor_rpc::handler]
fn bennu_encoding_report(
    _ctx: &BennuState,
    args: EncodingReportArgs,
) -> Result<Vec<EncodingIssue>, String> {
    Ok(IndexService::global().encoding_issues(&args.root))
}
