//! `completion doc` domain — `bennu_completion_doc` (the popup's documentation panel).
//!
//! The completion list is answered without documentation, and this fills it in for **one** item:
//! the row the user has highlighted. That is not an optimisation, it is the only workable shape —
//! a member list for a busy receiver is hundreds of candidates, and a library's Javadoc is read
//! out of a `-sources.jar` on disk. Resolving all of them to draw one panel would put an archive
//! read per candidate on the keystroke that opened the popup.
//!
//! The item carries the handle it is asked by: `owner` (the binary name of the declaring type)
//! plus its own label. Nothing about the caret is needed — by the time a row is highlighted, the
//! question "what does this name resolve to" has already been answered by the list it is in.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::HoverInfo;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_completion_doc`].
#[derive(Deserialize)]
pub struct CompletionDocArgs {
    /// Absolute path (forward slashes) of the buffer the completion was requested from. It
    /// locates the PROJECT — which index and which dependency jars to resolve against — not the
    /// symbol, which may be declared anywhere on the classpath.
    pub file: String,
    /// The `owner` the completion item carried: the binary name of the type that declares it.
    pub owner: String,
    /// The member's simple name, or `None` when the item IS the type.
    #[serde(default)]
    pub member: Option<String>,
    /// Whether the member is a field. Java lets one name be both a field and a method, and the
    /// item already knows which it is — it came out of the walk that decided its kind.
    #[serde(default)]
    pub is_field: bool,
}

/// The documentation card for one completion candidate. `None` when the project's index is still
/// building, the owner does not resolve, or there is simply nothing documented — all ordinary
/// states the popup renders as "no panel" rather than as an error.
#[arbor_rpc::handler]
fn bennu_completion_doc(
    _ctx: &BennuState,
    args: CompletionDocArgs,
) -> Result<Option<HoverInfo>, String> {
    Ok(IndexService::global().completion_doc(
        &args.file,
        &args.owner,
        args.member.as_deref(),
        args.is_field,
    ))
}
