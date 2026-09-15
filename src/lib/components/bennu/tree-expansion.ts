/**
 * Which rows of Bennu's trees are open — as something that survives closing the panel, switching
 * project and restarting the app.
 *
 * The state is a set of **overrides**, `scoped key → open`, per project root. An override is only
 * recorded where a row differs from what its tree would draw by default, so a tree whose defaults
 * change (a Maven reactor that grows past the size where modules start collapsed) follows the new
 * default for every row nobody touched.
 *
 * ## Keys are identities, never positions
 *
 * A key names *what* a row is, so it still names the same row after a reload reorders or rebuilds
 * the list:
 *
 * - **Project tree** (`project:`) — the folder's path relative to the project root
 *   (`core/src/main/java/it/acme`). The same project checked out somewhere else keeps its layout.
 * - **Maven tool window** (`maven:`) — `profiles`, `module/<artifactId>` and, under it,
 *   `/lifecycle`, `/plugins` and `/plugin/<groupId>:<artifactId>`.
 *
 * Persisted on the project's session in `workspace.toml` as two flat lists (see
 * {@link PersistedExpansion}), because that struct is a TOML array-of-tables and a nested table
 * there would trip "values must be emitted before tables".
 *
 * Pure — the reactive store is `stores/bennu/tree-expansion.svelte.ts`.
 */

import type { TreeNode } from '$lib/types/bennu';
import type { MavenModel, MavenModule, MavenPlugin } from '$lib/ipc/bennu/maven-build';

/** The trees whose expansion is remembered. One word each: it is the prefix of every key. */
export type ExpansionScope = 'project' | 'maven';

/** The persisted form — the two lists on a `ProjectSession`. */
export interface PersistedExpansion {
  /** Scoped keys of rows open against their default. */
  expanded: string[];
  /** Scoped keys of rows closed against their default. */
  collapsed: string[];
}

/** `project` + `src/main` → `project:src/main`. */
export function scopedKey(scope: ExpansionScope, key: string): string {
  return `${scope}:${key}`;
}

/** A persisted entry worth reading back: `<scope>:<non-empty key>`. Scopes this build does not
 *  know are kept — a newer Bennu may have written them, and dropping them would erase its state. */
function isScopedKey(entry: unknown): entry is string {
  return typeof entry === 'string' && /^[a-z][a-z0-9-]*:.+/.test(entry);
}

function trimmedRoot(root: string): string {
  return root.replace(/\\/g, '/').replace(/\/+$/, '');
}

/**
 * `path` relative to `root`, forward-slashed — the Project tree's stable key — or `null` for the
 * root itself and for anything outside it.
 *
 * The prefix is compared case-insensitively: Windows hands the same folder back with a different
 * drive-letter case depending on who asked, and a key that depends on that is a key that is lost.
 */
export function relativeKey(root: string, path: string): string | null {
  const base = trimmedRoot(root);
  const fwd = path.replace(/\\/g, '/').replace(/\/+$/, '');
  if (!base || fwd.length <= base.length + 1) return null;
  if (fwd.slice(0, base.length + 1).toLowerCase() !== `${base}/`.toLowerCase()) return null;
  return fwd.slice(base.length + 1);
}

/** The inverse of {@link relativeKey}: the absolute id a tree row carries. */
export function absoluteId(root: string, key: string): string {
  return `${trimmedRoot(root)}/${key}`;
}

/**
 * Read a persisted session's lists into overrides. Tolerant: absent lists (a session written
 * before expansion was remembered) and malformed entries read as "nothing remembered". A key in
 * both lists reads as collapsed — the cheaper mistake of the two.
 */
export function decodeExpansion(persisted: Partial<PersistedExpansion> | null | undefined): Map<string, boolean> {
  const out = new Map<string, boolean>();
  for (const key of persisted?.expanded ?? []) if (isScopedKey(key)) out.set(key, true);
  for (const key of persisted?.collapsed ?? []) if (isScopedKey(key)) out.set(key, false);
  return out;
}

/** Overrides → the persisted lists, sorted so an unchanged state writes an unchanged file. */
export function encodeExpansion(state: Iterable<[string, boolean]>): PersistedExpansion {
  const expanded: string[] = [];
  const collapsed: string[] = [];
  for (const [key, open] of state) (open ? expanded : collapsed).push(key);
  return { expanded: expanded.sort(), collapsed: collapsed.sort() };
}

/**
 * The scoped keys of `scope` that name no row any more, given every key a **complete** load of
 * that tree produced. Other scopes are never touched: one tree finishing a load says nothing
 * about the rows of another.
 */
export function staleKeys(keys: Iterable<string>, scope: ExpansionScope, live: ReadonlySet<string>): string[] {
  const prefix = `${scope}:`;
  const out: string[] = [];
  for (const key of keys) {
    if (key.startsWith(prefix) && !live.has(key.slice(prefix.length))) out.push(key);
  }
  return out;
}

/** Every directory under `tree` (the root node excluded), as Project-tree keys. */
export function directoryKeys(root: string, tree: TreeNode | null | undefined): Set<string> {
  const out = new Set<string>();
  const walk = (node: TreeNode) => {
    for (const child of node.children) {
      if (!child.is_dir) continue;
      const key = relativeKey(root, child.path);
      if (key) out.add(key);
      walk(child);
    }
  };
  if (tree) walk(tree);
  return out;
}

type ModuleIdentity = Pick<MavenModule, 'artifact_id' | 'dir'>;
type PluginIdentity = Pick<MavenPlugin, 'group_id' | 'artifact_id'>;

/** A module by its artifactId — what it is called in the reactor, unchanged by a pom reformat or a
 *  re-read. The directory only stands in for a pom that does not parse far enough to name one. */
function moduleKey(module: ModuleIdentity): string {
  return `module/${module.artifact_id.trim() || module.dir || '.'}`;
}

/** The Maven tool window's keys, one builder per kind of row. */
export const mavenKeys = {
  profiles: 'profiles',
  module: moduleKey,
  lifecycle: (module: ModuleIdentity) => `${moduleKey(module)}/lifecycle`,
  plugins: (module: ModuleIdentity) => `${moduleKey(module)}/plugins`,
  plugin: (module: ModuleIdentity, plugin: PluginIdentity) =>
    `${moduleKey(module)}/plugin/${plugin.group_id}:${plugin.artifact_id}`,
};

/** Every key a Maven model draws a collapsible row for — what pruning keeps. */
export function mavenLiveKeys(model: Pick<MavenModel, 'modules' | 'profiles'>): Set<string> {
  const out = new Set<string>();
  if (model.profiles.length > 0) out.add(mavenKeys.profiles);
  for (const module of model.modules) {
    out.add(mavenKeys.module(module));
    out.add(mavenKeys.lifecycle(module));
    out.add(mavenKeys.plugins(module));
    // Only a plugin that binds goals has a section; the rest are plain rows with nothing to open.
    for (const plugin of module.plugins) {
      if (plugin.goals.length > 0) out.add(mavenKeys.plugin(module, plugin));
    }
  }
  return out;
}
