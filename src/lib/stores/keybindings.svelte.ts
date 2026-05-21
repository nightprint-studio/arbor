import { DEFAULT_KEYBINDINGS, matchesBinding, type Keybinding } from '$lib/utils/keybindings';

const STORAGE_KEY = 'arbor:keybindings';

function load(): Record<string, Keybinding> {
  try { return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}'); } catch { return {}; }
}

function createKeybindingsStore() {
  let custom = $state<Record<string, Keybinding>>(load());

  function getBinding(action: string): Keybinding {
    return custom[action] ?? DEFAULT_KEYBINDINGS[action];
  }

  function setBinding(action: string, binding: Keybinding) {
    custom = { ...custom, [action]: binding };
    persist();
  }

  function resetBinding(action: string) {
    const next = { ...custom };
    delete next[action];
    custom = next;
    persist();
  }

  function resetAll() {
    custom = {};
    persist();
  }

  function isCustomized(action: string): boolean {
    return action in custom;
  }

  function persist() {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(custom)); } catch {}
  }

  /** Returns the action name that matches the keyboard event, or null. */
  function matchAction(event: KeyboardEvent): string | null {
    for (const action of Object.keys(DEFAULT_KEYBINDINGS)) {
      if (matchesBinding(event, getBinding(action))) return action;
    }
    return null;
  }

  return {
    get custom() { return custom; },
    getBinding,
    setBinding,
    resetBinding,
    resetAll,
    isCustomized,
    matchAction,
  };
}

export const keybindingsStore = createKeybindingsStore();
