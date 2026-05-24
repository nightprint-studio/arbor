import type { Theme } from '$lib/types/theme';
import {
  listCustomThemes,
  getActiveThemeId,
  setActiveThemeId,
  saveCustomTheme,
  deleteCustomTheme,
  notifyThemeChanged,
} from '$lib/ipc/theme';
import { listen } from '@tauri-apps/api/event';
import darkTheme from '$lib/themes/dark.json';
import lightTheme from '$lib/themes/light.json';
import { appearanceStore } from '$lib/stores/appearance.svelte';

const BUILT_IN: Theme[] = [darkTheme as Theme, lightTheme as Theme];

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let _activeId   = $state<string>('dark');
let _custom     = $state<Theme[]>([]);
let _ready      = $state(false);

// The "apply theme fonts" opt-in lives in `appearanceStore` (persisted in
// config.toml). The theme store calls `appearanceStore.syncThemeFonts(vars)`
// whenever the active vars change so the opt-in stays in sync without each
// caller having to know about both stores.

/** Plugin-applied CSS-var overlays, keyed by plugin name. RAM-only —
 *  cleared on reload. Each entry is merged on top of the active theme
 *  every time the theme changes, so the overrides outlive a theme
 *  switch but vanish if the plugin is disabled or `clear_theme_tokens`
 *  is called. The map insertion order also fixes precedence: later
 *  plugins win over earlier ones for the same var (last-write-wins). */
const _overlays = new Map<string, Record<string, string>>();

// ---------------------------------------------------------------------------
// Derived
// ---------------------------------------------------------------------------

