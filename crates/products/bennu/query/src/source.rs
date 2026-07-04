//! A project source file (path + text) — the input unit for whole-project query operations.
//!
//! The query engine's project-wide walks (inherited-members here; rename planning in `bennu-intel`,
//! which reuses this type) take a slice of these: every `.java` source (for supertype resolution /
//! import rewrites / local-scope walks) and every config `.xml` fragment (for Spring bean edits).

/// A project source file available to a whole-project query: its path and current text (the text
/// may be an unsaved buffer, not what's on disk).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFile {
    /// Absolute path (forward slashes) of the file.
    pub path: String,
    /// The file's current source text.
    pub source: String,
}
