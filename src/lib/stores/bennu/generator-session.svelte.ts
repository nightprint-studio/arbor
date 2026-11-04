/**
 * One open code-generator dialog: its options, the live preview and the write.
 *
 * Every generator's modal is the same loop — open (defaults and detection from the backend), edit
 * options, preview on each change (debounced, the newest answer wins), write — and the modals differ
 * only in their option forms, so the loop lives here. Create it during a component's initialisation: it
 * owns an `$effect` that re-previews whenever an option changes.
 *
 * Writing is two steps with one failure point first: new files (refused when one exists), then the
 * edits to the open file, which the editor applies as a single undo step.
 */

import { untrack } from 'svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  openGenerator,
  previewGenerator,
  type GenSite,
  type GeneratorPreview,
  type OpenedGenerator,
  type TextEdit,
} from '$lib/ipc/bennu/generate';
import { createFileFromTemplate } from '$lib/ipc/bennu/templates';

/** What a generator dialog needs from the window it is mounted in. */
export interface GeneratorHost {
  /** The project, file and caret now; `null` while no project is open. */
  site: () => GenSite | null;
  /** Apply byte-range edits to the open file as one undo step. */
  applyEdits: (edits: readonly TextEdit[]) => void;
  /** Open a file the generator just created. */
  openFile: (path: string) => Promise<unknown> | void;
}

const DEBOUNCE_MS = 160;

export function createGeneratorSession<T extends object>(id: string, host: GeneratorHost) {
  let opened = $state<OpenedGenerator | null>(null);
  let options = $state<T>({} as T);
  let preview = $state<GeneratorPreview | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let applying = $state(false);
  /** Plain: only the answer in flight reads it. */
  let ticket = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const site = () => host.site();

  async function open() {
    const at = site();
    if (!at) {
      error = 'Open a project first.';
      loading = false;
      return;
    }
    try {
      const found = await openGenerator(id, at);
      opened = found;
      options = found.defaults as T;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function run() {
    const at = site();
    if (!at || !opened) return;
    const mine = ++ticket;
    busy = true;
    try {
      const answer = await previewGenerator(id, at, $state.snapshot(options));
      if (mine !== ticket) return;
      preview = answer;
      error = null;
    } catch (e) {
      if (mine !== ticket) return;
      preview = null;
      error = String(e);
    } finally {
      if (mine === ticket) busy = false;
    }
  }

  // Every option is read through the stringify, so any of them changing previews again.
  $effect(() => {
    if (!opened) return;
    JSON.stringify(options);
    untrack(() => {
      clearTimeout(timer);
      timer = setTimeout(() => void run(), DEBOUNCE_MS);
    });
    return () => clearTimeout(timer);
  });

  const exists = $derived(preview?.files.filter((f) => f.exists) ?? []);
  const writes = $derived((preview?.files.length ?? 0) > 0 || (preview?.edits.length ?? 0) > 0);

  /** Write the preview. Resolves true when everything was written (the dialog may close). */
  async function apply(): Promise<boolean> {
    const at = site();
    const plan = preview;
    if (!at || !plan || applying || !writes || exists.length > 0) return false;
    applying = true;
    try {
      for (const file of plan.files) await createFileFromTemplate(at.root, file.path, file.text);
      if (plan.edits.length > 0) host.applyEdits(plan.edits);
      const created = plan.files.length;
      const edited = plan.edits.length > 0;
      toastStore.show(
        created > 0 && edited ? `Generated ${created} file${created === 1 ? '' : 's'} and edited the open file`
          : created > 0 ? `Generated ${created} file${created === 1 ? '' : 's'}`
          : 'Inserted into the open file',
        'success',
      );
      // Land on the main new file unless the open file is where the work went.
      if (created > 0 && !edited) void host.openFile(plan.files[0].path);
      return true;
    } catch (e) {
      error = `Couldn't write it: ${e}`;
      return false;
    } finally {
      applying = false;
    }
  }

  return {
    get opened() { return opened; },
    get options() { return options; },
    get preview() { return preview; },
    get error() { return error; },
    get loading() { return loading; },
    get busy() { return busy; },
    get applying() { return applying; },
    /** Files in the preview that are already on disk. */
    get exists() { return exists; },
    get canApply() { return writes && exists.length === 0 && !applying && !loading; },
    open,
    apply,
  };
}

export type GeneratorSession<T extends object> = ReturnType<typeof createGeneratorSession<T>>;
