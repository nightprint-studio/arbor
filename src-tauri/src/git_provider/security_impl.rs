//! Security dashboard — common types + provider-shared helpers.
//!
//! Mirrors the role of `ci_impl.rs` / `mr_impl.rs`: this module owns the
//! type definitions (`SecurityFinding`, `SecuritySummary`, `RiskScore`, ...)
//! and provider-agnostic utilities (severity mapping, host-side mediana,
//! GitLab GraphQL paginator). The trait impls in
//! `git_provider/{gitlab,github}/security.rs` consume these helpers — the
//! command layer never imports this module directly.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::git_provider::ProviderKind;
use crate::git_provider::ci_impl::{gitlab_send_with_refresh, github_send_with_refresh};

// ---------------------------------------------------------------------------
// Severity / state enums
// ---------------------------------------------------------------------------

/// Six-level severity ladder. Matches GitLab's vocabulary (`CRITICAL`, `HIGH`,
/// `MEDIUM`, `LOW`, `INFO`, `UNKNOWN`); GitHub's `security_severity_level`
/// maps onto the same ladder via `Severity::from_github_label`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
    Unknown,
}

impl Severity {
    pub const ALL: [Severity; 6] = [
        Severity::Critical,
        Severity::High,
        Severity::Medium,
        Severity::Low,
        Severity::Info,
        Severity::Unknown,
    ];

    /// GraphQL enum literal expected by GitLab (`CRITICAL`, `HIGH`, ...).
    pub fn gitlab_enum(&self) -> &'static str {
        match self {
            Severity::Critical => "CRITICAL",
            Severity::High     => "HIGH",
            Severity::Medium   => "MEDIUM",
            Severity::Low      => "LOW",
            Severity::Info     => "INFO",
            Severity::Unknown  => "UNKNOWN",
        }
    }

    /// Parse the GitLab GraphQL enum back into a `Severity`.
    pub(crate) fn from_gitlab(s: &str) -> Severity {
        match s.to_ascii_uppercase().as_str() {
            "CRITICAL" => Severity::Critical,
            "HIGH"     => Severity::High,
            "MEDIUM"   => Severity::Medium,
            "LOW"      => Severity::Low,
            "INFO"     => Severity::Info,
            _          => Severity::Unknown,
        }
    }

    /// Best-effort mapping of GitHub's free-form severity strings.
    /// `security_severity_level` (code-scanning) and `security_advisory.severity`
    /// (Dependabot) both produce these labels.
    pub(crate) fn from_github_label(s: &str) -> Severity {
        match s.to_ascii_lowercase().as_str() {
            "critical"            => Severity::Critical,
            "high"                => Severity::High,
            "medium" | "moderate" => Severity::Medium,
            "low"                 => Severity::Low,
            "note"  | "info"      => Severity::Info,
            _                     => Severity::Unknown,
        }
    }

    /// Heuristic weight for the locally-computed risk score.
    pub fn risk_weight(&self) -> f32 {
        match self {
            Severity::Critical => 10.0,
            Severity::High     =>  5.0,
            Severity::Medium   =>  2.0,
            Severity::Low      =>  0.5,
            Severity::Info     =>  0.0,
            Severity::Unknown  =>  0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingState {
    Detected,
    Confirmed,
    Resolved,
    Dismissed,
}

impl FindingState {
    pub fn gitlab_enum(&self) -> &'static str {
        match self {
            FindingState::Detected  => "DETECTED",
            FindingState::Confirmed => "CONFIRMED",
            FindingState::Resolved  => "RESOLVED",
            FindingState::Dismissed => "DISMISSED",
        }
    }

    pub(crate) fn from_gitlab(s: &str) -> FindingState {
        match s.to_ascii_uppercase().as_str() {
            "CONFIRMED" => FindingState::Confirmed,
            "RESOLVED"  => FindingState::Resolved,
            "DISMISSED" => FindingState::Dismissed,
            _           => FindingState::Detected,
        }
    }
}

// ---------------------------------------------------------------------------
// Public types (shared across providers + frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingIdentifier {
    /// e.g. "CVE", "CWE", "OWASP", "GHSA"
    pub kind:  String,
    pub value: String,
    pub url:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub id:          String,
    pub severity:    Severity,
    pub state:       FindingState,
    pub title:       String,
    pub description: Option<String>,
    /// Tool that produced the finding (e.g. "semgrep", "gemnasium", "trivy",
    /// "codeql", "dependabot"). Omitted when the API doesn't expose it.
    pub scanner:     Option<String>,
    /// Category of the finding (e.g. `sast`, `dependency_scanning`,
    /// `container_scanning`, `secret_detection`, `dast`).
    pub report_type: Option<String>,
    pub file_path:   Option<String>,
    pub start_line:  Option<u32>,
    /// Direct URL to the finding in the provider's web UI.
    pub web_url:     Option<String>,
    /// ISO-8601 timestamp from the provider.
    pub created_at:  String,
    /// Computed host-side from `created_at` at fetch time.
    pub age_days:    u32,
    pub identifiers: Vec<FindingIdentifier>,
    pub provider:    ProviderKind,
    /// Suggested remediation text. GitLab populates this directly from the
    /// `Vulnerability.solution` GraphQL field; for GitHub Dependabot we
    /// synthesise a short hint from `first_patched_version` +
    /// `vulnerable_version_range`. Empty when the provider doesn't expose a
    /// fix recommendation (most code-scanning / secret-scanning alerts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution:    Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeverityCounts {
    pub critical: u32,
    pub high:     u32,
    pub medium:   u32,
    pub low:      u32,
    pub info:     u32,
    pub unknown:  u32,
}

