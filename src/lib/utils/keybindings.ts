import { isMac } from './platform';

export interface Keybinding {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  description: string;
  group: string;
}

export const GROUP_ORDER = ['Navigation', 'Panels', 'Sidebar Sections', 'Git', 'Terminal', 'File Explorer'] as const;
export type BindingGroup = (typeof GROUP_ORDER)[number];

export const DEFAULT_KEYBINDINGS: Record<string, Keybinding> = {
  // Navigation
  open_repo:          { key: 'o',     ctrl: true,                description: 'Open repository',            group: 'Navigation' },
  // Pair with Ctrl+O / Ctrl+Shift+R — Ctrl+Shift+O is "open variant" (clone)
  // and Ctrl+Shift+I is its symmetric "init in place" sibling.
  clone_repo:         { key: 'o',     ctrl: true,  shift: true,  description: 'Clone repository',           group: 'Navigation' },
  init_repo:          { key: 'i',     ctrl: true,  shift: true,  description: 'Initialize repository',      group: 'Navigation' },
  open_recent:        { key: 'r',     ctrl: true,                description: 'Recent repos quick-switch',  group: 'Navigation' },
  // Moved off Ctrl+Shift+B (now toggles the right sidebar) — Ctrl+Shift+R
  // pairs naturally with Ctrl+R (recent repos) for "remote / repo browser".
  repo_browser:       { key: 'r',     ctrl: true,  shift: true,  description: 'Browse remote repositories', group: 'Navigation' },
  close_tab:          { key: 'w',     ctrl: true,                description: 'Close current tab',          group: 'Navigation' },
  next_tab:           { key: 'Tab',   ctrl: true,                description: 'Next tab',                   group: 'Navigation' },
  prev_tab:           { key: 'Tab',   ctrl: true,  shift: true,  description: 'Previous tab',               group: 'Navigation' },
  // Generic IDE-style left-rail expand/collapse — closes whatever section is
  // open or restores the last one. The explicit "open Branches" shortcut is
  // `toggle_branches_sidebar` (Alt+Shift+1) under the Sidebar Sections group.
  toggle_sidebar:     { key: 'b',     ctrl: true,                description: 'Toggle sidebar visibility',  group: 'Navigation' },
  toggle_right_sidebar: { key: 'b',   ctrl: true,  shift: true,  description: 'Toggle right sidebar',       group: 'Navigation' },
  // VS Code-style "show/hide the lower dock". Mirrors `toggle_sidebar` (Ctrl+B)
  // for the bottom panel — closes whichever section is open, or restores the
  // last one if nothing is active.
  toggle_bottom_panel: { key: 'j',    ctrl: true,                description: 'Toggle bottom panel',        group: 'Navigation' },
  // Sidebar quick-jump for the MR / PR section — mnemonic "M for Merge".
  // Ctrl+Shift+M is free across the existing bindings and avoids the
  // Ctrl+Alt+letter trap (AltGr suppression on IT/DE/FR/ES layouts). Kept
  // distinct from the Alt+Shift+digit scheme below because MR predates that
  // numbered scheme and the mnemonic is well-established.
  toggle_mr_sidebar:  { key: 'm',     ctrl: true,  shift: true,  description: 'Toggle Pull / Merge Requests sidebar', group: 'Sidebar Sections' },
  focus_graph:        { key: 'g',     alt: true,                 description: 'Focus commit graph',         group: 'Navigation' },
  // F6 cycles focus across the major layout zones (titlebar, tabs, activity
  // bars, sidebar, graph, bottom panel, status bar) so the whole UI is
  // reachable from the keyboard without dedicated per-zone shortcuts.
  // Mirrors the same chord already used by FileExplorerModal.
  cycle_focus:         { key: 'F6',                              description: 'Cycle focus to next panel',     group: 'Navigation' },
  cycle_focus_reverse: { key: 'F6',                shift: true,  description: 'Cycle focus to previous panel', group: 'Navigation' },
  // Workspace-aware project pickers (pre-fill the command palette).
  open_project:       { key: 'n',     ctrl: true,                description: 'Open project in workspace',  group: 'Navigation' },
  open_from_workspace:{ key: 'n',     ctrl: true,  shift: true,  description: 'Open project from another workspace', group: 'Navigation' },
  // Move between Arbor's open windows (Canopy, Corvus, Bennu, Merula, the File
  // Explorer, Tyto) without leaving the keyboard. Ctrl+Shift+G — "go to
  // window" — is free across the existing bindings: `focus_graph` is Alt+G and
  // `toggle_branch_grouping` is Alt+Shift+G, both different chords. Available
  // in EVERY window (mounted from `+page.svelte`), not just Corvus, because the
  // whole point is escaping the window you are in — on macOS especially, where
  // there is no per-window taskbar button to click.
  switch_window:      { key: 'g',     ctrl: true,  shift: true,  description: 'Switch window',              group: 'Navigation' },
  // Workspace registry / management modal — Alt+Shift+W avoids both the
  // Ctrl+Alt+letter trap (AltGr suppression) and the Win32 Alt-menu access
  // collision of bare Alt+W.
  workspace_manager:  { key: 'w',     alt:  true,  shift: true,  description: 'Open Workspace Manager',     group: 'Navigation' },

  // Panels
  command_palette: { key: 'k',   ctrl: true,                description: 'Command palette',       group: 'Panels' },
  settings:       { key: ',',     ctrl: true,                description: 'Open settings',         group: 'Panels' },
  plugins:        { key: 'x',     ctrl: true,  shift: true,  description: 'Open Plugin Manager',   group: 'Panels' },
  // Sibling of `plugins` (Ctrl+Shift+X). Marketplace is global — reachable
  // from any panel — so it gets an Alt+Shift+M binding, symmetric with
  // Alt+Shift+W (Workspace Manager). Avoids the Ctrl+Alt+letter AltGr trap.
  open_marketplace: { key: 'm',                 shift: true, alt: true, description: 'Open Plugin Marketplace', group: 'Panels' },
  stage_view:     { key: 's',     ctrl: true,  shift: true,  description: 'Toggle stage area',     group: 'Panels' },
  toggle_docs:    { key: 'F1',                               description: 'Toggle documentation',  group: 'Panels' },
  // Quick keyboard-shortcuts cheat-sheet (ShortcutsModal). Sibling of F1
  // (full documentation): F1 → docs, Shift+F1 → the searchable shortcut
  // reference. Shift+F1 is the universal "context help" convention and is
  // free across the existing bindings.
  open_shortcuts: { key: 'F1',                  shift: true, description: 'Show keyboard shortcuts', group: 'Panels' },
  // Avoid Ctrl+Alt+letter — on Italian / German / French / Spanish keyboards
  // Windows synthesises AltGr as Ctrl+Alt and Chromium suppresses the
  // shortcut so the user can still type AltGr-mapped characters (@ \ ~ …).
  // Alt+Shift+L is unambiguous and doesn't trigger Windows menu access.
  plugin_logs:    { key: 'l',                  shift: true, alt: true, description: 'Toggle plugin logs console', group: 'Panels' },
  // Toggle the active (or last-opened) plugin main-area view — the body
  // surface registered via `arbor.ui.add_view`. Alt+Shift+V mirrors the
  // Alt+Shift+letter scheme (avoids the Ctrl+Alt AltGr trap on IT/DE/FR/ES
  // layouts and the Win32 Alt-menu collision of bare Alt+V). No-op until a
  // plugin view has been opened at least once.
  toggle_plugin_view: { key: 'v',              shift: true, alt: true, description: 'Toggle plugin view', group: 'Panels' },
  // "Show keyboard inputs" — the demo/screencast overlay. Alt+Shift+K
  // mirrors the Alt+Shift+letter scheme used everywhere else (avoids the
  // Ctrl+Alt AltGr trap on IT/DE/FR/ES layouts and the Win32 Alt-menu
  // collision of bare Alt+K).
  toggle_keystrokes: { key: 'k',               shift: true, alt: true, description: 'Toggle keyboard-inputs overlay', group: 'Panels' },
  search:         { key: 'f',     ctrl: true,                description: 'Search commits',        group: 'Panels' },
  diff_split:     { key: '1',     alt: true,                 description: 'Split diff view',       group: 'Panels' },
  diff_unified:   { key: '2',     alt: true,                 description: 'Unified diff view',     group: 'Panels' },
  next_chunk:     { key: 'F3',                                description: 'Next diff chunk',       group: 'Panels' },
  prev_chunk:     { key: 'F3',                  shift: true,  description: 'Previous diff chunk',   group: 'Panels' },
  // Toggle the full-screen diff overlay for the currently visible diff
  // (stage panel, commit detail, MR detail). F11 is the universal
  // fullscreen convention (browsers, OS, IDEs); Tauri's webview doesn't
  // claim it for native window fullscreen, so it's free for us. Handled
  // inside DiffViewer via a capture-phase listener so the same chord
  // closes the overlay when it's already open (Modal-based modals
  // otherwise block global shortcuts).
  toggle_diff_fullscreen: { key: 'F11', description: 'Toggle full-screen diff', group: 'Panels' },

  // Terminal
  toggle_terminal: { key: '`',   ctrl: true,                 description: 'Toggle terminal panel',  group: 'Terminal' },
  new_terminal:    { key: '`',   ctrl: true,  shift: true,   description: 'New terminal tab',       group: 'Terminal' },

  // Navigation (graph)
  jump_to_head:   { key: 'Home',  ctrl: true,                description: 'Jump to HEAD commit',   group: 'Navigation' },
  // Open the context menu wherever the focus is — the graph (on the selected
  // commit), sidebar items, worktrees, the File Explorer. Ctrl+Shift+K works on
  // every keyboard (the dedicated "Menu" key is missing on most laptops); the
  // physical Menu key still opens menus natively too. Shift+F10 is intentionally
  // left to the Run shortcut.
  open_context_menu: { key: 'k', ctrl: true, shift: true,    description: 'Open context menu',     group: 'Navigation' },

  // Git
  fetch:          { key: 'f',     ctrl: true,  shift: true,  description: 'Fetch all remotes',     group: 'Git' },
  // Universal IDE convention — same handler as the StatusBar fetch spinner.
  // Kept separate from `fetch` so users can rebind one without losing the
  // other (and so Settings → Keybindings shows both as discrete rows).
  refresh_graph:  { key: 'F5',                                description: 'Refresh graph (fetch)', group: 'Git' },
  pull:           { key: 'l',     ctrl: true,  shift: true,  description: 'Pull current branch',   group: 'Git' },
  push:           { key: 'p',     ctrl: true,  shift: true,  description: 'Push current branch',   group: 'Git' },
  // Moved off Ctrl+Shift+N (workspace project picker took it) and then off
  // Ctrl+Alt+B because Italian / German / French / Spanish keyboards
  // synthesise AltGr as Ctrl+Alt — Chromium drops the chord. Alt+Shift+B
  // keeps the "B for branch" mnemonic and works everywhere.
  new_branch:     { key: 'b',     alt: true,   shift: true,  description: 'Create new branch',     group: 'Git' },
  stash:          { key: 'h',     ctrl: true,  shift: true,  description: 'Stash changes',         group: 'Git' },
  commit:         { key: 'Enter', ctrl: true,                description: 'Commit staged changes', group: 'Git' },
  // Pairs with `commit` (Ctrl+Enter): same chord with Shift commits and pushes
  // in one go. Only meaningful while the commit message field has focus.
  commit_and_push: { key: 'Enter', ctrl: true, shift: true,  description: 'Commit staged changes and push', group: 'Git' },
  stage_all:      { key: 'a',     ctrl: true,  shift: true,  description: 'Stage all changes',     group: 'Git' },
  unstage_all:    { key: 'u',     ctrl: true,  shift: true,  description: 'Unstage all changes',   group: 'Git' },

  // Sidebar Sections — IntelliJ-style numbered tool-window shortcuts.
  // Each binding is no-op when the matching ActivityBar button is hidden via
  // Settings → Customize Activity Bar (mirrors IntelliJ Alt+1..9 behavior).
  // Alt+Shift+digit avoids the AltGr (Ctrl+Alt+letter) trap on IT/DE/FR/ES
  // layouts and the Win32 Alt-menu access collision of bare Alt+digit.
  toggle_branches_sidebar: { key: '1', alt: true, shift: true, description: 'Toggle Branches & Stashes sidebar', group: 'Sidebar Sections' },
  toggle_files_sidebar:    { key: '2', alt: true, shift: true, description: 'Toggle Files sidebar',              group: 'Sidebar Sections' },
  toggle_gitflow_sidebar:  { key: '3', alt: true, shift: true, description: 'Toggle Git Flow sidebar',           group: 'Sidebar Sections' },
  toggle_issues_sidebar:   { key: '4', alt: true, shift: true, description: 'Toggle Issues sidebar',             group: 'Sidebar Sections' },
  toggle_pipelines_panel:  { key: '5', alt: true, shift: true, description: 'Toggle Pipelines panel',            group: 'Sidebar Sections' },
  toggle_reflog_sidebar:   { key: '6', alt: true, shift: true, description: 'Toggle Reflog sidebar',             group: 'Sidebar Sections' },
  toggle_stats_sidebar:    { key: '7', alt: true, shift: true, description: 'Toggle Repository Statistics sidebar', group: 'Sidebar Sections' },
  toggle_security_sidebar: { key: '8', alt: true, shift: true, description: 'Toggle Security / Vulnerability sidebar', group: 'Sidebar Sections' },
  // Branches sidebar: flip the per-repo path-grouping view (folder tree
  // vs. flat list). Alt+Shift+G keeps the "G for group" mnemonic, avoids
  // the Ctrl+Alt AltGr trap on IT/DE/FR/ES layouts, and is distinct from
  // `focus_graph` (Alt+G).
  toggle_branch_grouping:  { key: 'g', alt: true, shift: true, description: 'Toggle branch grouping (folder tree vs. flat list)', group: 'Sidebar Sections' },

  // File Explorer — preview pane. Handled locally by FileExplorerModal and only
  // active while a file preview is showing. F5 mirrors the universal "Refresh"
  // convention (it re-reads the previewed file). Live tail uses Ctrl+Shift+T
  // ("T for tail") — Ctrl+Shift+L is already Pull, so it's deliberately avoided;
  // both stay clear of the Ctrl+Alt+letter AltGr trap on IT/DE/FR/ES layouts.
  explorer_refresh_preview: { key: 'F5',                            description: 'Refresh file preview',        group: 'File Explorer' },
  explorer_toggle_live:     { key: 't', ctrl: true, shift: true,    description: 'Toggle live tail (follow file)', group: 'File Explorer' },
};

