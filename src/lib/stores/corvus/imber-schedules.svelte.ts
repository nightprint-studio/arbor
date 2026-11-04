/**
 * Imber's schedules, as the Schedules window shows them: the list, what is being edited, and the verbs
 * (save, turn on or off, run now, delete). The backend owns the list and the loop that fires it; this is the
 * window's copy, reloaded when a run ends so "last result" is never stale.
 */

import {
  imberScheduleDelete, imberScheduleList, imberScheduleRunNow, imberScheduleSave, imberScheduleSetEnabled,
} from '$lib/ipc/corvus/imber';
import type { ImberJob, ImberJobInput } from '$lib/types/corvus/imber';
import { uiStore } from '../ui.svelte';

function createImberSchedulesStore() {
  let open = $state(false);
  let jobs = $state<ImberJob[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  /** The job being edited, `'new'` for a blank form, `null` for the list. */
  let editing = $state<ImberJob | 'new' | null>(null);
  let deleting = $state<ImberJob | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      jobs = await imberScheduleList();
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  async function guarded<T>(what: string, run: () => Promise<T>): Promise<T | null> {
    try {
      return await run();
    } catch (err) {
      uiStore.showToast(`${what}: ${err}`, 'error');
      return null;
    }
  }

  return {
    get open() { return open; },
    get jobs() { return jobs; },
    get loading() { return loading; },
    get error() { return error; },
    get editing() { return editing; },
    get deleting() { return deleting; },

    load,
    /** Open the window on the list (or straight on a new schedule). */
    show(opts: { create?: boolean } = {}) { open = true; editing = opts.create ? 'new' : null; void load(); },
    hide() { open = false; editing = null; deleting = null; },

    edit(job: ImberJob | 'new') { editing = job; },
    closeEditor() { editing = null; },

    /** Save from the form. `true` when it was saved (the editor closes); on failure the reason is toasted. */
    async save(input: ImberJobInput): Promise<boolean> {
      const saved = await guarded('Could not save the schedule', () => imberScheduleSave(input));
      if (!saved) return false;
      await load();
      editing = null;
      return true;
    },

    async setEnabled(job: ImberJob, enabled: boolean) {
      const updated = await guarded('Could not change the schedule', () => imberScheduleSetEnabled(job.id, enabled));
      if (updated) jobs = jobs.map((j) => (j.id === job.id ? updated : j));
    },

    async runNow(job: ImberJob) {
      if (await guarded('Could not run the schedule', () => imberScheduleRunNow(job.id)) !== null) {
        uiStore.showToast(`“${job.name}” started`, 'success');
        // The outcome is recorded when the run ends; look again shortly.
        setTimeout(() => void load(), 4000);
      }
    },

    askDelete(job: ImberJob) { deleting = job; },
    cancelDelete() { deleting = null; },
    async confirmDelete() {
      const job = deleting;
      deleting = null;
      if (!job) return;
      if (await guarded('Could not delete the schedule', () => imberScheduleDelete(job.id)) !== null) await load();
    },

    /** A run ended somewhere: refresh the list while the window is open. */
    runEnded() { if (open) void load(); },

    /** For the gallery and tests: put jobs in the list without a backend. */
    seed(next: ImberJob[]) { jobs = next; loading = false; error = null; },
  };
}

export const imberSchedulesStore = createImberSchedulesStore();