impl SeverityCounts {
    pub fn total(&self) -> u32 {
        self.critical + self.high + self.medium + self.low + self.info + self.unknown
    }

    pub fn add(&mut self, sev: Severity) {
        match sev {
            Severity::Critical => self.critical += 1,
            Severity::High     => self.high     += 1,
            Severity::Medium   => self.medium   += 1,
            Severity::Low      => self.low      += 1,
            Severity::Info     => self.info     += 1,
            Severity::Unknown  => self.unknown  += 1,
        }
    }

    pub fn get(&self, sev: Severity) -> u32 {
        match sev {
            Severity::Critical => self.critical,
            Severity::High     => self.high,
            Severity::Medium   => self.medium,
            Severity::Low      => self.low,
            Severity::Info     => self.info,
            Severity::Unknown  => self.unknown,
        }
    }
}

/// Median age (in days) per severity. `None` when no findings exist for that
/// severity bucket — keeps the UI free to render an em-dash.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeverityMedians {
    pub critical: Option<u32>,
    pub high:     Option<u32>,
    pub medium:   Option<u32>,
    pub low:      Option<u32>,
    pub info:     Option<u32>,
    pub unknown:  Option<u32>,
}

impl SeverityMedians {
    pub fn set(&mut self, sev: Severity, median: u32) {
        match sev {
            Severity::Critical => self.critical = Some(median),
            Severity::High     => self.high     = Some(median),
            Severity::Medium   => self.medium   = Some(median),
            Severity::Low      => self.low      = Some(median),
            Severity::Info     => self.info     = Some(median),
            Severity::Unknown  => self.unknown  = Some(median),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskScore {
    /// Numeric value in `[0, 100]`. The exact scale is provider-dependent;
    /// for the host-side heuristic see `compute_local_risk_score`.
    pub value: f32,
    /// Friendly bucket label: `"Low"` | `"Medium"` | `"High"` | `"Critical"`.
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePoint {
    /// ISO-8601 date (no time component): `YYYY-MM-DD`.
    pub date:     String,
    pub critical: u32,
    pub high:     u32,
    pub medium:   u32,
    pub low:      u32,
    pub info:     u32,
    pub unknown:  u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnTimeSeries {
    pub points:     Vec<TimePoint>,
    pub range_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySummary {
    pub counts:          SeverityCounts,
    pub median_age_days: SeverityMedians,
    /// `None` when the provider doesn't expose a risk score (or the user's
    /// plan doesn't include it — e.g. GitLab without Ultimate).
    pub risk_score:      Option<RiskScore>,
    /// `None` when historical data is unavailable. Phase 1 always populates
    /// this for GitLab Ultimate; GitHub returns `None` until Phase 6.
    pub time_series:     Option<VulnTimeSeries>,
    pub provider_kind:   ProviderKind,
    /// URL to the provider-native dashboard, if any.
    pub web_url:         Option<String>,
    /// Total number of findings considered by the summary (capped, see
    /// `MAX_FINDINGS_FETCH`). The frontend uses this together with
    /// `truncated` to show "Showing N of M" hints.
    pub findings_seen:   u32,
    /// True when the host-side fetch hit `MAX_FINDINGS_FETCH` and stopped.
    pub truncated:       bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityFilters {
    pub severities:   Vec<Severity>,
    pub states:       Vec<FindingState>,
    pub report_types: Vec<String>,
    /// Host-side substring filter applied to title + file_path.
    pub search:       Option<String>,
    /// Hard cap on returned findings (defaults to `MAX_FINDINGS_FETCH`).
    pub limit:        Option<u32>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Hard upper bound on how many findings a single fetch will collect across
/// pagination. Above this, the UI shows a "refine filters" hint.
pub const MAX_FINDINGS_FETCH: u32 = 1000;

/// Risk-score weight ceiling used by the host-side heuristic. Tuned so a
/// repo with ~10 critical findings already sits in the "High risk" band.
const RISK_SCORE_CAP: f32 = 100.0;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Days elapsed between an ISO-8601 timestamp and `Utc::now()`.
/// Returns `0` when parsing fails or the timestamp is in the future.
pub(crate) fn age_days_from_iso(iso: &str) -> u32 {
    let Ok(t) = iso.parse::<DateTime<Utc>>() else { return 0 };
    let delta = Utc::now() - t;
    delta.num_days().max(0) as u32
}

/// Median (50th percentile) of a slice of u32 ages. `None` for an empty
/// slice. Uses simple sort+midpoint — input is bounded by
/// `MAX_FINDINGS_FETCH` so allocation cost is irrelevant.
pub(crate) fn median(ages: &[u32]) -> Option<u32> {
    if ages.is_empty() { return None }
    let mut sorted = ages.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    Some(if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    })
}

/// Build a `SeverityMedians` from a list of findings by bucketing ages.
pub(crate) fn medians_from_findings(findings: &[SecurityFinding]) -> SeverityMedians {
    let mut out = SeverityMedians::default();
    for sev in Severity::ALL {
        let ages: Vec<u32> = findings.iter()
            .filter(|f| f.severity == sev)
            .map(|f| f.age_days)
            .collect();
        if let Some(m) = median(&ages) {
            out.set(sev, m);
        }
    }
    out
}

/// Local heuristic risk score in `[0, 100]`:
///   `min(100, sum(count[s] * weight[s]) * 100 / cap)`.
/// Used by GitHub (no native score) and as a fallback when GitLab Ultimate
/// historical statistics aren't available.
pub(crate) fn compute_local_risk_score(counts: &SeverityCounts) -> RiskScore {
    let raw = counts.critical as f32 * Severity::Critical.risk_weight()
            + counts.high     as f32 * Severity::High.risk_weight()
            + counts.medium   as f32 * Severity::Medium.risk_weight()
            + counts.low      as f32 * Severity::Low.risk_weight();
    let pct = (raw * 100.0 / RISK_SCORE_CAP).clamp(0.0, 100.0);
    RiskScore { value: pct, label: risk_label(pct).to_string() }
}

pub(crate) fn risk_label(value: f32) -> &'static str {
    if value >= 75.0 { "Critical" }
    else if value >= 50.0 { "High" }
    else if value >= 25.0 { "Medium" }
    else { "Low" }
}

/// Apply a `SecurityFilters` host-side. Used as a final pass after the
/// provider has returned its server-filtered findings — guarantees that the
/// `search` clause (which providers don't natively support) is honored.
pub(crate) fn apply_filters(findings: Vec<SecurityFinding>, filters: &SecurityFilters) -> Vec<SecurityFinding> {
    let needle = filters.search.as_deref().map(|s| s.to_ascii_lowercase());
    findings.into_iter()
        .filter(|f| {
            if !filters.severities.is_empty() && !filters.severities.contains(&f.severity) {
                return false;
            }
            if !filters.states.is_empty() && !filters.states.contains(&f.state) {
                return false;
            }
            if !filters.report_types.is_empty() {
                let ok = match &f.report_type {
                    Some(r) => filters.report_types.iter().any(|t| t.eq_ignore_ascii_case(r)),
                    None    => false,
                };
                if !ok { return false; }
            }
            if let Some(n) = &needle {
                let title_hit = f.title.to_ascii_lowercase().contains(n);
                let path_hit  = f.file_path.as_deref()
                    .map(|p| p.to_ascii_lowercase().contains(n))
                    .unwrap_or(false);
                if !title_hit && !path_hit { return false; }
            }
            true
        })
        .collect()
}

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

// ---------------------------------------------------------------------------
// GitHub REST implementation
// ---------------------------------------------------------------------------
//
// GitHub doesn't expose a single "security dashboard" endpoint — we
// aggregate three independent alert streams (code-scanning,
// secret-scanning, dependabot) into the same `SecuritySummary` shape used
// by the GitLab branch. Each stream may be unavailable on a given repo
// (private repo without GHAS, missing scopes, feature disabled): we
// surface those as empty lists rather than hard failures so partial
// coverage still produces a useful summary.

const GITHUB_PAGE_SIZE: u32 = 100;
const GITHUB_API_BASE: &str = "https://api.github.com";

/// Best-effort probe for GitHub security data. The plan calls for
/// code-scanning as the primary signal, but many repos rely solely on
/// Dependabot alerts — so we fall back to a Dependabot probe when
/// code-scanning isn't available. Returns `true` if either endpoint
/// answers `200 OK`.
pub async fn github_supports_security(
    owner: &str,
    repo:  &str,
    token: &str,
) -> Result<bool> {
    // Independent REST endpoints — one per source — so a feature being off
    // (or the token missing the matching scope) on one path doesn't mask
    // the others. Order: code-scanning → Dependabot → secret-scanning.
    let cs = github_endpoint_available(
        &format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/code-scanning/alerts?per_page=1"),
        token,
    ).await?;
    if cs { return Ok(true); }

    let db = github_endpoint_available(
        &format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/dependabot/alerts?per_page=1"),
        token,
    ).await?;
    if db { return Ok(true); }

    let ss = github_endpoint_available(
        &format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/secret-scanning/alerts?per_page=1"),
        token,
    ).await?;
    if ss { return Ok(true); }

    tracing::warn!(
        target: "arbor::security",
        "GitHub security probe: '{owner}/{repo}' had no accessible source \
         (code-scanning, Dependabot, secret-scanning all unavailable) — \
         sidebar icon will stay hidden. Token likely lacks `security_events` / \
         `repo` scope, GHAS is off for the repo, or all sources are disabled."
    );
    Ok(false)
}

/// `true` for `200`; `false` for the usual "feature unavailable" responses
/// (`401`/`403`/`404`/`410`); error for unexpected `5xx`. Logs the exact
/// status on every unavailable response so the operator can tell scope-miss
/// (`401/403`) from feature-off (`404/410`).
async fn github_endpoint_available(url: &str, token: &str) -> Result<bool> {
    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client.get(url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0"),
        token,
    ).await?;

    let status = resp.status();
    if status.is_success() { return Ok(true); }
    if matches!(status.as_u16(), 401 | 403 | 404 | 410) {
        tracing::debug!(
            target: "arbor::security",
            "GitHub security probe: {url} → HTTP {} (treating as unavailable)",
            status.as_u16()
        );
        return Ok(false);
    }
    let body = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitHub security probe {status}: {body}")))
}

/// Fetch and unify findings across code-scanning, secret-scanning, and
/// Dependabot. Each source is fetched in parallel; an unavailable source
/// (403/404) contributes zero findings without aborting the whole call.
pub async fn fetch_github_security_findings(
    owner:   &str,
    repo:    &str,
    token:   &str,
    filters: &SecurityFilters,
) -> Result<(Vec<SecurityFinding>, bool)> {
    let limit = filters.limit
        .map(|l| l.min(MAX_FINDINGS_FETCH))
        .unwrap_or(MAX_FINDINGS_FETCH);
    // Per-source budget so one noisy stream can't crowd out the others.
    let per_source = limit.div_ceil(3);

    let cs_fut = fetch_github_code_scanning(owner, repo, token, per_source);
    let sc_fut = fetch_github_secret_scanning(owner, repo, token, per_source);
    let db_fut = fetch_github_dependabot(owner, repo, token, per_source);

    let (cs, sc, db) = tokio::join!(cs_fut, sc_fut, db_fut);

    let mut all = Vec::new();
    let mut truncated = false;

    let mut absorb = |res: Result<(Vec<SecurityFinding>, bool)>| -> Result<()> {
        match res {
            Ok((findings, t)) => {
                if t { truncated = true; }
                all.extend(findings);
                Ok(())
            }
            Err(e) => Err(e),
        }
    };
    absorb(cs)?;
    absorb(sc)?;
    absorb(db)?;

    if (all.len() as u32) > limit {
        all.truncate(limit as usize);
        truncated = true;
    }
    Ok((all, truncated))
}

/// Build the dashboard summary by reusing `fetch_github_security_findings`
/// for the counts and median ages. GitHub doesn't expose a vulnerabilities
/// time series, so `time_series` is always `None`; the risk score is the
/// host-side heuristic from `compute_local_risk_score`.
pub async fn fetch_github_security_summary(
    owner:        &str,
    repo:         &str,
    token:        &str,
    _range_days:  u32,
) -> Result<SecuritySummary> {
    let (findings, truncated) = fetch_github_security_findings(
        owner, repo, token,
        &SecurityFilters {
            limit: Some(MAX_FINDINGS_FETCH),
            ..SecurityFilters::default()
        },
    ).await?;

    let mut counts = SeverityCounts::default();
    for f in &findings {
        counts.add(f.severity);
    }
    let medians = medians_from_findings(&findings);
    let risk_score = Some(compute_local_risk_score(&counts));
    let findings_seen = findings.len() as u32;
    let web_url = Some(format!("https://github.com/{owner}/{repo}/security"));

    Ok(SecuritySummary {
        counts,
        median_age_days: medians,
        risk_score,
        time_series: None,
        provider_kind: ProviderKind::GitHub,
        web_url,
        findings_seen,
        truncated,
    })
}

// ── Per-source fetchers ─────────────────────────────────────────────────

async fn fetch_github_code_scanning(
    owner: &str,
    repo:  &str,
    token: &str,
    limit: u32,
) -> Result<(Vec<SecurityFinding>, bool)> {
    let base = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/code-scanning/alerts?state=open&per_page={GITHUB_PAGE_SIZE}"
    );
    fetch_github_paginated(&base, token, limit, parse_github_code_scanning_alert).await
}

async fn fetch_github_secret_scanning(
    owner: &str,
    repo:  &str,
    token: &str,
    limit: u32,
) -> Result<(Vec<SecurityFinding>, bool)> {
    let base = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/secret-scanning/alerts?state=open&per_page={GITHUB_PAGE_SIZE}"
    );
    fetch_github_paginated(&base, token, limit, parse_github_secret_scanning_alert).await
}

async fn fetch_github_dependabot(
    owner: &str,
    repo:  &str,
    token: &str,
    limit: u32,
) -> Result<(Vec<SecurityFinding>, bool)> {
    let base = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/dependabot/alerts?state=open&per_page={GITHUB_PAGE_SIZE}"
    );
    fetch_github_paginated(&base, token, limit, parse_github_dependabot_alert).await
}

/// Generic paginator: follows the `Link: <…>; rel="next"` header until it
/// disappears or `limit` is hit, applying `parse` to each JSON element.
/// Cursor-based pagination works on every GitHub list endpoint, including
/// Dependabot alerts (which reject the `?page=N` form). A `401`/`403`/
/// `404`/`410` response is treated as "feature unavailable" → empty Ok.
async fn fetch_github_paginated<F>(
    base_url: &str,
    token:    &str,
    limit:    u32,
    parse:    F,
) -> Result<(Vec<SecurityFinding>, bool)>
where
    F: Fn(&serde_json::Value) -> Option<SecurityFinding>,
{
    let client = reqwest::Client::new();
    let mut out: Vec<SecurityFinding> = Vec::new();
    let mut truncated = false;
    let mut next_url: Option<String> = Some(base_url.to_string());
    let mut hops: u32 = 0;

    while let Some(url) = next_url.take() {
        if (out.len() as u32) >= limit {
            truncated = true;
            break;
        }

        let resp = github_send_with_refresh(
            |tok| client.get(&url)
                .header("Authorization", format!("Bearer {tok}"))
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0"),
            token,
        ).await?;

        let status = resp.status();
        if !status.is_success() {
            if matches!(status.as_u16(), 401 | 403 | 404 | 410) {
                // Feature not enabled / no permission — return what we have.
                return Ok((out, truncated));
            }
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Other(format!("GitHub security {status}: {body}")));
        }

        let link_header = resp
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let v: serde_json::Value = resp.json().await
            .map_err(|e| AppError::Other(format!("GitHub security parse: {e}")))?;
        let nodes = match v.as_array() {
            Some(arr) => arr.clone(),
            None => break,
        };

        for node in nodes {
            if let Some(f) = parse(&node) {
                out.push(f);
                if (out.len() as u32) >= limit {
                    truncated = true;
                    break;
                }
            }
        }

        if truncated { break; }

        next_url = link_header.as_deref().and_then(parse_link_next);

        hops += 1;
        // Hard safety net at 50 pages (5000 items per source) — should
        // never be reached with `limit <= MAX_FINDINGS_FETCH/3`.
        if hops >= 50 {
            if next_url.is_some() { truncated = true; }
            break;
        }
    }
    Ok((out, truncated))
}

/// Extract the `next` URL from a GitHub `Link` header value, e.g.
/// `<https://api.github.com/...&page=2>; rel="next", <...>; rel="last"`.
fn parse_link_next(header: &str) -> Option<String> {
    for part in header.split(',') {
        let part = part.trim();
        let (url_part, rest) = part.split_once(';')?;
        let url = url_part.trim().trim_start_matches('<').trim_end_matches('>');
        if rest.split(';').any(|p| p.trim().eq_ignore_ascii_case("rel=\"next\"")) {
            return Some(url.to_string());
        }
    }
    None
}

// ── Per-alert parsers ───────────────────────────────────────────────────

fn parse_github_code_scanning_alert(node: &serde_json::Value) -> Option<SecurityFinding> {
    let number = node.get("number").and_then(|v| v.as_i64())?;
    let html_url = node.get("html_url").and_then(|v| v.as_str()).map(String::from);
    let created_at = node.get("created_at").and_then(|v| v.as_str())
        .unwrap_or_default().to_string();

    // Prefer the GHAS-aware `security_severity_level`; fall back to the
    // legacy `severity` (`error`/`warning`/`note`).
    let sev_label = node.pointer("/rule/security_severity_level")
        .and_then(|v| v.as_str())
        .or_else(|| node.pointer("/rule/severity").and_then(|v| v.as_str()))
        .unwrap_or("");
    let severity = match sev_label.to_ascii_lowercase().as_str() {
        "error"   => Severity::High,
        "warning" => Severity::Medium,
        "note"    => Severity::Info,
        other     => Severity::from_github_label(other),
    };

    let title = node.pointer("/rule/description")
        .and_then(|v| v.as_str())
        .or_else(|| node.pointer("/rule/name").and_then(|v| v.as_str()))
        .or_else(|| node.pointer("/most_recent_instance/message/text").and_then(|v| v.as_str()))
        .unwrap_or("Code scanning alert")
        .to_string();
    let description = node.pointer("/rule/full_description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let scanner = node.pointer("/tool/name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| Some("CodeQL".into()));

    let file_path = node.pointer("/most_recent_instance/location/path")
        .and_then(|v| v.as_str())
        .map(String::from);
    let start_line = node.pointer("/most_recent_instance/location/start_line")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let state = match node.get("state").and_then(|v| v.as_str()) {
        Some("dismissed") => FindingState::Dismissed,
        Some("fixed") | Some("closed") => FindingState::Resolved,
        _                 => FindingState::Detected,
    };

    let mut identifiers = Vec::new();
    if let Some(rule_id) = node.pointer("/rule/id").and_then(|v| v.as_str()) {
        identifiers.push(FindingIdentifier {
            kind:  "rule".into(),
            value: rule_id.to_string(),
            url:   None,
        });
    }

    Some(SecurityFinding {
        id: format!("code-scanning:{number}"),
        severity,
        state,
        title,
        description,
        scanner,
        report_type: Some("sast".into()),
        file_path,
        start_line,
        web_url: html_url,
        age_days: age_days_from_iso(&created_at),
        created_at,
        identifiers,
        provider: ProviderKind::GitHub,
        solution: None,
    })
}

fn parse_github_secret_scanning_alert(node: &serde_json::Value) -> Option<SecurityFinding> {
    let number = node.get("number").and_then(|v| v.as_i64())?;
    let html_url = node.get("html_url").and_then(|v| v.as_str()).map(String::from);
    let created_at = node.get("created_at").and_then(|v| v.as_str())
        .unwrap_or_default().to_string();

    let title = node.get("secret_type_display_name")
        .and_then(|v| v.as_str())
        .or_else(|| node.get("secret_type").and_then(|v| v.as_str()))
        .map(|s| format!("Exposed secret: {s}"))
        .unwrap_or_else(|| "Exposed secret".to_string());

    let state = match node.get("state").and_then(|v| v.as_str()) {
        Some("resolved") => match node.get("resolution").and_then(|v| v.as_str()) {
            Some("false_positive") | Some("revoked") => FindingState::Dismissed,
            _ => FindingState::Resolved,
        },
        _ => FindingState::Detected,
    };

    let mut identifiers = Vec::new();
    if let Some(secret_type) = node.get("secret_type").and_then(|v| v.as_str()) {
        identifiers.push(FindingIdentifier {
            kind:  "secret-type".into(),
            value: secret_type.to_string(),
            url:   None,
        });
    }

    Some(SecurityFinding {
        id: format!("secret-scanning:{number}"),
        // Exposed secrets are always treated as critical — matches GitHub's
        // own UI and the spec laid out in the multi-session plan.
        severity: Severity::Critical,
        state,
        title,
        description: None,
        scanner: Some("GitHub Secret Scanning".into()),
        report_type: Some("secret_detection".into()),
        file_path: None,
        start_line: None,
        web_url: html_url,
        age_days: age_days_from_iso(&created_at),
        created_at,
        identifiers,
        provider: ProviderKind::GitHub,
        solution: None,
    })
}

fn parse_github_dependabot_alert(node: &serde_json::Value) -> Option<SecurityFinding> {
    let number = node.get("number").and_then(|v| v.as_i64())?;
    let html_url = node.get("html_url").and_then(|v| v.as_str()).map(String::from);
    let created_at = node.get("created_at").and_then(|v| v.as_str())
        .unwrap_or_default().to_string();

    let severity = node.pointer("/security_advisory/severity")
        .and_then(|v| v.as_str())
        .map(Severity::from_github_label)
        .unwrap_or(Severity::Unknown);

    let pkg_name = node.pointer("/dependency/package/name")
        .and_then(|v| v.as_str())
        .unwrap_or("dependency");
    let summary = node.pointer("/security_advisory/summary")
        .and_then(|v| v.as_str())
        .unwrap_or("Vulnerable dependency");
    let title = format!("{pkg_name}: {summary}");
    let description = node.pointer("/security_advisory/description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let file_path = node.pointer("/dependency/manifest_path")
        .and_then(|v| v.as_str())
        .map(String::from);

    // GitHub doesn't return a free-form `solution` field for Dependabot
    // alerts, but it does expose `first_patched_version`. Build a short
    // remediation hint from it (+ vulnerable range, when available) so the
    // detail modal can surface "upgrade to X" prominently.
    let solution = {
        let patched = node
            .pointer("/security_vulnerability/first_patched_version/identifier")
            .and_then(|v| v.as_str());
        let vuln_range = node
            .pointer("/security_vulnerability/vulnerable_version_range")
            .and_then(|v| v.as_str());
        match (patched, vuln_range) {
            (Some(p), Some(r)) =>
                Some(format!("Upgrade `{pkg_name}` to `{p}` or later (vulnerable range: `{r}`).")),
            (Some(p), None) =>
                Some(format!("Upgrade `{pkg_name}` to `{p}` or later.")),
            _ => None,
        }
    };

    let state = match node.get("state").and_then(|v| v.as_str()) {
        Some("dismissed")      => FindingState::Dismissed,
        Some("fixed") | Some("auto_dismissed") => FindingState::Resolved,
        _                      => FindingState::Detected,
    };

    let mut identifiers = Vec::new();
    if let Some(arr) = node.pointer("/security_advisory/identifiers").and_then(|v| v.as_array()) {
        for ident in arr {
            let kind = ident.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let value = ident.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !value.is_empty() {
                identifiers.push(FindingIdentifier { kind, value, url: None });
            }
        }
    }
    if let Some(ghsa) = node.pointer("/security_advisory/ghsa_id").and_then(|v| v.as_str()) {
        if !identifiers.iter().any(|i| i.value == ghsa) {
            identifiers.push(FindingIdentifier {
                kind:  "GHSA".into(),
                value: ghsa.to_string(),
                url:   None,
            });
        }
    }

    Some(SecurityFinding {
        id: format!("dependabot:{number}"),
        severity,
        state,
        title,
        description,
        scanner: Some("Dependabot".into()),
        report_type: Some("dependency_scanning".into()),
        file_path,
        start_line: None,
        web_url: html_url,
        age_days: age_days_from_iso(&created_at),
        created_at,
        identifiers,
        provider: ProviderKind::GitHub,
        solution,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_handles_empty_and_odd_even() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[5]), Some(5));
        assert_eq!(median(&[1, 2, 3]), Some(2));
        assert_eq!(median(&[1, 2, 3, 4]), Some(2)); // (2+3)/2 with int div = 2
    }

    #[test]
    fn risk_label_buckets() {
        assert_eq!(risk_label(0.0),  "Low");
        assert_eq!(risk_label(24.9), "Low");
        assert_eq!(risk_label(25.0), "Medium");
        assert_eq!(risk_label(50.0), "High");
        assert_eq!(risk_label(75.0), "Critical");
    }

    #[test]
    fn local_risk_scales_with_critical() {
        let mut c = SeverityCounts::default();
        c.critical = 1;
        let s1 = compute_local_risk_score(&c);
        c.critical = 10;
        let s10 = compute_local_risk_score(&c);
        assert!(s10.value > s1.value);
        assert_eq!(s10.label, "Critical");
    }

    #[test]
    fn severity_from_github_label() {
        assert_eq!(Severity::from_github_label("Critical"), Severity::Critical);
        assert_eq!(Severity::from_github_label("moderate"), Severity::Medium);
        assert_eq!(Severity::from_github_label("note"),     Severity::Info);
        assert_eq!(Severity::from_github_label("???"),      Severity::Unknown);
    }

    #[test]
    fn parses_github_code_scanning_alert() {
        let v: serde_json::Value = serde_json::from_str(r#"{
            "number": 42,
            "html_url": "https://github.com/o/r/code-scanning/42",
            "created_at": "2024-01-01T00:00:00Z",
            "state": "open",
            "rule": {
                "id": "js/zipslip",
                "severity": "warning",
                "security_severity_level": "high",
                "name": "Zip slip",
                "description": "Zip slip vulnerability",
                "full_description": "Long description"
            },
            "tool": { "name": "CodeQL" },
            "most_recent_instance": {
                "location": { "path": "src/foo.js", "start_line": 17 },
                "message": { "text": "..." }
            }
        }"#).unwrap();
        let f = parse_github_code_scanning_alert(&v).expect("parses");
        assert_eq!(f.id, "code-scanning:42");
        assert_eq!(f.severity, Severity::High);
        assert_eq!(f.state, FindingState::Detected);
        assert_eq!(f.title, "Zip slip vulnerability");
        assert_eq!(f.scanner.as_deref(), Some("CodeQL"));
        assert_eq!(f.file_path.as_deref(), Some("src/foo.js"));
        assert_eq!(f.start_line, Some(17));
        assert_eq!(f.report_type.as_deref(), Some("sast"));
        assert!(f.identifiers.iter().any(|i| i.value == "js/zipslip"));
    }

    #[test]
    fn parses_github_secret_scanning_alert() {
        let v: serde_json::Value = serde_json::from_str(r#"{
            "number": 7,
            "html_url": "https://github.com/o/r/security/secret-scanning/7",
            "created_at": "2024-01-01T00:00:00Z",
            "state": "open",
            "secret_type": "aws_access_key_id",
            "secret_type_display_name": "AWS Access Key ID"
        }"#).unwrap();
        let f = parse_github_secret_scanning_alert(&v).expect("parses");
        assert_eq!(f.id, "secret-scanning:7");
        assert_eq!(f.severity, Severity::Critical);
        assert_eq!(f.report_type.as_deref(), Some("secret_detection"));
        assert!(f.title.contains("AWS Access Key ID"));
        assert!(f.identifiers.iter().any(|i| i.kind == "secret-type"));
    }

    #[test]
    fn parses_github_dependabot_alert() {
        let v: serde_json::Value = serde_json::from_str(r#"{
            "number": 3,
            "html_url": "https://github.com/o/r/security/dependabot/3",
            "created_at": "2024-01-01T00:00:00Z",
            "state": "open",
            "dependency": {
                "package": { "ecosystem": "npm", "name": "lodash" },
                "manifest_path": "package.json"
            },
            "security_advisory": {
                "ghsa_id": "GHSA-xxxx-yyyy-zzzz",
                "cve_id": "CVE-2024-0001",
                "summary": "Prototype pollution",
                "severity": "critical",
                "identifiers": [
                    { "type": "GHSA", "value": "GHSA-xxxx-yyyy-zzzz" },
                    { "type": "CVE",  "value": "CVE-2024-0001" }
                ]
            }
        }"#).unwrap();
        let f = parse_github_dependabot_alert(&v).expect("parses");
        assert_eq!(f.id, "dependabot:3");
        assert_eq!(f.severity, Severity::Critical);
        assert_eq!(f.report_type.as_deref(), Some("dependency_scanning"));
        assert!(f.title.contains("lodash"));
        assert_eq!(f.file_path.as_deref(), Some("package.json"));
        assert!(f.identifiers.iter().any(|i| i.kind == "CVE"));
        assert!(f.identifiers.iter().any(|i| i.kind == "GHSA"));
    }
}
