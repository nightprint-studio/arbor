//! Wire types shared between Tauri commands, the Lua namespace and opendal.
//!
//! These are deliberately serde-friendly and provider-agnostic at the top
//! level: a single `CloudConnection` covers GCS today and is shaped so
//! adding S3/Azure later is just two enum variants and two structs.

use serde::{Deserialize, Serialize};

// ── Provider tag ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gcs,
    S3,
    Azblob,
}

// ── GCS auth ────────────────────────────────────────────────────────────────

/// How to obtain credentials for a GCS connection.
///
/// `sa_inline` and `oauth` store their secret material in the OS keyring,
/// referenced by an opaque `secret_ref` (e.g. `"gcs/cfg_abc"`). The plugin
/// is responsible for choosing & persisting that ref; the host just looks
/// it up via `crate::secrets`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum GcsAuth {
    /// Service-account JSON on disk. `path` must be absolute.
    SaFile { path: String },
    /// Service-account JSON stored in keyring under `secret_ref`.
    SaInline { secret_ref: String },
    /// Application Default Credentials — discovered from
    /// `GOOGLE_APPLICATION_CREDENTIALS` env var or
    /// `~/.config/gcloud/application_default_credentials.json`.
    Adc,
    /// `gcloud auth print-access-token` — spawns the CLI, caches token ~50min.
    GcloudCli,
    /// Installed-app OAuth (loopback :7732 + PKCE). Refresh token stored in
    /// keyring under `secret_ref` as JSON `{ "refresh_token": "...", ... }`.
    Oauth { secret_ref: String },
}

// ── Connection envelope ─────────────────────────────────────────────────────

/// Everything the host needs to perform one cloud op. Sent fresh on every
/// call — the host never persists this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConnection {
    pub provider: Provider,
    /// Stable id the plugin chose for this connection (used for keyring
    /// scoping and progress events). Opaque to the host.
    #[serde(default)]
    pub config_id: String,
    /// Optional GCP project id (some ops require it). For non-GCS providers
    /// this is ignored.
    #[serde(default)]
    pub project_id: Option<String>,
    /// GCS auth — required when `provider == Gcs`.
    #[serde(default)]
    pub gcs: Option<GcsAuth>,
    // ── S3 / Azure stubs for forward-compat. Host accepts them but the v1
    // UI only exposes GCS, so these never arrive in practice. The opendal
    // builders for s3/azblob already exist behind feature flags — wiring is
    // trivial when the plugin UI catches up.
    #[serde(default)]
    pub s3: Option<S3Auth>,
    #[serde(default)]
    pub azblob: Option<AzBlobAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Auth {
    pub access_key_id:      String,
    /// Held in keyring under `secret_ref` — never sent inline by the UI.
    pub secret_ref:         String,
    pub region:             Option<String>,
    pub endpoint:           Option<String>,
    pub force_path_style:   Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AzBlobAuth {
    pub account_name: String,
    /// Account key in keyring.
    pub secret_ref:   String,
    pub endpoint:     Option<String>,
}

// ── Listing / objects ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudObject {
    /// Key relative to the bucket (e.g. `"folder/sub/file.bin"`). Folders
    /// end with `"/"`.
    pub path:          String,
    pub is_dir:        bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size:          Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag:          Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type:  Option<String>,
    /// ISO-8601 UTC string, e.g. `"2026-05-11T15:30:00Z"`. Optional — some
    /// providers omit it on prefixes / freshly-created objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudListPage {
    pub items:     Vec<CloudObject>,
    /// True when the listing was capped at `limit` — there may be more
    /// objects under this prefix that we didn't return. v1 does not expose
    /// real page tokens because opendal abstracts that detail away; the
    /// plugin should warn the user and offer to refine the prefix.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudTestReport {
    pub ok:           bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error:        Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method:  Option<String>,
    /// Best-effort identity surfaced by the auth flow (SA email, OAuth
    /// user email, …) — handy in the UI but never required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity:     Option<String>,
}

// ── Progress event payload ──────────────────────────────────────────────────

/// Emitted on `arbor://cloud-progress` from a transfer/sync job. The plugin
/// listens and renders a progress bar; the JobOutputPanel surfaces a
/// human-readable line per chunk separately via the host's job sink.
#[derive(Debug, Clone, Serialize)]
pub struct CloudProgress {
    pub job_id:     String,
    pub config_id:  String,
    pub kind:       &'static str, // "download" | "upload" | "sync"
    pub bucket:     String,
    pub path:       String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Bytes/second over a rolling ~1s window.
    pub speed_bps:  u64,
    /// Best-effort ETA in seconds, computed from current speed. `None` when
    /// total is unknown or speed is 0.
    pub eta_sec:    Option<u64>,
}
