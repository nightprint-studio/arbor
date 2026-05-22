# Changelog

All notable changes to Arbor are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Workspace Manager: Arrow Up / Down navigates groups, workspaces and repo
  rows; Space expands or collapses the focused row; Enter on a repo row opens
  it. Arrow Down from the search box jumps into the list.
- F6 / Shift+F6 cycles focus across the main layout zones — titlebar, tabs,
  activity bars, sidebar, graph, bottom panel, status bar — so the whole UI
  is reachable from the keyboard. Zones that aren't currently visible are
  skipped automatically.
- File / folder picker: F6 (Shift+F6 reverse) cycles focus between the file
  list, the sidebar locations and the address bar, and Arrow Up / Down walks
  the sidebar once focused there — keyboard-only navigation of the picker is
  now complete.
- Keyboard-only navigation in the commit graph: Arrow Up / Down follow the
  current lane (same branch column), Arrow Left / Right hop to the nearest
  sibling lane, PageUp / PageDown jump a viewport, Home / End jump to the
  newest / oldest loaded commit. Alt+G pulls focus into the graph viewport.
- Command Palette: new *View MR / PR Detail* verb opens the pull / merge
  request detail modal. The autocomplete fetches MRs across all states (open,
  merged, closed) lazily on first use and caches the result per tab —
  independent from the sidebar's state filter — with a spinner shown while it
  loads.
- `Ctrl+Shift+Enter` in the commit message field commits and pushes in one step.
- The Commit split-button menu now shows shortcut hints and tinted icons next
  to each option, matching the graph context menu.
- Appearance setting to switch the title-bar window controls between the
  Mac-inspired coloured trio (default) and a flat Windows-/IntelliJ-style set;
  the same switch also restyles the close button in modal and panel headers.

### Changed

- Command Palette: *Delete Tag* is now two commands — *Delete Tag (local)* and
  *Delete Tag (local + origin)* — and both open the same scope-aware confirm
  modal used by the sidebar, replacing the native browser confirm.
- All remaining native `confirm()` prompts are gone: Delete Branch, Drop Stash,
  Reset Hard, Discard All, Undo Last Commit, Unlink Worktree, Delete Theme,
  Delete Worktree Link, Remove Alias Group, Clear Pipeline Logs and the RON
  Studio Format / Convert-to-JSON actions now use the themed in-app confirm
  modal with Enter-to-confirm.

### Fixed

- Pressing Escape on a file/folder picker opened from inside another modal
  (Theme Editor, Add Worktree, Clone Repository, Studio export, …) now closes
  only the picker, leaving the underlying modal open.
- Opening Settings, Docs, About or any of the Studio modals from a cold start
  now shows a backdrop with a spinner while the underlying module loads,
  instead of leaving the click feeling dropped.
- Dropdowns now play nicely with keyboard-only use: Tab while a menu is open
  closes it and moves focus to the next field, Escape restores focus to the
  trigger, ArrowDown (or Alt+ArrowDown) on a focused trigger opens the menu
  and lands on the first item, and the Create PR / MR branch selects show
  a visible focus ring when reached with Tab.
- MR / PR sidebar no longer shows a raw 404 when the remote has pull/merge requests
  disabled — the sidebar, Command Palette entries, and `arbor://mr/open/<n>` deep link
  all gracefully report the feature as unavailable.
- Plugin Logs panel now shows plugin failures that used to be terminal-only: runtime
  errors from hook handlers and service callbacks, `arbor.ui.tree.set` payload
  validation errors, and malformed `plugin.toml` manifests (the broken folder also
  shows up as a "Failed to load" entry in the Plugin Manager).
- Restored Unicode glyphs (em-dashes, arrows, box-drawing characters, bullets) in
  the Docs panel pages and exported Markdown / HTML docs, which had been corrupted
  by a previous round-trip through Windows-1252.
- Clone Repository: the folder-picker button in the Base folder field is now
  reachable via Tab, so the dialog can be filled in entirely from the keyboard.
- Docs Markdown export now preserves inline `<code>` and other formatting inside
  table cells and headings; previously e.g. a table row referencing `<h1>` would
  render as a bare HTML tag (and be stripped by GitHub's renderer) instead of as
  inline code.
- Modals no longer pop a tooltip on the freshly-focused control when they open,
  and the share-worktree button in the Workspace Info header now has proper
  icon-button styling.

## [0.1.0] — 2026-05-21

Initial public release.
