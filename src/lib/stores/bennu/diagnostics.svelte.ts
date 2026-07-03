/**
 * Bennu project-level diagnostics store — the JDK resolution status + the non-compliant
 * (wrong-encoding) source files, shared by the Problems panel (tree sections) and the
 * titlebar warning badge.
 *
 * Both come from the backend and are re-fetched when the project changes or the index
 * (re)builds — the encoding report populates after the project phase, and a rebuild can
 * change either. The window drives `refresh()` from an effect on
 * `projectStore.project?.root` + `bennuIndexStore.buildRevision`.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import {
  jdkStatus as ipcJdkStatus,
  encodingReport as ipcEncodingReport,
  type JdkStatus,
  type EncodingIssue,
} from '$lib/ipc/bennu/inspect';

function createBennuDiagnosticsStore() {
  let jdk = $state<JdkStatus | null>(null);
  let encodingIssues = $state<EncodingIssue[]>([]);
  // Guards against an out-of-order response clobbering a newer one (root change / rebuild).
  let token = 0;

  async function refresh(root: string | null): Promise<void> {
    if (!root) {
      jdk = null;
      encodingIssues = [];
      return;
    }
    const mine = ++token;
    const [j, e] = await Promise.all([
      ipcJdkStatus(root).catch(() => null),
      ipcEncodingReport(root).catch(() => [] as EncodingIssue[]),
    ]);
    if (mine !== token) return; // superseded
    jdk = j;
    encodingIssues = e;
  }

  return {
    get jdk() { return jdk; },
    get encodingIssues() { return encodingIssues; },
    /** No JDK installed at all — completion / navigation can't resolve the standard library. */
    get jdkMissing() { return jdk != null && !jdk.any_installed; },
    /** A JDK is installed, but not the exact level the project targets — a fallback stands in. */
    get jdkFallback() { return jdk != null && jdk.any_installed && !jdk.exact; },
    /** Anything worth a titlebar badge (missing JDK) — the highest-severity project issue. */
    get hasCriticalIssue() { return jdk != null && !jdk.any_installed; },

    refresh,
    reset() { token += 1; jdk = null; encodingIssues = []; },
  };
}

export const bennuDiagnosticsStore = createBennuDiagnosticsStore();
