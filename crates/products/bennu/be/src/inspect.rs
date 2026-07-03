//! `inspect` domain — `bennu_index_entries` (the index inspector's per-kind entry lists).
//!
//! The reworked Index Inspector shows, per index kind, the actual entries behind each
//! headline stat: the project members, the classpath jars, the resolved JDK, and the
//! config-graph beans / actions / relations. `types` is served separately by
//! `bennu_class_index`; this one generic handler serves every other kind off the
//! already-built index structures (no re-walk / re-parse) via
//! [`IndexService::index_entries`].
//!
//! Always resolves to a (possibly empty) list — never an error: an unknown root, an
//! unrecognised kind, a still-building index, or a kind whose data genuinely isn't
//! available yet all yield `[]`, and the FE degrades to the "not available yet / building"
//! state gracefully.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::IndexEntry;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_index_entries`].
#[derive(Deserialize)]
pub struct IndexEntriesArgs {
    /// Absolute path to the project root (the key the index service opened the project on).
    pub root: String,
    /// The index kind to list: `"members"` | `"jars"` | `"jdk"` | `"beans"` | `"actions"`
    /// | `"relations"` (`"types"` is served by `bennu_class_index`).
    pub kind: String,
}

/// List every index entry of `kind` for the project at `root`. Always `Ok` with a
/// (possibly empty) list — see the module docs for the graceful-degradation contract.
#[arbor_rpc::handler]
fn bennu_index_entries(
    _ctx: &BennuState,
    args: IndexEntriesArgs,
) -> Result<Vec<IndexEntry>, String> {
    Ok(IndexService::global().index_entries(&args.root, &args.kind))
}
