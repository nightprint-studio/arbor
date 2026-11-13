//! `references` domain — `bennu_references` (find-usages, docs §5 #7).
//!
//! The read-only twin of `bennu_rename_plan`: it classifies the symbol under the caret
//! against the owning project's semantic engine (the whole-project reference index +
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
pub(crate) fn bennu_references(
    ctx: &BennuState,
    args: ReferencesArgs,
) -> Result<Option<UsagesResult>, String> {
    // The one engine that serves the file answers (see `engine`). `origin_file` is a Java-only concern (a caret
    // inside a decompiled library view), so only the Java side reads it.
    Ok(crate::engine::references(ctx, &args.file, &args.source, args.offset, || {
        let service = IndexService::global();
        let result = match &args.origin_file {
            Some(origin) => service.find_usages_from(origin, &args.file, &args.source, args.offset),
            None => service.find_usages(&args.file, &args.source, args.offset),
        };
        result.map(usages_result_of)
    }))
}

/// Map an intel [`ReferencesResult`] onto the wire [`UsagesResult`].
pub(crate) fn usages_result_of(result: ReferencesResult) -> UsagesResult {
    // The direct hits first, then each generated member's — tagged with what it is called, so the
    // list can say why `order.getName()` is a use of the field `name`. One flat list rather than
    // groups on the wire: they are all uses of the same declaration, the count in the header is the
    // total, and `via` is what a reader needs to tell them apart.
    let mut usages: Vec<UsageHit> = result.usages.into_iter().map(|u| usage_hit(u, None)).collect();
    for alias in result.aliases {
        let label = alias.label;
        usages.extend(alias.usages.into_iter().map(|u| usage_hit(u, Some(label.clone()))));
    }
    UsagesResult { target_label: result.target.label(), usages }
}

/// Map an intel [`UsageLocation`] onto the wire [`UsageHit`].
pub(crate) fn usage_hit(u: UsageLocation, via: Option<String>) -> UsageHit {
    UsageHit {
        file: u.file,
        start: u.start,
        end: u.end,
        line: u.line,
        col: u.col,
        preview: u.preview,
        via,
    }
}
