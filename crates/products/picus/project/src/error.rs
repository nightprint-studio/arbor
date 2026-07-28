//! Failures that can happen while reading a project.
//!
//! These strings cross the Model-D seam as `Display` output, so they are written
//! for the person who has to fix the project, not for a log grepper: they name the
//! file, the pattern or the character that is wrong.

use std::fmt;
use std::path::PathBuf;

/// Something went wrong reading or interpreting a script project.
#[derive(Debug)]
pub enum ProjectError {
    /// The root does not exist, or is not a directory.
    NotADirectory { path: PathBuf },
    /// The project file is there but is not valid TOML, or does not have the
    /// shape we expect.
    Malformed { path: PathBuf, reason: String },
    /// A naming pattern from the project file will not compile.
    NamingPattern { pattern: String, reason: String },
    /// Reading or writing the project file failed.
    Io { path: PathBuf, reason: String },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::NotADirectory { path } => {
                write!(f, "{} is not a folder", path.display())
            }
            ProjectError::Malformed { path, reason } => {
                write!(f, "{} could not be read: {reason}", path.display())
            }
            ProjectError::NamingPattern { pattern, reason } => {
                write!(f, "the update-file pattern `{pattern}` is not usable: {reason}")
            }
            ProjectError::Io { path, reason } => {
                write!(f, "{}: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for ProjectError {}
