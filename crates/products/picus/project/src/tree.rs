//! What is on disk, in the shape the interface renders.
//!
//! These are wire types: `camelCase`, field-for-field with `ScriptFile` /
//! `ScriptFolder` / `Branch` / `Project` in `src/lib/types/picus/index.ts`. They
//! describe **the repository**, never a database — the two were conflated once
//! early in this product and it leaked immediately (the generator started asking
//! the project for column types), so the separation is kept deliberately sharp.
//!
//! The dialect lives on the branch, which is the structural invariant of the whole
//! product: a file's dialect is a fact about where it sits, not about what is
//! selected in the toolbar.

use arbor_fs::prelude::encoding::EncodingSource;
use picus_types::prelude::{EngineKind, FolderRole};
use serde::{Deserialize, Serialize};

/// How a file's lines end. Preserved on write: a repository that is CRLF stays
/// CRLF, or every generated block arrives as a whole-file diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LineEnding {
    Crlf,
    Lf,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Crlf => "\r\n",
            LineEnding::Lf => "\n",
        }
    }

    /// Decide from the bytes, by majority.
    ///
    /// A file mixing both is not an error — it is a file two editors have touched
    /// — so the answer is "which one wins", and the minority endings are what a
    /// later diagnostic can report.
    pub fn detect(bytes: &[u8]) -> LineEnding {
        let mut crlf = 0usize;
        let mut lf = 0usize;
        let mut previous = 0u8;
        for &byte in bytes {
            if byte == b'\n' {
                if previous == b'\r' {
                    crlf += 1;
                } else {
                    lf += 1;
                }
            }
            previous = byte;
        }
        // Ties, including a file with no line ending at all, go to CRLF: these
        // repositories are Windows-authored, and inventing LF in one of them would
        // rewrite every line of the first file Picus touches.
        if lf > crlf {
            LineEnding::Lf
        } else {
            LineEnding::Crlf
        }
    }
}

/// One script file, as discovered.
///
/// Serialize only, like its three siblings below: the tree is *reported* to the
/// interface and never sent back, and `EncodingSource` is deliberately
/// write-only in `arbor-fs` for the same reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFile {
    /// Path relative to the project root, POSIX separators — the identity of a
    /// file everywhere in Picus, including on Windows.
    pub path: String,
    pub name: String,
    pub size: u64,
    /// The encoding actually detected.
    pub encoding: String,
    /// How that was decided, so the guess is never silent.
    pub encoding_source: EncodingSource,
    pub eol: LineEnding,
    /// What the folder expects. Different from `encoding` means the file was
    /// rewritten by something that did not know — the `ENC001` diagnostic.
    pub expected_encoding: String,
}

impl ScriptFile {
    /// Has this file drifted from what its folder expects?
    pub fn encoding_drifted(&self) -> bool {
        !self.encoding.eq_ignore_ascii_case(&self.expected_encoding)
    }
}

/// A folder of scripts with one purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFolder {
    pub id: String,
    pub label: String,
    pub role: FolderRole,
    /// Path relative to the project root, POSIX separators.
    pub path: String,
    pub files: Vec<ScriptFile>,
}

/// One per-dialect branch of the repository — in practice a top-level folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub id: String,
    pub label: String,
    /// `None` when nobody could tell which engine this branch is written in.
    /// Nothing is generated into a branch in that state — a wrong guess here
    /// writes Oracle syntax into a PostgreSQL file.
    pub dialect: Option<EngineKind>,
    pub path: String,
    pub folders: Vec<ScriptFolder>,
}

/// The repository as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    /// Absolute path of the root, in the platform's own form — this one is shown
    /// to the user and pasted into a terminal, so it is not POSIX-normalised.
    pub root: String,
    pub branches: Vec<Branch>,
}

impl Project {
    /// Every file, flattened — what searches and pickers want.
    pub fn all_files(&self) -> impl Iterator<Item = &ScriptFile> {
        self.branches.iter().flat_map(|b| b.folders.iter().flat_map(|f| f.files.iter()))
    }

