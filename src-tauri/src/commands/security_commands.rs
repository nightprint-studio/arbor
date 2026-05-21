//! Security dashboard — Tauri command layer.
//!
//! **Architecture rule (NON-NEGOZIABILE)**: every command in this file goes
//! through the `GitProvider` trait via `provider_for_tab`. We never import
//! `gitlab_*` / `github_*` impl modules directly; the trait is the single
//! extension point so adding a third provider (Bitbucket, Gitea) is zero
//! work at the command layer.
//!
//! The frontend talks to these commands; commands lock the repo registry
//! through `provider_for_tab`, then dispatch to whichever provider matched
//! the active tab's remote.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;
use crate::error::{AppError, Result};
use crate::git_provider::provider_for_tab;
use crate::git_provider::types::{
    SecuritySummary, SecurityFinding, SecurityFilters,
    error::ProviderError,
};

fn pe(e: ProviderError) -> AppError {
    // Surface `Unsupported` as a plain string so the frontend can branch
    // on it without parsing variants — same convention as the CI commands.
    AppError::Other(e.to_string())
}

/// Probe whether the active repo's remote provider exposes a security
/// dashboard for the current user. Lightweight (single GraphQL query for
/// GitLab, instant `false` for GitHub until Phase 6).
///
/// Returns `false` when:
///   - the tab has no GitHub/GitLab remote
///   - no token is stored for the matched host
///   - the provider responds with 401/403/404 to the probe
#[tauri::command]
pub async fn supports_security(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<bool> {
    let resolved = match provider_for_tab(&state, &tab_id) {
        Ok(r)  => r,
        // No remote / no provider registered → not supported, no error.
        Err(_) => return Ok(false),
    };
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
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
#[tauri::command]
pub async fn fetch_security_summary(
    state:      State<'_, AppState>,
    tab_id:     String,
    range_days: u32,
) -> Result<SecuritySummary> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let summary = resolved.provider
        .fetch_security_summary(&resolved.repo, range_days)
        .await
        .map_err(pe)?;

    // Fire-and-forget hook — never let a misbehaving plugin block the
    // dashboard load. The payload mirrors the catalog entry in
    // `plugin/hook_catalog.rs`.
    if let Ok(host) = state.lock_plugin_host() {
        let total = summary.counts.total();
        let ctx = serde_json::json!({
            "tab_id":     tab_id,
            "provider":   match summary.provider_kind {
                crate::git_provider::ProviderKind::GitHub => "github",
                crate::git_provider::ProviderKind::GitLab => "gitlab",
                _ => "unknown",
            },
            "counts":     summary.counts,
            "total":      total,
            "risk_label": summary.risk_score.as_ref().map(|r| r.label.as_str()),
            "web_url":    summary.web_url,
        });
        let _ = host.fire_hook("on_security_summary_loaded", &ctx.to_string());
    }

    Ok(summary)
}

/// Fetch the detailed findings list for the active tab's repo.
/// Server-side filters: severity / state / report_type. Host-side: the
/// `search` substring is applied to title + file_path inside each provider.
#[tauri::command]
pub async fn fetch_security_findings(
    state:   State<'_, AppState>,
    tab_id:  String,
    filters: SecurityFilters,
) -> Result<Vec<SecurityFinding>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
        .fetch_security_findings(&resolved.repo, filters)
        .await
        .map_err(pe)
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

fn finish_export_job(app: &AppHandle, job_id: &str, success: bool, message: &str) {
    let state = app.state::<crate::AppState>();
    if let Ok(mut jobs) = state.jobs.lock() {
        let status = if success {
            crate::jobs::JobStatus::Completed { exit_code: 0 }
        } else {
            crate::jobs::JobStatus::Failed { error: message.to_string() }
        };
        jobs.set_status(job_id, status);
    }
    let _ = app.emit("arbor://job-done", serde_json::json!({
        "job_id":    job_id,
        "success":   success,
        "exit_code": if success { 0i32 } else { -1i32 },
        "cancelled": false,
    }));
    let (title, level) = if success {
        ("Security export complete", "success")
    } else {
        ("Security export failed", "error")
    };
    let _ = app.emit("plugin:notification", serde_json::json!({
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
#[tauri::command]
pub async fn export_security_report(
    state:       State<'_, AppState>,
    app:         AppHandle,
    tab_id:      String,
    output_path: String,
    format:      String,
    theme:       Option<crate::git_provider::security_export::ThemeTokens>,
) -> Result<String> {
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};

    if format != "html" && format != "csv" {
        return Err(AppError::Other(format!(
            "Unknown export format '{format}'. Expected 'html' or 'csv'."
        )));
    }

    // Resolve provider on the calling thread so we can return early on
    // "no remote / no token" without registering a job that would never run.
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    // Repo display name (workdir folder; falls back to tab_id).
    let repo_name = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .unwrap_or_else(|| repo.inner().path())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tab_id.clone())
    };

    let job_id = {
        let mut jobs = state.jobs.lock()
            .map_err(|_| AppError::Other("mutex poisoned".into()))?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            format!("Export Security Report as {}", format.to_uppercase()),
            plugin_name:     "arbor".into(),
            command:         format!("→ {output_path}"),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("Export".into()),
            non_cancellable: true,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
        });
        id
    };

    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        format!("Export Security Report as {}", format.to_uppercase()),
        "plugin_name": "arbor",
        "command":     format!("→ {output_path}"),
        "category":    "Export",
    }));

    let logo_override = state.branding.snapshot().logo_svg;
    let theme_tokens  = theme.unwrap_or_default();
    let provider = resolved.provider.clone();
    let repo_ref = resolved.repo.clone();
    let jid = job_id.clone();
    let ah  = app.clone();
    let out = output_path.clone();
    let fmt = format.clone();

    tokio::spawn(async move {
        let emit_line = |line: &str| {
            let s = ah.state::<crate::AppState>();
            if let Ok(mut jobs) = s.jobs.lock() {
                jobs.append_output(&jid, line.to_string());
            }
            let _ = ah.emit("arbor://job-output", serde_json::json!({
                "job_id": &jid,
                "text":   line,
            }));
        };

        emit_line("Fetching security summary…");
        let summary = match provider.fetch_security_summary(&repo_ref, 30).await {
            Ok(s)  => s,
            Err(e) => {
                let err = format!("Failed to fetch summary: {e}");
                emit_line(&format!("[error] {err}"));
                finish_export_job(&ah, &jid, false, &err);
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
                finish_export_job(&ah, &jid, false, &err);
                return;
            }
        };

        emit_line(&format!("Writing {fmt} export…"));
        let path = std::path::PathBuf::from(&out);
        match crate::git_provider::security_export::export_to_file(
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
                finish_export_job(&ah, &jid, true, &ok_msg);
            }
            Err(e) => {
                emit_line(&format!("[error] {e}"));
                finish_export_job(&ah, &jid, false, &e);
            }
        }
    });

    Ok(job_id)
}
