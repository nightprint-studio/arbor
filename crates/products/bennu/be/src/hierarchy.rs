//! `hierarchy` domain — `bennu_hierarchy_prepare` / `bennu_hierarchy_step`.
//!
//! One pair of handlers over both engines, because "who calls this" is one question. A language
//! with a server is answered by its server; Java is answered by Bennu's own engine over the
//! reference index, since Bennu *is* the Java engine and there is no server to ask. The editor
//! calls these and never has to know which of the two answered — which is what stopped Ctrl+H and
//! Ctrl+Shift+H being hidden on the one language the product exists for.
//!
//! Routed on the file, exactly like `format`: a Java buffer goes to the index, everything else to
//! the server that covers it.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{HierarchyDirection, HierarchyHandle, HierarchyItem};
use bennu_proto::prelude::{HierarchyCallSite, HierarchyNode};
use serde::Deserialize;

use crate::index_service::IndexService;

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
    _ctx: &BennuState,
    args: PrepareArgs,
) -> Result<Vec<HierarchyNode>, String> {
    if crate::intel::is_java_file(&args.file) {
        let items =
            IndexService::global().prepare_hierarchy(&args.file, &args.source, args.offset, args.calls);
        return Ok(items.into_iter().map(node_of).collect());
    }
    Ok(crate::lsp_route::prepare_hierarchy(&args.file, &args.source, args.offset, args.calls))
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
fn bennu_hierarchy_step(_ctx: &BennuState, args: StepArgs) -> Result<Vec<HierarchyNode>, String> {
    if crate::intel::is_java_file(&args.scope) {
        // A handle this engine did not issue, or a direction this build has never heard of. Both
        // answer nothing rather than guessing: hanging the wrong list under an expanded node is a
        // worse outcome than an empty one, and it is a state a newer frontend can produce.
        let Some(direction) = HierarchyDirection::from_wire(&args.direction) else {
            return Ok(Vec::new());
        };
        let Ok(handle) = serde_json::from_value::<HierarchyHandle>(args.item) else {
            return Ok(Vec::new());
        };
        let items = IndexService::global().hierarchy_step(&args.scope, &handle, direction);
        return Ok(items.into_iter().map(node_of).collect());
    }
    Ok(crate::lsp_route::hierarchy_step(&args.scope, args.item, &args.direction))
}

/// A Java engine item on the wire.
///
/// The handle is serialised here rather than carried as JSON through the engine: it is a typed enum
/// in there, which is what keeps `step` from having to interpret something it did not issue.
fn node_of(item: HierarchyItem) -> HierarchyNode {
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
