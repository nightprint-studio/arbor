import type {
  TerminalTab, BuiltinShellInfo, DetectedShell, TerminalsConfig,
} from '$lib/types/terminal';
import {
  listBuiltinShells, getTerminalsConfig,
} from '$lib/ipc/terminal';
import { startStream } from '$lib/ipc/stream';

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

  /// Kick off shell detection over the streaming seam (`docs/streaming-seam.md`).
  /// `startStream` subscribes to `arbor://shell-detection-*` (filtered by the
  /// minted stream_id) BEFORE invoking `start_shell_detection`, so the `-done`
  /// event carrying the result can't outrun the listener. The detected shell
  /// list arrives under `{ shells }` on `-done`; per-shell progress lines still
  /// ride the standard `arbor://job-*` events into the Jobs overlay.
  /// The previous in-flight detection (if any) is disposed before starting a
  /// new one, and listeners are detached once `-done` fires.
  let activeDetection: { dispose: () => void } | null = null;

  /// Detection, once per window.
  ///
  /// Every product with a terminal wants the shell list at startup, and every one of them
  /// used to have to remember to ask — which is how Bennu shipped with a terminal whose
  /// picker was empty until you opened Arbor's settings. It is idempotent in both directions
  /// that matter: a completed detection is not repeated, and a second call while one is in
  /// flight joins it rather than restarting it (`detectShells` disposes the previous stream,
  /// so two callers racing would leave the first one's listeners torn down and its result
  /// dropped).
  ///
  /// Per window, not per machine: a window is its own JS realm, so each has its own copy of
  /// this store to fill.
  let detecting: Promise<void> | null = null;

  async function ensureDetected(): Promise<void> {
    if (detectionDone) return;
    if (detecting) return detecting;
    detecting = detectShells().finally(() => { detecting = null; });
    return detecting;
  }

  async function detectShells(): Promise<void> {
    activeDetection?.dispose();
    activeDetection = null;
    // `finished` guards the replay-before-assign window: if `-done` fires
    // synchronously inside `startStream` (before `handle` is assigned), we still
    // tear the listeners down once the handle resolves below.
    let finished = false;
    const settle = () => { finished = true; activeDetection?.dispose(); activeDetection = null; };
    const handle = await startStream<{ shells: DetectedShell[] }>(
      'platform',
      'arbor://shell-detection',
      { cmd: 'start_shell_detection' },
      {
        onChunk: () => {},
        onDone:  (p) => {
          detectedShells = (p.shells as DetectedShell[] | undefined) ?? [];
          detectionDone  = true;
          settle();
        },
        onError: settle,
      },
    );
    if (finished) handle.dispose();
    else activeDetection = handle;
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
    loadCatalogue, loadConfig, detectShells, ensureDetected, pickerOptions,
    setConfig(c: TerminalsConfig)        { config = c; },
    setDetectedShells(d: DetectedShell[]) { detectedShells = d; detectionDone = true; },
  };
}

export const terminalStore = createTerminalStore();
