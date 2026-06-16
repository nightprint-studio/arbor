//! Security dashboard — common DTOs shared across providers + the frontend.
//!
//! Provider-agnostic computation helpers (severity mapping medians, host-side
//! risk score, filter application) and the GitLab/GitHub fetch logic live in
//! the provider impl crates / shell; this module owns only the wire types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kind::ProviderKind;

// ── Severity / state enums ────────────────────────────────────────────────

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
    pub fn from_gitlab(s: &str) -> Severity {
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
    pub fn from_github_label(s: &str) -> Severity {
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

    pub fn from_gitlab(s: &str) -> FindingState {
        match s.to_ascii_uppercase().as_str() {
            "CONFIRMED" => FindingState::Confirmed,
            "RESOLVED"  => FindingState::Resolved,
            "DISMISSED" => FindingState::Dismissed,
            _           => FindingState::Detected,
        }
    }
}

// ── Public types (shared across providers + frontend) ─────────────────────

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
    /// `None` when historical data is unavailable. GitLab Ultimate always
    /// populates this; GitHub returns `None` until a later phase.
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

/// Hard upper bound on how many findings a single fetch will collect across
/// pagination. Above this, the UI shows a "refine filters" hint.
pub const MAX_FINDINGS_FETCH: u32 = 1000;

/// Risk-score weight ceiling used by the host-side heuristic. Tuned so a
/// repo with ~10 critical findings already sits in the "High risk" band.
const RISK_SCORE_CAP: f32 = 100.0;

// ── Provider-agnostic computation helpers ─────────────────────────────────
//
// Pure functions over the DTOs above — severity bucketing, median age, the
// host-side risk score, and the final host-side filter pass. The GitHub /
// GitLab fetch code in the provider impl crates calls these after pulling
// raw findings; they live here so both providers share one implementation.

/// Days elapsed between an ISO-8601 timestamp and `Utc::now()`.
/// Returns `0` when parsing fails or the timestamp is in the future.
pub fn age_days_from_iso(iso: &str) -> u32 {
    let Ok(t) = iso.parse::<DateTime<Utc>>() else { return 0 };
    let delta = Utc::now() - t;
    delta.num_days().max(0) as u32
}

/// Median (50th percentile) of a slice of u32 ages. `None` for an empty
/// slice. Uses simple sort+midpoint — input is bounded by
/// `MAX_FINDINGS_FETCH` so allocation cost is irrelevant.
pub fn median(ages: &[u32]) -> Option<u32> {
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
pub fn medians_from_findings(findings: &[SecurityFinding]) -> SeverityMedians {
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
pub fn compute_local_risk_score(counts: &SeverityCounts) -> RiskScore {
    let raw = counts.critical as f32 * Severity::Critical.risk_weight()
            + counts.high     as f32 * Severity::High.risk_weight()
            + counts.medium   as f32 * Severity::Medium.risk_weight()
            + counts.low      as f32 * Severity::Low.risk_weight();
    let pct = (raw * 100.0 / RISK_SCORE_CAP).clamp(0.0, 100.0);
    RiskScore { value: pct, label: risk_label(pct).to_string() }
}

pub fn risk_label(value: f32) -> &'static str {
    if value >= 75.0 { "Critical" }
    else if value >= 50.0 { "High" }
    else if value >= 25.0 { "Medium" }
    else { "Low" }
}

/// Apply a `SecurityFilters` host-side. Used as a final pass after the
/// provider has returned its server-filtered findings — guarantees that the
/// `search` clause (which providers don't natively support) is honored.
pub fn apply_filters(findings: Vec<SecurityFinding>, filters: &SecurityFilters) -> Vec<SecurityFinding> {
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
}
