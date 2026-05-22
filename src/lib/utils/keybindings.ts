export interface Keybinding {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  description: string;
  group: string;
}

export const GROUP_ORDER = ['Navigation', 'Panels', 'Sidebar Sections', 'Git', 'Terminal'] as const;
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
  focus_sidebar:      { key: 's',     alt: true,                 description: 'Focus sidebar',              group: 'Navigation' },
  // Workspace-aware project pickers (pre-fill the command palette).
  open_project:       { key: 'n',     ctrl: true,                description: 'Open project in workspace',  group: 'Navigation' },
  open_from_workspace:{ key: 'n',     ctrl: true,  shift: true,  description: 'Open project from another workspace', group: 'Navigation' },
  // Workspace registry / management modal — Alt+Shift+W avoids both the
  // Ctrl+Alt+letter trap (AltGr suppression) and the Win32 Alt-menu access
  // collision of bare Alt+W.
  workspace_manager:  { key: 'w',     alt:  true,  shift: true,  description: 'Open Workspace Manager',     group: 'Navigation' },

  // Panels
  command_palette: { key: 'k',   ctrl: true,                description: 'Command palette',       group: 'Panels' },
  settings:       { key: ',',     ctrl: true,                description: 'Open settings',         group: 'Panels' },
  plugins:        { key: 'x',     ctrl: true,  shift: true,  description: 'Open Plugin Manager',   group: 'Panels' },
  stage_view:     { key: 's',     ctrl: true,  shift: true,  description: 'Toggle stage area',     group: 'Panels' },
  toggle_docs:    { key: 'F1',                               description: 'Toggle documentation',  group: 'Panels' },
  // Avoid Ctrl+Alt+letter — on Italian / German / French / Spanish keyboards
  // Windows synthesises AltGr as Ctrl+Alt and Chromium suppresses the
  // shortcut so the user can still type AltGr-mapped characters (@ \ ~ …).
  // Alt+Shift+L is unambiguous and doesn't trigger Windows menu access.
  plugin_logs:    { key: 'l',                  shift: true, alt: true, description: 'Toggle plugin logs console', group: 'Panels' },
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

  // Terminal
  toggle_terminal: { key: '`',   ctrl: true,                 description: 'Toggle terminal panel',  group: 'Terminal' },
  new_terminal:    { key: '`',   ctrl: true,  shift: true,   description: 'New terminal tab',       group: 'Terminal' },

  // Navigation (graph)
  jump_to_head:   { key: 'Home',  ctrl: true,                description: 'Jump to HEAD commit',   group: 'Navigation' },

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
  toggle_files_sidebar:    { key: '2', alt: true, shift: true, description: 'Toggle File Tree sidebar',          group: 'Sidebar Sections' },
  toggle_gitflow_sidebar:  { key: '3', alt: true, shift: true, description: 'Toggle Git Flow sidebar',           group: 'Sidebar Sections' },
  toggle_issues_sidebar:   { key: '4', alt: true, shift: true, description: 'Toggle Issues sidebar',             group: 'Sidebar Sections' },
  toggle_pipelines_panel:  { key: '5', alt: true, shift: true, description: 'Toggle Pipelines panel',            group: 'Sidebar Sections' },
  toggle_reflog_sidebar:   { key: '6', alt: true, shift: true, description: 'Toggle Reflog sidebar',             group: 'Sidebar Sections' },
  toggle_stats_sidebar:    { key: '7', alt: true, shift: true, description: 'Toggle Repository Statistics sidebar', group: 'Sidebar Sections' },
  toggle_security_sidebar: { key: '8', alt: true, shift: true, description: 'Toggle Security / Vulnerability sidebar', group: 'Sidebar Sections' },
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

export function formatBinding(binding: Keybinding): string {
  const parts: string[] = [];
  if (binding.ctrl)  parts.push('Ctrl');
  if (binding.alt)   parts.push('Alt');
  if (binding.shift) parts.push('Shift');
  parts.push(binding.key.length === 1 ? binding.key.toUpperCase() : binding.key);
  return parts.join('+');
}
