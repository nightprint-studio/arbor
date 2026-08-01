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
//! **Hooks fire here**: `fetch_security_summary` fires `corvus:security_summary_loaded`
//! fire-and-forget to the co-located host — copied byte-identically from the
//! shell's in-process command (same `provider_kind` match arms, same field order,
//! same `summary.counts.total()`). No other method in this domain fires a hook.
//!
//! **`export_security_report`** runs here too. It registers a background job in
//! the shell's single-source registry over the reverse channel ([`JobHandle`]),
//! snapshots the branding logo via the `__branding_logo` host call (the OOP twin
//! of the shell's `state.branding.snapshot()`), reads the repo display name from
//! the libgit2 handle corvus-be already opens, and spawns a detached
//! `tokio::spawn` whose progress/terminal egress goes through
//! [`CorvusState::event_sink`]. The `arbor://job-started` / `arbor://job-output`
//! / `arbor://job-done` payloads and the `plugin:notification` toast are
//! byte-identical to the shell's in-process command.

use corvus_core::prelude::{hooks, CorvusState};
use corvus_git_provider_api::prelude::{
    export_to_file, ProviderKind, SecurityFilters, SecurityFinding, SecuritySummary, ThemeTokens,
};

use crate::jobs::JobHandle;
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
/// Fires `corvus:security_summary_loaded` on success so plugins can react to
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
        hooks::SECURITY_SUMMARY_LOADED,
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

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

use std::sync::Arc;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::EventSink;

/// Mark an export job done and emit the completion event + notification toast.
/// Drives the shell's single-source registry through [`JobHandle::set_status`]
/// and emits via the captured event sink — byte-identical to the shell's
/// in-process `finish_export_job`.
fn finish_export_job(
    sink: &Arc<dyn EventSink>,
    job: &JobHandle,
    success: bool,
    message: &str,
) {
    let status = if success {
        JobStatus::Completed { exit_code: 0 }
    } else {
        JobStatus::Failed { error: message.to_string() }
    };
    job.set_status(status);
    sink.emit("arbor://job-done", serde_json::json!({
        "job_id":    job.id,
        "success":   success,
        "exit_code": if success { 0i32 } else { -1i32 },
        "cancelled": false,
    }));
    let (title, level) = if success {
        ("Security export complete", "success")
    } else {
        ("Security export failed", "error")
    };
    sink.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   title,
        "message": message,
        "level":   level,
    }));
}

/// Export the active tab's security posture to a self-contained HTML
/// report or a flat CSV.
///
/// Returns a job-id immediately; the export runs in a background task
/// (network fetch + file write). Emits `arbor://job-started`,
/// `arbor://job-output`, `arbor://job-done` and `plugin:notification` so
/// the export shows up in the Jobs overlay alongside other system jobs.
///
/// `format` is `"html"` or `"csv"`. The HTML report mirrors the in-app
/// dashboard (counter grid + risk gauge + time-series chart + findings
/// table); the CSV is raw rows only — no summary banner.
#[arbor_rpc::handler]
async fn export_security_report(
    state: &CorvusState,
    tab_id: String,
    output_path: String,
    format: String,
    theme: Option<ThemeTokens>,
) -> Result<String, String> {
    if format != "html" && format != "csv" {
        return Err(format!(
            "Unknown export format '{format}'. Expected 'html' or 'csv'."
        ));
    }

    // Resolve provider on the calling thread so we can return early on
    // "no remote / no token" without registering a job that would never run.
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);

    // The reverse channel: the job registry lives in the shell, and the
    // branding logo is read with a host call. `None` only in-process, where
    // the `SplitBroker` would have routed to the in-process copy instead.
    let host = state
        .host_caller()
        .ok_or_else(|| "host caller unavailable".to_string())?;
    let sink = state.event_sink();

    // Repo display name (workdir folder; falls back to tab_id) — read from the
    // libgit2 handle corvus-be already opens for this tab.
    let repo_name = {
        let repo = crate::repo::open(state, &tab_id)?;
        repo.workdir()
            .unwrap_or_else(|| repo.path())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tab_id.clone())
    };

    let job = JobHandle::register(Arc::clone(&host), JobSpec {
        name:            format!("Export Security Report as {}", format.to_uppercase()),
        plugin_name:     "arbor".into(),
        command:         format!("→ {output_path}"),
        category:        Some("Export".into()),
        non_cancellable: true,
        hidden:          false,
        is_system:       true,
        target:          None,
    })?;
    let job_id = job.id.clone();

    sink.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        format!("Export Security Report as {}", format.to_uppercase()),
        "plugin_name": "arbor",
        "command":     format!("→ {output_path}"),
        "category":    "Export",
    }));

    // Snapshot the branding logo override over the reverse channel — the OOP
    // twin of `state.branding.snapshot().logo_svg`.
    let logo_override: Option<String> = host
        .call("__branding_logo", serde_json::Value::Null)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok());
    let theme_tokens = theme.unwrap_or_default();
    let provider = resolved.provider.clone();
    let repo_ref = resolved.repo.clone();
    let out = output_path.clone();
    let fmt = format.clone();
    let sink_bg = Arc::clone(&sink);

    tokio::spawn(async move {
        let emit_line = |line: &str| {
            job.append(line);
            sink_bg.emit("arbor://job-output", serde_json::json!({
                "job_id": &job.id,
                "text":   line,
            }));
        };

        emit_line("Fetching security summary…");
        let summary = match provider.fetch_security_summary(&repo_ref, 30).await {
            Ok(s)  => s,
            Err(e) => {
                let err = format!("Failed to fetch summary: {e}");
                emit_line(&format!("[error] {err}"));
                finish_export_job(&sink_bg, &job, false, &err);
                return;
            }
        };

        emit_line("Fetching findings…");
        let findings = match provider
            .fetch_security_findings(&repo_ref, SecurityFilters::default())
            .await
        {
            Ok(v)  => v,
            Err(e) => {
                let err = format!("Failed to fetch findings: {e}");
                emit_line(&format!("[error] {err}"));
                finish_export_job(&sink_bg, &job, false, &err);
                return;
            }
        };

        emit_line(&format!("Writing {fmt} export…"));
        let path = std::path::PathBuf::from(&out);
        match export_to_file(
            &summary,
            &findings,
            &path,
            &fmt,
            &repo_name,
            logo_override.as_deref(),
            &theme_tokens,
        ) {
            Ok(()) => {
                let ok_msg = format!("Security report exported to '{out}'.");
                emit_line(&ok_msg);
                finish_export_job(&sink_bg, &job, true, &ok_msg);
            }
            Err(e) => {
                emit_line(&format!("[error] {e}"));
                finish_export_job(&sink_bg, &job, false, &e);
            }
        }
    });

    Ok(job_id)
}
