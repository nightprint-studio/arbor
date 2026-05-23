/**
 * useStudioSaveFlow — Save / Save As state shared by every Studio modal.
 * Owns the `saving / saveError / savePickerOpen` triplet and the
 * doSave → (sourcePath ? runSave : openSaveAs) routing.
 */

export interface SaveFlowConfig {
  getSourcePath: () => string | null;
  /** Persist the document. `path === null` keeps the existing binding,
   *  `path !== null` switches the doc's source to the new path. */
  save: (opts: { path: string | null; bindToDoc: boolean }) => Promise<void>;
  /** Bump the diff view after a successful save (so original ⇄ current
   *  recomputes immediately). */
  onSaved?: () => void;
}

export interface SaveFlow {
  readonly saving: boolean;
  readonly saveError: string | null;
  savePickerOpen: boolean;

  doSave(): Promise<void>;
  runSave(): Promise<void>;
  openSaveAs(): void;
  onSaveAsPicked(path: string): Promise<void>;
}

export function useStudioSaveFlow(config: SaveFlowConfig): SaveFlow {
  let saving         = $state(false);
  let saveError      = $state<string | null>(null);
  let savePickerOpen = $state(false);

  async function doSave(): Promise<void> {
    if (!config.getSourcePath()) { savePickerOpen = true; return; }
    await runSave();
  }
  async function runSave(): Promise<void> {
    saving = true; saveError = null;
    try {
      await config.save({ path: null, bindToDoc: false });
      config.onSaved?.();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
  function openSaveAs() { savePickerOpen = true; }
  async function onSaveAsPicked(path: string): Promise<void> {
    savePickerOpen = false;
    saving = true; saveError = null;
    try {
      await config.save({ path, bindToDoc: true });
      config.onSaved?.();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }

  return {
    get saving()    { return saving; },
    get saveError() { return saveError; },
    get savePickerOpen()    { return savePickerOpen; },
    set savePickerOpen(v: boolean) { savePickerOpen = v; },
    doSave,
    runSave,
    openSaveAs,
    onSaveAsPicked,
  };
}
