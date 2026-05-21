import { untrack } from 'svelte';
import { matchesBinding } from '$lib/utils/keybindings';
import { keybindingsStore } from './keybindings.svelte';
import { contributionStore } from './contribution.svelte';
import { pluginStore } from './plugin.svelte';

/**
 * "Show keyboard inputs" overlay store.
 *
 * Renders a floating stack of recently-pressed keys / chords on top of the
 * whole window — useful when recording screencasts, pair-programming over
 * screen-share, or giving a demo.  All settings persist to localStorage so
 * the user keeps their pick across sessions.
 *
 * Capture model:
 *   - Global `keydown` listener on `window` (capture phase) so we see the
 *     key BEFORE any other handler runs — even when modals trap focus.
 *   - We never `preventDefault()` so behavior is unchanged.
 *   - The listener auto-attaches when `enabled` is true and detaches the
 *     moment it flips back to false.
 *
 * Filtering:
 *   - `showInInputs` defaults to false → no per-letter spam while the user
 *     types into a text field. Modifier chords (Ctrl/Alt/Meta) are always
 *     captured though — Ctrl+S inside a textarea is still useful to see.
 *   - `onlyShortcuts` defaults to false. When true, plain printable keys
 *     are skipped — only chords with at least one modifier survive.
 *
 * Repeat collapsing:
 *   - When the same chord fires within `REPEAT_WINDOW_MS`, we bump a
 *     counter on the most-recent entry rather than stacking duplicates.
 */

export type KeystrokePosition =
  | 'top-left' | 'top-right'
  | 'bottom-left' | 'bottom-right'
  | 'bottom-center';
export type KeystrokeSize = 'sm' | 'md' | 'lg';
export type KeystrokeTone = 'accent' | 'neon' | 'aqua' | 'amber' | 'mono';

/** CSS colour resolved for each named tone. Empty string ⇒ fall back to
 *  the theme variable (defined inside the overlay component).         */
export const TONE_COLORS: Record<KeystrokeTone, string> = {
  accent: '',                  // → var(--accent)
  neon:   '#d946ef',
  aqua:   '#06b6d4',
  amber:  '#f59e0b',
  mono:   '',                  // → var(--text-primary) (handled by overlay)
};

export interface KeystrokeEntry {
  id:       number;
  /** Pre-rendered chord parts, e.g. ['Ctrl','Shift','K'] or ['Enter']. */
  parts:    string[];
  /** Times this chord was repeated rapidly (collapsed display). */
  count:    number;
  /** Wall-clock ms when the entry was recorded — drives auto-dismiss. */
  addedAt:  number;
  /** True when this entry is a mouse click rather than a key press. */
  isMouse?: boolean;
  /** Human-readable label of the action this chord triggers — looked up
   *  against built-in keybindings AND plugin contributions at capture
   *  time so a rebind in Settings flows through automatically. Null
   *  when nothing is bound to this chord. */
  action?:  string | null;
}

const STORAGE_KEY        = 'arbor:keystrokes';
/** Bump whenever a default change must be force-propagated to existing
 *  users (their saved value would otherwise keep the broken behaviour).
 *  History:
 *    1 → 2 : opacity default 0.94 → 1.0 (card was bleeding through
 *            anything underneath because the gradient mixed bg-* with
 *            `transparent` and stacked with the root opacity).
 */
const STORAGE_VERSION    = 2;
const REPEAT_WINDOW_MS   = 600;
const MAX_ENTRIES        = 6;
const DEFAULT_DISPLAY_MS = 2200;

interface PersistedSettings {
  enabled:         boolean;
  position:        KeystrokePosition;
  size:            KeystrokeSize;
  tone:            KeystrokeTone;
  displayMs:       number;
  onlyShortcuts:   boolean;
  showInInputs:    boolean;
  showMouseClicks: boolean;
  groupRepeats:    boolean;
  showAction:      boolean;       // render the action label under the chord
  compact:         boolean;       // force a single-line chord+action layout
  opacity:         number;        // 0.4 → 1.0
  edgeOffset:      number;        // px from the anchored screen edge
}

const DEFAULTS: PersistedSettings = {
  enabled:         false,
  position:        'bottom-right',
  size:            'md',
  tone:            'accent',
  displayMs:       DEFAULT_DISPLAY_MS,
  onlyShortcuts:   false,
  showInInputs:    false,
  showMouseClicks: false,
  groupRepeats:    true,
  showAction:      true,
  compact:         false,
  opacity:         1.0,
  edgeOffset:      44,
};

type PersistedShape = PersistedSettings & { _v?: number };

function loadSettings(): PersistedSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULTS };
    const parsed = JSON.parse(raw) as Partial<PersistedShape>;
    const savedVersion = parsed._v ?? 1;
    if (savedVersion < STORAGE_VERSION) {
      // V1 → V2: the card was bleeding through whatever was underneath
      // because we mixed bg-* tokens with `transparent` AND multiplied
      // by the root opacity.  Force-reset opacity to 1.0 so the visual
      // fix lands for existing users (the slider is right there if they
      // want to dial translucency back in deliberately).
      parsed.opacity = 1.0;
      parsed._v      = STORAGE_VERSION;
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...DEFAULTS, ...parsed }));
      } catch { /* ignore */ }
    }
    return { ...DEFAULTS, ...parsed };
  } catch {
    return { ...DEFAULTS };
  }
}

