//! GitHub security dashboard.
//!
//! GitHub doesn't expose a single "security dashboard" endpoint — we aggregate
//! three independent alert streams (code-scanning, secret-scanning, dependabot)
//! into the same `SecuritySummary` shape used by the GitLab branch. Each stream
//! may be unavailable on a given repo (private repo without GHAS, missing
//! scopes, feature disabled): we surface those as empty lists rather than hard
//! failures so partial coverage still produces a useful summary.
//!
//! Folds together the old delegate (`github/security.rs`: RepoRef destructuring,
//! token gating, host-side `apply_filters`) and the REST bodies from
//! `security_impl.rs` (GitHub functions only). Error strings are preserved
//! verbatim and wrapped via `classify`; auth uses the injected session.

use corvus_git_provider_api::prelude::*;

use crate::http::{classify, GithubHttp};

const GITHUB_PAGE_SIZE: u32 = 100;
const GITHUB_API_BASE: &str = "https://api.github.com";

// ---------------------------------------------------------------------------
// RepoRef plumbing (from the delegate)
// ---------------------------------------------------------------------------

fn repo_parts(repo: &RepoRef) -> Result<(&str, &str), ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name = repo
        .name
        .as_deref()
        .ok_or_else(|| ProviderError::BadRequest("GitHub RepoRef requires name".into()))?;
    Ok((owner, name))
}

// ---------------------------------------------------------------------------
// Public entry points (delegate behavior preserved)
// ---------------------------------------------------------------------------

/// Best-effort probe for GitHub security data. The plan calls for code-scanning
/// as the primary signal, but many repos rely solely on Dependabot alerts — so
/// we fall back to a Dependabot probe (then secret-scanning) when code-scanning
/// isn't available. Returns `true` if any endpoint answers `200 OK`.
///
/// Delegate gate: when no credentials are present we never probe, returning
/// `false` so the gating UI hides the entry rather than rendering a misleading
/// state.
pub(crate) async fn supports_security(
    http: &GithubHttp,
    repo: &RepoRef,
) -> Result<bool, ProviderError> {
    if !http.has_credentials() {
        return Ok(false);
    }
    let (owner, name) = repo_parts(repo)?;

    // Independent REST endpoints — one per source — so a feature being off
    // (or the token missing the matching scope) on one path doesn't mask
    // the others. Order: code-scanning → Dependabot → secret-scanning.
    let cs = github_endpoint_available(
        http,
        &format!("{GITHUB_API_BASE}/repos/{owner}/{name}/code-scanning/alerts?per_page=1"),
    )
    .await?;
    if cs {
        return Ok(true);
    }

    let db = github_endpoint_available(
        http,
        &format!("{GITHUB_API_BASE}/repos/{owner}/{name}/dependabot/alerts?per_page=1"),
    )
    .await?;
    if db {
        return Ok(true);
    }

    let ss = github_endpoint_available(
        http,
        &format!("{GITHUB_API_BASE}/repos/{owner}/{name}/secret-scanning/alerts?per_page=1"),
    )
    .await?;
    if ss {
        return Ok(true);
    }

    tracing::warn!(
        target: "arbor::security",
        "GitHub security probe: '{owner}/{name}' had no accessible source \
         (code-scanning, Dependabot, secret-scanning all unavailable) — \
         sidebar icon will stay hidden. Token likely lacks `security_events` / \
         `repo` scope, GHAS is off for the repo, or all sources are disabled."
    );
    Ok(false)
}

