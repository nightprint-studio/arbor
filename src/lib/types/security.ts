// TypeScript mirrors of src-tauri/src/git_provider/security_impl.rs types.
//
// Keep this file byte-for-byte aligned with the Rust serde output. Snake-case
// field names are intentional — they match the JSON over the IPC boundary.

export type Severity = 'critical' | 'high' | 'medium' | 'low' | 'info' | 'unknown';
export type FindingState = 'detected' | 'confirmed' | 'resolved' | 'dismissed';
export type ProviderKind = 'github' | 'gitlab' | 'gitea' | 'bitbucket';

export const SEVERITY_ORDER: Severity[] = [
  'critical', 'high', 'medium', 'low', 'info', 'unknown',
];

export interface FindingIdentifier {
  /** e.g. "CVE", "CWE", "OWASP", "GHSA" */
  kind:  string;
  value: string;
  url:   string | null;
}

export interface SecurityFinding {
  id:          string;
  severity:    Severity;
  state:       FindingState;
  title:       string;
  description: string | null;
  scanner:     string | null;
  /** sast | dependency_scanning | container_scanning | secret_detection | dast */
  report_type: string | null;
  file_path:   string | null;
  start_line:  number | null;
  web_url:     string | null;
  /** ISO-8601 timestamp from the provider. */
  created_at:  string;
  /** Computed host-side from `created_at` at fetch time. */
  age_days:    number;
  identifiers: FindingIdentifier[];
  provider:    ProviderKind;
  /** Suggested remediation. GitLab populates this from the
   *  `Vulnerability.solution` GraphQL field; for GitHub Dependabot we
   *  synthesise a short hint from `first_patched_version` +
   *  `vulnerable_version_range`. Absent on most code-scanning /
   *  secret-scanning alerts. */
  solution?:   string | null;
}

export interface SeverityCounts {
  critical: number;
  high:     number;
  medium:   number;
  low:      number;
  info:     number;
  unknown:  number;
}

export interface SeverityMedians {
  critical: number | null;
  high:     number | null;
  medium:   number | null;
  low:      number | null;
  info:     number | null;
  unknown:  number | null;
}

export interface RiskScore {
  /** Numeric value in `[0, 100]`. */
  value: number;
  /** "Low" | "Medium" | "High" | "Critical" */
  label: string;
}

export interface TimePoint {
  /** ISO-8601 date (no time component): YYYY-MM-DD. */
  date:     string;
  critical: number;
  high:     number;
  medium:   number;
  low:      number;
  info:     number;
  unknown:  number;
}

export interface VulnTimeSeries {
  points:     TimePoint[];
  range_days: number;
}

export interface SecuritySummary {
  counts:          SeverityCounts;
  median_age_days: SeverityMedians;
  risk_score:      RiskScore | null;
  time_series:     VulnTimeSeries | null;
  provider_kind:   ProviderKind;
  /** URL to the provider-native dashboard, if any. */
  web_url:         string | null;
  /** Total findings considered by the summary (capped). */
  findings_seen:   number;
  /** True when the host-side fetch hit the cap and stopped. */
  truncated:       boolean;
}

export interface SecurityFilters {
  severities:   Severity[];
  states:       FindingState[];
  report_types: string[];
  /** Host-side substring filter applied to title + file_path. */
  search:       string | null;
  /** Hard cap on returned findings. */
  limit:        number | null;
}

export function emptySecurityFilters(): SecurityFilters {
  return {
    severities:   [],
    /** Active findings only by default. The detail-modal scope toggle
     *  swaps this for `['resolved', 'dismissed']` when the user wants to
     *  see closed items instead. */
    states:       ['detected', 'confirmed'],
    report_types: [],
    search:       null,
    limit:        null,
  };
}

export function severityCount(counts: SeverityCounts, sev: Severity): number {
  return counts[sev];
}

export function severityMedian(medians: SeverityMedians, sev: Severity): number | null {
  return medians[sev];
}

export function totalCount(counts: SeverityCounts): number {
  return counts.critical + counts.high + counts.medium + counts.low + counts.info + counts.unknown;
}
