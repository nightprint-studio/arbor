# Changelog

All notable changes to Arbor are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- First-run welcome tour: a polished onboarding modal walks new users
  through Git identity, provider connection, opening the first repository,
  and a quick tour of the Command Palette, plugin marketplace, issue-tracker
  ticket chips, linked worktrees and workspaces. Re-openable any time from
  the Command Palette (*Welcome Tour*) or the Documentation panel.
- Marketplace registry now supports external plugin entries — third-party
  plugins maintained in their own GitHub repo can be listed from
  `arbor-extensions` with a one-line pointer, instead of hosting the code in
  the registry itself. Entries without a `pinned_sha` surface an "Unpinned"
  badge in the detail view.
- Plugin Marketplace is now reachable directly from the Command Palette and via
  the new Alt+Shift+M shortcut — no need to open Plugin Manager first.
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

- Documentation pages now use the shared Callout and Kbd widgets — keybindings displayed in Docs reflect user remaps live.
- The welcome tour and the plugin form wizard now share the same step
  indicator widget, so step badges, active highlight and done states stay
  visually identical across both surfaces.
- The "Stash changes" prompt and the full-screen diff viewer now use the
  standard modal shell — consistent backdrop, focus trap, ESC handling and
  animation across the app.
- Plugin enable / disable / uninstall now cascades along required dependencies.
  Disabling a plugin asks for confirmation and turns off every transitively-
  required dependent (leaves first); enabling a plugin with required deps off
  asks to turn them on first, and is refused when a required dep is missing or
  unloadable. Uninstalling a plugin disables its dependents so they don't keep
  running against a vanished service. The Plugin Manager's expanded detail row
  now shows "Depends on" and "Required by".
- Marketplace install now resolves transitive required dependencies against
  the catalog: the confirm modal lists "Will also install: …" and downloads
  the cascade in dep-first order. Required deps that aren't in the catalog
  block the install with an explicit error.
- Command Palette: *Delete Tag* is now two commands — *Delete Tag (local)* and
  *Delete Tag (local + origin)* — and both open the same scope-aware confirm
  modal used by the sidebar, replacing the native browser confirm.
- All remaining native `confirm()` prompts are gone: Delete Branch, Drop Stash,
  Reset Hard, Discard All, Undo Last Commit, Unlink Worktree, Delete Theme,
  Delete Worktree Link, Remove Alias Group, Clear Pipeline Logs and the RON
  Studio Format / Convert-to-JSON actions now use the themed in-app confirm
  modal with Enter-to-confirm.
- Settings — font scale, animations enable / speed, commit-template fallback,
  diff algorithm / context / view mode / word-wrap / confirm-discard, graph
  page size, branch / tag visibility and ticket-link chip toggle, plus the
  "use theme fonts" opt-in — now live in `~/.config/arbor/config.toml` instead
  of browser localStorage, so the choices survive WebView cache clears and can
  be edited from disk.

### Fixed

- Enabling or disabling a plugin from the Marketplace detail pane now refreshes
  the Plugin Manager if it's open in the background, instead of leaving its
  rows out of sync until reopened.
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
