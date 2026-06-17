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

    #[error("nemus error: {0}")]
    Nemus(String),

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

/// Bridge `arbor_cloud::prelude::CloudError` into the host error enum so cloud
/// commands can `?`-propagate without bespoke mapping at every call site.
/// Mapping is variant-by-variant to preserve the existing wire shape (the
/// frontend matches on the prefix in the message string).
impl From<arbor_cloud::prelude::CloudError> for AppError {
    fn from(e: arbor_cloud::prelude::CloudError) -> Self {
        use arbor_cloud::prelude::CloudError as C;
        match e {
            C::Io(e)          => AppError::Io(e),
            C::Json(e)        => AppError::Json(e),
            C::AuthFailed(s)  => AppError::AuthFailed(s),
            C::Cancelled     => AppError::Cancelled,
            C::Other(s)       => AppError::Other(s),
        }
    }
}

/// Bridge `arbor_core::prelude::CoreError` (paths / http builder failures) into the
/// host enum. Mapped to existing variants so the wire shape stays
/// untouched.
impl From<arbor_core::prelude::CoreError> for AppError {
    fn from(e: arbor_core::prelude::CoreError) -> Self {
        use arbor_core::prelude::CoreError as C;
        match e {
            C::Io(e)   => AppError::Io(e),
            C::Http(e) => AppError::Other(format!("HTTP: {e}")),
        }
    }
}

/// Bridge `arbor_plugin_marketplace::MarketplaceError` into the host enum.
/// Variant-by-variant so the wire shape stays informative — the marketplace
/// crate distinguishes `PinMismatch` / `InvalidArchive` / `InstallCollision`
/// where the old codebase had a single opaque `AppError::Other` string.
impl From<arbor_plugin_marketplace::prelude::MarketplaceError> for AppError {
    fn from(e: arbor_plugin_marketplace::prelude::MarketplaceError) -> Self {
        use arbor_plugin_marketplace::prelude::MarketplaceError as M;
        match e {
            M::Io(e)               => AppError::Io(e),
            M::Json(e)             => AppError::Json(e),
            M::TomlDe(e)           => AppError::TomlDe(e),
            M::TomlSer(e)          => AppError::TomlSer(e),
            M::Http(e)             => AppError::Other(format!("HTTP: {e}")),
            M::InvalidUrl(s)       => AppError::Other(format!("invalid GitHub URL: {s}")),
            M::NotFound(s)         => AppError::Other(format!("not found: {s}")),
            M::PinMismatch(s)      => AppError::Other(format!("pinned SHA mismatch: {s}")),
            M::InvalidArchive(s)   => AppError::Other(format!("invalid archive: {s}")),
            M::InstallCollision(s) => AppError::Other(s),
            M::Other(s)            => AppError::Other(s),
        }
    }
}

/// Bridge `arbor_plugin_core::error::PluginCoreError` into the host enum.
/// Mapped to existing variants so wire shape (Plugin / IO / Other) is
/// unchanged from the pre-split codebase.
impl From<arbor_plugin_core::prelude::PluginCoreError> for AppError {
    fn from(e: arbor_plugin_core::prelude::PluginCoreError) -> Self {
        use arbor_plugin_core::prelude::PluginCoreError as P;
        match e {
            P::Plugin(s) => AppError::Plugin(s),
            P::Io(e)     => AppError::Io(e),
            P::Other(s)  => AppError::Other(s),
        }
    }
}

/// Bridge the nemus facade's unified error into the host enum. nemus's own
/// crates already preserve detail (and language errors carry a span surfaced
/// separately as `nemus:diagnostics`); at the IPC boundary a flattened string is
/// enough — infra failures (no audio device, render IO) ride this, not user
/// language errors.
impl From<arbor_nemus::prelude::NemusError> for AppError {
    fn from(e: arbor_nemus::prelude::NemusError) -> Self {
        AppError::Nemus(e.to_string())
    }
}

/// Bridge `arbor_fs::FsError` into the host enum. Mapped so the wire string the
/// explorer shows is byte-for-byte what it was before the FS logic moved into
/// the `arbor-fs` crate: IO failures keep their action context, cancellation
/// reuses the `Cancelled` message, and the pre-formatted string variants pass
/// through as `Other`.
impl From<arbor_fs::prelude::FsError> for AppError {
    fn from(e: arbor_fs::prelude::FsError) -> Self {
        use arbor_fs::prelude::FsError as F;
        match e {
            F::Io { context, source } => AppError::Other(format!("{context}: {source}")),
            F::Cancelled              => AppError::Cancelled,
            F::Invalid(s) | F::Trash(s) | F::Zip(s) | F::Unsupported(s) => AppError::Other(s),
        }
    }
}

/// Bridge `corvus_git::prelude::GitError` into the host enum. Variant-for-variant
/// so the frontend wire string is byte-identical to before the local-git logic
/// moved into the `corvus-git` crate (`Io` → `Io`, `Other` → `Other`).
impl From<corvus_git::prelude::GitError> for AppError {
    fn from(e: corvus_git::prelude::GitError) -> Self {
        use corvus_git::prelude::GitError as G;
        match e {
            G::Git(e)             => AppError::Git(e),
            G::Io(e)              => AppError::Io(e),
            G::StashNotFound(i)   => AppError::StashNotFound(i),
            G::Other(s)           => AppError::Other(s),
        }
    }
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
