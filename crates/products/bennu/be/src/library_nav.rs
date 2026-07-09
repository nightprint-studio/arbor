//! `library_nav` — go-to-declaration + hover INSIDE a library/JDK source view.
//!
//! A library source view (a read-only `.java` under `bennu_data_dir()/decompiled/`) is under no
//! project root, so the project declaration / hover engines (keyed by `slot_for_file`) can't
//! classify a caret in it. These handlers route through the ORIGIN project (`origin_file` — the
//! project file the view was opened from), whose classpath resolver CAN resolve library types.
//! See [`IndexService::library_declaration`] / [`IndexService::library_hover`].

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::HoverInfo;
use serde::Deserialize;

use crate::index_service::IndexService;
use crate::intel::DecompiledLocation;

/// Args for [`bennu_library_declaration`] / [`bennu_library_hover`].
#[derive(Deserialize)]
pub struct LibraryNavArgs {
    /// A file inside the ORIGIN project (picks its classpath resolver) — the project the library
    /// view was opened from. A library view's own path resolves to no project.
    pub origin_file: String,
    /// The library tab's current buffer.
    pub source: String,
    /// The caret byte offset in `source`.
    pub offset: usize,
}

/// Resolve the caret inside a library source view to another source view (member-precise) — the
/// "navigate within a decompiled / JDK source" gesture, chaining library → library. Returns the
/// target view's path + jump offset (+ whether it's a downloadable dependency stub), or an empty
/// result when the caret isn't a resolvable type / member access.
#[arbor_rpc::handler]
fn bennu_library_declaration(
    _ctx: &BennuState,
    args: LibraryNavArgs,
) -> Result<Option<DecompiledLocation>, String> {
    Ok(IndexService::global()
        .library_declaration(&args.origin_file, &args.source, args.offset)
        .map(|v| DecompiledLocation { file: v.file, offset: v.offset, can_download: v.can_download }))
}

/// Hover inside a library source view — the inferred type of the local / `var` / parameter /
/// expression at the caret (via the origin project's full resolver). Empty when the caret isn't on
/// a typeable local.
#[arbor_rpc::handler]
fn bennu_library_hover(_ctx: &BennuState, args: LibraryNavArgs) -> Result<Option<HoverInfo>, String> {
    Ok(IndexService::global().library_hover(&args.origin_file, &args.source, args.offset))
}