const allThemes  = $derived<Theme[]>([...BUILT_IN, ..._custom]);
const activeTheme = $derived<Theme>(
  allThemes.find(t => t.id === _activeId) ?? BUILT_IN[0],
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function applyVars(vars: Record<string, string>) {
  const root = document.documentElement;
  for (const [k, v] of Object.entries(vars)) {
    root.style.setProperty(k, v);
  }
  appearanceStore.syncThemeFonts(vars);
}

/** Active theme vars + every plugin overlay, merged. Later overlays win
 *  over earlier ones; plugin overlays always win over the active theme.
 *  Used both to repaint and to ship to the on_theme_changed hook. */
function effectiveVars(): Record<string, string> {
  const merged: Record<string, string> = { ...activeTheme.vars };
  for (const overlay of _overlays.values()) {
    for (const [k, v] of Object.entries(overlay)) merged[k] = v;
  }
  return merged;
}

/** Repaint the document with the merged stylesheet and notify the backend
 *  so plugins observe a single coherent on_theme_changed event. `source`
 *  lets handlers ignore changes they triggered themselves. */
function applyAndBroadcast(source: 'user' | 'plugin' | 'init') {
  const merged = effectiveVars();
  applyVars(merged);
  notifyThemeChanged(activeTheme.id, activeTheme.name, merged, source)
    .catch(() => { /* dev mode / backend offline */ });
}

// ---------------------------------------------------------------------------
// Store API
// ---------------------------------------------------------------------------

async function init() {
  // Pull the appearance config first so `applyAndBroadcast('init')` honors
  // the persisted `use_theme_fonts` opt-in on first paint. Both stores hit
  // the same `~/.config/arbor/config.toml` so serializing the calls costs
  // ~nothing in practice and avoids a flash of "default fonts".
  if (!appearanceStore.loaded) {
    try { await appearanceStore.loadConfig(); } catch { /* ignore */ }
  }
  try {
    const [id, custom] = await Promise.all([
      getActiveThemeId(),
      listCustomThemes(),
    ]);
    _custom   = custom;
    _activeId = id;
  } catch {
    // Backend not ready yet (dev mode) — fall back to the built-in default.
    _activeId = 'dark';
  }
  applyAndBroadcast('init');
  // Subscribe to plugin-driven overlays. Empty `vars` is the agreed
  // "release my overlay" signal coming from arbor.ui.clear_theme_tokens.
  await listen<{ plugin: string; vars: Record<string, string> }>(
    'arbor://theme-overlay',
    (e) => {
      const { plugin, vars } = e.payload;
      if (!plugin) return;
      const isEmpty = !vars || Object.keys(vars).length === 0;
      if (isEmpty) {
        if (!_overlays.delete(plugin)) return;
      } else {
        _overlays.set(plugin, vars);
      }
      applyAndBroadcast('plugin');
    },
  );
  // Marketplace install/uninstall (and any other backend-side write to the
  // themes dir) emits this — pull the fresh list so the TitleBar dropdown
  // and Theme Editor see the new entry without a restart.
  await listen('arbor://themes-changed', () => {
    void refresh();
  });
  _ready = true;
}

/** Re-read custom themes from disk. If the currently active theme was
 *  removed while we weren't looking, fall back to the default. */
async function refresh() {
  try {
    _custom = await listCustomThemes();
  } catch {
    return;
  }
  const stillExists = [...BUILT_IN, ..._custom].some(t => t.id === _activeId);
  if (!stillExists) await setActive('dark');
}

async function setActive(id: string) {
  _activeId = id;
  applyAndBroadcast('user');
  try { await setActiveThemeId(id); } catch { /* ignore in dev */ }
}

/** Preview a theme without persisting — used by the editor modal. */
function preview(vars: Record<string, string>) {
  applyVars(vars);
}

/** Revert the live CSS vars back to the currently active theme — including
 *  every plugin overlay so an in-flight preview never strips them. */
function revertPreview() {
  applyVars(effectiveVars());
}

async function saveCustom(theme: Theme) {
  await saveCustomTheme(theme);
  const idx = _custom.findIndex(t => t.id === theme.id);
  if (idx >= 0) {
    _custom = _custom.map((t, i) => (i === idx ? theme : t));
  } else {
    _custom = [..._custom, theme];
  }
}

async function deleteCustom(id: string) {
  await deleteCustomTheme(id);
  _custom = _custom.filter(t => t.id !== id);
  if (_activeId === id) await setActive('dark');
}

// ---------------------------------------------------------------------------
// Import / Export helpers
// ---------------------------------------------------------------------------

/** Parse + validate a theme JSON string. Returns the Theme on success or
 *  throws an Error with a user-friendly message on failure. The id is left
 *  as-is — callers are responsible for re-keying if needed. */
function parseThemeJson(raw: string): Theme {
  let obj: any;
  try {
    obj = JSON.parse(raw);
  } catch (e) {
    throw new Error(`Invalid JSON: ${(e as Error).message}`);
  }
  if (!obj || typeof obj !== 'object') {
    throw new Error('Theme file is not an object');
  }
  if (typeof obj.name !== 'string' || obj.name.trim() === '') {
    throw new Error('Theme is missing a "name"');
  }
  if (!obj.vars || typeof obj.vars !== 'object') {
    throw new Error('Theme is missing a "vars" map');
  }
  // Strip non-string vars to keep things sane.
  const vars: Record<string, string> = {};
  for (const [k, v] of Object.entries(obj.vars)) {
    if (typeof v === 'string' && k.startsWith('--')) vars[k] = v;
  }
  if (Object.keys(vars).length === 0) {
    throw new Error('Theme has no usable CSS variables');
  }
  return {
    id:          typeof obj.id === 'string' ? obj.id : '',
    name:        obj.name.trim(),
    description: typeof obj.description === 'string' ? obj.description : undefined,
    built_in:    false,
    vars,
  };
}

/** Generate a fresh, collision-free custom id derived from a theme name. */
function freshCustomId(name: string): string {
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'theme';
  let id = `custom-${slug}-${Date.now().toString(36)}`;
  // Extremely unlikely collision but be paranoid in case multiple imports run
  // back-to-back within the same millisecond.
  let n = 1;
  const taken = new Set([...BUILT_IN, ..._custom].map(t => t.id));
  while (taken.has(id)) id = `custom-${slug}-${Date.now().toString(36)}-${n++}`;
  return id;
}

/** Import a parsed Theme — always re-keys to a fresh `custom-*` id so the
 *  imported entry never collides with built-ins or other customs. Returns
 *  the saved theme. */
async function importTheme(parsed: Theme): Promise<Theme> {
  const theme: Theme = { ...parsed, id: freshCustomId(parsed.name), built_in: false };
  await saveCustom(theme);
  return theme;
}

/** Read a theme JSON string and import it as a fresh custom entry. */
async function importThemeFromJson(raw: string): Promise<Theme> {
  return importTheme(parseThemeJson(raw));
}

/** Bulk-import: each entry is either a successful import or an error message
 *  tied to its source label (filename / index). Never throws — caller renders. */
export interface ImportResult {
  source: string;       // filename or label, for error toasts
  ok:     boolean;
  theme?: Theme;
  error?: string;
}

async function importThemesBulk(items: { source: string; raw: string }[]): Promise<ImportResult[]> {
  const out: ImportResult[] = [];
  for (const { source, raw } of items) {
    try {
      const t = await importThemeFromJson(raw);
      out.push({ source, ok: true, theme: t });
    } catch (e) {
      out.push({ source, ok: false, error: (e as Error).message });
    }
  }
  return out;
}

/** Serialise a theme to a stable, indented JSON string for export to disk. */
function serializeTheme(t: Theme): string {
  // Strip `built_in` from exports — the import side always re-flags as false.
  const { id, name, description, vars } = t;
  return JSON.stringify({ id, name, description, built_in: false, vars }, null, 2);
}

// ---------------------------------------------------------------------------

export const themeStore = {
  get ready()       { return _ready; },
  get activeId()    { return _activeId; },
  get activeTheme() { return activeTheme; },
  get allThemes()   { return allThemes; },
  get builtIn()     { return BUILT_IN; },
  get custom()      { return _custom; },
  init,
  refresh,
  setActive,
  preview,
  revertPreview,
  saveCustom,
  deleteCustom,
  // Import / export
  importTheme,
  importThemeFromJson,
  importThemesBulk,
  parseThemeJson,
  serializeTheme,
};
