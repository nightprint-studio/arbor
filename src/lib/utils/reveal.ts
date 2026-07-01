/**
 * Single entry point for the app's "Open / Reveal in File Explorer" actions.
 *
 * Every place that wants to show a path in a file manager (worktree info,
 * plugin folders, notification reveals, …) goes through here so the OS-vs-
 * built-in choice lives in exactly one spot. When the user opts in
 * (Settings → File Explorer → "Open in the built-in explorer", persisted as
 * `explorer.reveal_in_builtin`) the path is routed to Arbor's built-in explorer
 * window; otherwise it's handed to the platform shell as before.
 *
 * The explorer's OWN "Reveal in File Explorer" context item deliberately does
 * NOT use this — it's the explicit escape hatch to the OS file manager.
 */

import { openPath } from '@tauri-apps/plugin-opener';
import { fsRevealInDir, revealInExplorerWindow } from '$lib/ipc/fs';
import { explorerStore } from '$lib/stores/sitta/explorer.svelte';

/** Open a folder in the file explorer (built-in window when opted in, else the
 *  OS file manager). No selection — the folder is shown as the listing. */
export async function openFolder(path: string): Promise<void> {
  if (explorerStore.revealInBuiltin) return revealInExplorerWindow(path, false);
  return openPath(path);
}

/** Reveal a file, selecting it inside its containing folder (built-in window
 *  when opted in, else the OS file manager). */
export async function revealFile(path: string): Promise<void> {
  if (explorerStore.revealInBuiltin) return revealInExplorerWindow(path, true);
  return fsRevealInDir(path);
}
