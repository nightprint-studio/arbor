use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("Repository not open for tab '{0}'")]
    RepoNotOpen(String),

    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    #[error("Commit not found: {0}")]
    CommitNotFound(String),

    #[error("Reference not found: {0}")]
    RefNotFound(String),

    #[error("Stash not found at index {0}")]
    StashNotFound(usize),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Unsupported: {0}")]
    Unsupported(String),

    /// Returned when an internal Mutex is poisoned (a previous thread panicked
    /// while holding the lock). The string names the component (e.g. "repos").
    #[error("Internal state corrupted (mutex poisoned: {0})")]
    MutexPoisoned(String),

    #[error("{0}")]
    Other(String),
}

/// Implements Serialize so AppError can be returned from Tauri commands directly.
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Convenience alias used throughout the backend.
pub type Result<T> = std::result::Result<T, AppError>;