/** Human-readable label for keys whose `event.key` value is awkward / verbose. */
const KEY_LABELS: Record<string, string> = {
  ' ':           'Space',
  ArrowUp:       '↑',
  ArrowDown:     '↓',
  ArrowLeft:     '←',
  ArrowRight:    '→',
  Escape:        'Esc',
  Enter:         'Enter',
  Tab:           'Tab',
  Backspace:     'Backspace',
  Delete:        'Del',
  PageUp:        'PgUp',
  PageDown:      'PgDn',
  Home:          'Home',
  End:           'End',
  CapsLock:      'Caps',
  Control:       'Ctrl',
  Shift:         'Shift',
  Alt:           'Alt',
  Meta:          'Meta',
  ContextMenu:   'Menu',
};

function formatKey(e: KeyboardEvent): string {
  if (e.key in KEY_LABELS) return KEY_LABELS[e.key];
  if (e.key.length === 1) return e.key.toUpperCase();
  // F-keys, MediaPlay, etc. pass through as-is.
  return e.key;
}

function isModifierOnly(key: string): boolean {
  return key === 'Control' || key === 'Shift' || key === 'Alt' || key === 'Meta';
}

function isTextEditingTarget(t: EventTarget | null): boolean {
  if (!t || !(t instanceof HTMLElement)) return false;
  const tag = t.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (t.isContentEditable) return true;
  return false;
}

function partsEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

