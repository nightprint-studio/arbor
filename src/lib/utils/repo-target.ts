/**
 * Which repository a "go to Corvus" lands on, and what has to happen first.
 *
 * Pure: the decision from what Corvus knows (open tabs, the registry, the workspaces), so the rules are tests.
 * `openRepoFromPath` carries the plan out.
 */

export interface TabLike { id: string; path: string }
export interface RegistryLike { id: string; path: string }
export interface WorkspaceLike { id: string; name: string; repo_ids: string[]; order?: number }

/** Compare paths the way the UI does: separators and case folded, no trailing slash. */
export function normPath(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

export type OpenPlan =
  /** Already a tab here: bring it forward. */
  | { kind: 'activate'; tabId: string }
  /** Registered: open it. `workspaceId` is the workspace to be in (null: it belongs to none — open it without adding it anywhere). */
  | { kind: 'open'; repoId: string; workspaceId: string | null; switchWorkspace: boolean }
  /** Not registered: the user decides whether (and where) to add it. */
  | { kind: 'ask' };

/**
 * - an open tab for it → that tab;
 * - registered → open it in the active workspace if that contains it, else in the FIRST workspace that does
 *   (in the order the workspaces are listed), else without adding it to any;
 * - not registered → ask.
 */
export function planOpen(path: string, tabs: TabLike[], registry: RegistryLike[], workspaces: WorkspaceLike[], activeId: string | null): OpenPlan {
  const target = normPath(path);
  const tab = tabs.find((t) => normPath(t.path) === target);
  if (tab) return { kind: 'activate', tabId: tab.id };

  const repo = registry.find((r) => normPath(r.path) === target);
  if (!repo) return { kind: 'ask' };

  const containing = [...workspaces].sort((a, b) => (a.order ?? 0) - (b.order ?? 0)).filter((w) => w.repo_ids.includes(repo.id));
  if (activeId && containing.some((w) => w.id === activeId)) return { kind: 'open', repoId: repo.id, workspaceId: activeId, switchWorkspace: false };
  if (containing.length > 0) return { kind: 'open', repoId: repo.id, workspaceId: containing[0].id, switchWorkspace: true };
  return { kind: 'open', repoId: repo.id, workspaceId: null, switchWorkspace: false };
}

/** What the user picked for a repository that is not registered. */
export type AddChoice = { kind: 'add'; workspaceId: string } | { kind: 'only' };

/** The choice the prompt starts on: the active workspace, or just opening it when there is none. */
export function defaultChoice(activeId: string | null, workspaces: WorkspaceLike[]): AddChoice {
  return activeId && workspaces.some((w) => w.id === activeId) ? { kind: 'add', workspaceId: activeId } : { kind: 'only' };
}

/** The value a `<Select>` carries for a choice, and back. */
export const ONLY_VALUE = '';
export const choiceValue = (c: AddChoice) => (c.kind === 'add' ? c.workspaceId : ONLY_VALUE);
export const choiceFromValue = (v: string): AddChoice => (v === ONLY_VALUE ? { kind: 'only' } : { kind: 'add', workspaceId: v });
