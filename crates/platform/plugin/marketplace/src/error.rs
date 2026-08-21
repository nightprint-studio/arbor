//! Error type for failures originating inside `arbor-plugin-marketplace`.
//!
//! Domain-specific variants ([`MarketplaceError::Network`],
//! [`MarketplaceError::NotFound`], …) replace the catch-all `AppError::Other`
//! strings the original module produced — the host shell adds a
//! `From<MarketplaceError> for AppError` mapping so `?` propagation still
//! works at command boundaries and the wire shape stays compatible.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// The repo URL the user supplied isn't a recognised GitHub URL.
    #[error("invalid GitHub URL: {0}")]
    InvalidUrl(String),

    /// Catalog / plugin / theme entry wasn't found by name or id.
    #[error("not found: {0}")]
    NotFound(String),

    /// Pin verification failed — the resolved ref's SHA doesn't match the
    /// `pinned_sha` recorded in the index entry. Defends against tag-hijack
    /// on third-party (custom + External) sources.
    #[error("pinned SHA mismatch: {0}")]
    PinMismatch(String),

    /// A downloaded artifact's sha256 did not match the digest the registry entry
    /// recorded for it. Distinct from [`MarketplaceError::PinMismatch`], which is about a
    /// git ref having moved: this is about the bytes themselves, and it is the only check
    /// available for an artifact that is a build output rather than a checkout.
    #[error("integrity check failed: {0}")]
    IntegrityMismatch(String),

    /// An index entry is internally inconsistent — it records artifact digests but rides a
    /// moving ref, or names an asset the release does not carry.
    #[error("invalid registry entry: {0}")]
    InvalidEntry(String),

    /// Archive content didn't match the expected shape (multiple roots,
    /// missing subpath, unsafe paths, …).
    #[error("invalid archive: {0}")]
    InvalidArchive(String),

    /// Refusing to overwrite a hand-managed plugin / theme on disk. The
    /// marketplace never silently shadows a user-controlled folder.
    #[error("install collision: {0}")]
    InstallCollision(String),

    /// Anything that doesn't deserve its own variant yet — kept narrow on
    /// purpose so the host can downgrade to `AppError::Other` cleanly.
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, MarketplaceError>;

/// Bridge `arbor_core::CoreError` so internal calls (paths / http builder)
/// propagate via `?`.
impl From<arbor_core::prelude::CoreError> for MarketplaceError {
    fn from(e: arbor_core::prelude::CoreError) -> Self {
        use arbor_core::prelude::CoreError as C;
        match e {
            C::Io(e)   => MarketplaceError::Io(e),
            C::Http(e) => MarketplaceError::Http(e),
        }
    }
}
