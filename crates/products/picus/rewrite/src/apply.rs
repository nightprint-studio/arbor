//! Two phases, and the split between them is the whole design.
//!
//! * [`prepare`] does everything that can fail *except* writing: it reads each
//!   file, proves it can be reproduced byte for byte, converts the generated text
//!   to the file's own line endings, splices, and encodes. What comes out is the
//!   exact bytes that would land on disk — which is also what the diff preview
//!   shows, so the user reviews the real thing rather than an approximation of it.
//! * [`commit`] writes those bytes and nothing else. Because every fallible
//!   decision was already made, the only failures left are I/O ones — and when one
//!   happens part-way through, every file already written is put back.
//!
//! "All or nothing" matters more here than in most places: half of a change
//! applied across a two-dialect repository is worse than none of it, because the
//! branches now disagree and the tool that is supposed to detect that is the one
//! that caused it.

use std::path::PathBuf;

use arbor_fs::prelude::encoding::EncodingContext;
use encoding_rs::Encoding;

use crate::error::RewriteError;
use crate::source::{Eol, SourceText};
use crate::splice::{apply_splices, Splice};

/// One file's worth of edits.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub splices: Vec<Splice>,
}

/// A file's edits, resolved down to the bytes that would be written.
#[derive(Debug, Clone)]
pub struct PreparedFile {
    pub path: PathBuf,
    /// The decoded text before and after — what the diff view renders.
    pub before: String,
    pub after: String,
    /// Exactly what [`commit`] will write.
    pub new_bytes: Vec<u8>,
    /// What was there, for the rollback. `None` when the file is being created.
    pub original_bytes: Option<Vec<u8>>,
    pub encoding: String,
    pub eol: Eol,
    /// One line per edit, in file order, for the diff's hunk headers.
    pub reasons: Vec<String>,
}

impl PreparedFile {
    /// Would writing this change anything? A generation re-run with no differences
    /// should not touch a single file's timestamp.
    pub fn is_noop(&self) -> bool {
        match &self.original_bytes {
            Some(original) => *original == self.new_bytes,
            None => false,
        }
    }

    pub fn creates_file(&self) -> bool {
        self.original_bytes.is_none()
    }
}

/// What a successful [`commit`] did.
#[derive(Debug, Clone, Default)]
pub struct Applied {
    pub written: Vec<PathBuf>,
    pub created: Vec<PathBuf>,
    /// Prepared but identical, so left alone.
    pub unchanged: Vec<PathBuf>,
}

/// Resolve one file's edits without touching the disk. Pure.
pub fn prepare_one(source: &SourceText, splices: &[Splice]) -> Result<PreparedFile, RewriteError> {
    // Before anything else: can this file be written back as it was found? If not,
    // no edit to it is safe, however correct the edit itself is.
    source.verify_round_trip()?;

    // Generated SQL arrives with `\n`; the file may well be CRLF. Converting here
    // rather than at the emitter keeps the emitter's golden tests free of line-
    // ending variants, and means every path into this crate gets it right.
    let converted: Vec<Splice> = splices
        .iter()
        .map(|s| Splice {
            range: s.range.clone(),
            replacement: source.eol.normalise(&s.replacement),
            reason: s.reason.clone(),
        })
        .collect();

    let after = apply_splices(&source.text, &converted)?;
    let new_bytes = source.encode(&after)?;

    let mut reasons: Vec<(usize, String)> =
        converted.iter().map(|s| (s.range.start, s.reason.clone())).collect();
    reasons.sort_by_key(|(at, _)| *at);

    Ok(PreparedFile {
        path: source.path.clone(),
        before: source.text.clone(),
        after,
        new_bytes,
        original_bytes: source.exists.then(|| source.bytes.clone()),
        encoding: source.encoding.name().to_string(),
        eol: source.eol,
        reasons: reasons.into_iter().map(|(_, r)| r).collect(),
    })
}

/// Read every file and resolve every edit. Writes nothing.
pub fn prepare(
    changes: &[FileChange],
    ctx: &EncodingContext,
    fallback_encoding: &'static Encoding,
    fallback_eol: Eol,
) -> Result<Vec<PreparedFile>, RewriteError> {
    let mut out = Vec::with_capacity(changes.len());
    for change in changes {
        let source = SourceText::read(&change.path, ctx, fallback_encoding, fallback_eol)?;
        out.push(prepare_one(&source, &change.splices)?);
    }
    Ok(out)
}

