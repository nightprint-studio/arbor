//! `opendal::Operator` factory. Translates a [`CloudConnection`] into a
//! ready-to-use Operator scoped to a single bucket.
//!
//! Operators are cheap to construct (no network handshake) so we build a
//! fresh one per call rather than caching — keeps the host stateless and
//! avoids the dance of cache invalidation when secrets rotate.

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use opendal::Operator;
use opendal::services;

use crate::error::{AppError, Result};
use crate::cloud::auth_gcs;
use crate::cloud::secrets;
use crate::cloud::types::{AzBlobAuth, CloudConnection, Provider, S3Auth};

/// Build an Operator for the given connection + bucket.
///
/// The Operator's root is `/<bucket>/` (opendal convention is that the
/// service builder owns the bucket; subsequent ops take object keys).
pub async fn build(conn: &CloudConnection, bucket: &str) -> Result<Operator> {
    match conn.provider {
        Provider::Gcs    => build_gcs(conn, bucket).await,
        Provider::S3     => build_s3(conn, bucket).await,
        Provider::Azblob => build_azblob(conn, bucket).await,
    }
}

async fn build_gcs(conn: &CloudConnection, bucket: &str) -> Result<Operator> {
    let auth = conn.gcs.as_ref().ok_or_else(||
        AppError::AuthFailed("GCS connection is missing a `gcs` auth block".into())
    )?;
    let resolved = auth_gcs::resolve(auth).await?;

    // project_id is not required by the GCS object API (the bucket implies
    // it) — kept on `CloudConnection` for forward-compat with project-scoped
    // ops like bucket listing, not used here.
    let _ = &conn.project_id;

    let builder = services::Gcs::default()
        .bucket(bucket)
        .scope("https://www.googleapis.com/auth/devstorage.read_write");

    // opendal::Gcs::credential expects the SA JSON to be base64-encoded —
    // passing the raw JSON makes it silently fall back to metadata-server
    // discovery (which dies on a non-GCE host with a confusing "connection
    // refused" against `metadata.google.internal`). Always encode here so
    // every Credential branch (SaFile, SaInline, ADC service_account) goes
    // through the same gate.
    let builder = match resolved {
        auth_gcs::Resolved::Credential { json, .. } => {
            let encoded = B64.encode(json.as_bytes());
            builder.credential(&encoded)
        }
        auth_gcs::Resolved::Token { access_token, .. } => builder.token(access_token),
    };

    Ok(Operator::new(builder).map_err(map_op_err)?.finish())
}

async fn build_s3(conn: &CloudConnection, bucket: &str) -> Result<Operator> {
    let auth: &S3Auth = conn.s3.as_ref().ok_or_else(||
        AppError::AuthFailed("S3 connection is missing an `s3` auth block".into())
    )?;
    if auth.access_key_id.is_empty() {
        return Err(AppError::AuthFailed("S3 connection is missing access_key_id".into()));
    }
    if auth.secret_ref.is_empty() {
        return Err(AppError::AuthFailed("S3 connection is missing secret_ref".into()));
    }
    let secret_access_key = secrets::get(&auth.secret_ref)
        .map_err(|e| AppError::AuthFailed(format!("S3 secret lookup ({}): {e}", auth.secret_ref)))?
        .ok_or_else(|| AppError::AuthFailed(format!(
            "S3 secret missing from keyring ({}) — re-enter the secret access key in the connection editor",
            auth.secret_ref
        )))?;

    let mut builder = services::S3::default()
        .bucket(bucket)
        .access_key_id(&auth.access_key_id)
        .secret_access_key(&secret_access_key);
    if let Some(region) = auth.region.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.region(region);
    }
    if let Some(endpoint) = auth.endpoint.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.endpoint(endpoint);
    }
    if auth.force_path_style.unwrap_or(false) {
        builder = builder.enable_virtual_host_style();
    }

    Ok(Operator::new(builder).map_err(map_op_err)?.finish())
}

async fn build_azblob(conn: &CloudConnection, bucket: &str) -> Result<Operator> {
    let auth: &AzBlobAuth = conn.azblob.as_ref().ok_or_else(||
        AppError::AuthFailed("Azure Blob connection is missing an `azblob` auth block".into())
    )?;
    if auth.account_name.is_empty() {
        return Err(AppError::AuthFailed("Azure Blob connection is missing account_name".into()));
    }
    if auth.secret_ref.is_empty() {
        return Err(AppError::AuthFailed("Azure Blob connection is missing secret_ref".into()));
    }
    let account_key = secrets::get(&auth.secret_ref)
        .map_err(|e| AppError::AuthFailed(format!("Azure secret lookup ({}): {e}", auth.secret_ref)))?
        .ok_or_else(|| AppError::AuthFailed(format!(
            "Azure secret missing from keyring ({}) — re-enter the account key in the connection editor",
            auth.secret_ref
        )))?;

    let mut builder = services::Azblob::default()
        .container(bucket)
        .account_name(&auth.account_name)
        .account_key(&account_key);
    if let Some(endpoint) = auth.endpoint.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.endpoint(endpoint);
    }

    Ok(Operator::new(builder).map_err(map_op_err)?.finish())
}

pub(crate) fn map_op_err(e: opendal::Error) -> AppError {
    AppError::Other(format!("opendal: {e}"))
}
