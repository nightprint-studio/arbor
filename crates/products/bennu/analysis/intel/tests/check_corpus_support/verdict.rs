//! javac against Bennu, line by line.
//!
//! * every javac ERROR is an expectation: **hit** when a Bennu error covering its line carries a check
//!   the key maps to, **miss** when none does, **not covered** when `bennu_check::engine::javac` maps the key
//!   to no check at all;
//! * every Bennu ERROR covering no javac error is a **false positive**;
//! * a Bennu error covering javac errors none of whose keys map to its check is **mislabeled**.

use bennu_check::prelude::{coverage, Coverage};

use super::golden::JavacDiagnostic;

/// One error Bennu reported.
#[derive(Debug, Clone)]
pub struct BennuError {
    pub file: String,
    /// The inclusive line range the diagnostic stands for (see `anchor`).
    pub first_line: usize,
    pub last_line: usize,
    pub code: String,
    pub message: String,
}

impl BennuError {
    fn covers(&self, file: &str, line: usize) -> bool {
        self.file == file && (self.first_line..=self.last_line).contains(&line)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Hit,
    Miss,
    NotCovered,
}

/// One javac error and what Bennu made of it.
#[derive(Debug, Clone)]
pub struct Expectation {
    pub file: String,
    pub line: usize,
    pub key: String,
    pub status: Status,
}

/// A Bennu error beside the javac keys reported on its lines.
#[derive(Debug, Clone)]
pub struct Mislabel {
    pub error: BennuError,
    pub javac_keys: Vec<String>,
}

pub struct Verdicts {
    pub expectations: Vec<Expectation>,
    pub false_positives: Vec<BennuError>,
    pub mislabeled: Vec<Mislabel>,
}

pub fn score(golden: &[JavacDiagnostic], bennu: &[BennuError]) -> Verdicts {
    let javac_errors: Vec<&JavacDiagnostic> = golden.iter().filter(|d| d.is_error()).collect();
    let expectations = javac_errors.iter().map(|e| expectation(e, bennu)).collect();

    let mut false_positives = Vec::new();
    let mut mislabeled = Vec::new();
    for error in bennu {
        let javac_keys: Vec<String> = javac_errors
            .iter()
            .filter(|e| error.covers(&e.file, e.line))
            .map(|e| e.key.clone())
            .collect();
        if javac_keys.is_empty() {
            false_positives.push(error.clone());
        } else if !javac_keys.iter().any(|k| mapped_codes(k).contains(&error.code.as_str())) {
            mislabeled.push(Mislabel { error: error.clone(), javac_keys });
        }
    }
    Verdicts { expectations, false_positives, mislabeled }
}

fn expectation(javac: &JavacDiagnostic, bennu: &[BennuError]) -> Expectation {
    let codes = mapped_codes(&javac.key);
    let status = if codes.is_empty() {
        Status::NotCovered
    } else if bennu.iter().any(|b| b.covers(&javac.file, javac.line) && codes.contains(&b.code.as_str())) {
        Status::Hit
    } else {
        Status::Miss
    };
    Expectation { file: javac.file.clone(), line: javac.line, key: javac.key.clone(), status }
}

/// The Bennu codes `bennu_check::engine::javac` maps `key` to; empty when it maps it to no check.
pub fn mapped_codes(key: &str) -> Vec<&'static str> {
    match coverage(key) {
        Some(Coverage::Check(ids)) => ids.iter().map(|id| id.code()).collect(),
        _ => Vec::new(),
    }
}

/// How the mapping table classifies `key`, for the report.
pub fn coverage_label(key: &str) -> &'static str {
    match coverage(key) {
        Some(Coverage::Check(_)) => "mapped",
        Some(Coverage::Missing) => "declared missing",
        Some(Coverage::OutOfScope(_)) => "out of scope",
        None => "not in the table",
    }
}

/// The corpus category of `file`: the package segment after `corpus` (`corpus/args/X.java` → `args`).
pub fn category(file: &str) -> &str {
    let mut segments = file.split('/');
    match (segments.next(), segments.next()) {
        (Some("corpus"), Some(category)) if !category.ends_with(".java") => category,
        _ => "(root)",
    }
}
