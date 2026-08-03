//! The crate's error type — the `error.rs` a leaf crate depends on (docs §10:
//! "dipende solo dal modello progetto + `error`").

use bennu_proto::prelude::ERR_EXTERNALLY_MODIFIED;

/// Errors from opening / modelling a project.
#[derive(Debug)]
pub enum ProjectError {
    /// The project root doesn't exist or isn't a directory.
    NotADirectory(String),
    /// Neither `pom.xml` nor `Cargo.toml` at the project root — nothing tells Bennu
    /// what this directory is.
    NoManifest(String),
    /// The file changed on disk since the caller read it, so writing would have thrown
    /// somebody else's edit away. Carries the file path.
    ///
    /// Its `Display` starts with [`ERR_EXTERNALLY_MODIFIED`] because this is the one error
    /// a caller must **branch** on rather than just show: error strings are the contract
    /// across the RPC seam, and matching on prose would break the day the prose improves.
    ExternallyModified(String),
    /// An I/O failure reading a project file.
    Io(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::NotADirectory(p) => write!(f, "not a directory: {p}"),
            ProjectError::NoManifest(p) => {
                write!(f, "no pom.xml or Cargo.toml at project root: {p}")
            }
            ProjectError::ExternallyModified(p) => write!(
                f,
                "{ERR_EXTERNALLY_MODIFIED}: {p} changed on disk since it was opened",
            ),
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
