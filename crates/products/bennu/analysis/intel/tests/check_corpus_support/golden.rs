//! The committed javac transcript (`<module>/expected.txt`) and the corpus author's inline markers.
//!
//! Golden line: `corpus/args/ArgsTypeBad.java:42:9: compiler.err.cant.apply.symbol` (`#` lines are
//! comments). Marker: a trailing `// error: compiler.err.key` (or `// warn:`), several keys separated
//! by spaces or commas, on the line javac is expected to report.

use std::collections::BTreeSet;
use std::path::Path;

/// One diagnostic javac reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavacDiagnostic {
    /// Relative to the module's `src/main/java`, forward slashes.
    pub file: String,
    /// 1-based.
    pub line: usize,
    /// The full key, `compiler.err.…` or `compiler.warn.…`.
    pub key: String,
}

impl JavacDiagnostic {
    pub fn is_error(&self) -> bool {
        self.key.starts_with("compiler.err.")
    }
}

/// The golden file's diagnostics, or `None` when it has not been generated.
pub fn read_golden(path: &Path) -> Option<Vec<JavacDiagnostic>> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.lines().filter_map(parse_golden_line).collect())
}

fn parse_golden_line(line: &str) -> Option<JavacDiagnostic> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (locator, key) = line.split_once(": ")?;
    let mut parts = locator.rsplitn(3, ':');
    let _column = parts.next()?;
    let line_number = parts.next()?.parse().ok()?;
    let file = parts.next()?.to_string();
    Some(JavacDiagnostic { file, line: line_number, key: key.trim().to_string() })
}

/// One `(file, line, key)` expectation written in the source by the corpus author.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Marker {
    pub file: String,
    pub line: usize,
    pub key: String,
}

/// Every marker in `source` (whose path relative to the source root is `file`).
pub fn markers_in(file: &str, source: &str) -> Vec<Marker> {
    let mut markers = Vec::new();
    for (index, text) in source.lines().enumerate() {
        for key in marker_keys(text) {
            markers.push(Marker { file: file.to_string(), line: index + 1, key });
        }
    }
    markers
}

fn marker_keys(line: &str) -> Vec<String> {
    let Some(at) = line.rfind("//") else { return Vec::new() };
    let comment = line[at + 2..].trim();
    let Some(keys) = comment.strip_prefix("error:").or_else(|| comment.strip_prefix("warn:")) else {
        return Vec::new();
    };
    keys.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|key| key.starts_with("compiler."))
        .map(str::to_string)
        .collect()
}

/// Which side of a marker/golden disagreement is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisagreementKind {
    /// The marker announces a diagnostic javac did not report.
    Unreported,
    /// javac reports a diagnostic no marker announces.
    Unmarked,
}

/// A place where the corpus author's guess and javac differ — the corpus case (or its marker) is
/// wrong, or the golden file is stale.
#[derive(Debug, Clone)]
pub struct Disagreement {
    pub marker: Marker,
    pub kind: DisagreementKind,
}

pub fn disagreements(markers: &[Marker], golden: &[JavacDiagnostic]) -> Vec<Disagreement> {
    let announced: BTreeSet<Marker> = markers.iter().cloned().collect();
    let reported: BTreeSet<Marker> = golden
        .iter()
        .map(|d| Marker { file: d.file.clone(), line: d.line, key: d.key.clone() })
        .collect();
    let unreported = announced
        .difference(&reported)
        .map(|m| Disagreement { marker: m.clone(), kind: DisagreementKind::Unreported });
    let unmarked = reported
        .difference(&announced)
        .map(|m| Disagreement { marker: m.clone(), kind: DisagreementKind::Unmarked });
    unreported.chain(unmarked).collect()
}
