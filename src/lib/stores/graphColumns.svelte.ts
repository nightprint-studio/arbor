/**
 * Singleton store for the commit-graph column layout.
 *
 * Persists order + per-column width + visibility to a dedicated TOML
 * (`~/.config/arbor/graph_columns.toml`, separate from the main
 * `config.toml`) via the `get_graph_columns` / `set_graph_columns` Tauri
 * commands.
 *
 * The `graph` column (the SVG lane renderer) lives in the same array as
 * the text columns — there's no special-case "lane width" field. Its
 * stored `width` is the adaptive cap used by `CommitGraph.svelte` when it
 * computes the actual track size.
 *
 * Like `graphConfigStore`, the load is fired at module-import time so the
 * value is ready (or close to it) by the time `CommitGraph` mounts.
 */

import { getGraphColumns, setGraphColumns } from '$lib/ipc/graphColumns';
import type {
  GraphColumn,
  GraphColumnId,
  GraphColumnsConfig,
} from '$lib/types/config';

const DEFAULTS: GraphColumnsConfig = {
  columns: [
    { id: 'graph',   width: 480, visible: true },
    { id: 'refs',    width: 220, visible: true },
    { id: 'subject', width: 280, visible: true },
    { id: 'author',  width: 160, visible: true },
    { id: 'date',    width: 150, visible: true },
    { id: 'hash',    width:  80, visible: true },
  ],
};

const MIN_COL_WIDTH = 48;
const SAVE_DEBOUNCE_MS = 250;

function cloneConfig(c: GraphColumnsConfig): GraphColumnsConfig {
  return { columns: c.columns.map(x => ({ ...x })) };
}

function createGraphColumnsStore() {
  let _cfg = $state<GraphColumnsConfig>(cloneConfig(DEFAULTS));
  let _ready = $state(false);

  // Module-import load — same pattern as graphConfigStore.
  getGraphColumns()
    .then(cfg => {
      if (cfg && Array.isArray(cfg.columns) && cfg.columns.length > 0) {
        // Merge: drop unknown ids, append missing known ids at the end so
        // an old TOML written before a new column type was introduced still
        // surfaces that column instead of silently hiding it.
        const known: GraphColumnId[] = ['graph', 'refs', 'subject', 'author', 'date', 'hash'];
        const seen = new Set<GraphColumnId>();
        const merged: GraphColumn[] = [];
        for (const c of cfg.columns) {
          if (known.includes(c.id) && !seen.has(c.id)) {
            seen.add(c.id);
            merged.push({
              id: c.id,
              width: Math.max(MIN_COL_WIDTH, Math.round(c.width)),
              visible: c.visible !== false,
            });
          }
        }
        // Append any column the persisted file is missing, preserving the
        // default order. `graph` was added after the initial release, so
        // older files don't carry it — prepend it instead of appending so
        // it lands at index 0 like a fresh install.
        for (const def of DEFAULTS.columns) {
          if (!seen.has(def.id)) {
            if (def.id === 'graph') merged.unshift({ ...def });
            else                    merged.push({ ...def });
          }
        }
        _cfg = { columns: merged };
      }
    })
    .catch(() => { /* keep defaults */ })
    .finally(() => { _ready = true; });

  let _saveTimer: ReturnType<typeof setTimeout> | null = null;
  function schedulePersist() {
    if (_saveTimer) clearTimeout(_saveTimer);
    _saveTimer = setTimeout(() => {
      _saveTimer = null;
      void setGraphColumns(cloneConfig(_cfg)).catch(() => {});
    }, SAVE_DEBOUNCE_MS);
  }

  function setColumnWidth(id: GraphColumnId, px: number) {
    const v = Math.max(MIN_COL_WIDTH, Math.round(px));
    const next = _cfg.columns.map(c => c.id === id ? { ...c, width: v } : c);
    if (next.every((c, i) => c.width === _cfg.columns[i].width)) return;
    _cfg = { ..._cfg, columns: next };
    schedulePersist();
  }

  function toggleVisible(id: GraphColumnId) {
    const next = _cfg.columns.map(c => c.id === id ? { ...c, visible: !c.visible } : c);
    _cfg = { ..._cfg, columns: next };
    schedulePersist();
  }

  function setVisible(id: GraphColumnId, visible: boolean) {
    const next = _cfg.columns.map(c => c.id === id ? { ...c, visible } : c);
    if (next.every((c, i) => c.visible === _cfg.columns[i].visible)) return;
    _cfg = { ..._cfg, columns: next };
    schedulePersist();
  }

  /** Move column `id` to index `to` in the order array. */
  function moveTo(id: GraphColumnId, to: number) {
    const from = _cfg.columns.findIndex(c => c.id === id);
    if (from < 0) return;
    const clamped = Math.max(0, Math.min(_cfg.columns.length - 1, to));
    if (clamped === from) return;
    const next = _cfg.columns.slice();
    const [item] = next.splice(from, 1);
    next.splice(clamped, 0, item);
    _cfg = { ..._cfg, columns: next };
    schedulePersist();
  }

  function moveLeft(id: GraphColumnId)  {
    const i = _cfg.columns.findIndex(c => c.id === id);
    if (i > 0) moveTo(id, i - 1);
  }
  function moveRight(id: GraphColumnId) {
    const i = _cfg.columns.findIndex(c => c.id === id);
    if (i >= 0 && i < _cfg.columns.length - 1) moveTo(id, i + 1);
  }

  function reset() {
    _cfg = cloneConfig(DEFAULTS);
    schedulePersist();
  }

  return {
    get config()  { return _cfg; },
    get columns() { return _cfg.columns; },
    get ready()   { return _ready; },
    setColumnWidth,
    toggleVisible,
    setVisible,
    moveTo,
    moveLeft,
    moveRight,
    reset,
  };
}

export const graphColumnsStore = createGraphColumnsStore();