/// Write the prepared bytes. All of them, or none of them.
///
/// The rollback restores previous content and deletes files that were created.
/// If the rollback itself fails the error names every file left in doubt, because
/// that is the one situation where a summary is useless and a list is not.
pub fn commit(prepared: &[PreparedFile]) -> Result<Applied, RewriteError> {
    let mut applied = Applied::default();
    // What has been written so far, newest last — the order the rollback undoes.
    let mut done: Vec<&PreparedFile> = Vec::new();

    for file in prepared {
        if file.is_noop() {
            applied.unchanged.push(file.path.clone());
            continue;
        }
        if let Err(reason) = write_file(file) {
            let (restored, unrestored) = roll_back(&done);
            return Err(if unrestored.is_empty() {
                RewriteError::RolledBack { failed: file.path.clone(), reason, restored }
            } else {
                RewriteError::RollbackFailed { failed: file.path.clone(), reason, unrestored }
            });
        }
        if file.creates_file() {
            applied.created.push(file.path.clone());
        } else {
            applied.written.push(file.path.clone());
        }
        done.push(file);
    }
    Ok(applied)
}

fn write_file(file: &PreparedFile) -> Result<(), String> {
    if let Some(parent) = file.path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(&file.path, &file.new_bytes).map_err(|e| e.to_string())
}

