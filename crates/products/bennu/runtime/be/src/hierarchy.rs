//! `hierarchy` domain — `bennu_hierarchy_prepare` / `bennu_hierarchy_step`.
//!
//! One pair of handlers over both engines, because "who calls this" is one question. A language
//! with a server is answered by its server; Java is answered by Bennu's own engine over the
//! reference index, since Bennu *is* the Java engine and there is no server to ask. The editor
//! calls these and never has to know which of the two answered — which is what stopped Ctrl+H and
//! Ctrl+Shift+H being hidden on the one language the product exists for.
//!
//! Routed by `engine`, like every other question: a Java buffer goes to the index, a `.ptn` to the Patina
//! package (who calls), everything else to the server that covers it.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::HierarchyItem;
use bennu_proto::prelude::{HierarchyCallSite, HierarchyNode};
use serde::Deserialize;

/// Args for [`bennu_hierarchy_prepare`].
#[derive(Deserialize)]
pub struct PrepareArgs {
    pub file: String,
    pub source: String,
    pub offset: usize,
    /// `true` for the **call** hierarchy, `false` for the **type** hierarchy.
    ///
    /// One handler for both because the two share a wire shape and the panel that draws them is one
    /// panel — two handlers would be two copies of the same marshalling.
    pub calls: bool,
}

/// The item at the caret a hierarchy can be built from, or an empty list when there is none.
#[arbor_rpc::handler]
fn bennu_hierarchy_prepare(
    ctx: &BennuState,
    args: PrepareArgs,
) -> Result<Vec<HierarchyNode>, String> {
    // The one engine that serves the file answers (see `engine`): Java's index, Patina's package, or a server.
    Ok(crate::engine::prepare_hierarchy(ctx, &args.file, &args.source, args.offset, args.calls))
}

/// Args for [`bennu_hierarchy_step`].
#[derive(Deserialize)]
pub struct StepArgs {
    /// Any path inside the project — which engine answers. Not the item's own file: a caller can
    /// live in a dependency's source, which is deliberately not a workspace of its own.
    pub scope: String,
    /// The node's `handle`, verbatim.
    pub item: serde_json::Value,
    /// `incoming` · `outgoing` · `supertypes` · `subtypes`.
    pub direction: String,
}

/// One level of a hierarchy, expanded from a node's handle.
#[arbor_rpc::handler]
fn bennu_hierarchy_step(ctx: &BennuState, args: StepArgs) -> Result<Vec<HierarchyNode>, String> {
    Ok(crate::engine::hierarchy_step(ctx, &args.scope, args.item, &args.direction))
}

/// A Java engine item on the wire.
///
/// The handle is serialised here rather than carried as JSON through the engine: it is a typed enum
/// in there, which is what keeps `step` from having to interpret something it did not issue.
pub(crate) fn node_of(item: HierarchyItem) -> HierarchyNode {
    HierarchyNode {
        name: item.name,
        kind: item.kind,
        detail: item.detail,
        file: item.file,
        start: item.start,
        end: item.end,
        line: item.line,
        col: item.col,
        preview: item.preview,
        call_sites: item
            .call_sites
            .into_iter()
            .map(|s| HierarchyCallSite {
                file: s.file,
                start: s.start,
                end: s.end,
                line: s.line,
                preview: s.preview,
            })
            .collect(),
        // Infallible in practice — the handle is two string fields — and an item whose handle did
        // not serialise would be a node that cannot be expanded, so null says exactly that.
        handle: serde_json::to_value(&item.handle).unwrap_or(serde_json::Value::Null),
    }
}
