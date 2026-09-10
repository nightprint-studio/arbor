//! `generate hint` domain — `bennu_generate_hint` (the ghost text for a member being written).
//!
//! Typing `getCust` in a class body has, most of the time, exactly one thing it can mean, and the
//! editor already knows what: the field is in the buffer, the accessor's name is Java's own
//! convention, and its body is the only body it could have. So it is drawn ahead of the caret, and
//! Tab writes it.
//!
//! **Never a guess.** Ghost text sits inline, where it reads like text that is already there —
//! being wrong there costs trust rather than a keystroke. So this answers only when exactly one
//! accessor matches; two, however close the second is, produce nothing. That is the same rule
//! `bennu-complete`'s `unique_continuation` holds every other ghost proposal in Arbor to.
//!
//! It reads the buffer, not the diagnostics — which is the point. A half-typed name in a class
//! body is a syntax error, so validation stops for the whole file (`bennu-check` returns the
//! syntax error and nothing else, deliberately: a tree nobody believes produces a page of errors
//! about code that compiles). The offer has to survive being typed, so it comes from the tree.
//!
//! The project's resolver is consulted for one thing only: telling a method the class does not
//! declare from one it INHERITS. Without it that family is skipped rather than guessed at.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::AccessorHint;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_generate_hint`].
#[derive(Deserialize)]
pub struct GenerateHintArgs {
    /// Absolute path (forward slashes) of the buffer. Only its extension is consulted — the
    /// answer comes from `source`, not from disk.
    pub file: String,
    /// The live buffer.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
    /// The editor's match-case setting, as everywhere else completion is asked.
    #[serde(default)]
    pub case_sensitive: bool,
}

/// The member the caret is certainly writing, or `None` — which is the ordinary answer.
#[arbor_rpc::handler]
fn bennu_generate_hint(
    _ctx: &BennuState,
    args: GenerateHintArgs,
) -> Result<Option<AccessorHint>, String> {
    if !args.file.to_ascii_lowercase().ends_with(".java") {
        return Ok(None);
    }
    let resolver = IndexService::global().caret_resolver_for(&args.file);
    Ok(bennu_intel::prelude::generated_hint(
        &args.source,
        args.offset,
        bennu_complete::prelude::MatchCase::from_flag(args.case_sensitive),
        resolver.as_deref().map(|r| r as &dyn bennu_java::prelude::TypeResolver),
    ))
}
