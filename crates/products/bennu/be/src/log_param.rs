//! `log_param` domain — `bennu_log_param`: offer the "parameterize a logging call" quick-fix for
//! the logging statement under the caret.
//!
//! Thin wrapper over [`bennu_java::prelude::parameterize_log_call`] (the pure, tested transform).
//! Given the current buffer + a caret **byte offset**, it returns the edit (byte range to replace +
//! replacement text) when the caret sits inside a SLF4J/Log4j/JUL call whose message is built by
//! string concatenation, or `None` otherwise. The FE surfaces it as an Alt+Enter intention and
//! applies the single edit.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

/// Args for [`bennu_log_param`].
#[derive(Deserialize)]
pub struct LogParamArgs {
    /// Absolute path of the file (unused by the transform; echoed for symmetry with the other
    /// per-file handlers).
    #[allow(dead_code)]
    pub file: String,
    /// The current (possibly-unsaved) buffer text.
    pub source: String,
    /// Caret position as a **UTF-8 byte offset** into `source`.
    pub offset: usize,
}

/// The edit to apply — replace `source[start..end]` (the argument list, parens excluded) with
/// `replacement`. Mirrors [`bennu_java::prelude::LogParamRewrite`] over the wire.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LogParamResult {
    /// Start byte offset of the argument list (just after `(`).
    pub start: usize,
    /// End byte offset of the argument list (the `)` position).
    pub end: usize,
    /// The rewritten argument list.
    pub replacement: String,
}

/// Return the parameterized-logging rewrite for the call under the caret, or `None`.
#[arbor_rpc::handler]
fn bennu_log_param(_ctx: &BennuState, args: LogParamArgs) -> Result<Option<LogParamResult>, String> {
    Ok(bennu_java::prelude::parameterize_log_call(&args.source, args.offset).map(|rw| {
        LogParamResult { start: rw.start, end: rw.end, replacement: rw.replacement }
    }))
}
