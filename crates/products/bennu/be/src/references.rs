//! `references` domain — `bennu_references` (find-usages, docs §5 #7).
//!
//! The read-only twin of `bennu_rename_plan`: it classifies the symbol under the caret
//! against the owning project's rename engine (the whole-project reference index +
//! resolver built off-thread on `bennu_open_project`) and returns every resolved use site.
//! Unlike rename, it never edits — it only reports where a declaration is used, for the
//! FE's find-usages panel.
//!
//! Returns `None` (never an error) when no project owns the file, the engine is still
//! building, or the caret isn't on a referenceable symbol (a local variable / parameter
//! is scope-exact and not bucketed cross-file) — the FE degrades gracefully.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{ReferencesResult, UsageLocation};
use bennu_proto::prelude::{UsageHit, UsagesResult};
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_references`].
#[derive(Deserialize)]
pub struct ReferencesArgs {
    /// Absolute path (forward slashes) to the file the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer text — the caret is classified against this.
    pub source: String,
    /// Byte offset of the caret.
    pub offset: usize,
    /// For a caret inside a **library source view**: a file in the project the view was opened
    /// from. That file is under no project root, so its own path cannot pick the index the use
    /// sites live in — this does. Absent for an ordinary project buffer.
    #[serde(default)]
    pub origin_file: Option<String>,
}

/// Find all usages of the symbol at `file`:`offset`. `None` when no project owns the file,
/// its index is still building, or the caret isn't on a referenceable symbol.
#[arbor_rpc::handler]
fn bennu_references(
    _ctx: &BennuState,
    args: ReferencesArgs,
) -> Result<Option<UsagesResult>, String> {
    let service = IndexService::global();
    let result = match &args.origin_file {
        Some(origin) => service.find_usages_from(origin, &args.file, &args.source, args.offset),
        None => service.find_usages(&args.file, &args.source, args.offset),
    };
    Ok(result.map(usages_result_of))
}

/// Map an intel [`ReferencesResult`] onto the wire [`UsagesResult`].
fn usages_result_of(result: ReferencesResult) -> UsagesResult {
    UsagesResult {
        target_label: result.target.label(),
        usages: result.usages.into_iter().map(usage_hit).collect(),
    }
}

/// Map an intel [`UsageLocation`] onto the wire [`UsageHit`] (field-for-field).
fn usage_hit(u: UsageLocation) -> UsageHit {
    UsageHit {
        file: u.file,
        start: u.start,
        end: u.end,
        line: u.line,
        col: u.col,
        preview: u.preview,
    }
}
