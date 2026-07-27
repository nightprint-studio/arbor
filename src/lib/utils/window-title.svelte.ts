/**
 * Keep the OS window title in sync with what the window is actually showing.
 *
 * Every Arbor window is created with a STATIC title ("Corvus — Arbor"), which
 * the user meets again in the Windows taskbar, in Alt-Tab, in Mission Control
 * and in the macOS Window menu — three open repositories produce three
 * identical entries there. Each shell calls this once with its live subject
 * (repository, project, folder) and the title follows it.
 *
 * Shape: `subject — Product` when there is a subject, `Product — Arbor`
 * otherwise (the build-time title, so an empty shell reads unchanged).
 */
import { setWindowTitle } from '$lib/ipc/window';

/**
 * Publish `subject() — product` as this window's title, re-publishing whenever
 * the subject changes. Call during component init (it owns an `$effect`).
 *
 * @param product Product name as the user knows it — "Corvus", "Bennu",
 *                "merula", "File Explorer".
 * @param subject Reactive accessor for the window's current subject; return
 *                null/empty when the window is showing nothing in particular.
 */
export function syncWindowTitle(
  product: string,
  subject: () => string | null | undefined,
  opts?: { active?: () => boolean },
): void {
  $effect(() => {
    // Inside the tabbed container several shells are mounted at once; only the
    // tab on screen may name the window, or they'd overwrite each other.
    if (opts?.active?.() === false) return;
    const s = subject()?.trim();
    void setWindowTitle(s ? `${s} — ${product}` : `${product} — Arbor`).catch(() => {
      // Non-Tauri (SSR/tests) or a window that just went away — the title is
      // cosmetic, never worth surfacing.
    });
  });
}
