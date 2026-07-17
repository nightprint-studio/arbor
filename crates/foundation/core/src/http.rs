//! `reqwest` clients pre-configured for Arbor.
//!
//! Centralizes the defaults that used to vary across modules
//! (timeouts of 0/20/30 seconds, user-agents of `"arbor-git-gui"` /
//! `"Arbor-Git-GUI/1.0"` / none) so the wire shape Arbor presents to
//! third-party APIs is uniform.
//!
//! Two flavours, because one timeout policy cannot serve both:
//! - [`client`] — **API calls**. A total deadline ([`DEFAULT_TIMEOUT`]) bounds the
//!   whole request so a hung endpoint can't wedge a caller waiting on a join.
//! - [`download_client`] — **bulk transfers**. NO total deadline (a multi-GB body
//!   legitimately takes minutes), bounded instead by connect + idle timeouts.
//!
//! Picking the wrong one is a real bug, not a style nit: `reqwest`'s
//! `ClientBuilder::timeout` is a deadline on the *entire* request — connect through
//! the last body byte — so an API client used for a large streaming download aborts
//! mid-body the moment the clock runs out, surfacing as a confusing
//! `error decoding response body` rather than a timeout.

use std::time::Duration;

/// User-agent sent by every Arbor HTTP request that goes through this module.
/// Version tracks the workspace package version automatically.
pub const USER_AGENT: &str = concat!("Arbor-Git-GUI/", env!("CARGO_PKG_VERSION"));

/// Total-request deadline for [`client`]. Long enough for slow GitLab self-hosted
/// instances, short enough that a hung connection doesn't wedge the UI thread
/// waiting on a join. Applies connect-through-body, so it is only correct for
/// responses that are small and bounded — see [`download_client`] for transfers.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Connection-establishment budget for [`download_client`]. Covers DNS + TCP + TLS
/// only, so an unreachable host still fails fast even though the transfer itself is
/// unbounded.
pub const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Idle budget for [`download_client`]: the longest a transfer may go without any
/// bytes arriving. Bounds a *stalled* connection without capping total duration, so
/// a 4 GB pack on a slow link keeps going while a dead socket still errors out.
pub const DOWNLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Builder with the wire identity every Arbor request shares and NO timeout policy —
/// each public flavour layers its own on top. Private: callers must choose
/// [`client_builder`] or [`download_client_builder`] so the choice stays explicit.
fn base_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().user_agent(USER_AGENT)
}

/// Pre-built [`reqwest::Client`] for **API calls** (bounded by [`DEFAULT_TIMEOUT`]).
/// For downloading a file, use [`download_client`] instead — this client's total
/// deadline would abort the body mid-stream.
///
/// Panics only if `reqwest` fails to construct a client from a fully-default
/// builder, which in practice means the TLS backend failed to initialise —
/// in that case the app is already unusable and a default fallback would
/// just defer the failure. Callers that need finer control (e.g. self-signed
/// cert acceptance for Jira Data Center) should use [`client_builder`] and
/// build the client themselves.
pub fn client() -> reqwest::Client {
    client_builder()
        .build()
        .expect("reqwest client with default config")
}

/// Pre-configured [`reqwest::ClientBuilder`] for API calls, for further
/// customization. Already sets [`USER_AGENT`] and [`DEFAULT_TIMEOUT`]; the caller
/// layers on top.
pub fn client_builder() -> reqwest::ClientBuilder {
    base_builder().timeout(DEFAULT_TIMEOUT)
}

/// Pre-built [`reqwest::Client`] for **bulk transfers** (sample packs, models, any
/// streamed body that can run for minutes). Unbounded in total duration, bounded by
/// [`DOWNLOAD_CONNECT_TIMEOUT`] and [`DOWNLOAD_IDLE_TIMEOUT`].
///
/// Panics under the same (TLS-init) conditions as [`client`].
pub fn download_client() -> reqwest::Client {
    download_client_builder()
        .build()
        .expect("reqwest download client with default config")
}

/// Pre-configured [`reqwest::ClientBuilder`] for bulk transfers, for further
/// customization. Sets [`USER_AGENT`] plus the connect/idle budgets and
/// deliberately sets **no** total timeout.
pub fn download_client_builder() -> reqwest::ClientBuilder {
    base_builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_IDLE_TIMEOUT)
}
