/**
 * The Live panel's state — the project's managed processes, its recipes and scenarios, the log of
 * the selected process, and the unified timeline.
 *
 * ## How it stays current
 *
 * The backend pushes one coalesced "something changed" event (a few a second at most, however chatty
 * a service is); this store answers with a refresh of what is **on screen**: the overview always, the
 * selected process's new log lines on the Processes tab, the new timeline events on the Timeline tab.
 * Mounted only while the panel is shown — a hidden panel costs the backend nothing.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import {
  liveClearTimeline, liveCreateTemplate, liveLogs, liveRunRecipe, liveRunScenario, liveScenarioResult, liveState, liveStop,
  liveStopAll, liveStopRecipe, liveTimeline, liveTrace, onLiveChanged,
  type LiveEvent, type LiveKindFilter, type LiveLogLine, type LiveScenarioResult, type LiveState,
} from '$lib/ipc/bennu/live';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { projectStore } from './project.svelte';

export type LiveTab = 'processes' | 'recipes' | 'scenarios' | 'timeline';

/** Most log lines kept on screen for the selected process. */
const LOG_KEEP = 2000;
/** Most timeline events kept on screen. */
const TIMELINE_KEEP = 3000;
const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

function createLiveStore() {
  let tab = $state<LiveTab>('processes');
  let state = $state<LiveState | null>(null);
  let error = $state('');
  let selectedId = $state<string | null>(null);
  let logLines = $state<LiveLogLine[]>([]);
  let logCursor = 0;
  let selectedScenario = $state<string | null>(null);
  let scenarioResult = $state<LiveScenarioResult | null>(null);
  let events = $state<LiveEvent[]>([]);
  let eventCursor = 0;
  let filterText = $state('');
  let filterKinds = $state<LiveKindFilter[]>([]);
  let filterCorr = $state('');
  let unlisten: (() => void) | null = null;
  let mounted = false;

  const root = (): string | null => projectStore.project?.root ?? null;

  async function refreshOverview() {
    const r = root();
    if (!r) return;
    try {
      state = await liveState(r);
      error = '';
      // A selected process that is gone (the backend forgot it) is deselected.
      if (selectedId && !state.processes.some((p) => p.id === selectedId)) selectedId = null;
      if (!selectedId && state.processes.length > 0) selectedId = state.processes[state.processes.length - 1].id;
      if (!selectedScenario && state.scenarios.length > 0) selectedScenario = state.scenarios[0].file;
    } catch (e) {
      error = message(e);
    }
  }

  async function refreshLogs(reset = false) {
    const r = root();
    if (!r || !selectedId) { logLines = []; logCursor = 0; return; }
    if (reset) { logLines = []; logCursor = 0; }
    try {
      const got = await liveLogs(r, selectedId, logCursor || undefined, LOG_KEEP);
      if (got.lines.length > 0) {
        logLines = [...logLines, ...got.lines].slice(-LOG_KEEP);
        logCursor = got.lines[got.lines.length - 1].seq;
      }
    } catch {
      // The process was forgotten between the overview and this read; the next overview deselects it.
    }
  }

  async function refreshTimeline(reset = false) {
    const r = root();
    if (!r) return;
    // A filter change re-reads from the start; otherwise only what is new.
    if (reset) { events = []; eventCursor = 0; }
    try {
      const got = await liveTimeline({
        root: r, since_seq: eventCursor || undefined, text: filterText || undefined,
        kinds: filterKinds.length ? filterKinds : undefined, corr: filterCorr || undefined, limit: TIMELINE_KEEP,
      });
      // `last_seq` moves even when the filter hides every new event, so the cursor never re-reads them.
      eventCursor = got.last_seq;
      if (got.events.length > 0) events = [...events, ...got.events].slice(-TIMELINE_KEEP);
    } catch (e) {
      error = message(e);
    }
  }

  async function refreshScenarioResult() {
    const r = root();
    if (!r || !selectedScenario) { scenarioResult = null; return; }
    const name = state?.scenarios.find((s) => s.file === selectedScenario)?.name ?? selectedScenario;
    try { scenarioResult = await liveScenarioResult(r, name); } catch { scenarioResult = null; }
  }

  async function refresh() {
    await refreshOverview();
    if (tab === 'processes') await refreshLogs();
    else if (tab === 'timeline') await refreshTimeline();
    else if (tab === 'scenarios') await refreshScenarioResult();
  }

  /** Run a backend verb, surfacing its failure in the panel's banner. */
  async function act(run: (root: string) => Promise<unknown>) {
    const r = root();
    if (!r) return;
    try { await run(r); error = ''; } catch (e) { error = message(e); }
  }

  return {
    get tab() { return tab; },
    get state() { return state; },
    get error() { return error; },
    get selectedId() { return selectedId; },
    get logLines() { return logLines; },
    get selectedScenario() { return selectedScenario; },
    get scenarioResult() { return scenarioResult; },
    get events() { return events; },
    get filterText() { return filterText; },
    get filterKinds() { return filterKinds; },
    get filterCorr() { return filterCorr; },
    get root() { return root(); },
    get running() { return state?.processes.filter((p) => p.phase === 'running' || p.phase === 'ready').length ?? 0; },

    /** Called when the panel mounts. */
    async mount() {
      mounted = true;
      unlisten = await onLiveChanged((changed) => {
        const r = root();
        // Events are per project; ignore another project's chatter (same spelling-insensitive compare as the backend).
        if (mounted && r && changed.replace(/\\/g, '/').toLowerCase() === r.replace(/\\/g, '/').toLowerCase()) void refresh();
      });
      await refresh();
    },
    /** Called when the panel unmounts. */
    unmount() {
      mounted = false;
      unlisten?.();
      unlisten = null;
    },
    refresh,

    async setTab(t: LiveTab) {
      tab = t;
      if (t === 'timeline' && events.length === 0) await refreshTimeline(true);
      else await refresh();
    },
    async select(id: string) {
      selectedId = id;
      await refreshLogs(true);
    },
    async selectScenario(file: string | null) {
      selectedScenario = file;
      await refreshScenarioResult();
    },

    async setFilter(patch: { text?: string; kinds?: LiveKindFilter[]; corr?: string }) {
      if (patch.text !== undefined) filterText = patch.text;
      if (patch.kinds !== undefined) filterKinds = patch.kinds;
      if (patch.corr !== undefined) filterCorr = patch.corr;
      await refreshTimeline(true);
    },
    /** Show only what one request touched, across every process. */
    async followCorrelation(corr: string) {
      tab = 'timeline';
      filterCorr = corr;
      await refreshTimeline(true);
    },
    async clearTimeline() {
      await act((r) => liveClearTimeline(r));
      await refreshTimeline(true);
    },
    /** The ordered trace of one correlation id (request, response, every log line that mentions it). */
    async trace(corr: string) {
      const r = root();
      return r ? liveTrace(r, corr, 8) : null;
    },

    async runRecipe(name: string, approve: boolean) { await act((r) => liveRunRecipe(r, name, approve)); },
    async stopRecipe(name: string) { await act((r) => liveStopRecipe(r, name)); },
    async stop(id: string) { await act((r) => liveStop(r, id)); },
    async stopAll() { await act((r) => liveStopAll(r)); },
    async runScenario(file: string, approve: boolean) {
      selectedScenario = file;
      await act((r) => liveRunScenario(r, file, approve));
      await refresh();
    },
    /** Create a starter file and return its path, for the caller to open in the editor. */
    async createTemplate(kind: 'recipes' | 'scenario'): Promise<string | null> {
      const r = root();
      if (!r) return null;
      try {
        const path = await liveCreateTemplate(r, kind);
        await refreshOverview();
        return path;
      } catch (e) {
        toastStore.show(message(e), 'error');
        return null;
      }
    },
  };
}

export const bennuLiveStore = createLiveStore();
