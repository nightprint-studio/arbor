//! Security dashboard — common types + provider-shared helpers.
//!
//! Mirrors the role of `ci_impl.rs` / `mr_impl.rs`: this module owns the
//! type definitions (`SecurityFinding`, `SecuritySummary`, `RiskScore`, ...)
//! and provider-agnostic utilities (severity mapping, host-side mediana,
//! GitLab GraphQL paginator). The trait impls in
//! `git_provider/{gitlab,github}/security.rs` consume these helpers — the
//! command layer never imports this module directly.

use chrono::{Duration, Utc};

use crate::error::{AppError, Result};
use crate::git_provider::ProviderKind;
use crate::git_provider::ci_impl::gitlab_send_with_refresh;

// ---------------------------------------------------------------------------
// Public types — defined in `corvus-git-provider-api`, re-exported here so the
// fetch logic + helpers below and external `security_impl::*` call sites keep
// resolving. Includes the `Severity` / `FindingState` enums (whose
// `from_gitlab` / `from_github_label` parsers the fetch code calls) and
// `MAX_FINDINGS_FETCH`.
// ---------------------------------------------------------------------------

// The wire DTOs + the pure computation helpers (`age_days_from_iso`, `median`,
// `medians_from_findings`, `compute_local_risk_score`, `risk_label`,
// `apply_filters`) live in `corvus-git-provider-api`; the GitLab/GitHub fetch
// code below calls them via this glob.
pub use corvus_git_provider_api::security::*;

// ---------------------------------------------------------------------------
// GitLab GraphQL implementation
// ---------------------------------------------------------------------------
//
// GitLab exposes the security dashboard data exclusively through GraphQL
// (REST coverage is sparse and inconsistent).  These functions are called
// from `gitlab/security.rs`, which is itself reached only through the trait
// dispatcher — the command layer never invokes them directly.

const GITLAB_FINDINGS_PAGE_SIZE: u32 = 100;

/// Lightweight probe: returns `true` when the GitLab project exposes
/// vulnerability data to the current user.  Issues a single GraphQL query
/// asking for `vulnerabilitySeveritiesCount` — present on Ultimate
/// (license-gated). Treats `null` and `403`-like errors as `false`.
pub async fn gitlab_supports_security(
    project_path: &str,
    base_url:     &str,
    token:        &str,
) -> Result<bool> {
    // Two independent nullable signals so a permission/availability error
    // on one doesn't blank out the whole `project` (GraphQL propagates
    // non-null field errors up to the nearest nullable parent — older
    // GitLab editions raise `undefinedField` on `userPermissions.readSecurityResource`,
    // which is why that probe path is intentionally avoided here):
    //   - `vulnerabilitySeveritiesCount` — Ultimate-gated counts object.
    //   - `vulnerabilities(first: 1) { nodes { id } }` — Ultimate-gated
    //     connection; returns an empty list rather than null when the user
    //     has access but no findings exist.
    // Either one being non-null flips the probe to `true`. `id` is queried
    // separately to distinguish "project not visible" from "security access
    // denied".
    let query = r#"
        query($fullPath: ID!) {
            project(fullPath: $fullPath) {
                id
                vulnerabilitySeveritiesCount { critical }
                vulnerabilities(first: 1) { nodes { id } }
            }
        }
    "#;
    let body = serde_json::json!({
        "query": query,
        "variables": { "fullPath": project_path },
    });

    let url = format!("{base_url}/api/graphql");
    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Content-Type", "application/json")
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        // 401/403/404 → no support; bubble other 5xx as an error.
        let status = resp.status().as_u16();
        if status == 401 || status == 403 || status == 404 {
            tracing::warn!(
                target: "arbor::security",
                "GitLab security probe HTTP {status} for project '{project_path}' — \
                 token may lack `api`/`read_api` scope or project does not exist"
            );
            return Ok(false);
        }
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab security probe {status}: {body}")));
    }

    let v: serde_json::Value = resp.json()
        .await
        .map_err(|e| AppError::Other(format!("GitLab security probe parse: {e}")))?;

    // Surface GraphQL `errors[]` so users can see why a field came back null
    // (commonly: "Field 'vulnerabilitySeveritiesCount' doesn't exist on type
    // 'Project'" on older GitLab editions, or permission denials).
    if let Some(errors) = v.get("errors").and_then(|e| e.as_array()) {
        if !errors.is_empty() {
            tracing::warn!(
                target: "arbor::security",
                "GitLab security probe GraphQL errors for project '{project_path}': {}",
                serde_json::to_string(errors).unwrap_or_default()
            );
        }
    }

    let project = v.pointer("/data/project");
    if project.map(|p| p.is_null()).unwrap_or(true) {
        tracing::warn!(
            target: "arbor::security",
            "GitLab security probe: project '{project_path}' resolved to null \
             (path mismatch, private project the token can't see, or auth)"
        );
        return Ok(false);
    }

    let counts_present = v.pointer("/data/project/vulnerabilitySeveritiesCount")
        .map(|x| !x.is_null())
        .unwrap_or(false);
    let vulns_present = v.pointer("/data/project/vulnerabilities")
        .map(|x| !x.is_null())
        .unwrap_or(false);

    let supported = counts_present || vulns_present;
    if !supported {
        tracing::warn!(
            target: "arbor::security",
            "GitLab security probe: project '{project_path}' has neither \
             vulnerabilitySeveritiesCount nor vulnerabilities — sidebar icon \
             will stay hidden. Raw response: {}",
            serde_json::to_string(&v).unwrap_or_default()
        );
    } else {
        tracing::debug!(
            target: "arbor::security",
            "GitLab security probe: project '{project_path}' supported \
             (counts={counts_present}, vulns={vulns_present})"
        );
    }
    Ok(supported)
}

