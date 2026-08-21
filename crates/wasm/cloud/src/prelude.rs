//! Canonical entry point for `arbor-cloud`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_cloud::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation (and the cloud command layer still reaches `oauth_google` / `ops`
//! / `secrets` / `transfer` / `types` directly), but are not the canonical
//! call-site path for the re-exported types.

pub use crate::error::{CloudError, Result};
pub use crate::host::{
    CloudCancellations, CloudHost, CloudJobInfo, CloudJobStatus, CloudPendingOps,
};
pub use crate::transport::{
    install_resolver, ObjectTransport, TransportReader, MAX_WHOLE_WRITE,
};
pub use crate::types::CloudConnection;