    /// Which branch a project-relative path belongs to, and therefore its dialect.
    pub fn branch_of(&self, path: &str) -> Option<&Branch> {
        self.branches
            .iter()
            .find(|b| b.folders.iter().any(|f| f.files.iter().any(|x| x.path == path)))
    }

    /// The dialect a file must be written in. `None` when the branch's engine is
    /// unknown — the caller must refuse to generate rather than pick one.
    pub fn dialect_of(&self, path: &str) -> Option<EngineKind> {
        self.branch_of(path).and_then(|b| b.dialect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_are_decided_by_majority() {
        assert_eq!(LineEnding::detect(b"a\r\nb\r\nc"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect(b"a\nb\nc"), LineEnding::Lf);
        assert_eq!(LineEnding::detect(b"a\r\nb\nc\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect(b"a\r\nb\r\nc\n"), LineEnding::Crlf);
    }

    #[test]
    fn a_file_with_no_line_ending_does_not_become_lf() {
        // Inventing LF here would rewrite every line of the first file touched.
        assert_eq!(LineEnding::detect(b"SELECT 1;"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect(b""), LineEnding::Crlf);
    }

    #[test]
    fn the_line_ending_wire_words_match_the_frontend() {
        assert_eq!(serde_json::to_string(&LineEnding::Crlf).unwrap(), "\"CRLF\"");
        assert_eq!(serde_json::to_string(&LineEnding::Lf).unwrap(), "\"LF\"");
    }

    fn file(path: &str, encoding: &str, expected: &str) -> ScriptFile {
        ScriptFile {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap().to_string(),
            size: 10,
            encoding: encoding.to_string(),
            encoding_source: EncodingSource::Heuristic,
            eol: LineEnding::Crlf,
            expected_encoding: expected.to_string(),
        }
    }

    fn project() -> Project {
        Project {
            name: "PROD_CORE".to_string(),
            root: r"C:\progetti\prod-core".to_string(),
            branches: vec![
                Branch {
                    id: "ora".to_string(),
                    label: "ORACLE".to_string(),
                    dialect: Some(EngineKind::Oracle),
                    path: "ORACLE".to_string(),
                    folders: vec![ScriptFolder {
                        id: "ora-upd".to_string(),
                        label: "AGGIORNAMENTO".to_string(),
                        role: FolderRole::Update,
                        path: "ORACLE/AGGIORNAMENTO".to_string(),
                        files: vec![file("ORACLE/AGGIORNAMENTO/4_12__4_13.sql", "windows-1252", "windows-1252")],
                    }],
                },
                Branch {
                    id: "common".to_string(),
                    label: "COMMON".to_string(),
                    dialect: None,
                    path: "COMMON".to_string(),
                    folders: vec![ScriptFolder {
                        id: "common".to_string(),
                        label: "COMMON".to_string(),
                        role: FolderRole::Ignored,
                        path: "COMMON".to_string(),
                        files: vec![file("COMMON/notes.sql", "UTF-8", "windows-1252")],
                    }],
                },
            ],
        }
    }

    #[test]
    fn a_files_dialect_comes_from_its_branch() {
        let p = project();
        assert_eq!(p.dialect_of("ORACLE/AGGIORNAMENTO/4_12__4_13.sql"), Some(EngineKind::Oracle));
        // Unknown branch engine: no dialect, so no generation. Not a default.
        assert_eq!(p.dialect_of("COMMON/notes.sql"), None);
        assert_eq!(p.dialect_of("nowhere.sql"), None);
    }

    #[test]
    fn drift_is_what_the_folder_expected_versus_what_was_found() {
        let p = project();
        let drifted: Vec<&str> =
            p.all_files().filter(|f| f.encoding_drifted()).map(|f| f.path.as_str()).collect();
        assert_eq!(drifted, ["COMMON/notes.sql"]);
    }
}
