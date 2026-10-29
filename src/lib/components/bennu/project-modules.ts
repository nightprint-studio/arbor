/**
 * Which build tool lives where — the pure half of Bennu's module awareness.
 *
 * `bennu_project_modules` lists the manifests of ONE project (`Cargo` in `src-tauri`, `Node` at the
 * root…). The window works on a WORKSPACE of several projects, so everything here is about the flat
 * list of {@link ModuleRef}s across all of them: does the workspace carry this ecosystem at all
 * (what the rail asks), which module does a file belong to (what a command acts on when nothing is
 * selected), and what folder does a tool take as its `root` (the module's, not the project's).
 *
 * Pure on purpose — no store, no IPC — so the rules are tested without a window.
 */

import type { ModuleKind, ProjectModule } from '$lib/ipc/bennu/modules';
import type { ProjectKind } from '$lib/types/bennu';

/** A module placed in a workspace: which project it belongs to, and the folder tools should run in. */
export interface ModuleRef {
  /** Unique across the workspace. */
  id: string;
  /** The owning project's canonical root. */
  root: string;
  kind: ModuleKind;
  /** Relative to {@link root}; `.` for the project root itself. */
  dir: string;
  /** The folder a tool takes as its `root` — the project root joined with {@link dir}. */
  toolRoot: string;
}

export type ModulesByRoot = Readonly<Record<string, readonly ProjectModule[]>>;

function trimSlash(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/+$/, '');
}

/** The project root joined with a module's folder; the root itself for `.` (or empty). */
export function toolRootOf(root: string, dir: string): string {
  const base = trimSlash(root);
  const rel = trimSlash(dir).replace(/^\.\//, '');
  return !rel || rel === '.' ? base : `${base}/${rel}`;
}

/** The module list a project is known to have before (or without) asking the backend: its primary
 *  kind alone. Keeps the rail steady across the first paint and on a backend that cannot answer. */
export function modulesFromKind(kind: ProjectKind | undefined): ProjectModule[] {
  if (kind === 'maven') return [{ kind: 'Maven', dir: '.' }];
  if (kind === 'cargo') return [{ kind: 'Cargo', dir: '.' }];
  return [];
}

/** Every module of every project, in workspace order; inside a project the root first, then by folder. */
export function moduleRefs(roots: readonly string[], byRoot: ModulesByRoot): ModuleRef[] {
  const out: ModuleRef[] = [];
  for (const root of roots) {
    const mods = [...(byRoot[root] ?? [])].sort(
      (a, b) => Number(b.dir === '.') - Number(a.dir === '.') || a.dir.localeCompare(b.dir),
    );
    for (const m of mods) {
      out.push({ id: `${root}|${m.kind}|${m.dir}`, root, kind: m.kind, dir: m.dir, toolRoot: toolRootOf(root, m.dir) });
    }
  }
  return out;
}

/** The modules of the given ecosystems. */
export function ofKinds(refs: readonly ModuleRef[], kinds: readonly ModuleKind[]): ModuleRef[] {
  return refs.filter((r) => kinds.includes(r.kind));
}

/** Whether the workspace carries `kind` at all — the rail's visibility test. */
export function hasEcosystem(refs: readonly ModuleRef[], kind: ModuleKind): boolean {
  return refs.some((r) => r.kind === kind);
}

/** Modules grouped by owning project, keeping workspace order. */
export function groupByProject(refs: readonly ModuleRef[]): { root: string; modules: ModuleRef[] }[] {
  const groups: { root: string; modules: ModuleRef[] }[] = [];
  for (const r of refs) {
    const last = groups[groups.length - 1];
    if (last && last.root === r.root) last.modules.push(r);
    else groups.push({ root: r.root, modules: [r] });
  }
  return groups;
}

function inside(file: string, folder: string): boolean {
  const f = trimSlash(file).toLowerCase();
  const d = folder.toLowerCase();
  return f === d || f.startsWith(`${d}/`);
}

/** The module `file` is in: the one with the deepest folder containing it, or `undefined`. */
export function moduleOfFile(refs: readonly ModuleRef[], file: string | null | undefined): ModuleRef | undefined {
  if (!file) return undefined;
  let best: ModuleRef | undefined;
  for (const r of refs) {
    if (!inside(file, r.toolRoot)) continue;
    if (!best || r.toolRoot.length > best.toolRoot.length) best = r;
  }
  return best;
}

export interface TargetQuery {
  /** The candidates, already narrowed to the ecosystems the caller works on. */
  refs: readonly ModuleRef[];
  /** What the user picked in the tree, if anything. */
  selectedId?: string | null;
  activeFile?: string | null;
  /** The project on screen. */
  activeRoot?: string | null;
}

/**
 * The module a command acts on.
 *
 * In order: the one the user selected; else the one containing the active file; else the first
 * module of the project on screen (so a single-ecosystem project behaves exactly as it did before
 * modules existed); else the first one anywhere.
 */
export function resolveTarget(q: TargetQuery): ModuleRef | undefined {
  const { refs } = q;
  if (!refs.length) return undefined;
  const picked = q.selectedId ? refs.find((r) => r.id === q.selectedId) : undefined;
  if (picked) return picked;
  const byFile = moduleOfFile(refs, q.activeFile);
  if (byFile) return byFile;
  return refs.find((r) => r.root === q.activeRoot) ?? refs[0];
}

/**
 * What a palette command should do about the module: `act` on the one that applies, or `ask`.
 *
 * It asks only when the answer is ambiguous — several modules, and neither a pinned selection nor
 * the file being edited says which. A pin or the active file already answers the question, and
 * interrupting every keystroke-driven command with a picker for something the window knows would
 * make the palette slower than the button it is meant to beat.
 */
export function moduleChoice(q: TargetQuery): { ask: false; module: ModuleRef | undefined } | { ask: true; modules: readonly ModuleRef[] } {
  if (q.refs.length <= 1) return { ask: false, module: q.refs[0] };
  const pinned = q.selectedId ? q.refs.find((r) => r.id === q.selectedId) : undefined;
  if (pinned) return { ask: false, module: pinned };
  const byFile = moduleOfFile(q.refs, q.activeFile);
  if (byFile) return { ask: false, module: byFile };
  return { ask: true, modules: q.refs };
}