export function matchesBinding(event: KeyboardEvent, binding: Keybinding): boolean {
  if (!binding.key) return false;
  const ctrlMatch  = !!binding.ctrl  === (event.ctrlKey || event.metaKey);
  const shiftMatch = !!binding.shift === event.shiftKey;
  const altMatch   = !!binding.alt   === event.altKey;
  if (!(ctrlMatch && shiftMatch && altMatch)) return false;

  // Fallback to `event.code` for digits and letters: with Shift held the
  // browser reports the shifted character (e.g. Shift+1 → `event.key === '!'`
  // on US, `£`/`!`/`"` on EU layouts), which would otherwise never match a
  // binding declared as `'1'`. `event.code` is layout-independent.
  const k = binding.key;
  if (event.key.toLowerCase() === k.toLowerCase()) return true;
  if (k.length === 1) {
    if (/[0-9]/.test(k) && (event.code === `Digit${k}` || event.code === `Numpad${k}`)) return true;
    if (/[a-z]/i.test(k) && event.code === `Key${k.toUpperCase()}`) return true;
  }
  return false;
}

// Friendlier display names for keys whose `KeyboardEvent.key` value reads
// awkwardly (the dedicated context-menu key reports as 'ContextMenu').
const KEY_DISPLAY: Record<string, string> = { ContextMenu: 'Menu' };

