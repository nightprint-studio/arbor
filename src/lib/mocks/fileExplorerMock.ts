/**
 * Placeholder data for the FileExplorerModal **Overview** dashboard.
 *
 * The explorer's browse view is backed by the real filesystem (see the
 * `fs_*` IPC commands). The Overview dashboard, however, would need a library
 * indexer (total size, per-kind counts) that doesn't exist yet — so these
 * numbers are illustrative placeholders, surfaced under a "demo data" tag in
 * the UI. Devices/Locations on the dashboard use the real FS roots.
 */

export interface OverviewStat {
  label: string;
  value: string;
}

export const overviewStats: OverviewStat[] = [
  { label: 'Library size',   value: '308.3 GB' },
  { label: 'Total capacity', value: '931.5 GB' },
  { label: 'Free space',     value: '418.1 GB' },
  { label: 'Index size',     value: '348.1 MB' },
  { label: 'Preview media',  value: '1.2 GB' },
];

/** File-kind breakdown for the overview segment chart. */
export interface KindSlice {
  kind:  string;
  count: number;
  /** CSS color token for the slice. */
  color: string;
}

export const kindBreakdown: KindSlice[] = [
  { kind: 'Image',    count: 4210, color: 'var(--accent)' },
  { kind: 'Document', count: 1890, color: 'var(--success)' },
  { kind: 'Video',    count: 312,  color: 'var(--info, #4aa3df)' },
  { kind: 'Code',     count: 5621, color: 'var(--warning, #e6a817)' },
  { kind: 'Archive',  count: 96,   color: 'var(--text-muted)' },
  { kind: 'Other',    count: 738,  color: 'var(--border)' },
];

export const totalFiles = kindBreakdown.reduce((s, k) => s + k.count, 0);
