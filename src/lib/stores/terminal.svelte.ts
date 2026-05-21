import type {
  TerminalTab, BuiltinShellInfo, DetectedShell, TerminalsConfig,
} from '$lib/types/terminal';
import {
  listBuiltinShells, getTerminalsConfig,
} from '$lib/ipc/terminal';
import { listen } from '@tauri-apps/api/event';

// ---------------------------------------------------------------------------
// Terminal store — open tabs + shell catalogue + detection state.
//
// Tabs: actual xterm.js instances live inside TerminalInstance.svelte (DOM).
// Catalogue/detection: mirrors the IDE pattern in worktreeStore — config and
// detected-shells are populated once at startup, kept here so any consumer
// (settings panel, "+" dropdown) reads from the same source.
// ---------------------------------------------------------------------------

function createTerminalStore() {
  let tabs           = $state<TerminalTab[]>([]);
  let activeId       = $state<string | null>(null);

  let builtinShells  = $state<BuiltinShellInfo[]>([]);
  let detectedShells = $state<DetectedShell[]>([]);
  let detectionDone  = $state(false);
  let config         = $state<TerminalsConfig | null>(null);

  const shellCounters = new Map<string, number>();

  // ── Tabs ───────────────────────────────────────────────────────────────

  function addTab(id: string, shell: string, cwd: string): TerminalTab {
    const count  = (shellCounters.get(shell) ?? 0) + 1;
    shellCounters.set(shell, count);
    const title  = count === 1 ? shell : `${shell} ${count}`;
    const tab: TerminalTab = { id, title, shell, cwd };
    tabs.push(tab);
    activeId = id;
    return tab;
  }

  function removeTab(id: string) {
    const idx = tabs.findIndex(t => t.id === id);
    if (idx === -1) return;
    tabs.splice(idx, 1);
    if (activeId === id) {
      activeId = tabs[Math.max(0, idx - 1)]?.id ?? null;
    }
  }

  function setActive(id: string) {
    if (tabs.some(t => t.id === id)) activeId = id;
  }

  function renameTab(id: string, title: string) {
    const tab = tabs.find(t => t.id === id);
    if (tab) tab.title = title;
  }

  function clear() {
    tabs   = [];
    activeId = null;
    shellCounters.clear();
  }

  // ── Catalogue + detection ─────────────────────────────────────────────

  async function loadCatalogue() {
    try { builtinShells = await listBuiltinShells(); } catch { builtinShells = []; }
  }

  async function loadConfig() {
    try { config = await getTerminalsConfig(); } catch { /* keep null */ }
  }

  async function setupDetectionListener() {
    return listen<DetectedShell[]>('arbor://shell-detection-done', (event) => {
      detectedShells = event.payload;
      detectionDone  = true;
    });
  }

  /**
   * Shells visible in the new-terminal dropdown:
   *   • all custom shells (always usable — user defined them on purpose)
   *   • detected built-in shells once detection has run
   *   • before detection completes, fall back to the full built-in catalogue
   *     so the picker isn't empty during the first ~hundred ms of startup
   */
  function pickerOptions(): { id: string; name: string; custom: boolean }[] {
    const customs = (config?.custom_shells ?? []).map(c => ({
      id: c.id, name: c.name, custom: true as const,
    }));

    const builtins: { id: string; name: string; custom: boolean }[] =
      detectionDone
        ? detectedShells
            .filter(d => d.available)
            .map(d => ({ id: d.id, name: d.name, custom: false }))
        : builtinShells.map(b => ({ id: b.id, name: b.name, custom: false }));

    return [...builtins, ...customs];
  }

  return {
    get tabs()           { return tabs;           },
    get activeId()       { return activeId;       },
    get activeTab()      { return tabs.find(t => t.id === activeId) ?? null; },
    get count()          { return tabs.length;    },
    get builtinShells()  { return builtinShells;  },
    get detectedShells() { return detectedShells; },
    get detectionDone()  { return detectionDone;  },
    get config()         { return config;         },
    addTab, removeTab, setActive, renameTab, clear,
    loadCatalogue, loadConfig, setupDetectionListener, pickerOptions,
    setConfig(c: TerminalsConfig)        { config = c; },
    setDetectedShells(d: DetectedShell[]) { detectedShells = d; detectionDone = true; },
  };
}

export const terminalStore = createTerminalStore();
