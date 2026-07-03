//! `validation` domain — `bennu_validation_context` (the "New validator" modal).
//!
//! Given a `<Action>-validation.xml` file path, returns the bound action class (by the
//! file-name convention), that class's writable bean properties (the `<field name>`
//! candidates), and the fields already validated in the file. Read-only, off the owning
//! project's index; never errors (an unresolved action yields empty lists).

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::ValidationContext;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_validation_context`].
#[derive(Deserialize)]
pub struct ValidationContextArgs {
    /// Absolute path (forward slashes) to the `<Action>-validation.xml` being edited.
    pub file: String,
}

/// Resolve the modal context for a validation file. `[]`/`None` fields when the action
/// class isn't indexed yet (the modal degrades to a free-text field name).
#[arbor_rpc::handler]
fn bennu_validation_context(
    _ctx: &BennuState,
    args: ValidationContextArgs,
) -> Result<ValidationContext, String> {
    Ok(IndexService::global().validation_context(&args.file))
}
