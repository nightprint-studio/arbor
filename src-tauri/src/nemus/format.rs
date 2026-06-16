//! Format a `.nemus` document — the explicit "reformat code" command.
//!
//! Parses the source to the AST and prints it back through the canonical
//! pretty-printer (`arbor-nemus-lang`'s [`emit`]), the same printer the
//! materialiser uses. The round-trip is *semantic*, not byte-exact: comments and
//! incidental whitespace live only in the source and are not recovered (see
//! `design/nemus/editing-model.md`). A syntax error is surfaced verbatim so the
//! editor can tell the user formatting was skipped — it then leaves the buffer
//! untouched rather than dropping content.

use arbor_nemus::prelude::{emit, parse};

use crate::error::AppError;

/// Reformat `.nemus` source to canonical style. Returns the formatted text, or a
/// language error (a syntax error the buffer currently has) which the caller
/// shows without modifying the document.
#[tauri::command]
pub fn nemus_format(source: String) -> Result<String, AppError> {
    let program = parse(&source).map_err(|e| AppError::Nemus(e.to_string()))?;
    Ok(emit(&program))
}
