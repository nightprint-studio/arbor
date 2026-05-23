// ── Conflict modal local types ──────────────────────────────────────────────
//
// Shared shapes used across the conflict modal's Svelte components but not
// useful outside this folder. Living here (rather than in
// `$lib/utils/conflict/`) keeps the public API of the utils package focused
// on pure-logic helpers; these are component contracts.

export type Status = 'conflict' | 'resolved' | 'viewed' | 'added' | 'deleted';

export interface FileItem {
  path:        string;
  /** Status icon + visual treatment driver. */
  status:      Status;
  /** Monogram for the right-aligned badge (A/D/C/✓/…). Optional — when
   *  unset, a lucide icon is used instead. */
  monogram?:   string;
  /** Tooltip shown over the monogram. */
  monoTip?:    string;
  /** Right-aligned decision badge (blocking mode only). */
  decisionBadge?: { kind: 'keep_mine' | 'use_stash' | 'custom'; tooltip: string };
  /** Shown when a file is currently being staged. */
  saving?:     boolean;
}