// macOS renders modifiers as glyphs. Arbor's logical `ctrl` maps to Cmd on the
// Mac (see matchesBinding: `event.metaKey` counts as ctrl), so Ctrl → ⌘; the
// Tauri-accelerator aliases (CmdOrCtrl, Super, …) fold into the same bucket.
const MAC_MOD_ALIASES: Record<string, 'alt' | 'shift' | 'ctrl'> = {
  ctrl: 'ctrl', control: 'ctrl', cmd: 'ctrl', command: 'ctrl',
  cmdorctrl: 'ctrl', commandorcontrol: 'ctrl', meta: 'ctrl', super: 'ctrl', win: 'ctrl',
  '⌘': 'ctrl',
  alt: 'alt', option: 'alt', opt: 'alt', '⌥': 'alt',
  shift: 'shift', '⇧': 'shift',
};

// Named (non-glyph) keys → their macOS glyph. Keyed case-insensitively and by
// both the short label form ("Up") and the KeyboardEvent.key form ("ArrowUp"),
// since free-form <Kbd label="…" /> strings use either.
const MAC_KEY_ALIASES: Record<string, string> = {
  enter: '↩', return: '↩', tab: '⇥', backspace: '⌫', delete: '⌦', del: '⌦',
  escape: '⎋', esc: '⎋', space: '␣', spacebar: '␣',
  up: '↑', down: '↓', left: '←', right: '→',
  arrowup: '↑', arrowdown: '↓', arrowleft: '←', arrowright: '→',
  home: '↖', end: '↘', pageup: '⇞', pagedown: '⇟', pgup: '⇞', pgdn: '⇟',
};

