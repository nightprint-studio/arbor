/**
 * Tell the plugins which file the editor is on.
 *
 * ## Why this exists
 *
 * A plugin that opens a panel *about the file you are editing* — a preview, a linter, a
 * reference pane — has to know when that file changes. Nothing said so: `arbor:tab_switch` is
 * about project tabs and is fired by the shell for Corvus, and Bennu declared no events of its
 * own. So a shader preview opened on one `.wgsl` kept showing it while you edited another, and
 * the panel's title was the only thing that admitted it.
 *
 * `bennu:file_opened` is that event, and `bennu:file_closed` is the other end of it.
 *
 * ## Why one watcher instead of a fire at every setter
 *
 * `activeFilePath` is written from at least five places — opening, switching, restoring a
 * session, closing the last tab, changing project. Firing at each is five chances to add a
 * sixth and forget. This is called from a single `$effect` on that one value, so any way it
 * changes is announced, including ways that do not exist yet.
 */
import { execHook } from '$lib/ipc/plugin';

/** Mirrors `hook_names::bennu` — the constants live in Rust and this is the seam. */
const FILE_OPENED = 'bennu:file_opened';
const FILE_CLOSED = 'bennu:file_closed';

/** Last path announced, so a re-render that recomputes the same value stays quiet. */
let announced: string | null = null;

/**
 * Announce the editor's active file. Safe to call on every change of the underlying value —
 * a repeat of what was already announced is dropped.
 */
export function notifyActiveFile(path: string | null): void {
  const next = path && path.length > 0 ? path : null;
  if (next === announced) return;
  announced = next;

  if (!next) {
    void execHook(FILE_CLOSED, '{}').catch(() => { /* no host in this window */ });
    return;
  }

  const name = next.split(/[/\\]/).pop() ?? next;
  const dot  = name.lastIndexOf('.');
  // The extension is what a plugin filters on, so it is handed over rather than left for
  // every subscriber to re-derive from the path with its own idea of what a dot means.
  const ext  = dot > 0 ? name.slice(dot + 1).toLowerCase() : undefined;

  const ctx: Record<string, unknown> = { path: next, name };
  if (ext) ctx.ext = ext;
  void execHook(FILE_OPENED, JSON.stringify(ctx)).catch(() => { /* no host in this window */ });
}

/** Forget what was announced — for a window tearing down, so the next mount re-announces. */
export function resetActiveFileNotifier(): void {
  announced = null;
}
