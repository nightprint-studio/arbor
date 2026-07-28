//! What can go wrong while writing into someone's repository.
//!
//! Every message names the file and, where there is one, the character or the
//! offset. These strings cross the Model-D seam as `Display` output and end up in
//! front of the person whose repository is involved, so "invalid input" is not an
//! acceptable thing for any of them to say.

use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

/// A failure while preparing or committing a set of file edits.
#[derive(Debug)]
pub enum RewriteError {
    /// An edit range lies outside the file it was computed against — almost
    /// always a stale plan applied after the file changed underneath it.
    SpliceOutOfBounds { range: Range<usize>, length: usize },
    /// An edit range cuts a character in half. These files contain accented text;
    /// an offset computed one way and used another has to fail loudly.
    SpliceOffBoundary { range: Range<usize> },
    /// Two edits overlap, so the result would depend on the order they were
    /// listed in.
    SplicesOverlap { first: usize, second: usize },
    /// The file cannot be reproduced byte for byte from what we decoded, so Picus
    /// refuses to write it at all. See `SourceText::verify_round_trip`.
    NotReproducible { path: PathBuf, detail: String },
    /// The new content contains a character the file's encoding cannot represent.
    Unrepresentable { path: PathBuf, detail: String },
    Io { path: PathBuf, reason: String },
    /// A write failed part-way through a multi-file apply, and the files already
    /// written were put back.
    RolledBack { failed: PathBuf, reason: String, restored: usize },
    /// A write failed **and** the rollback failed. The worst case, and the one
    /// that must never be summarised: it names every file left in doubt.
    RollbackFailed { failed: PathBuf, reason: String, unrestored: Vec<PathBuf> },
}

impl fmt::Display for RewriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RewriteError::SpliceOutOfBounds { range, length } => write!(
                f,
                "an edit points at bytes {}..{} of a {length}-byte file — the file changed after \
                 the change was planned; re-generate it",
                range.start, range.end
            ),
            RewriteError::SpliceOffBoundary { range } => write!(
                f,
                "an edit at bytes {}..{} falls in the middle of a character",
                range.start, range.end
            ),
            RewriteError::SplicesOverlap { first, second } => write!(
                f,
                "two edits overlap (one ends at {first}, another starts at {second}) — \
                 the result would depend on which was applied first"
            ),
            RewriteError::NotReproducible { path, detail } => write!(
                f,
                "{}: Picus cannot reproduce this file byte for byte ({detail}), so it will not \
                 write to it — saving would change parts of the file nobody asked to change",
                path.display()
            ),
            RewriteError::Unrepresentable { path, detail } => write!(
                f,
                "{}: {detail}",
                path.display()
            ),
            RewriteError::Io { path, reason } => write!(f, "{}: {reason}", path.display()),
            RewriteError::RolledBack { failed, reason, restored } => write!(
                f,
                "writing {} failed ({reason}); the {restored} file(s) already written were put back \
                 and nothing was changed",
                failed.display()
            ),
            RewriteError::RollbackFailed { failed, reason, unrestored } => {
                write!(
                    f,
                    "writing {} failed ({reason}) AND the rollback did not complete. \
                     These files may be half-changed and need checking by hand:",
                    failed.display()
                )?;
                for path in unrestored {
                    write!(f, "\n  {}", path.display())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for RewriteError {}
