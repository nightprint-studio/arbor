// Static metadata for the six severities surfaced in the dashboard.
// Centralised here so counters / pills / detail rows all read the same labels
// and CSS vars instead of repeating the mapping.

import type { Severity } from '$lib/types/security';

export interface SeverityMeta {
  label:    string;
  color:    string;  // CSS var
  bgColor:  string;  // CSS var (alpha-12 background)
}

export const SEVERITY_META: Record<Severity, SeverityMeta> = {
  critical: { label: 'Critical', color: 'var(--severity-critical)', bgColor: 'var(--severity-critical-bg)' },
  high:     { label: 'High',     color: 'var(--severity-high)',     bgColor: 'var(--severity-high-bg)'     },
  medium:   { label: 'Medium',   color: 'var(--severity-medium)',   bgColor: 'var(--severity-medium-bg)'   },
  low:      { label: 'Low',      color: 'var(--severity-low)',      bgColor: 'var(--severity-low-bg)'      },
  info:     { label: 'Info',     color: 'var(--severity-info)',     bgColor: 'var(--severity-info-bg)'     },
  unknown:  { label: 'Unknown',  color: 'var(--severity-unknown)',  bgColor: 'var(--severity-unknown-bg)'  },
};

export function formatMedianAge(days: number | null): string {
  if (days == null) return '—';
  if (days < 1)   return '<1 day';
  if (days === 1) return '1 day';
  if (days < 30)  return `${days} days`;
  if (days < 365) return `${Math.round(days / 30)} mo`;
  return `${(days / 365).toFixed(1)} y`;
}
