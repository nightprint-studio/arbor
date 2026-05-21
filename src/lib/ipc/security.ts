import { invoke } from '@tauri-apps/api/core';
import type {
  SecuritySummary, SecurityFinding, SecurityFilters,
} from '$lib/types/security';

/**
 * Probe whether the active tab's repo provider exposes a security
 * dashboard. Returns `false` when there is no remote, no token, or the
 * provider doesn't support the surface (GitHub today, until Phase 6).
 */
export function supportsSecurity(tabId: string): Promise<boolean> {
  return invoke('supports_security', { tabId });
}

/**
 * Fetch the headline summary (counter grid + risk score + optional
 * vulnerabilities-over-time series). `rangeDays` controls the time-series
 * window; the UI exposes 30/60/90.
 */
export function fetchSecuritySummary(
  tabId:     string,
  rangeDays: number,
): Promise<SecuritySummary> {
  return invoke('fetch_security_summary', { tabId, rangeDays });
}

/**
 * Fetch the detailed findings list for the active tab's repo. Filters are
 * passed through verbatim — server-side filtering covers severity / state /
 * report_type, with the `search` clause applied host-side inside the Rust
 * provider.
 */
export function fetchSecurityFindings(
  tabId:   string,
  filters: SecurityFilters,
): Promise<SecurityFinding[]> {
  return invoke('fetch_security_findings', { tabId, filters });
}

/**
 * Snapshot of CSS custom properties the security HTML report depends on,
 * captured from the running app at export time. Lets the exported file
 * mirror whatever theme the user has active (default dark, plugin overlay,
 * custom palette) instead of shipping a hardcoded light/dark scheme.
 */
export interface SecurityExportTheme {
  bg:             string;
  bgElevated:     string;
  bgCard:         string;
  bgInput:        string;
  textPrimary:    string;
  textBody:       string;
  textMuted:      string;
  accent:         string;
  border:         string;
  borderSubtle:   string;
  warning:        string;
  warningSubtle:  string;
  sevCritical:    string;
  sevHigh:        string;
  sevMedium:      string;
  sevLow:         string;
  sevInfo:        string;
  sevUnknown:     string;
}

/**
 * Read the current `:root` CSS custom-property values used by the security
 * report. Trims whitespace; falls back to backend defaults for any token
 * the host page doesn't define (which the Rust struct handles via serde).
 */
export function captureSecurityExportTheme(): SecurityExportTheme {
  const cs   = getComputedStyle(document.documentElement);
  const read = (name: string) => cs.getPropertyValue(name).trim();
  return {
    bg:            read('--bg-base'),
    bgElevated:    read('--bg-elevated'),
    bgCard:        read('--bg-modal'),
    bgInput:       read('--bg-input'),
    textPrimary:   read('--text-primary'),
    textBody:      read('--text-secondary'),
    textMuted:     read('--text-muted'),
    accent:        read('--accent'),
    border:        read('--border'),
    borderSubtle:  read('--border-subtle'),
    warning:       read('--warning'),
    warningSubtle: read('--warning-subtle'),
    sevCritical:   read('--severity-critical'),
    sevHigh:       read('--severity-high'),
    sevMedium:     read('--severity-medium'),
    sevLow:        read('--severity-low'),
    sevInfo:       read('--severity-info'),
    sevUnknown:    read('--severity-unknown'),
  };
}

/**
 * Export the active tab's security posture to a self-contained file.
 *
 * - `'html'` produces a single-file report with counter grid, risk-score
 *   gauge, time-series chart and findings table (inline CSS + SVG, no JS).
 *   `theme` is embedded so the file matches the current app theme.
 * - `'csv'` produces a flat findings dump (one row per finding, no banner).
 *   `theme` is ignored.
 *
 * Returns the registered job-id immediately; the export runs in the
 * background and emits `arbor://job-done` plus a `plugin:notification`
 * toast on completion.
 */
export function exportSecurityReport(
  tabId:      string,
  outputPath: string,
  format:     'html' | 'csv',
  theme?:     SecurityExportTheme,
): Promise<string> {
  return invoke('export_security_report', { tabId, outputPath, format, theme });
}
