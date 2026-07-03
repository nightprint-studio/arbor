//! `inherited` domain — `bennu_inherited_members` (Structure panel's lazy "Inherited"
//! bucket).
//!
//! Given a type identified by its declaring `(file_path, type_name, line)` — the simple
//! name + its 1-based declaration line, to disambiguate a nested / same-simple-named type —
//! returns the members inherited from its SUPERCLASS + INTERFACES (not the type's own
//! declared members), each tagged with the FQCN that declares it, its visibility, and a
//! `source` file+line WHEN the declaring type resolves to project source (else `null`, like
//! go-to-declaration for a JDK / jar member).
//!
//! Read-only, off the owning project's rename engine (the whole-project resolver + source
//! sets built off-thread on `bennu_open_project`). Returns `[]` (never an error) when no
//! project owns the file, the engine is still building, or the type can't be resolved — the
//! FE shows an empty bucket gracefully.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::InheritedMember;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_inherited_members`].
#[derive(Deserialize)]
pub struct InheritedMembersArgs {
    /// Absolute path (forward slashes) to the file declaring the type.
    pub file_path: String,
    /// The simple (unqualified) name of the type whose inherited members are wanted.
    pub type_name: String,
    /// 1-based declaration line of the type, disambiguating a nested / overloaded name.
    pub line: i64,
}

/// The inherited ("super") members of the type declared at `file_path`:(`type_name`,`line`).
/// `[]` when no project owns the file, its engine is still building, or the type can't be
/// resolved.
#[arbor_rpc::handler]
fn bennu_inherited_members(
    _ctx: &BennuState,
    args: InheritedMembersArgs,
) -> Result<Vec<InheritedMember>, String> {
    Ok(IndexService::global().inherited_members(&args.file_path, &args.type_name, args.line))
}
