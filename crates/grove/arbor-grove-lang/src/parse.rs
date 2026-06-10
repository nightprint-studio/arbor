//! Front-end entry point: source text → [`Program`] AST.
//!
//! The Tree-sitter grammar + external scanner + CST→AST walker land in a
//! following step (`README.md`). Until then this is a stub so the rest of the
//! crate — and `import` resolution, which parses loaded modules through here —
//! compiles and links against the final entry point.

use crate::ast::Program;
use crate::error::{LangError, LangErrorKind, Result};

/// Parse `.grove` source into an AST.
pub fn parse(_source: &str) -> Result<Program> {
    Err(LangError::unlocated(LangErrorKind::Parse(
        "the Tree-sitter front end is not wired yet".to_string(),
    )))
}
