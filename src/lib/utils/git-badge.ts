/**
 * How a git status is spoken, once: the glyph that marks it, the word for it, and the theme
 * colour that carries it. The file explorer draws these as corner badges and Bennu's file tree
 * as a coloured name — different shapes, one vocabulary, so *amber* means modified and *green*
 * means newly staged in both.
 */
import type { GitBadge } from '$lib/ipc/fs';

export const GIT_BADGE_GLYPH: Record<GitBadge, string> = {
  modified: '●', added: '+', untracked: '?', deleted: '−', renamed: '»', conflicted: '!', ignored: '',
};

export const GIT_BADGE_LABEL: Record<GitBadge, string> = {
  modified: 'Modified', added: 'Staged', untracked: 'Untracked', deleted: 'Deleted',
  renamed: 'Renamed', conflicted: 'Conflicted', ignored: 'Ignored',
};

/** The CSS colour a badge is drawn in. `ignored` is dimmed rather than coloured, so it has none. */
export const GIT_BADGE_COLOR: Record<GitBadge, string | undefined> = {
  modified: 'var(--warning)',
  added: 'var(--success)',
  untracked: 'var(--info)',
  deleted: 'var(--error)',
  renamed: 'var(--accent)',
  conflicted: 'var(--error)',
  ignored: undefined,
};

/** Strongest first — what a folder shows when several of its descendants differ. */
const STRENGTH: GitBadge[] = ['conflicted', 'deleted', 'modified', 'renamed', 'added', 'untracked'];

/** The stronger of two badges (either may be absent). */
export function strongerBadge(a: GitBadge | undefined, b: GitBadge | undefined): GitBadge | undefined {
  if (!a) return b;
  if (!b) return a;
  return STRENGTH.indexOf(a) <= STRENGTH.indexOf(b) ? a : b;
}