/**
 * Convert a `'+'`-joined chord string ("Ctrl+Shift+E", "Alt+Left", "F1") to its
 * macOS form: word modifiers become glyphs (⌥⇧⌘) reordered into Apple's
 * canonical order, named keys become glyphs (Enter → ↩, Tab → ⇥, arrows, …).
 * The `'+'` separator is preserved as a split token for {@link Kbd}; it is not
 * necessarily rendered (macOS chords show glyphs with no separator).
 *
 * Non-macOS callers never invoke this — the wording (Ctrl/Alt/Shift) stays.
 */
export function macKeyLabel(label: string): string {
  const mods = { alt: false, shift: false, ctrl: false };
  const rest: string[] = [];
  for (const tok of label.split('+').map(t => t.trim()).filter(Boolean)) {
    const mod = MAC_MOD_ALIASES[tok.toLowerCase()];
    if (mod) { mods[mod] = true; continue; }
    rest.push(MAC_KEY_ALIASES[tok.toLowerCase()] ?? tok);
  }
  const out: string[] = [];
  if (mods.alt)   out.push('⌥');
  if (mods.shift) out.push('⇧');
  if (mods.ctrl)  out.push('⌘');
  out.push(...rest);
  return out.join('+');
}

/**
 * Format a binding for display. Builds the canonical "Ctrl+Alt+Shift+Key"
 * string and, on macOS, folds it to native glyphs via {@link macKeyLabel}.
 */