/// Fetch all findings up to `MAX_FINDINGS_FETCH`, paginating with the
/// GitLab GraphQL `vulnerabilities(first: ..., after: ...)` connection.
///
/// Server-side filters: severity, state, report_type. Client-side filters
/// (search) are applied later via `apply_filters`.
pub async fn fetch_gitlab_security_findings(
    project_path: &str,
    base_url:     &str,
    token:        &str,
    filters:      &SecurityFilters,
) -> Result<(Vec<SecurityFinding>, bool)> {
    let limit = filters.limit
        .map(|l| l.min(MAX_FINDINGS_FETCH))
        .unwrap_or(MAX_FINDINGS_FETCH);

    let severities: Vec<&str>   = filters.severities.iter().map(|s| s.gitlab_enum()).collect();
    let states:     Vec<&str>   = filters.states.iter().map(|s| s.gitlab_enum()).collect();
    let report_types: Vec<String> = filters.report_types.iter()
        .map(|s| s.to_ascii_uppercase())
        .collect();

    let query = r#"
        query(
            $fullPath: ID!,
            $first: Int!,
            $after: String,
            $severity: [VulnerabilitySeverity!],
            $state: [VulnerabilityState!],
            $reportType: [VulnerabilityReportType!]
        ) {
            project(fullPath: $fullPath) {
                vulnerabilities(
                    first: $first,
                    after: $after,
                    severity: $severity,
                    state: $state,
                    reportType: $reportType
                ) {
                    pageInfo { hasNextPage endCursor }
                    nodes {
                        id
                        title
                        description
                        solution
                        severity
                        state
                        reportType
                        scanner { name }
                        location {
                            ... on VulnerabilityLocationSast { file startLine }
                            ... on VulnerabilityLocationSecretDetection { file startLine }
                            ... on VulnerabilityLocationDependencyScanning { file }
                            ... on VulnerabilityLocationContainerScanning { image }
                            ... on VulnerabilityLocationDast { hostname path }
                        }
                        identifiers { externalType name url }
                        webUrl
                        detectedAt
                    }
                }
            }
        }
    "#;

    let url = format!("{base_url}/api/graphql");
    let client = reqwest::Client::new();
    let mut out: Vec<SecurityFinding> = Vec::new();
    let mut after: Option<String> = None;
    let mut truncated = false;

    loop {
        let remaining = limit.saturating_sub(out.len() as u32);
        if remaining == 0 {
            truncated = true;
            break;
        }
        let page_size = remaining.min(GITLAB_FINDINGS_PAGE_SIZE);

        let mut vars = serde_json::json!({
            "fullPath": project_path,
            "first":    page_size,
            "after":    after,
        });
        if !severities.is_empty() {
            vars["severity"] = serde_json::json!(severities);
        }
        if !states.is_empty() {
            vars["state"] = serde_json::json!(states);
        }
        if !report_types.is_empty() {
            vars["reportType"] = serde_json::json!(report_types);
        }
        let body = serde_json::json!({ "query": query, "variables": vars });

        let resp = gitlab_send_with_refresh(
            |tok| client.post(&url)
                .header("Authorization", format!("Bearer {tok}"))
                .header("Content-Type", "application/json")
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body),
            base_url,
            token,
        ).await?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(AppError::Other(format!("GitLab vulnerabilities {s}: {b}")));
        }

        let v: serde_json::Value = resp.json()
            .await
            .map_err(|e| AppError::Other(format!("GitLab vulnerabilities parse: {e}")))?;

        if let Some(errs) = v.get("errors") {
            return Err(AppError::Other(format!("GitLab GraphQL errors: {errs}")));
        }

        let nodes = v.pointer("/data/project/vulnerabilities/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        for node in nodes {
            out.push(parse_gitlab_finding(&node));
            if out.len() as u32 >= limit {
                truncated = true;
                break;
            }
        }

        let page_info = v.pointer("/data/project/vulnerabilities/pageInfo");
        let has_next = page_info
            .and_then(|p| p.get("hasNextPage"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let end_cursor = page_info
            .and_then(|p| p.get("endCursor"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());

        if !has_next || (out.len() as u32) >= limit {
            if has_next && (out.len() as u32) >= limit { truncated = true; }
            break;
        }
        after = end_cursor;
        if after.is_none() { break; }
    }

    Ok((out, truncated))
}

fn parse_gitlab_finding(node: &serde_json::Value) -> SecurityFinding {
    let id = node.get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let title = node.get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let description = node.get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let solution = node.get("solution")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let severity = node.get("severity")
        .and_then(|v| v.as_str())
        .map(Severity::from_gitlab)
        .unwrap_or(Severity::Unknown);
    let state = node.get("state")
        .and_then(|v| v.as_str())
        .map(FindingState::from_gitlab)
        .unwrap_or(FindingState::Detected);
    let report_type = node.get("reportType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_ascii_lowercase());
    let scanner = node.pointer("/scanner/name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let web_url = node.get("webUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let created_at = node.get("detectedAt")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let age_days = age_days_from_iso(&created_at);

    let (file_path, start_line) = parse_gitlab_location(node.get("location"));

    let identifiers = node.get("identifiers")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(|i| FindingIdentifier {
            kind:  i.get("externalType").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            value: i.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            url:   i.get("url").and_then(|x| x.as_str()).map(|s| s.to_string()),
        }).collect())
        .unwrap_or_default();

    SecurityFinding {
        id,
        severity,
        state,
        title,
        description,
        scanner,
        report_type,
        file_path,
        start_line,
        web_url,
        created_at,
        age_days,
        identifiers,
        provider: ProviderKind::GitLab,
        solution,
    }
}

/// GitLab location is a union; extract a `(file_path, start_line)` pair from
/// whichever variant the API returned.
fn parse_gitlab_location(loc: Option<&serde_json::Value>) -> (Option<String>, Option<u32>) {
    let Some(loc) = loc else { return (None, None) };
    let file = loc.get("file")
        .and_then(|v| v.as_str())
        .or_else(|| loc.get("image").and_then(|v| v.as_str()))
        .or_else(|| loc.get("path").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let line = loc.get("startLine")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    (file, line)
}

/// Build a full `SecuritySummary` for a GitLab project.
///
/// Strategy:
///   1. Fire `vulnerabilitySeveritiesCount` for the headline counter grid.
///   2. Fetch a bounded slice of findings (capped at `MAX_FINDINGS_FETCH`)
///      to compute median ages host-side.
///   3. Try `vulnerabilitiesCountByDay` for the time-series chart; if the
///      project's GitLab plan doesn't include it (Ultimate-only), fall back
///      to `None` and let the UI render the gauge alone.
pub async fn fetch_gitlab_security_summary(
    project_path: &str,
    base_url:     &str,
    token:        &str,
    range_days:   u32,
) -> Result<SecuritySummary> {
    let counts = fetch_gitlab_severity_counts(project_path, base_url, token).await?;

    // Sample findings for the median computation. Restrict to open states
    // so the medians line up with the (active-only) counts above — a
    // resolved 600-day-old finding shouldn't drag the median for a
    // severity that's now empty in practice.
    let (findings, truncated) = fetch_gitlab_security_findings(
        project_path,
        base_url,
        token,
        &SecurityFilters {
            states: vec![FindingState::Detected, FindingState::Confirmed],
            limit:  Some(MAX_FINDINGS_FETCH),
            ..SecurityFilters::default()
        },
    ).await?;
    let medians = medians_from_findings(&findings);
    let findings_seen = findings.len() as u32;

    let time_series = fetch_gitlab_time_series(project_path, base_url, token, range_days)
        .await
        .ok()
        .flatten();

    // Use the local heuristic until we wire up `vulnerabilityHistoricalStatistics`
    // (Ultimate-only) — Phase 1 keeps the score consistent across plans.
    let risk_score = Some(compute_local_risk_score(&counts));

    let web_url = Some(format!("{base_url}/{project_path}/-/security/dashboard"));

    Ok(SecuritySummary {
        counts,
        median_age_days: medians,
        risk_score,
        time_series,
        provider_kind: ProviderKind::GitLab,
        web_url,
        findings_seen,
        truncated,
    })
}

async fn fetch_gitlab_severity_counts(
    project_path: &str,
    base_url:     &str,
    token:        &str,
) -> Result<SeverityCounts> {
    // The dashboard counts must reflect *open* findings only — managed
    // ones (Resolved / Dismissed) are noise for posture monitoring.  The
    // detail modal exposes a separate scope toggle to view those.
    let query = r#"
        query($fullPath: ID!, $state: [VulnerabilityState!]) {
            project(fullPath: $fullPath) {
                vulnerabilitySeveritiesCount(state: $state) {
                    critical high medium low info unknown
                }
            }
        }
    "#;
    let body = serde_json::json!({
        "query": query,
        "variables": {
            "fullPath": project_path,
            "state":    ["DETECTED", "CONFIRMED"],
        },
    });

    let url = format!("{base_url}/api/graphql");
    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Content-Type", "application/json")
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab severity counts {s}: {b}")));
    }

    let v: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab severity counts parse: {e}")))?;

    let node = v.pointer("/data/project/vulnerabilitySeveritiesCount");
    let mut c = SeverityCounts::default();
    if let Some(n) = node {
        c.critical = n.get("critical").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        c.high     = n.get("high")    .and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        c.medium   = n.get("medium")  .and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        c.low      = n.get("low")     .and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        c.info     = n.get("info")    .and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        c.unknown  = n.get("unknown") .and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    }
    Ok(c)
}

async fn fetch_gitlab_time_series(
    project_path: &str,
    base_url:     &str,
    token:        &str,
    range_days:   u32,
) -> Result<Option<VulnTimeSeries>> {
    let end = Utc::now().date_naive();
    let start = end - Duration::days(range_days as i64);

    let query = r#"
        query($fullPath: ID!, $startDate: ISO8601Date!, $endDate: ISO8601Date!) {
            project(fullPath: $fullPath) {
                vulnerabilitiesCountByDay(startDate: $startDate, endDate: $endDate) {
                    nodes { date critical high medium low info unknown }
                }
            }
        }
    "#;
    let body = serde_json::json!({
        "query": query,
        "variables": {
            "fullPath":  project_path,
            "startDate": start.format("%Y-%m-%d").to_string(),
            "endDate":   end.format("%Y-%m-%d").to_string(),
        },
    });

    let url = format!("{base_url}/api/graphql");
    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Content-Type", "application/json")
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let v: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab time series parse: {e}")))?;

    // GraphQL permission failures show up as `errors[]` + `data.project.vulnerabilitiesCountByDay = null`.
    // Either case → graceful `None`.
    if v.get("errors").is_some() {
        return Ok(None);
    }
    let nodes = match v.pointer("/data/project/vulnerabilitiesCountByDay/nodes") {
        Some(serde_json::Value::Array(a)) => a.clone(),
        _ => return Ok(None),
    };

    let points: Vec<TimePoint> = nodes.iter().map(|n| TimePoint {
        date:     n.get("date")    .and_then(|v| v.as_str()).unwrap_or("").to_string(),
        critical: n.get("critical").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        high:     n.get("high")    .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        medium:   n.get("medium")  .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        low:      n.get("low")     .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        info:     n.get("info")    .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        unknown:  n.get("unknown") .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    }).collect();

    if points.is_empty() {
        return Ok(None);
    }
    Ok(Some(VulnTimeSeries { points, range_days }))
}

