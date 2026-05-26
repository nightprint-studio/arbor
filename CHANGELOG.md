# Changelog

All notable changes to Arbor are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Status bar slimmed down: repo path moved to the left segment; Fetch and
  "Open in browser" relocated to the graph toolbar; version pill removed
  (About still reachable from the Command Palette and the menu).

### Fixed

- Windows taskbar icon goes blank after the system resumes from sleep —
  re-applied on every power-resume notification.

## [0.2.0] — 2026-05-24

### Added

- First-run welcome tour covering Git identity, provider connection, opening
  the first repo, Command Palette, plugin marketplace, ticket chips, linked
  worktrees and workspaces. Reopenable from the Command Palette (*Welcome
  Tour*) or the Docs panel.
- Appearance settings: Activity bar position (Left / Right / Hidden with
  edge-hover reveal), Compact title bar toggle, diff Tab width (2 / 4 / 8),
  and a switch between Mac-style coloured window controls and a flat
  Windows/IntelliJ set (also restyles close buttons in modals and panel
  headers).
- Marketplace registry can list external plugins via a one-line pointer to a
  third-party GitHub repo instead of vendoring the code. Entries without a
  `pinned_sha` get an "Unpinned" badge.
- Plugin Marketplace is reachable from the Command Palette and via
  `Alt+Shift+M`.
- Command Palette: *View MR / PR Detail* verb. Autocomplete fetches MRs
  across all states lazily and caches per tab, independent from the sidebar
  state filter.
- `Ctrl+Shift+Enter` in the commit message field commits and pushes.
- Commit split-button menu shows shortcut hints and tinted icons, matching
  the graph context menu.
- Keyboard navigation in the commit graph: Up/Down follow the current lane,
  Left/Right hop to sibling lanes, PageUp/PageDown jump a viewport,
  Home/End jump to newest/oldest loaded commit. `Alt+G` focuses the graph.
- Workspace Manager: Up/Down walks groups, workspaces and repo rows, Space
  expands/collapses, Enter on a repo opens it. Down from the search box
  drops into the list.
- File/folder picker: F6 (Shift+F6 reverse) cycles focus between the file
  list, sidebar locations and address bar; Up/Down walks the sidebar.
- F6 / Shift+F6 cycles focus across the main layout zones (titlebar, tabs,
  activity bars, sidebar, graph, bottom panel, status bar). Hidden zones
  are skipped.

### Changed

- Checkout (branch, detached commit, remote tracking branch) auto-stashes a
  dirty working directory, switches HEAD, then reapplies the stash — same
  flow as Pull. If the reapply conflicts, the resolution modal opens with
  the stash kept at index 0.
- Plugin enable / disable / uninstall cascades along required dependencies.
  Disabling asks for confirmation and turns off every transitively-required
  dependent (leaves first). Enabling with required deps off asks to turn
  them on first, and refuses if a required dep is missing. Uninstalling
  disables dependents so they don't keep running against a vanished
  service. Plugin Manager detail rows show "Depends on" and "Required by".
- Marketplace install resolves transitive required deps against the catalog.
  The confirm modal lists "Will also install: …" and downloads in dep-first
  order. Required deps not in the catalog block the install.
- Settings moved from localStorage to `~/.config/arbor/config.toml`: font
  scale, animations and speed, commit-template fallback, diff settings
  (algorithm, context, view mode, word-wrap, confirm-discard), graph page
  size, branch / tag visibility, ticket-link chips, "use theme fonts". They
  now survive WebView cache clears and can be edited from disk.
- Every remaining native `confirm()` is gone: Delete Branch, Delete Tag,
  Drop Stash, Reset Hard, Discard All, Undo Last Commit, Unlink Worktree,
  Delete Theme, Delete Worktree Link, Remove Alias Group, Clear Pipeline
  Logs, RON Studio Format and Convert-to-JSON now use the in-app confirm
  modal with Enter-to-confirm.
- Command Palette: *Delete Tag* split into *Delete Tag (local)* and *Delete
  Tag (local + origin)*, sharing the sidebar's scope-aware confirm modal.
- Conflict resolution modal: clicking *Apply resolution* / *Merge* /
  *Complete* with unresolved files jumps to the first unresolved file and
  shows a toast, instead of a hover-only tooltip on a disabled button.
- Conflict resolution toolbar: the action button is always labelled *Stage
  file*, including for modify/delete and add/modify conflicts (the choice
  is made in the two cards underneath).
- "Stash changes" prompt and full-screen diff viewer use the standard modal
  shell (backdrop, focus trap, ESC, animation).
- Welcome tour and plugin form wizard share the same step indicator widget.
- Docs pages use the shared Callout and Kbd widgets, so displayed
  keybindings reflect user remaps live.

### Removed

- Inline hover buttons (Apply / Pop / Drop) on stash markers in the graph.
  Use right-click, the sidebar Stash list, or the Command Palette.

### Fixed

- Enabling/disabling a plugin from the Marketplace detail pane refreshes
  the Plugin Manager if it's open in the background.
- Escape on a file/folder picker opened from inside another modal (Theme
  Editor, Add Worktree, Clone Repository, Studio export, …) closes only
  the picker.
- Settings, Docs, About and Studio modals show a backdrop with a spinner
  on cold start instead of feeling dropped while the module loads.
- Dropdowns are fully keyboard-driven: Tab in an open menu closes it and
  moves to the next field, Escape returns focus to the trigger,
  ArrowDown (or Alt+ArrowDown) on a focused trigger opens the menu on the
  first item, and the Create PR / MR branch selects show a focus ring.
- MR / PR sidebar no longer shows a raw 404 when the remote has pull/merge
  requests disabled — sidebar, palette entries and `arbor://mr/open/<n>`
  report the feature as unavailable.
- Plugin Logs panel surfaces failures that used to be terminal-only:
  runtime errors from hook handlers and service callbacks,
  `arbor.ui.tree.set` payload validation errors, malformed `plugin.toml`
  manifests (the broken folder also shows up in the Plugin Manager as
  "Failed to load").
- Restored Unicode glyphs (em-dashes, arrows, box-drawing, bullets) in the
  Docs pages and Markdown/HTML exports, corrupted by a previous round-trip
  through Windows-1252.
- Clone Repository: the folder-picker button in the Base folder field is
  reachable via Tab.
- Docs Markdown export preserves inline `<code>` inside table cells and
  headings (previously stripped by GitHub's renderer).
- Modals no longer pop a tooltip on the freshly-focused control when they
  open. Share-worktree button in the Workspace Info header now has proper
  icon-button styling.

## [0.1.0] — 2026-05-21

Initial public release.