/// Build the dashboard summary by reusing the findings fetch for the counts and
/// median ages. GitHub doesn't expose a vulnerabilities time series, so
/// `time_series` is always `None`; the risk score is the host-side heuristic
/// from `compute_local_risk_score`.
pub(crate) async fn fetch_security_summary(
    http: &GithubHttp,
    repo: &RepoRef,
    _range_days: u32,
) -> Result<SecuritySummary, ProviderError> {
    let (owner, name) = repo_parts(repo)?;

    let (findings, truncated) = fetch_all_findings(
        http,
        owner,
        name,
        &SecurityFilters {
            limit: Some(MAX_FINDINGS_FETCH),
            ..SecurityFilters::default()
        },
    )
    .await?;

    let mut counts = SeverityCounts::default();
    for f in &findings {
        counts.add(f.severity);
    }
    let medians = medians_from_findings(&findings);
    let risk_score = Some(compute_local_risk_score(&counts));
    let findings_seen = findings.len() as u32;
    let web_url = Some(format!("https://github.com/{owner}/{name}/security"));

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

/// Fetch findings and apply the full filter set host-side (GitHub doesn't
/// support the `search` clause server-side, so callers always get a post-filter
/// list — matches the delegate's behavior).
pub(crate) async fn fetch_security_findings(
    http: &GithubHttp,
    repo: &RepoRef,
    filters: SecurityFilters,
) -> Result<Vec<SecurityFinding>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let (findings, _truncated) = fetch_all_findings(http, owner, name, &filters).await?;
    Ok(apply_filters(findings, &filters))
}

// ---------------------------------------------------------------------------
// Aggregation across the three sources
// ---------------------------------------------------------------------------

