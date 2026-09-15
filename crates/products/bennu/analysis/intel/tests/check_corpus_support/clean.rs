//! The clean modules (`clean8`, `clean21`): realistic, legal Java that javac compiles without a single
//! diagnostic. Nothing is scored there: every Bennu diagnostic on them, error or warning, is a false
//! positive, and anything in their golden file means the corpus itself is broken.

use std::path::Path;

use super::golden::JavacDiagnostic;
use super::module::{validate_module, CorpusModule, Kept};
use super::verdict::BennuError;

/// Everything found in one clean module.
pub struct CleanResult {
    pub name: &'static str,
    pub files: usize,
    pub lines: usize,
    /// Every Bennu diagnostic on the module, each one a false positive.
    pub false_positives: Vec<BennuError>,
    /// What javac reported: must be empty, anything here is a corpus bug.
    pub javac_diagnostics: Vec<JavacDiagnostic>,
}

/// Validate one clean module; `Err` carries the reason it was skipped.
pub fn run_clean_module(root: &Path, module: &CorpusModule) -> Result<CleanResult, String> {
    let validated = validate_module(root, module, Kept::All)?;
    Ok(CleanResult {
        name: module.name,
        files: validated.sources.len(),
        lines: validated.sources.iter().map(|source| source.text.lines().count()).sum(),
        false_positives: validated.diagnostics,
        javac_diagnostics: validated.golden,
    })
}
