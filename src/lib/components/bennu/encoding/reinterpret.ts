/**
 * Reinterpret a file: change the encoding Bennu READS it with. Nothing is written to the file —
 * the choice is an `encoding_overrides` entry in the profile's config, so it is undone by choosing
 * "As the project reads it" again. (Rewriting the bytes is `encoding-convert`'s job.)
 */
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
import { getBennuConfig } from '$lib/ipc/bennu/config';

const same = (a: string, b: string) => a.replace(/\\/g, '/').toLowerCase() === b.replace(/\\/g, '/').toLowerCase();

/** The override pinned to `path`, if any. */
export function fileEncodingOverride(path: string): string | undefined {
  const map = bennuConfigStore.cfg?.encoding_overrides ?? {};
  const key = Object.keys(map).find((k) => same(k, path));
  return key ? map[key] : undefined;
}

/**
 * Pin `path` to `label` (or release it with `null`), then re-read it. Unsaved edits are saved first:
 * the reload discards the buffer, and what it holds belongs on disk in the encoding it was written in.
 */
export async function reinterpretFile(path: string, label: string | null): Promise<void> {
  if (projectStore.isDirty(path)) await projectStore.saveAllDirty();
  const current = await getBennuConfig();
  const next = { ...(current.encoding_overrides ?? {}) };
  for (const key of Object.keys(next)) if (same(key, path)) delete next[key];
  if (label) next[path] = label;
  await bennuConfigStore.patch({ encoding_overrides: next });
  await projectStore.reload(path);
  const root = projectStore.project?.root;
  if (root) void projectStore.refreshProjectInfo(root);
}
