/**
 * Plugin store — owns the bits of plugin state that are NOT contributions:
 *
 *   • `disabledPlugins`     — user-controlled enable/disable, persisted in localStorage
 *   • `comboSelections`     — currently-picked option per combo, persisted in localStorage
 *   • `pendingForm`         — modal form pushed by `arbor.ui.form(...)` waiting to render
 *
 * UI registration data (sidebar sections, context-menu items, command-palette
 * entries, keybindings, activity-bar combos, panel content, …) lives in the
 * contribution store. Components that need plugin-disable filtering call
 * `contributionStore.<adapter>()` which already excludes disabled plugins.
 */
import type { PluginFormConfig } from '$lib/types/plugin';
import { enablePlugin, disablePlugin } from '$lib/ipc/plugin';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';

const DISABLED_KEY = 'arbor:disabled-plugins';

function loadDisabled(): Set<string> {
  try {
    const arr = JSON.parse(localStorage.getItem(DISABLED_KEY) ?? '[]');
    return new Set(arr);
  } catch { return new Set(); }
}

function saveDisabled(set: Set<string>) {
  try { localStorage.setItem(DISABLED_KEY, JSON.stringify([...set])); } catch { /* ignore */ }
}

function tryLoadComboSelection(key: string): string | null {
  try { return localStorage.getItem(`arbor:combo:${key}`); } catch { return null; }
}

function createPluginStore() {
  let pendingForm     = $state<PluginFormConfig | null>(null);
  let formKey         = $state(0);
  let disabledPlugins = $state<Set<string>>(loadDisabled());
  /** Per-combo selected value: key = "pluginName::comboId". Lazily populated
   *  by `setComboSelection` and the `plugin:combo-select` listener; reads
   *  fall back to localStorage when in-memory state is empty. */
  let comboSelections = $state<Record<string, string>>({});

  /** Listen for plugin-driven combo selection pushes. A plugin calling
   *  `set_combo_options(id, opts, selected_value)` emits this event when the
   *  passed value is valid against the new options — we adopt it as the
   *  current selection. Option list itself flows via the contribution store. */
  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'plugin:combo-select',
        handler: (e: { payload: { plugin_name: string; combo_id: string; value: string } }) => {
          const { plugin_name, combo_id, value } = e.payload;
          const key = `${plugin_name}::${combo_id}`;
          comboSelections = { ...comboSelections, [key]: value };
          try { localStorage.setItem(`arbor:combo:${key}`, value); } catch { /* ignore */ }
        },
      },
    ]);
  }

  /** Reconcile the frontend `disabledPlugins` Set with the backend's truth.
   *  Call this on startup and after every backend mutation that may change
   *  the enable state (toggle, reload). Without this sync, freshly-discovered
   *  plugins (which default to disabled in `plugin_states.json`) would be
   *  treated as enabled by the frontend filters — and the first toggle click
   *  would invert the wrong direction. */
  function syncFromInfos(infos: { name: string; enabled: boolean }[]) {
    const next = new Set<string>();
    for (const i of infos) if (!i.enabled) next.add(i.name);
    disabledPlugins = next;
    saveDisabled(next);
  }

  /** Toggle a plugin enabled/disabled. Optimistic UI + backend sync.
   *  The backend now performs a dependency cascade, so the returned list
   *  may include more than the requested plugin — we reconcile the local
   *  `disabledPlugins` set with that list so dependents picked up by the
   *  cascade stay in sync without an extra round-trip. */
  async function togglePlugin(name: string) {
    const willEnable = disabledPlugins.has(name);
    const next = new Set(disabledPlugins);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    disabledPlugins = next;
    saveDisabled(next);
    try {
      const touched = willEnable
        ? await enablePlugin(name)
        : await disablePlugin(name);
      if (touched.length > 0) {
        const reconciled = new Set(next);
        for (const n of touched) {
          if (willEnable) reconciled.delete(n);
          else reconciled.add(n);
        }
        disabledPlugins = reconciled;
        saveDisabled(reconciled);
      }
    } catch {
      // Backend refused (e.g. enable blocker) — roll back the optimistic flip.
      const rollback = new Set(disabledPlugins);
      if (willEnable) rollback.add(name);
      else rollback.delete(name);
      disabledPlugins = rollback;
      saveDisabled(rollback);
      throw new Error('toggle-failed');
    }
  }

  function isEnabled(name: string): boolean {
    return !disabledPlugins.has(name);
  }

  function setPendingForm(form: PluginFormConfig) { formKey++; pendingForm = form; }
  function clearPendingForm()                     { pendingForm = null; }

  /** Read the current combo selection. Falls back to the persisted value in
   *  localStorage, then to the empty string — callers (RepoActions /
   *  ActivityBar) display "—" when this returns ''.
   *
   *  This function is called from $derived contexts (combo widgets re-render
   *  every time a contribution changes), so it MUST be pure — no mutation of
   *  reactive state. The persisted-value lookup is O(1) and harmless to repeat. */
  function getComboSelection(pluginName: string, comboId: string): string {
    const key = `${pluginName}::${comboId}`;
    return comboSelections[key] ?? tryLoadComboSelection(key) ?? '';
  }

  function setComboSelection(pluginName: string, comboId: string, value: string) {
    const key = `${pluginName}::${comboId}`;
    comboSelections = { ...comboSelections, [key]: value };
    try { localStorage.setItem(`arbor:combo:${key}`, value); } catch { /* ignore */ }
  }

  return {
    get pendingForm()     { return pendingForm; },
    get formKey()         { return formKey; },
    get disabledPlugins() { return disabledPlugins; },
    setupListeners,
    syncFromInfos,
    togglePlugin,
    isEnabled,
    setPendingForm,
    clearPendingForm,
    getComboSelection,
    setComboSelection,
  };
}

export const pluginStore = createPluginStore();