function createKeystrokesStore() {
  const initial = loadSettings();

  let enabled         = $state(initial.enabled);
  let position        = $state<KeystrokePosition>(initial.position);
  let size            = $state<KeystrokeSize>(initial.size);
  let tone            = $state<KeystrokeTone>(initial.tone);
  let displayMs       = $state(initial.displayMs);
  let onlyShortcuts   = $state(initial.onlyShortcuts);
  let showInInputs    = $state(initial.showInInputs);
  let showMouseClicks = $state(initial.showMouseClicks);
  let groupRepeats    = $state(initial.groupRepeats);
  let showAction      = $state(initial.showAction);
  let compact         = $state(initial.compact);
  let opacity         = $state(initial.opacity);
  let edgeOffset      = $state(initial.edgeOffset);

  /** Visible queue, newest first. */
  let entries = $state<KeystrokeEntry[]>([]);
  let counter = 0;

  /** Per-entry sweep timers so each entry expires independently. */
  const sweepTimers = new Map<number, ReturnType<typeof setTimeout>>();

  function persist() {
    untrack(() => {
      try {
        const data: PersistedShape = {
          enabled, position, size, tone, displayMs,
          onlyShortcuts, showInInputs, showMouseClicks, groupRepeats,
          showAction, compact, opacity, edgeOffset,
          _v: STORAGE_VERSION,
        };
        localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
      } catch { /* ignore — disabled storage */ }
    });
  }

  function scheduleSweep(id: number) {
    const t = setTimeout(() => {
      sweepTimers.delete(id);
      entries = entries.filter(e => e.id !== id);
    }, displayMs);
    sweepTimers.set(id, t);
  }

  function resetSweep(id: number) {
    const existing = sweepTimers.get(id);
    if (existing) clearTimeout(existing);
    scheduleSweep(id);
  }

  function clearAll() {
    for (const t of sweepTimers.values()) clearTimeout(t);
    sweepTimers.clear();
    entries = [];
  }

  function pushEntry(parts: string[], isMouse = false, action: string | null = null) {
    if (parts.length === 0) return;

    // Repeat collapsing — only when groupRepeats is on, the most recent
    // entry is the same chord, AND it landed within REPEAT_WINDOW_MS.
    const now    = Date.now();
    const newest = entries[0];
    if (groupRepeats && newest && newest.isMouse === isMouse
        && partsEqual(newest.parts, parts)
        && now - newest.addedAt < REPEAT_WINDOW_MS) {
      newest.count   += 1;
      newest.addedAt  = now;
      entries = [newest, ...entries.slice(1)];
      resetSweep(newest.id);
      return;
    }

    const id: number = ++counter;
    const entry: KeystrokeEntry = { id, parts, count: 1, addedAt: now, isMouse, action };
    entries = [entry, ...entries].slice(0, MAX_ENTRIES);
    scheduleSweep(id);
  }

  /** Resolve a `KeyboardEvent` to the human label of whichever Arbor
   *  action it triggers, or `null` if nothing is bound. Plugin
   *  contributions take priority (mirrors AppShell's onKeydown order). */
  function resolveActionLabel(e: KeyboardEvent): string | null {
    try {
      const points = contributionStore.forPoint('arbor:keybinding');
      for (const c of points) {
        if (pluginStore.disabledPlugins.has(c.plugin_name)) continue;
        const p = c.payload as { key?: string; ctrl?: boolean; shift?: boolean; alt?: boolean; description?: string };
        const binding = {
          key:         p.key ?? '',
          ctrl:        !!p.ctrl,
          shift:       !!p.shift,
          alt:         !!p.alt,
          description: p.description ?? '',
          group:       '',
        };
        if (binding.key && matchesBinding(e, binding)) {
          return p.description ? `${p.description} · ${c.plugin_name}` : c.plugin_name;
        }
      }
    } catch { /* contribution store not ready yet */ }

    try {
      const action = keybindingsStore.matchAction(e);
      if (action) {
        const desc = keybindingsStore.getBinding(action)?.description;
        return desc && desc.trim() ? desc : null;
      }
    } catch { /* ignore */ }

    return null;
  }

  /** Public — record a synthetic entry (used by tests / the settings preview). */
  function recordChord(parts: string[], action: string | null = null) {
    pushEntry(parts, false, action);
  }

  // ── Global capture handlers ────────────────────────────────────────────
  function onKeydown(e: KeyboardEvent) {
    // Ignore key-repeat firings (held-down key) — the visual repeat counter
    // already conveys "held".
    if (e.repeat) return;
    if (isModifierOnly(e.key)) return;
    if (!showInInputs && isTextEditingTarget(e.target) && !(e.ctrlKey || e.metaKey || e.altKey)) return;

    const parts: string[] = [];
    if (e.ctrlKey || e.metaKey) parts.push('Ctrl');
    if (e.altKey)               parts.push('Alt');
    if (e.shiftKey)             parts.push('Shift');
    const label = formatKey(e);
    parts.push(label);

    if (onlyShortcuts && parts.length === 1) return;

    pushEntry(parts, false, resolveActionLabel(e));
  }

  function onMousedown(e: MouseEvent) {
    if (!showMouseClicks) return;
    const which = e.button === 0 ? 'Left Click'
                : e.button === 1 ? 'Middle Click'
                : e.button === 2 ? 'Right Click'
                : `Mouse ${e.button}`;
    pushEntry([which], true);
  }

  // Attach / detach the global listeners reactively. The `untrack` keeps the
  // effect from re-running when other state inside the handler closures
  // changes — we only care about `enabled`.
  let listenersAttached = false;
  function syncListeners() {
    const shouldAttach = enabled && typeof window !== 'undefined';
    if (shouldAttach && !listenersAttached) {
      window.addEventListener('keydown',   onKeydown,   { capture: true });
      window.addEventListener('mousedown', onMousedown, { capture: true });
      listenersAttached = true;
    } else if (!shouldAttach && listenersAttached) {
      window.removeEventListener('keydown',   onKeydown,   { capture: true });
      window.removeEventListener('mousedown', onMousedown, { capture: true });
      listenersAttached = false;
      clearAll();
    }
  }

  // Run once at module init so the listener is live before any UI mounts
  // (the user might have toggled it on in a previous session).
  if (typeof window !== 'undefined') {
    queueMicrotask(syncListeners);
  }

  return {
    get enabled()         { return enabled; },
    get position()        { return position; },
    get size()            { return size; },
    get tone()            { return tone; },
    get displayMs()       { return displayMs; },
    get onlyShortcuts()   { return onlyShortcuts; },
    get showInInputs()    { return showInInputs; },
    get showMouseClicks() { return showMouseClicks; },
    get groupRepeats()    { return groupRepeats; },
    get showAction()      { return showAction; },
    get compact()         { return compact; },
    get opacity()         { return opacity; },
    get edgeOffset()      { return edgeOffset; },
    get entries()         { return entries; },

    setEnabled(v: boolean)            { enabled = v;         persist(); syncListeners(); },
    toggle()                          { this.setEnabled(!enabled); },
    setPosition(v: KeystrokePosition) { position = v;        persist(); },
    setSize(v: KeystrokeSize)         { size = v;            persist(); },
    setTone(v: KeystrokeTone)         { tone = v;            persist(); },
    setDisplayMs(v: number)           { displayMs = Math.max(500, Math.min(10000, v)); persist(); },
    setOnlyShortcuts(v: boolean)      { onlyShortcuts = v;   persist(); },
    setShowInInputs(v: boolean)       { showInInputs = v;    persist(); },
    setShowMouseClicks(v: boolean)    { showMouseClicks = v; persist(); syncListeners(); },
    setGroupRepeats(v: boolean)       { groupRepeats = v;    persist(); },
    setShowAction(v: boolean)         { showAction = v;      persist(); },
    setCompact(v: boolean)            { compact = v;         persist(); },
    setOpacity(v: number)             { opacity = Math.max(0.3, Math.min(1, v)); persist(); },
    setEdgeOffset(v: number)          { edgeOffset = Math.max(8, Math.min(200, Math.round(v))); persist(); },
    recordChord,
    clearAll,
  };
}

export const keystrokesStore = createKeystrokesStore();
