//! `rename` domain — `bennu_rename_plan` / `bennu_rename_apply` (docs §5 #10-12).
//!
//! Both handlers plan the rename off the owning project's semantic engine (the whole-project
//! reference index + resolver + source sets the [`crate::index_service`] builds off-thread
//! on `bennu_open_project`). They differ only in what they return:
//!   * `bennu_rename_plan` → the **preview** (`old→new` per file, with a `reason` and an
//!     `inferred` flag per edit) the FE renders before the user confirms;
//!   * `bennu_rename_apply` → the same edits **flattened**, plus the file move they imply — the
//!     FE applies them through CodeMirror so undo works (the backend never writes the buffers).
//!
//! Both return the empty/`None` answer (never an error) when the engine is still building
//! or the caret isn't on a renameable identifier — the FE degrades gracefully.
//!
//! Best-effort + honest limits: a **local variable / parameter** is scope-exact (no index
//! needed). A **method** rewrites all same-named calls on the owner and flags them
//! `inferred` (overloads collapse to one key). A **class** also edits `import`s and Spring
//! `<bean class="FQCN">`, but a Struts `<action class="beanId">` uses a bean-id (not the
//! FQCN) so it is correctly untouched. OGNL / JSP references are NOT renamed yet.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{Edit as IntelEdit, RenamePlan};
use bennu_proto::prelude::{
    RenameApplyResult, RenameEdit, RenameFileEdits, RenameFileMove, RenamePreview,
};
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_rename_plan`] / [`bennu_rename_apply`].
#[derive(Deserialize)]
pub struct RenameArgs {
    /// Absolute path (forward slashes) to the file the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer text — the caret is classified against this.
    pub source: String,
    /// Byte offset of the caret.
    pub offset: usize,
    /// The requested new name.
    pub new_name: String,
}

/// Plan a rename for the symbol at `file`:`offset` → `new_name`, returning the PREVIEW.
/// `None` when no project owns the file, its semantic engine is still building, or the caret
/// isn't on a renameable identifier.
#[arbor_rpc::handler]
fn bennu_rename_plan(_ctx: &BennuState, args: RenameArgs) -> Result<Option<RenamePreview>, String> {
    // A server-backed file plans through its own server. Its edits are never `inferred` — a
    // language server resolved them, so unlike the Java engine's same-name-method heuristic
    // there is no guesswork for the preview to flag.
    if let Some(preview) =
        crate::lsp_route::rename_plan(&args.file, &args.source, args.offset, &args.new_name)
    {
        return Ok(preview);
    }
    let plan =
        IndexService::global().plan_rename(&args.file, &args.source, args.offset, &args.new_name);
    Ok(plan.map(preview_of))
}

/// The concrete edits the FE applies (the flattened plan), **and the file move they imply**.
///
/// Same classification as [`bennu_rename_plan`]; an empty result when there's nothing to do
/// (unrenameable / still building).
///
/// The `file_rename` is here and not only in the preview because of where this handler is
/// actually called from: the **inline** rename, the small field Shift+F6 opens at the caret. That
/// path never asks for a preview, so for as long as the move travelled only with `RenamePreview`,
/// renaming a public top-level type through it left the type renamed everywhere and the file still
/// called `Order.java` — which does not compile. The move is not a detail of the preview; it is
/// half of what renaming a type *is*.
#[arbor_rpc::handler]
fn bennu_rename_apply(_ctx: &BennuState, args: RenameArgs) -> Result<RenameApplyResult, String> {
    // A server-backed file: the server's edits, and no move. A rename that has to move a file is a
    // Java rule (the public-type/filename tie), and LSP expresses file operations in a channel this
    // seam does not carry — inventing one here would be guessing on the server's behalf.
    if let Some(edits) =
        crate::lsp_route::rename_apply(&args.file, &args.source, args.offset, &args.new_name)
    {
        return Ok(RenameApplyResult { edits, file_rename: None });
    }
    let plan =
        IndexService::global().plan_rename(&args.file, &args.source, args.offset, &args.new_name);
    let Some(plan) = plan else {
        return Ok(RenameApplyResult::default());
    };
    let file_rename = plan.file_rename.map(|r| RenameFileMove { from: r.from, to: r.to });
    let edits = plan.files.into_iter().flat_map(|f| f.edits).map(wire_edit).collect();
    Ok(RenameApplyResult { edits, file_rename })
}

/// Map an intel [`RenamePlan`] onto the wire [`RenamePreview`].
pub(crate) fn preview_of(plan: RenamePlan) -> RenamePreview {
    let total_edits = plan.total_edits();
    RenamePreview {
        old_name: plan.old_name,
        new_name: plan.new_name,
        target_label: plan.target_label,
        files: plan
            .files
            .into_iter()
            .map(|f| RenameFileEdits {
                file: f.file,
                edits: f.edits.into_iter().map(wire_edit).collect(),
            })
            .collect(),
        total_edits,
        has_inferred: plan.has_inferred,
        blocked: plan.blocked,
        file_rename: plan
            .file_rename
            .map(|r| RenameFileMove { from: r.from, to: r.to }),
    }
}

/// Map an intel [`IntelEdit`] onto the wire [`RenameEdit`] (stringify the reason enum).
pub(crate) fn wire_edit(e: IntelEdit) -> RenameEdit {
    RenameEdit {
        file: e.file,
        start: e.start,
        end: e.end,
        new_text: e.new_text,
        old: e.old,
        reason: e.reason.label().to_string(),
        inferred: e.inferred,
    }
}