/// Fetch and unify findings across code-scanning, secret-scanning, and
/// Dependabot. Each source is fetched in parallel; an unavailable source
/// (403/404) contributes zero findings without aborting the whole call.
async fn fetch_all_findings(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    filters: &SecurityFilters,
) -> Result<(Vec<SecurityFinding>, bool), ProviderError> {
    let limit = filters
        .limit
        .map(|l| l.min(MAX_FINDINGS_FETCH))
        .unwrap_or(MAX_FINDINGS_FETCH);
    // Per-source budget so one noisy stream can't crowd out the others.
    let per_source = limit.div_ceil(3);

    let cs_fut = fetch_github_code_scanning(http, owner, repo, per_source);
    let sc_fut = fetch_github_secret_scanning(http, owner, repo, per_source);
    let db_fut = fetch_github_dependabot(http, owner, repo, per_source);

    let (cs, sc, db) = tokio::join!(cs_fut, sc_fut, db_fut);

    let mut all = Vec::new();
    let mut truncated = false;

    let mut absorb =
        |res: Result<(Vec<SecurityFinding>, bool), ProviderError>| -> Result<(), ProviderError> {
            match res {
                Ok((findings, t)) => {
                    if t {
                        truncated = true;
                    }
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

// ---------------------------------------------------------------------------
// Probe helper
// ---------------------------------------------------------------------------

/// `true` for `200`; `false` for the usual "feature unavailable" responses
/// (`401`/`403`/`404`/`410`); error for unexpected `5xx`. Logs the exact
/// status on every unavailable response so the operator can tell scope-miss
/// (`401/403`) from feature-off (`404/410`).
async fn github_endpoint_available(http: &GithubHttp, url: &str) -> Result<bool, ProviderError> {
    let resp = http
        .send(|s| {
            http.client()
                .get(url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }
    if matches!(status.as_u16(), 401 | 403 | 404 | 410) {
        tracing::debug!(
            target: "arbor::security",
            "GitHub security probe: {url} → HTTP {} (treating as unavailable)",
            status.as_u16()
        );
        return Ok(false);
    }
    let body = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitHub security probe {status}: {body}")))
}

// ── Per-source fetchers ─────────────────────────────────────────────────

async fn fetch_github_code_scanning(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    limit: u32,
) -> Result<(Vec<SecurityFinding>, bool), ProviderError> {
    let base = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/code-scanning/alerts?state=open&per_page={GITHUB_PAGE_SIZE}"
    );
    fetch_github_paginated(http, &base, limit, parse_github_code_scanning_alert).await
}

async fn fetch_github_secret_scanning(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    limit: u32,
) -> Result<(Vec<SecurityFinding>, bool), ProviderError> {
    let base = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/secret-scanning/alerts?state=open&per_page={GITHUB_PAGE_SIZE}"
    );
    fetch_github_paginated(http, &base, limit, parse_github_secret_scanning_alert).await
}

async fn fetch_github_dependabot(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    limit: u32,
) -> Result<(Vec<SecurityFinding>, bool), ProviderError> {
    let base = format!(
        "{GITHUB_API_BASE}/repos/{owner}/{repo}/dependabot/alerts?state=open&per_page={GITHUB_PAGE_SIZE}"
    );
    fetch_github_paginated(http, &base, limit, parse_github_dependabot_alert).await
}

/// Generic paginator: follows the `Link: <…>; rel="next"` header until it
/// disappears or `limit` is hit, applying `parse` to each JSON element.
/// Cursor-based pagination works on every GitHub list endpoint, including
/// Dependabot alerts (which reject the `?page=N` form). A `401`/`403`/`404`/
/// `410` response is treated as "feature unavailable" → empty Ok.
async fn fetch_github_paginated<F>(
    http: &GithubHttp,
    base_url: &str,
    limit: u32,
    parse: F,
) -> Result<(Vec<SecurityFinding>, bool), ProviderError>
where
    F: Fn(&serde_json::Value) -> Option<SecurityFinding>,
{
    let mut out: Vec<SecurityFinding> = Vec::new();
    let mut truncated = false;
    let mut next_url: Option<String> = Some(base_url.to_string());
    let mut hops: u32 = 0;

    while let Some(url) = next_url.take() {
        if (out.len() as u32) >= limit {
            truncated = true;
            break;
        }

        let resp = http
            .send(|s| {
                http.client()
                    .get(&url)
                    .header("Authorization", &s.auth_header)
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28")
                    .header("User-Agent", "arbor-git-gui/1.0")
            })
            .await?;

        let status = resp.status();
        if !status.is_success() {
            if matches!(status.as_u16(), 401 | 403 | 404 | 410) {
                // Feature not enabled / no permission — return what we have.
                return Ok((out, truncated));
            }
            let body = resp.text().await.unwrap_or_default();
            return Err(classify(format!("GitHub security {status}: {body}")));
        }

        let link_header = resp
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| classify(format!("GitHub security parse: {e}")))?;
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

        if truncated {
            break;
        }

        next_url = link_header.as_deref().and_then(parse_link_next);

        hops += 1;
        // Hard safety net at 50 pages (5000 items per source) — should
        // never be reached with `limit <= MAX_FINDINGS_FETCH/3`.
        if hops >= 50 {
            if next_url.is_some() {
                truncated = true;
            }
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
        if rest
            .split(';')
            .any(|p| p.trim().eq_ignore_ascii_case("rel=\"next\""))
        {
            return Some(url.to_string());
        }
    }
    None
}

// ── Per-alert parsers ───────────────────────────────────────────────────

fn parse_github_code_scanning_alert(node: &serde_json::Value) -> Option<SecurityFinding> {
    let number = node.get("number").and_then(|v| v.as_i64())?;
    let html_url = node.get("html_url").and_then(|v| v.as_str()).map(String::from);
    let created_at = node
        .get("created_at")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // Prefer the GHAS-aware `security_severity_level`; fall back to the
    // legacy `severity` (`error`/`warning`/`note`).
    let sev_label = node
        .pointer("/rule/security_severity_level")
        .and_then(|v| v.as_str())
        .or_else(|| node.pointer("/rule/severity").and_then(|v| v.as_str()))
        .unwrap_or("");
    let severity = match sev_label.to_ascii_lowercase().as_str() {
        "error" => Severity::High,
        "warning" => Severity::Medium,
        "note" => Severity::Info,
        other => Severity::from_github_label(other),
    };

    let title = node
        .pointer("/rule/description")
        .and_then(|v| v.as_str())
        .or_else(|| node.pointer("/rule/name").and_then(|v| v.as_str()))
        .or_else(|| {
            node.pointer("/most_recent_instance/message/text")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("Code scanning alert")
        .to_string();
    let description = node
        .pointer("/rule/full_description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let scanner = node
        .pointer("/tool/name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| Some("CodeQL".into()));

    let file_path = node
        .pointer("/most_recent_instance/location/path")
        .and_then(|v| v.as_str())
        .map(String::from);
    let start_line = node
        .pointer("/most_recent_instance/location/start_line")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let state = match node.get("state").and_then(|v| v.as_str()) {
        Some("dismissed") => FindingState::Dismissed,
        Some("fixed") | Some("closed") => FindingState::Resolved,
        _ => FindingState::Detected,
    };

    let mut identifiers = Vec::new();
    if let Some(rule_id) = node.pointer("/rule/id").and_then(|v| v.as_str()) {
        identifiers.push(FindingIdentifier {
            kind: "rule".into(),
            value: rule_id.to_string(),
            url: None,
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
    let created_at = node
        .get("created_at")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let title = node
        .get("secret_type_display_name")
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
            kind: "secret-type".into(),
            value: secret_type.to_string(),
            url: None,
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
    let created_at = node
        .get("created_at")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let severity = node
        .pointer("/security_advisory/severity")
        .and_then(|v| v.as_str())
        .map(Severity::from_github_label)
        .unwrap_or(Severity::Unknown);

    let pkg_name = node
        .pointer("/dependency/package/name")
        .and_then(|v| v.as_str())
        .unwrap_or("dependency");
    let summary = node
        .pointer("/security_advisory/summary")
        .and_then(|v| v.as_str())
        .unwrap_or("Vulnerable dependency");
    let title = format!("{pkg_name}: {summary}");
    let description = node
        .pointer("/security_advisory/description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let file_path = node
        .pointer("/dependency/manifest_path")
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
            (Some(p), Some(r)) => Some(format!(
                "Upgrade `{pkg_name}` to `{p}` or later (vulnerable range: `{r}`)."
            )),
            (Some(p), None) => Some(format!("Upgrade `{pkg_name}` to `{p}` or later.")),
            _ => None,
        }
    };

    let state = match node.get("state").and_then(|v| v.as_str()) {
        Some("dismissed") => FindingState::Dismissed,
        Some("fixed") | Some("auto_dismissed") => FindingState::Resolved,
        _ => FindingState::Detected,
    };

    let mut identifiers = Vec::new();
    if let Some(arr) = node
        .pointer("/security_advisory/identifiers")
        .and_then(|v| v.as_array())
    {
        for ident in arr {
            let kind = ident
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value = ident
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !value.is_empty() {
                identifiers.push(FindingIdentifier { kind, value, url: None });
            }
        }
    }
    if let Some(ghsa) = node
        .pointer("/security_advisory/ghsa_id")
        .and_then(|v| v.as_str())
    {
        if !identifiers.iter().any(|i| i.value == ghsa) {
            identifiers.push(FindingIdentifier {
                kind: "GHSA".into(),
                value: ghsa.to_string(),
                url: None,
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
    fn severity_from_github_label() {
        assert_eq!(Severity::from_github_label("Critical"), Severity::Critical);
        assert_eq!(Severity::from_github_label("moderate"), Severity::Medium);
        assert_eq!(Severity::from_github_label("note"), Severity::Info);
        assert_eq!(Severity::from_github_label("???"), Severity::Unknown);
    }

    #[test]
    fn parses_github_code_scanning_alert() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
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
        }"#,
        )
        .unwrap();
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
        let v: serde_json::Value = serde_json::from_str(
            r#"{
            "number": 7,
            "html_url": "https://github.com/o/r/security/secret-scanning/7",
            "created_at": "2024-01-01T00:00:00Z",
            "state": "open",
            "secret_type": "aws_access_key_id",
            "secret_type_display_name": "AWS Access Key ID"
        }"#,
        )
        .unwrap();
        let f = parse_github_secret_scanning_alert(&v).expect("parses");
        assert_eq!(f.id, "secret-scanning:7");
        assert_eq!(f.severity, Severity::Critical);
        assert_eq!(f.report_type.as_deref(), Some("secret_detection"));
        assert!(f.title.contains("AWS Access Key ID"));
        assert!(f.identifiers.iter().any(|i| i.kind == "secret-type"));
    }

    #[test]
    fn parses_github_dependabot_alert() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
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
        }"#,
        )
        .unwrap();
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
