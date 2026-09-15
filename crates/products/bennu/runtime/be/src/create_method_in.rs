//! **Create method in the receiver's class** — the half of "create method" that crosses a file.
//!
//! ## Why it is here and not in the transform
//!
//! `bennu-refactor`'s `create_method` writes the method a call is asking for, and refuses the
//! moment the call is on another object: `order.total()` says which method, which arguments and
//! which return type, and says nothing at all about **which file**. That is a question about the
//! whole classpath, and the transform crate has no resolver — so it refused, with a message
//! telling the user to go and write it themselves.
//!
//! It is answerable here, where the resolver is. The receiver's type resolves to a binary name,
//! the index says which file declares it, and the member goes at the end of that class's body.
//!
//! ## Two files, and the second one is not open
//!
//! The call site is the buffer being edited — possibly unsaved, which is why it arrives as text.
//! The target is read from disk **in the project's declared encoding**, because a legacy tree is
//! frequently Cp1252 and an edit computed against a mis-decoded buffer lands at the wrong offset.
//!
//! The edits come back addressed by file, the same shape a rename returns, so the editor applies
//! them with the machinery it already has.
//!
//! ## What it declines to do, and why that is the right answer
//!
//! * a receiver whose type is in a **jar** — there is no source to write into, and decompiled
//!   output is not a file anyone can edit;
//! * a class that **already declares** that name at any arity — a wrong overload is a different
//!   problem, and quietly adding a second `total` beside the first is not what was asked;
//! * a receiver whose type does not resolve — the method would go somewhere guessed at.
//!
//! Each of those is silence rather than an error: the offer simply is not made.

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::SourceEdit;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_create_method_in`].
#[derive(Deserialize)]
pub struct CreateMethodInArgs {
    /// The file the CALL is in — the buffer being edited.
    pub file: String,
    /// Its live text. The call site may be unsaved; the target is read from disk.
    pub source: String,
    /// The diagnostic's span: the called method's NAME.
    pub start: usize,
    pub end: usize,
}

/// Write the method into the receiver's own class. Returns the edits, addressed by file — empty
/// whenever the answer would be a guess (see the module doc).
#[arbor_rpc::handler]
fn bennu_create_method_in(
    _ctx: &BennuState,
    args: CreateMethodInArgs,
) -> Result<Vec<SourceEdit>, String> {
    Ok(IndexService::global().create_method_in(&args.file, &args.source, args.start, args.end))
}
