/**
 * Imber's schedules, as the Schedules window shows them: the list, what is being edited, and the verbs
 * (save, turn on or off, run now, delete). The backend owns the list and the loop that fires it; this is the
 * window's copy, reloaded when a run ends so "last result" is never stale.
 *
 * Product-neutral: Corvus and Bennu each make one store with `createImberSchedulesStore(adapter)`. The adapter
 * says which backend to call and what the product allows (its actions, whether a run may be given write
 * access, which project a new schedule is about), so the window and the form never name a product.
 */

import type { imberScheduleApi, ImberScheduleProduct } from '$lib/ipc/imber-schedule';
import type { ImberJob, ImberJobInput, ImberScheduleActionKind } from '$lib/types/imber-schedule';
import { uiStore } from './ui.svelte';

export type ImberScheduleApi = ReturnType<typeof imberScheduleApi>;

export interface ImberScheduleAdapter extends ImberScheduleApi {
  product: ImberScheduleProduct;
  /** How the window refers to the product's app ("Corvus", "Bennu"). */
  productName: string;
  /** The actions the product's backend runs. */
  actions: ImberScheduleActionKind[];
  /** Whether the user picks `manual` or `auto`; when false the run is always read-only and the form says so. */
  allowModeChoice: boolean;
  /** The project / repository a new schedule is about; `null` when none is open. */
  defaultRoot(): string | null;
  /** The conversation a new *awake* schedule would wake, when one is open. */
  currentConversationId(): string | null;
  /** Show the conversation an *awake* schedule wakes. Absent when the product cannot. */
  openConversation?(conversationId: string): void | Promise<void>;
}

/** What a blank form starts with, when the window opens on "new". */
export interface ImberScheduleSeed {
  action?: ImberScheduleActionKind;
  prompt?: string;
}

export function createImberSchedulesStore(adapter: ImberScheduleAdapter) {
  let open = $state(false);
  let jobs = $state<ImberJob[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  /** The job being edited, `'new'` for a blank form, `null` for the list. */
  let editing = $state<ImberJob | 'new' | null>(null);
  let seedForNew = $state<ImberScheduleSeed>({});
  let deleting = $state<ImberJob | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      jobs = await adapter.list();
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
    get adapter() { return adapter; },
    get open() { return open; },
    get jobs() { return jobs; },
    get loading() { return loading; },
    get error() { return error; },
    get editing() { return editing; },
    get seedForNew() { return seedForNew; },
    get deleting() { return deleting; },

    load,
    /** Open the window on the list, or straight on a new schedule (optionally with the prompt already written). */
    show(opts: { create?: boolean; seed?: ImberScheduleSeed } = {}) {
      open = true;
      seedForNew = opts.seed ?? {};
      editing = opts.create ? 'new' : null;
      void load();
    },
    hide() { open = false; editing = null; deleting = null; },

    edit(job: ImberJob | 'new') { if (job === 'new') seedForNew = {}; editing = job; },
    closeEditor() { editing = null; },

    /** Save from the form. `true` when it was saved (the editor closes); on failure the reason is toasted. */
    async save(input: ImberJobInput): Promise<boolean> {
      const saved = await guarded('Could not save the schedule', () => adapter.save(input));
      if (!saved) return false;
      await load();
      editing = null;
      return true;
    },

    async setEnabled(job: ImberJob, enabled: boolean) {
      const updated = await guarded('Could not change the schedule', () => adapter.setEnabled(job.id, enabled));
      if (updated) jobs = jobs.map((j) => (j.id === job.id ? updated : j));
    },

    /** The "started" toast comes from the backend's own event, so a manual run is announced like any other. */
    async runNow(job: ImberJob) {
      if (await guarded('Could not run the schedule', () => adapter.runNow(job.id)) !== null) {
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
      if (await guarded('Could not delete the schedule', () => adapter.remove(job.id)) !== null) await load();
    },

    /** Show the conversation an awake schedule wakes (closes the window first). */
    async openConversation(job: ImberJob) {
      if (job.action.kind !== 'awake' || !job.action.conversation_id || !adapter.openConversation) return;
      open = false;
      editing = null;
      await adapter.openConversation(job.action.conversation_id);
    },

    /** A run started or ended somewhere: refresh the list while the window is open. */
    runEnded() { if (open) void load(); },

    /** For the gallery and tests: put jobs in the list without a backend. */
    seed(next: ImberJob[]) { jobs = next; loading = false; error = null; },
  };
}

export type ImberSchedulesStore = ReturnType<typeof createImberSchedulesStore>;
