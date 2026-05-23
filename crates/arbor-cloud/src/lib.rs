//! Arbor cloud object-storage layer.
//!
//! Provider-agnostic at the API surface (a single [`types::CloudConnection`]
//! covers GCS / S3 / Azure Blob); per-provider plumbing lives behind opendal.
//!
//! The crate is **Tauri-agnostic**: anything that needs to reach back into
//! the host (the JobRegistry, the PluginHost for Lua hooks, the Tauri event
//! bus, the cancellation flag map) goes through the [`host::CloudHost`]
//! trait. The host (`src-tauri`) constructs an impl in `setup()` and stores
//! an `Arc<dyn CloudHost>` in Tauri's managed state; the command layer pulls
//! it out and passes it into the functions below.
//!
//! Earmarked for deletion when the cloud-storage plugin moves to a
//! subprocess runtime — at that point this entire crate + the host's
//! `commands/cloud_commands.rs` + `plugin/api/ns/cloud.rs` disappear.

pub mod auth_gcs;
pub mod error;
pub mod host;
pub mod oauth_google;
pub mod operator;
pub mod ops;
pub mod secrets;
pub mod transfer;
pub mod types;

pub use error::{CloudError, Result};
pub use host::{CloudCancellations, CloudHost, CloudJobInfo, CloudJobStatus, CloudPendingOps};
