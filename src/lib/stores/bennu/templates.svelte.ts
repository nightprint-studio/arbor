/**
 * Code templates — the ones the user owns, by kind, as the open project sees them.
 *
 * One store for every surface that shows them — the DTO Lab, Generate from a template, the New file
 * dialog, Settings, a template's preview — so a template created or chosen in one of them is what
 * the others show, without asking the backend again from each.
 */

import {
  deleteTemplate,
  listTemplates,
  newTemplate,
  openTemplate,
  renameTemplate,
  setTemplateAbbrev,
  setProjectTemplate,
  type KindTemplates,
  type TemplateKindId,
} from '$lib/ipc/bennu/templates';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { projectStore } from './project.svelte';

function createBennuTemplatesStore() {
  let kinds = $state<Partial<Record<TemplateKindId, KindTemplates>>>({});
  let order = $state<TemplateKindId[]>([]);
  /** The project root the lists were read for — a project's choices are part of the answer. */
  let loadedFor = $state<string | null | undefined>(undefined);

  const root = () => projectStore.project?.root ?? null;

  async function load(kind?: TemplateKindId): Promise<void> {
    const at = root();
    try {
      const found = await listTemplates(at, kind);
      // Another project opened while this was asked: its lists are not these.
      if (at !== root()) return;
      const next = loadedFor === at ? { ...kinds } : {};
      for (const entry of found) next[entry.kind] = entry;
      kinds = next;
      loadedFor = at;
      if (!kind) order = found.map((entry) => entry.kind);
    } catch (e) {
      toastStore.show(`Couldn't list the code templates: ${e}`, 'error');
    }
  }

  return {
    /** One kind's templates, once loaded for the open project. */
    of(kind: TemplateKindId): KindTemplates | null {
      return loadedFor === root() ? kinds[kind] ?? null : null;
    },
    /** Every kind, in the backend's order — after a `load()` with no kind. */
    get all(): KindTemplates[] {
      if (loadedFor !== root()) return [];
      return order.flatMap((kind) => (kinds[kind] ? [kinds[kind]!] : []));
    },

    load,

    async setProject(kind: TemplateKindId, name: string) {
      const at = root();
      if (!at) return;
      try {
        await setProjectTemplate(at, kind, name);
        await load(kind);
      } catch (e) {
        toastStore.show(`Couldn't save the project's template: ${e}`, 'error');
      }
    },

    /** Create a template from `from` and open it in the editor. `extension` is what it writes, for
     *  the kinds that let the author choose. */
    async create(
      kind: TemplateKindId,
      name: string,
      from: string | null,
      extension?: string | null,
    ): Promise<boolean> {
      try {
        const path = await newTemplate(kind, name, from, extension);
        await load(kind);
        await projectStore.openFile(path);
        return true;
      } catch (e) {
        toastStore.show(`${e}`, 'error');
        return false;
      }
    },

    /** Rename a template of the user's. A tab open on it follows the file, which has moved. */
    async rename(kind: TemplateKindId, name: string, newName: string): Promise<boolean> {
      const before = kinds[kind]?.templates.find((t) => t.name === name)?.path ?? null;
      try {
        const path = await renameTemplate(kind, name, newName, root());
        await load(kind);
        if (before && projectStore.openFilePaths.includes(before)) {
          projectStore.closeFile(before);
          await projectStore.openFile(path);
        }
        return true;
      } catch (e) {
        toastStore.show(`${e}`, 'error');
        return false;
      }
    },

    async remove(kind: TemplateKindId, name: string) {
      try {
        await deleteTemplate(kind, name);
        await load(kind);
        toastStore.show(`Deleted “${name}”`, 'success');
      } catch (e) {
        toastStore.show(`Couldn't delete the template: ${e}`, 'error');
      }
    },

    /** Set the word that expands an abbreviation; empty means "name it by its file". */
    async setAbbrev(name: string, abbrev: string): Promise<boolean> {
      try {
        await setTemplateAbbrev(name, abbrev);
        await load('live');
        return true;
      } catch (e) {
        toastStore.show(`${e}`, 'error');
        return false;
      }
    },

    /** Open a template in the editor — a built-in too, which opens read-only. */
    async edit(kind: TemplateKindId, name: string) {
      try {
        await projectStore.openFile(await openTemplate(kind, name));
      } catch (e) {
        toastStore.show(`Couldn't open the template: ${e}`, 'error');
      }
    },
  };
}

export const bennuTemplatesStore = createBennuTemplatesStore();