export function formatBinding(binding: Keybinding): string {
  const key = KEY_DISPLAY[binding.key] ?? (binding.key.length === 1 ? binding.key.toUpperCase() : binding.key);
  const parts: string[] = [];
  if (binding.ctrl)  parts.push('Ctrl');
  if (binding.alt)   parts.push('Alt');
  if (binding.shift) parts.push('Shift');
  parts.push(key);
  const label = parts.join('+');
  return isMac ? macKeyLabel(label) : label;
}

// ───────────────────────────────────────────────────────────────────────────
//  Tauri accelerators (macOS system menu bar)
// ───────────────────────────────────────────────────────────────────────────
//
// The native menu bar wants Tauri/muda accelerator strings ("CmdOrCtrl+Shift+O")
// — a different alphabet from both the display label and `KeyboardEvent.key`.
// An unparsable string is *not* an error downstream: muda drops it and the item
// simply renders without a shortcut, so best-effort mapping is safe.

/** `KeyboardEvent.key` values whose accelerator spelling differs. */
const ACCEL_KEY_ALIASES: Record<string, string> = {
  arrowup: 'Up', arrowdown: 'Down', arrowleft: 'Left', arrowright: 'Right',
  up: 'Up', down: 'Down', left: 'Left', right: 'Right',
  esc: 'Escape', escape: 'Escape', del: 'Delete', ' ': 'Space', spacebar: 'Space',
  pgup: 'PageUp', pgdn: 'PageDown', return: 'Enter',
};

/** Modifier tokens accepted in free-form `shortcut` strings, folded to accelerator form. */
const ACCEL_MOD_ALIASES: Record<string, 'CmdOrCtrl' | 'Alt' | 'Shift'> = {
  ctrl: 'CmdOrCtrl', control: 'CmdOrCtrl', cmd: 'CmdOrCtrl', command: 'CmdOrCtrl',
  cmdorctrl: 'CmdOrCtrl', meta: 'CmdOrCtrl', super: 'CmdOrCtrl', win: 'CmdOrCtrl',
  alt: 'Alt', option: 'Alt', opt: 'Alt',
  shift: 'Shift',
};

/** One key token → its accelerator spelling (letters uppercase, arrows shortened). */
function acceleratorKey(key: string): string {
  const alias = ACCEL_KEY_ALIASES[key.toLowerCase()];
  if (alias) return alias;
  return key.length === 1 ? key.toUpperCase() : key;
}

/**
 * A binding as a Tauri accelerator — `null` when unbound. Arbor's logical `ctrl`
 * is Cmd on the Mac (see {@link matchesBinding}), which is exactly what
 * `CmdOrCtrl` means.
 */
export function acceleratorFor(binding: Keybinding): string | null {
  if (!binding.key) return null;
  const parts: string[] = [];
  if (binding.ctrl)  parts.push('CmdOrCtrl');
  if (binding.alt)   parts.push('Alt');
  if (binding.shift) parts.push('Shift');
  parts.push(acceleratorKey(binding.key));
  return parts.join('+');
}

/**
 * Same, from a hand-written `'+'`-joined chord ("Ctrl+Shift+R", "Shift+F10") —
 * the `shortcut` escape hatch on menu items that have no keybinding id.
 */
export function acceleratorFromLabel(label: string): string | null {
  const mods: string[] = [];
  const keys: string[] = [];
  for (const tok of label.split('+').map(t => t.trim()).filter(Boolean)) {
    const mod = ACCEL_MOD_ALIASES[tok.toLowerCase()];
    if (mod) { if (!mods.includes(mod)) mods.push(mod); continue; }
    keys.push(acceleratorKey(tok));
  }
  // Exactly one non-modifier key, or muda can't parse it.
  if (keys.length !== 1) return null;
  return [...mods, keys[0]].join('+');
}
