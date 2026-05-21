import type { PluginLogEntry, PluginLogLevel } from '$lib/types/plugin-logs';
import { listPluginLogs, clearPluginLogs, clearPluginLogsByPipeline } from '$lib/ipc/plugin-logs';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { coalesceBatch } from '$lib/utils/coalesce';

const ALL_LEVELS: PluginLogLevel[] = ['debug', 'info', 'warn', 'error'];

/** Local cap, mirrors `MAX_ENTRIES` in `src-tauri/src/plugin_logs.rs`. */
const MAX_ENTRIES = 5000;

/** Sentinel encoding "entries without a pipeline tag" inside `pipelineFilter`.
 *  Lets non-pipeline `arbor.log.*` calls be a checkbox in the same dropdown
 *  as the actual pipeline names — picking it shows plain logs, leaving it
 *  unchecked hides them. Real pipeline names can never collide because
 *  `def.name` is a non-empty user-typed string. */
export const NON_PIPELINE_SENTINEL = '__non_pipeline__';

function createPluginLogsStore() {
  let entries = $state<PluginLogEntry[]>([]);

  /**
   * Selected plugins to display.  `null` means "all plugins" — the panel
   * dropdown defaults to this.  Using a Set keeps multi-select toggles O(1).
   */
  let pluginFilter = $state<Set<string> | null>(null);

  /**
   * Selected pipeline names to display. `null` means "everything passes"
   * (the default — pipeline-mirrored lines and plain `arbor.log.*` calls
   * all show). When non-null, only entries whose pipeline tag matches are
   * included.
   *
   * The Set may contain the sentinel string `NON_PIPELINE_SENTINEL` to
   * also let entries WITHOUT a `pipeline` field through (i.e. plain
   * `arbor.log.*` calls). This lets the user pick "non-pipeline entries"
   * as a first-class option alongside specific pipeline runs without
   * inventing a separate `mode` field on the filter state.
   *
   * Empty Set therefore means "show nothing" — user explicitly turned
   * every checkbox off.
   */
  let pipelineFilter = $state<Set<string> | null>(null);

  /** Selected severity levels.  Defaults to every level. */
  let levelFilter  = $state<Set<PluginLogLevel>>(new Set(ALL_LEVELS));

  /** Free-text search applied to the message body (case-insensitive). */
  let search       = $state('');

  // ── Initial load ──────────────────────────────────────────────────────────

  async function load() {
    try { entries = await listPluginLogs(); } catch { /* ignore */ }
  }

  // ── Tauri stream ──────────────────────────────────────────────────────────

  // Coalesce a burst of plugin-log events into one reactive update per
  // animation frame.  Without this, every line would clone the entries
  // array (O(N²) on a long buffer) and Svelte 5 would queue a reactivity
  // tick per event — disastrous when the WebView drains an IPC backlog
  // after the window regains focus.
  const flushPluginLogs = coalesceBatch<PluginLogEntry>((batch) => {
    if (!batch.length) return;
    const next = entries.concat(batch);
    if (next.length > MAX_ENTRIES) next.splice(0, next.length - MAX_ENTRIES);
    entries = next;
  });

  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://plugin-log',
        handler: (e: { payload: PluginLogEntry }) => {
          flushPluginLogs(e.payload);
        },
      },
    ]);
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  async function clear() {
    try { await clearPluginLogs(); } catch { /* ignore */ }
    entries = [];
  }

  /** Drop every entry tagged with the given pipeline name. The backend
   *  ring buffer is the source of truth, but we also prune the local copy
   *  so the panel reflects the change before the next stream tick. */
  async function clearByPipeline(name: string) {
    try { await clearPluginLogsByPipeline(name); } catch { /* ignore */ }
    entries = entries.filter(e => e.pipeline !== name);
    // If this was the only pipeline currently filtered, fall back to "all"
    // so the user isn't left looking at an empty panel after the wipe.
    if (pipelineFilter && pipelineFilter.has(name)) {
      const next = new Set(pipelineFilter);
      next.delete(name);
      pipelineFilter = next.size === 0 ? null : next;
    }
  }

  function setPluginFilter(plugins: Set<string> | null) { pluginFilter = plugins; }

  function togglePlugin(name: string) {
    // Materialise the implicit "all" set the first time the user picks one
    // — from then on the filter is explicit and includes every plugin
    // except the toggled one.
    const current = pluginFilter ?? new Set(allPluginNames);
    const next = new Set(current);
    if (next.has(name)) next.delete(name); else next.add(name);
    pluginFilter = next;
  }

  function selectAllPlugins()   { pluginFilter = null; }
  function selectNoPlugins()    { pluginFilter = new Set(); }

  function togglePipeline(name: string) {
    // Materialise-on-first-toggle: starting from `null` (= every entry
    // passes), the first explicit pick seeds an empty Set so the user
    // opts INTO the runs they care about. Matches "show ONLY logs from
    // pipeline X" — explicit-positive selection rather than
    // negative-exclusion.
    const current = pipelineFilter ?? new Set<string>();
    const next = new Set(current);
    if (next.has(name)) next.delete(name); else next.add(name);
    pipelineFilter = next;
  }
  function selectAllPipelines() { pipelineFilter = null; }

  function toggleLevel(level: PluginLogLevel) {
    const next = new Set(levelFilter);
    if (next.has(level)) next.delete(level); else next.add(level);
    levelFilter = next;
  }

  function setSearch(q: string) { search = q; }

  // ── Derived ────────────────────────────────────────────────────────────────

  const allPluginNames = $derived.by(() => {
    const names = new Set<string>();
    for (const e of entries) names.add(e.plugin);
    return [...names].sort();
  });

  /** Distinct pipeline names that currently appear in the buffer. Drives
   *  the pipeline-filter dropdown; recomputed every time entries change so
   *  newly-mirrored runs show up without a page reload. */
  const allPipelineNames = $derived.by(() => {
    const names = new Set<string>();
    for (const e of entries) {
      if (e.pipeline) names.add(e.pipeline);
    }
    return [...names].sort();
  });

  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return entries.filter(e => {
      if (!levelFilter.has(e.level)) return false;
      if (pluginFilter !== null && !pluginFilter.has(e.plugin)) return false;
      // Pipeline filter when explicit:
      //   • non-pipeline entry passes iff Set contains NON_PIPELINE_SENTINEL
      //   • pipeline entry passes iff Set contains its pipeline name
      // Empty Set therefore drops everything (user explicitly unchecked
      // every box) — the dropdown's "All" preset is how they undo that.
      if (pipelineFilter !== null) {
        const key = e.pipeline ?? NON_PIPELINE_SENTINEL;
        if (!pipelineFilter.has(key)) return false;
      }
      if (q && !e.message.toLowerCase().includes(q) && !e.plugin.toLowerCase().includes(q)) return false;
      return true;
    });
  });

  return {
    get entries()           { return entries; },
    get filtered()          { return filtered; },
    get allPluginNames()    { return allPluginNames; },
    get allPipelineNames()  { return allPipelineNames; },
    get pluginFilter()      { return pluginFilter; },
    get pipelineFilter()    { return pipelineFilter; },
    get levelFilter()       { return levelFilter; },
    get search()            { return search; },
    load,
    setupListeners,
    clear,
    clearByPipeline,
    setPluginFilter,
    togglePlugin,
    selectAllPlugins,
    selectNoPlugins,
    togglePipeline,
    selectAllPipelines,
    toggleLevel,
    setSearch,
  };
}

export const pluginLogsStore = createPluginLogsStore();
