//! `completion picked` domain — `bennu_completion_accepted`.
//!
//! One verb, and the whole of the completion memory's input: the editor says which candidate was
//! accepted, and the next list in the same place offers it first. See `bennu-query`'s `picked`
//! module for what is remembered and why it is not persisted.
//!
//! Fire-and-forget by design. A ranking hint that fails is a list in a slightly worse order, and
//! nothing about accepting a completion should be able to fail visibly because of it.

use bennu_core::prelude::BennuState;
use serde::Deserialize;

/// Args for [`bennu_completion_accepted`].
#[derive(Deserialize)]
pub struct AcceptedArgs {
    /// The `owner` the accepted item carried — the binary name of the type that declares it. The
    /// memory is keyed by it, so what is learned is "reaching into this type, you pick this".
    /// Absent for a candidate that has no owner (a local, a keyword), which is remembered under
    /// the empty context rather than not at all.
    #[serde(default)]
    pub owner: Option<String>,
    /// The label that was accepted.
    pub label: String,
    /// The item's kind slug, when the editor knows it.
    ///
    /// A member is remembered under the type that declares it, which is the context the next list
    /// in the same place is built from. A **type name** has no such context: its `owner` is the
    /// type itself, so filing it there writes a fact nobody ever reads back. Those are remembered
    /// under the kind of hole they came out of — `bennu_query`'s `ANNOTATION_CONTEXT` for an `@`.
    #[serde(default)]
    pub kind: Option<String>,
}

/// Record an accepted completion. Always `Ok(())` — see the module doc.
#[arbor_rpc::handler]
fn bennu_completion_accepted(_ctx: &BennuState, args: AcceptedArgs) -> Result<(), String> {
    let context = match args.kind.as_deref() {
        Some("annotation") => bennu_query::prelude::ANNOTATION_CONTEXT,
        _ => args.owner.as_deref().unwrap_or(""),
    };
    bennu_query::prelude::record_pick(context, &args.label);
    Ok(())
}
