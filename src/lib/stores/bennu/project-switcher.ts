/** The pure half of the project switcher: how entries are told apart, with no store behind it. */

import type { WorkspaceProject } from './workspaces.svelte';

export type SwitcherEntry = WorkspaceProject;

/** Roots compare the same however they were written: slashes, trailing slash, case. */
export function normRoot(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

/**
 * One entry per project of a workspace, however its folder was spelled: `/a/b`, `/a/b/` and `/A/B` are one place, and a list that
 * shows it twice shows a second, nameless one next to the right one. Where two spellings are listed the one with a name is kept.
 */
export function oneEntryPerProject(projects: SwitcherEntry[]): SwitcherEntry[] {
  const kept = new Map<string, SwitcherEntry>();
  for (const p of projects) {
    if (!p.root) continue;
    const key = `${p.workspaceId}|${normRoot(p.root)}`;
    const before = kept.get(key);
    if (!before || (!before.name && p.name)) kept.set(key, p);
  }
  return [...kept.values()];
}

