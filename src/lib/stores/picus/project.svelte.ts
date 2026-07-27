/**
 * Picus project — the script repository on disk: per-dialect branches, their
 * folders (each with a role), and the files inside.
 *
 * The structural invariant: **the dialect belongs to the folder**. Nothing here
 * exposes a "current dialect"; every consumer reads it off the branch it is
 * looking at.
 *
 * What a live database contains is NOT here — that is `schema.svelte.ts`. The
 * project is what is on disk; the schema is what a connection reports.
 *
 * MOCK: fed from `ipc/picus/mock` until `picus-be` scans a real project.
 */

import type { Branch, Dialect, InventoryObject, Project, ScriptFile } from '$lib/types/picus';
import { MOCK_FILE_TEXT, MOCK_INVENTORY, MOCK_PROJECT } from '$lib/ipc/picus/mock';

function createProjectStore() {
  let project = $state<Project | null>(MOCK_PROJECT);
  let inventory = $state<InventoryObject[]>(MOCK_INVENTORY);
  /** Tree expansion, keyed by branch/folder id. */
  let expanded = $state<Record<string, boolean>>({
    ora: true, 'ora-init': false, 'ora-upd': true,
    pg: true, 'pg-init': false, 'pg-upd': true,
  });
  let fileFilter = $state('');

  const branches = $derived<Branch[]>(project?.branches ?? []);

  /** Every file across every branch — the flat form searches and pickers want. */
  const allFiles = $derived<ScriptFile[]>(
    branches.flatMap((b) => b.folders.flatMap((f) => f.files)),
  );

  return {
    get project() { return project; },
    get branches() { return branches; },
    get inventory() { return inventory; },
    get allFiles() { return allFiles; },
    get fileFilter() { return fileFilter; },
    get fileCount() { return allFiles.length; },

    /** Files whose encoding no longer matches what the folder expects (ENC001). */
    get driftedFiles() {
      return allFiles.filter((f) => f.encoding !== f.expectedEncoding);
    },

    isExpanded(id: string) { return expanded[id] ?? false; },
    toggle(id: string) { expanded = { ...expanded, [id]: !expanded[id] }; },
    setExpanded(id: string, open: boolean) { expanded = { ...expanded, [id]: open }; },

    setFileFilter(v: string) { fileFilter = v; },

    fileByPath(path: string): ScriptFile | null {
      return allFiles.find((f) => f.path === path) ?? null;
    },

    /** Which branch a project-relative path belongs to — and therefore its dialect. */
    branchOfFile(path: string): Branch | null {
      return branches.find((b) => b.folders.some((f) => f.files.some((x) => x.path === path))) ?? null;
    },

    dialectOfFile(path: string): Dialect | null {
      return this.branchOfFile(path)?.dialect ?? null;
    },

    /** File contents for the editor tab. MOCK — real reads go through picus-be. */
    fileText(path: string): string {
      return MOCK_FILE_TEXT[path] ?? `-- ${path}\n-- (file contents are served by picus-be; this window is running on fixtures)\n`;
    },
  };
}

export const picusProjectStore = createProjectStore();
