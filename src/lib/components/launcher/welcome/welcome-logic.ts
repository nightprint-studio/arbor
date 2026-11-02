/**
 * What the welcome page decides, apart from how it looks: the greeting for an hour, the phrases the prompt cycles
 * through, how recents are filtered, ordered and grouped by age, and what a search matches.
 */
import type { RecentProject } from '$lib/ipc/recents';

const DAY = 86_400;

/** The greeting for a local hour (0–23). */
export function greetingFor(hour: number): string {
  if (hour < 5) return 'Still up';
  if (hour < 12) return 'Good morning';
  if (hour < 18) return 'Good afternoon';
  return 'Good evening';
}

/** The lines the prompt writes under the greeting, in turn. `lastName` is the project last opened, if any. */
export function promptPhrases(lastName: string | null): string[] {
  return [
    ...(lastName ? [`Continue with ${lastName}`] : []),
    'Open a project, a repository or a folder',
    'Press / to search everything',
    'Type 1–9 to open a recent one',
    'Clone a repository',
    'Start something new',
  ];
}

/** Compact "when", the way a welcome screen wants it. */
export function whenLabel(openedAt: number, nowSecs: number): string {
  const days = Math.floor((nowSecs - openedAt) / DAY);
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.floor(days / 7)}w ago`;
  return new Date(openedAt * 1000).toLocaleDateString();
}

export type AgeGroup = 'Today' | 'Yesterday' | 'This week' | 'This month' | 'Earlier';

export function ageGroup(openedAt: number, nowSecs: number): AgeGroup {
  const d = (nowSecs - openedAt) / DAY;
  if (d < 1) return 'Today';
  if (d < 2) return 'Yesterday';
  if (d < 7) return 'This week';
  if (d < 30) return 'This month';
  return 'Earlier';
}

export const recentKey = (r: Pick<RecentProject, 'product' | 'path'>) => `${r.product}:${r.path}`;

export interface RecentsView {
  pinned: RecentProject[];
  groups: { label: AgeGroup; items: RecentProject[] }[];
  /** Unpinned ones the page is not showing yet. */
  hidden: number;
  /** The order the keys 1–9 follow: the pinned first, then what is shown. */
  order: RecentProject[];
}

/** Recents for a product filter (`'all'` or a product id), pinned first, the rest grouped by age and capped at `limit`. */
export function recentsView(
  recents: readonly RecentProject[], filter: string, nowSecs: number, opts: { all: boolean; limit: number },
): RecentsView {
  const keep = recents.filter((r) => filter === 'all' || r.product === filter);
  const newest = (a: RecentProject, b: RecentProject) => b.opened_at - a.opened_at;
  const pinned = keep.filter((r) => r.pinned).sort(newest);
  const rest = keep.filter((r) => !r.pinned).sort(newest);
  const shown = opts.all ? rest : rest.slice(0, opts.limit);
  const groups: RecentsView['groups'] = [];
  for (const item of shown) {
    const label = ageGroup(item.opened_at, nowSecs);
    const last = groups[groups.length - 1];
    if (last?.label === label) last.items.push(item); else groups.push({ label, items: [item] });
  }
  return { pinned, groups, hidden: rest.length - shown.length, order: [...pinned, ...shown] };
}

export interface SearchEntry { id: string; title: string; detail: string; kind: 'project' | 'product' | 'action'; }

/** Entries whose title, or detail for a project (its path), contain `query`; best five projects. Empty query lists everything. */
export function searchEntries(query: string, entries: readonly SearchEntry[]): { label: string; items: SearchEntry[] }[] {
  const q = query.trim().toLowerCase();
  const has = (e: SearchEntry) => !q || e.title.toLowerCase().includes(q) || (e.kind === 'project' && e.detail.toLowerCase().includes(q));
  const pick = (kind: SearchEntry['kind'], label: string, cap = Infinity) => {
    const items = entries.filter((e) => e.kind === kind && has(e)).slice(0, cap);
    return items.length ? [{ label, items }] : [];
  };
  return [...pick('project', 'Projects', 5), ...pick('product', 'Products'), ...pick('action', 'Actions')];
}
