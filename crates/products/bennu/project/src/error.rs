//! The crate's error type — the `error.rs` a leaf crate depends on (docs §10:
//! "dipende solo dal modello progetto + `error`").

/// Errors from opening / modelling a project.
#[derive(Debug)]
pub enum ProjectError {
    /// The project root doesn't exist or isn't a directory.
    NotADirectory(String),
    /// No `pom.xml` at the project root (Phase 0 is Maven-only).
    NoPom(String),
    /// An I/O failure reading a project file.
    Io(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::NotADirectory(p) => write!(f, "not a directory: {p}"),
            ProjectError::NoPom(p) => write!(f, "no pom.xml at project root: {p}"),
            ProjectError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<ProjectError> for String {
    fn from(e: ProjectError) -> String {
        e.to_string()
    }
}
