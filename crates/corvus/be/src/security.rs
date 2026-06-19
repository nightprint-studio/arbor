//! `security` domain — code-scanning / security-findings handlers, served
//! **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::security`), but the context is [`CorvusState`] and
//! the provider comes from the reverse-channel registry ([`crate::provider`])
//! instead of the shell's `GitProviderRegistry`. The trait work is the shared
//! `corvus-git-provider-{api,github,gitlab}` crates, so the results and the
//! `ProviderError` wire strings are identical to in-process. Each method resolves
//! the provider via `provider_for_tab` (a brief sync lock returning owned `Arc`s
//! — no guard held across the `.await`), then `.await`s the provider's
//! REST/GraphQL call. [`crate::provider::maybe_refresh`] keeps the OAuth token
//! fresh over the reverse channel before each round-trip (the OOP twin of the
//! shell's `maybe_refresh_for_provider`).
//!
//! **Hooks fire here**: `fetch_security_summary` fires `on_security_summary_loaded`
//! fire-and-forget to the co-located host — copied byte-identically from the
//! shell's in-process command (same `provider_kind` match arms, same field order,
//! same `summary.counts.total()`). No other method in this domain fires a hook.
//!
//! **Left in-process**: `export_security_report`. It registers a background job
//! in the job registry (`state.jobs`), snapshots the branding logo
//! (`state.branding.snapshot()`), reads the repo display name via
//! `state.lock_repos()`, and spawns a detached `tokio::spawn` capturing the
//! shell's event sink — none of which `CorvusState` exposes. The `SplitBroker`
//! routes per-method, so it keeps running in the shell transparently.

use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::{
    ProviderKind, SecurityFilters, SecurityFinding, SecuritySummary,
};

use crate::provider::{maybe_refresh, pe, provider_for_tab};

/// Probe whether the active repo's remote provider exposes a security
/// dashboard for the current user. Lightweight (single GraphQL query for
/// GitLab, instant `false` for GitHub until Phase 6).
///
/// Returns `false` when:
///   - the tab has no GitHub/GitLab remote
///   - no token is stored for the matched host
///   - the provider responds with 401/403/404 to the probe
#[arbor_rpc::handler]
async fn supports_security(state: &CorvusState, tab_id: String) -> Result<bool, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r) => r,
        // No remote / no provider registered → not supported, no error.
        Err(_) => return Ok(false),
    };
    maybe_refresh(&resolved.info.provider);
    resolved
        .provider
        .supports_security(&resolved.repo)
        .await
        .map_err(pe)
}

/// Fetch the headline summary (counter grid + risk score + optional
/// time-series) for the active tab's repo. `range_days` controls the
/// vulnerabilities-over-time window; the GitLab impl tolerates anything
/// up to 90 days, but the frontend exposes only 30/60/90.
///
/// Fires `on_security_summary_loaded` on success so plugins can react to
/// posture changes (notifications, dashboards, external trackers).
#[arbor_rpc::handler]
async fn fetch_security_summary(
    state: &CorvusState,
    tab_id: String,
    range_days: u32,
) -> Result<SecuritySummary, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let summary = resolved
        .provider
        .fetch_security_summary(&resolved.repo, range_days)
        .await
        .map_err(pe)?;

    // Fire-and-forget hook — never let a misbehaving plugin block the
    // dashboard load. The payload mirrors the catalog entry in
    // `arbor-plugin-types::hook_catalog`.
    let total = summary.counts.total();
    state.fire_hook(
        "on_security_summary_loaded",
        serde_json::json!({
            "tab_id":     tab_id,
            "provider":   match summary.provider_kind {
                ProviderKind::GitHub => "github",
                ProviderKind::GitLab => "gitlab",
                _ => "unknown",
            },
            "counts":     summary.counts,
            "total":      total,
            "risk_label": summary.risk_score.as_ref().map(|r| r.label.as_str()),
            "web_url":    summary.web_url,
        }),
    );

    Ok(summary)
}

/// Fetch the detailed findings list for the active tab's repo.
/// Server-side filters: severity / state / report_type. Host-side: the
/// `search` substring is applied to title + file_path inside each provider.
#[arbor_rpc::handler]
async fn fetch_security_findings(
    state: &CorvusState,
    tab_id: String,
    filters: SecurityFilters,
) -> Result<Vec<SecurityFinding>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    resolved
        .provider
        .fetch_security_findings(&resolved.repo, filters)
        .await
        .map_err(pe)
}
