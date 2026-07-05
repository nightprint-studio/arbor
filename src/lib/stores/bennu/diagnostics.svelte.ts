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
import type { Diagnostic, FileDiagnostics } from '$lib/types/bennu';

/** Normalise a path to forward slashes so BE (`/`) and FE (`\`) keys compare. */
function norm(path: string): string {
  return path.replace(/\\/g, '/');
}

function createBennuDiagnosticsStore() {
  let jdk = $state<JdkStatus | null>(null);
  let encodingIssues = $state<EncodingIssue[]>([]);
  // The last whole-project validation's diagnostics, grouped by file (set by the run store after a
  // `Validate project` run). Shown in the Problems panel as per-file sections so a project-wide
  // validation lands where problems belong, not only in the Build tool window.
  let projectDiagnostics = $state<FileDiagnostics[]>([]);
  // Whether project-wide problems are "armed": once the user runs an explicit "Validate project"
  // (opting into the project-wide view), a save silently refreshes it (cross-file). Before that we
  // don't auto-populate the panel — on a legacy project with thousands of dependency problems, a
  // stray save shouldn't flood it unasked.
  let armed = $state(false);
  // The active file's LIVE diagnostics — pushed by the editor on every (debounced) buffer
  // validation, so the Problems panel's active-file section updates as you type / fix, without a
  // whole-project re-run. Keyed by `activeFile` so a stale push for an old file is ignored.
  let activeFile = $state<string | null>(null);
  let activeFileDiagnostics = $state<Diagnostic[]>([]);
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

  /** Merge one file's diagnostics into the project-wide map: replace its entry, remove it (empty),
   *  or append a new one — keyed by normalised path. Shared by the live active-file push and the
   *  on-save refresh's active-file preservation. */
  function applyFileEntry(file: string, list: Diagnostic[]) {
    const key = norm(file);
    const idx = projectDiagnostics.findIndex((f) => norm(f.file) === key);
    if (list.length === 0) {
      if (idx >= 0) {
        projectDiagnostics = [
          ...projectDiagnostics.slice(0, idx),
          ...projectDiagnostics.slice(idx + 1),
        ];
      }
    } else if (idx >= 0) {
      const next = projectDiagnostics.slice();
      next[idx] = { file, diagnostics: list };
      projectDiagnostics = next;
    } else {
      projectDiagnostics = [...projectDiagnostics, { file, diagnostics: list }];
    }
  }

  return {
    get jdk() { return jdk; },
    get encodingIssues() { return encodingIssues; },
    /** The last whole-project validation's diagnostics, grouped by file (empty until a run). */
    get projectDiagnostics() { return projectDiagnostics; },
    /** Total problems across the last project validation. */
    get projectProblemCount() {
      return projectDiagnostics.reduce((n, f) => n + f.diagnostics.length, 0);
    },
    /** Replace the project-validation diagnostics (called by the run store after a validation). Arms
     *  the silent on-save cross-file refresh from now on. */
    setProjectDiagnostics(list: FileDiagnostics[]) { projectDiagnostics = list; armed = true; },
    /** Clear the project-validation diagnostics (a new run starts, or the results go stale). */
    clearProjectDiagnostics() { projectDiagnostics = []; },
    /** Whether project-wide problems are armed (an explicit validation has run) — the gate the save
     *  hook checks before its silent cross-file refresh. */
    get armed() { return armed; },

    /** The active file's live diagnostics + which file they're for (the editor's last buffer
     *  validation). The Problems panel reads these for its active-file section. */
    get activeFile() { return activeFile; },
    get activeFileDiagnostics() { return activeFileDiagnostics; },
    /** Publish the active file's freshly-validated (live buffer) diagnostics. Called by the editor
     *  after each debounced validation. Also refreshes THIS file's entry in the project-wide map so
     *  the Problems panel stays correct after you switch away — a fixed file drops out, a newly
     *  broken one appears, without waiting for a manual "Validate project" re-run. */
    setActiveFileDiagnostics(file: string, list: Diagnostic[]) {
      activeFile = file;
      activeFileDiagnostics = list;
      applyFileEntry(file, list);
    },
    /** Replace the project-wide diagnostics with a fresh whole-project (disk) validation — the
     *  silent on-save cross-file refresh. Preserves the active file's LIVE (buffer) entry so an
     *  unsaved edit in the file you're looking at isn't clobbered by the disk-based result. */
    refreshProjectDiagnostics(list: FileDiagnostics[]) {
      projectDiagnostics = list;
      if (activeFile) applyFileEntry(activeFile, activeFileDiagnostics);
    },
    /** No JDK installed at all — completion / navigation can't resolve the standard library. */
    get jdkMissing() { return jdk != null && !jdk.any_installed; },
    /** A JDK is installed, but not the exact level the project targets — a fallback stands in. */
    get jdkFallback() { return jdk != null && jdk.any_installed && !jdk.exact; },
    /** Anything worth a titlebar badge (missing JDK) — the highest-severity project issue. */
    get hasCriticalIssue() { return jdk != null && !jdk.any_installed; },

    refresh,
    reset() {
      token += 1;
      jdk = null;
      encodingIssues = [];
      projectDiagnostics = [];
      activeFile = null;
      activeFileDiagnostics = [];
      armed = false;
    },
  };
}

export const bennuDiagnosticsStore = createBennuDiagnosticsStore();