/// Undo what was written, most recent first. Returns how many were put back and
/// which could not be.
fn roll_back(done: &[&PreparedFile]) -> (usize, Vec<PathBuf>) {
    let mut restored = 0usize;
    let mut unrestored = Vec::new();
    for file in done.iter().rev() {
        let outcome = match &file.original_bytes {
            Some(original) => std::fs::write(&file.path, original),
            // It did not exist before this apply, so putting it back means
            // removing it. A file that is already gone is a success, not a
            // failure — the desired state is "absent".
            None => match std::fs::remove_file(&file.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        };
        match outcome {
            Ok(()) => restored += 1,
            Err(_) => unrestored.push(file.path.clone()),
        }
    }
    (restored, unrestored)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> EncodingContext {
        EncodingContext::new().with_legacy(encoding_rs::WINDOWS_1252)
    }

    fn cp1252(text: &str) -> Vec<u8> {
        encoding_rs::WINDOWS_1252.encode(text).0.into_owned()
    }

    fn source(text: &str) -> SourceText {
        SourceText::from_bytes("x.sql", cp1252(text), &ctx())
    }

    #[test]
    fn generated_text_takes_the_files_line_endings() {
        // The emitter produces `\n`; the file is CRLF. Splicing it unchanged would
        // give the file mixed endings and turn the diff into noise.
        let file = source("-- header\r\nSELECT 1;\r\n");
        let prepared = prepare_one(
            &file,
            &[Splice::insert(file.text.len(), "INSERT INTO A VALUES (1);\nCOMMIT;\n", "add")],
        )
        .unwrap();
        assert!(prepared.after.ends_with("INSERT INTO A VALUES (1);\r\nCOMMIT;\r\n"));
        assert!(!prepared.after.contains("\n\r"));
        // And nothing that was already there moved.
        assert!(prepared.after.starts_with("-- header\r\nSELECT 1;\r\n"));
    }

    #[test]
    fn an_lf_file_keeps_lf() {
        let file = SourceText::from_bytes("x.sql", cp1252("-- header\nSELECT 1;\n"), &ctx());
        let prepared =
            prepare_one(&file, &[Splice::insert(file.text.len(), "COMMIT;\n", "add")]).unwrap();
        assert!(!prepared.after.contains('\r'));
    }

    #[test]
    fn no_edits_produces_the_original_bytes_exactly() {
        // The byte-identical round trip, asserted end to end through the prepare
        // path rather than only on the decoder.
        let bytes = cp1252("-- soglia già applicata\r\nINSERT INTO A VALUES ('città');\r\n");
        let file = SourceText::from_bytes("x.sql", bytes.clone(), &ctx());
        let prepared = prepare_one(&file, &[]).unwrap();
        assert_eq!(prepared.new_bytes, bytes);
        assert!(prepared.is_noop());
    }

    #[test]
    fn accented_text_survives_being_edited_around() {
        let bytes = cp1252("-- perché\r\nSELECT 1;\r\n");
        let file = SourceText::from_bytes("x.sql", bytes, &ctx());
        let prepared =
            prepare_one(&file, &[Splice::insert(file.text.len(), "-- fine\n", "add")]).unwrap();
        // The new bytes still decode to text containing the accent, and the
        // accented line is byte-identical to what it was.
        let (back, _) = encoding_rs::WINDOWS_1252.decode_with_bom_removal(&prepared.new_bytes);
        assert!(back.contains("perché"));
        assert!(prepared.new_bytes.starts_with(&cp1252("-- perché\r\n")));
    }

    #[test]
    fn a_file_that_cannot_be_reproduced_is_refused_before_any_edit_is_considered() {
        let mut file = source("caffè\r\n");
        file.encoding = encoding_rs::UTF_8; // detection got it wrong
        let err = prepare_one(&file, &[Splice::insert(0, "-- x\n", "add")]).unwrap_err();
        assert!(matches!(err, RewriteError::NotReproducible { .. }));
    }

    #[test]
    fn a_new_file_is_created_with_the_folders_conventions() {
        let file = SourceText::new_file("new.sql", encoding_rs::WINDOWS_1252, Eol::Crlf);
        let prepared = prepare_one(&file, &[Splice::insert(0, "-- città\nSELECT 1;\n", "new")])
            .unwrap();
        assert!(prepared.creates_file());
        assert!(!prepared.is_noop());
        assert_eq!(prepared.new_bytes, cp1252("-- città\r\nSELECT 1;\r\n"));
    }

    #[test]
    fn reasons_come_back_in_file_order_whatever_order_they_were_given_in() {
        let file = source("aaaa\r\nbbbb\r\n");
        let prepared = prepare_one(&file, &[
            Splice::insert(6, "-- second\n", "second"),
            Splice::insert(0, "-- first\n", "first"),
        ])
        .unwrap();
        assert_eq!(prepared.reasons, ["first", "second"]);
    }

    // ── The transactional half. These touch the disk, in a temporary folder of
    // their own making, because "all or nothing" cannot be proven without it.

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("picus-rewrite-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn prepared_for(path: PathBuf, before: Option<&str>, after: &str) -> PreparedFile {
        PreparedFile {
            path,
            before: before.unwrap_or_default().to_string(),
            after: after.to_string(),
            new_bytes: cp1252(after),
            original_bytes: before.map(cp1252),
            encoding: "windows-1252".to_string(),
            eol: Eol::Crlf,
            reasons: vec!["test".to_string()],
        }
    }

    #[test]
    fn a_commit_writes_everything_and_says_what_it_did() {
        let dir = temp_dir("commit");
        let existing = dir.join("a.sql");
        std::fs::write(&existing, cp1252("old\r\n")).unwrap();

        let applied = commit(&[
            prepared_for(existing.clone(), Some("old\r\n"), "new\r\n"),
            prepared_for(dir.join("sub/b.sql"), None, "created\r\n"),
            prepared_for(dir.join("c.sql"), Some("same\r\n"), "same\r\n"),
        ])
        .unwrap();

        assert_eq!(applied.written, [existing.clone()]);
        assert_eq!(applied.created, [dir.join("sub/b.sql")]);
        assert_eq!(applied.unchanged, [dir.join("c.sql")]);
        assert_eq!(std::fs::read(&existing).unwrap(), cp1252("new\r\n"));
        // A no-op was genuinely not written — the file was never created.
        assert!(!dir.join("c.sql").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_half_way_puts_every_earlier_file_back() {
        // The test that matters: fail on the n-th file, verify the previous ones
        // rolled back. The failure is engineered by aiming a write at a path whose
        // parent is a *file*, which no platform will accept as a directory.
        let dir = temp_dir("rollback");
        let first = dir.join("first.sql");
        std::fs::write(&first, cp1252("original\r\n")).unwrap();
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();

        let created = dir.join("created.sql");
        let err = commit(&[
            prepared_for(first.clone(), Some("original\r\n"), "changed\r\n"),
            prepared_for(created.clone(), None, "new\r\n"),
            prepared_for(blocker.join("nope.sql"), None, "boom\r\n"),
        ])
        .unwrap_err();

        assert!(matches!(err, RewriteError::RolledBack { restored: 2, .. }), "{err}");
        // The edited file is back to what it was…
        assert_eq!(std::fs::read(&first).unwrap(), cp1252("original\r\n"));
        // …and the file that had been created is gone again.
        assert!(!created.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_no_op_commit_touches_nothing_at_all() {
        let dir = temp_dir("noop");
        let path = dir.join("a.sql");
        std::fs::write(&path, cp1252("same\r\n")).unwrap();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();

        let applied = commit(&[prepared_for(path.clone(), Some("same\r\n"), "same\r\n")]).unwrap();
        assert_eq!(applied.unchanged, [path.clone()]);
        assert!(applied.written.is_empty());
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
