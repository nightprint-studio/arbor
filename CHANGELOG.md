# Changelog

All notable changes to Arbor are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Plugin form-DSL: six new display-only nodes — `breadcrumb`, `url_block`, `monogram`, `state_block`, `step_indicator`, `status_list`. They forward 1:1 to the corresponding shared widgets (`Breadcrumb`, `UrlBlock`, `Monogram`, `StateBlock`, `StepIndicator`, `StatusList`) so plugins can build IntelliJ-grade panels without hand-rolling chrome. Each has a `FormBuilder` chainable helper and full EmmyLua type defs in the SDK.
- Plugin form-DSL: three more display-only nodes — `copy_button` (standalone click-to-copy button with chrome, distinct from the inline `copy_link`), `experimental_badge` (amber→coral pill for in-flight features), and `section_header` (headline + secondary description, no body — for free-form layouts where the body is laid out by siblings).
- Plugin form-DSL: `filter_button` action-only chip-style button — same pill chrome as the host filter chips (rounded outline, accent when active, optional count badge), fires a plugin action on click; the active look is owned upstream and toggled via `arbor.ui.form.patch`, so filter state stays out of `values`.
- Plugin form-DSL: `panel_shell` chrome wrapper — same look as the host's `<PanelShell>` (icon + uppercase title + count badge + right-aligned actions + optional toolbar row + scrollable body + fixed footer). `variant = "plugin"` enables the floating-card chrome the Plugin Manager and `arbor.ui.add_view` bodies use; the standard `.ps-btn` header-button class is available to plugins.
- Plugin form-DSL: `bottom_panel_header` — header bar (no body) styled like the host's bottom-docked panels (build output / run console). Icon + uppercase title + count + inline children + right-aligned actions + optional mac-style close button driven by a plugin action.
- Plugin form-DSL: `tooltip` wrapper node — attaches the host's singleton hover/focus tooltip (smart placement, viewport-aware flipping, keyboard focus, optional shortcut hint, optional Markdown body) to one or more child nodes. `display = "inline"` (default) wraps inline-sized widgets like a `button` / `monogram` / `copy_button`; `display = "block"` is the opt-in for wrapping a block-level subtree (`section`, `panel_shell`, `info_card`). `FormBuilder:tooltip(cfg)` chainable helper plus full EmmyLua type defs.
- Plugin form-DSL: `alert` gains a `style` prop. `style = "banner"` (default) keeps the existing full-width tinted block; `style = "inline"` renders the in-document `Callout` (left-bar accent, bold title) designed to embed inside body copy / docs.
- Plugin form-DSL: `inline_edit` value-bearing field — click-to-edit single-line input. Renders the current value as a clickable label; activating it swaps in the host's `<InlineEdit>` widget (Enter commits, Esc reverts, ✓ / ✕ buttons mirror those keys). No blur-commit, so dismissing focus reverts the draft. Use for header titles, row names, or anywhere a full text input would be too noisy.
- Plugin SDK: EmmyLua type defs for `alert`, `info_card`, `chip_bar`, and `form_field` (previously rendered but unblessed in `sdk.d.lua`) — autocomplete now covers the full form-DSL surface. `FormBuilder` gains `:alert`, `:info_card`, `:chip_bar`, `:copy_button`, `:experimental_badge`, and `:section_header` chainable helpers.
- Plugin form-DSL: `checkbox`, `toggle`, and `radio` nodes accept the same `actions = { change = "…" }` slot that `select` already had, so plugins can react to a boolean flip or option pick without waiting for Submit. The slot also accepts a `DispatchTarget` for scoped per-node dispatch.
- Plugin form-DSL: `radio` gains an `appearance` prop — `"segment"` renders a pill-style toggle bar (IntelliJ studio-style View switcher), `"card"` renders title+description cards, `"radio"` (the default) keeps classic radio dots.
- "What's New" modal auto-opens once after every upgrade, sourced from the in-app `CHANGELOG.md` and grouped by category. Reachable any time from the Command Palette (*Show What's New*), the About panel, or the Getting Started doc. Fresh installs stay silent — the dialog only pops after a real version bump.
- Command Palette → *Show Active Schedules*: read-only modal listing every currently-registered timer (plugin actions, marketplace auto-refresh, …) grouped by namespace, with trigger cadence, enabled state, and focus-gating / fire-on-load flags.
- Commit graph splits into six fully reorderable columns — Graph, Branches / Tags, Subject, Author, Date, Hash. Any column (including Graph) can be dragged to any position; edge-dragging resizes; right-click offers hide / show / reset. The Graph column itself is adaptive (auto-sizes to the lanes, capped by its stored width which acts as a maximum). Drag uses a mouse-pointer pattern (no native HTML5 DnD) so the forbidden cursor never shows up. Branch and tag chips live in their own dedicated column. Layout is persisted host-wide in `~/.config/arbor/graph_columns.toml`, separate from the main settings file.

### Changed

- Opening a repo no longer freezes the commit graph and sidebar while git enumerates the working directory: the WIP status fetch (untracked recursion + rename detection — the slowest single call on big repos) now runs in the background, so the graph and sidebar render immediately. While the scan is in flight after a tab switch, the WIP row renders a spinner instead of the previous tab's modified/added/deleted counts. Same for safe-checkout flows, which no longer take a duplicate recovery snapshot.
- Title bar gains a Command Palette icon button (next to the Documentation entry) for click-discoverable access to `Ctrl+K`. The Theme switcher moves out of its own title-bar icon into the Settings menu as a hover entry — built-in and custom themes are still one mouse move away, plus "Edit themes…" at the bottom.
- Marketplace auto-refresh toggles (refresh interval, poll cadence) now
  take effect on the running schedule immediately instead of waiting for
  the next poll cycle.
- HTTP requests to Jira, Linear, and the GitHub releases API used by the
  PortableGit downloader now share a uniform `Arbor-Git-GUI/<version>`
  user-agent. Jira and Linear requests share a 30s timeout (was 30s and
  20s respectively); the PortableGit download stays untimed since the
  same connection streams a multi-MB archive.
- Markdown editor live preview: GFM tables render as a real HTML table
  with framed cells, header row and column alignment from the
  `|:--:|--:|` markers; cell content supports bold, italic,
  strikethrough, inline code, and links. An all-empty header row
  renders as a headerless grid. The caret entering the block flips it
  back to source mode for editing. Unordered list markers (`-` / `*`
  / `+`) show as a proper bullet glyph off the active line, and
  ordered list numbers align tabularly.
- Status bar slimmed down: repo path moved to the left segment; Fetch and
  "Open in browser" relocated to the graph toolbar; version pill removed
  (About still reachable from the Command Palette and the menu).
- Issue detail dialog is now self-contained: the tracker, API routing,
  and linked-commits source repo are pinned at open time, so a Linear
  ticket stays usable from a Jira-configured repo (and vice versa), and
  restoring from the parked-dialog dock no longer forces a checkout back
  to the source repo. The Linked Commits section explains itself when
  the original tab is no longer open and offers a one-click fallback to
  the current repo.
- Branches / Tags column rendering: local branches use a monitor icon, remote branches a globe icon; chips are now squared off (matching tag shape); branch names stretch with the column width instead of capping at 180px; multiple tags on a single commit collapse into a click-to-expand chip (also triggered when any tag shares a row with one or more branches). The standalone HEAD pill is gone — the HEAD branch is already styled green so the duplicate badge was noise. Default dark theme's lane-3 colour shifted from magenta-purple to indigo, and the four community themes that shipped `lane-3` literally identical to `color-tag` (Ayu Dark/Light, Solarized Dark/Light) were also retuned so branch chips on that lane don't read as tags.
- The WIP (working directory) row now lives inside the scrolling history pane, pinned just below the sticky column header. It uses the same grid layout as the commit rows so the dashed lane node lines up with the lane SVG behind it, and the "Working Directory" label / change pills sit in the Subject column — wherever the user has dragged it. It stays visible while exploring older history. Scroll-area lateral inset bumped to match the toolbar's so header, WIP and rows share the same 4 px left/right gap as the toolbar above.
- Branches sidebar groups local & remote branches and worktrees by their `/` path segments — GitKraken / Fork style folder tree with collapsible, lane-colour-matched folders. On by default; toggle from the folder-tree icon in the sidebar header (always visible, next to refresh), the **Alt+Shift+G** shortcut, or the Command Palette entry *Branches: Group by Path*. The on/off and collapsed-folder state is per-repo (saved in `.arbor/config.toml`); a host-wide `branches.grouping_recursive` knob flips between deep split and single-level.
- Plugins can mount a full main-area view via `arbor.ui.add_view` — a body surface (where the commit graph lives) that renders form-DSL content with the full node + dispatch/patch protocol. It surfaces as an activity-bar icon, a Command Palette entry, and the **Alt+Shift+V** toggle; `placement = "graph"` keeps the tab bar and bottom panel, `placement = "main"` takes over the whole body. New `on_view_open` / `on_view_close` hooks fire on the owning plugin.
- Plugins can embed an editable code/text editor in a form via the new `editor` node — a CodeMirror 6 field with syntax highlighting, line numbers and search. Its content is submitted like any field and can be pushed from the plugin with `set_value`; it can also emit scoped, debounced `on_edit` and `on_select` events for live, high-frequency UIs.
- Plugins can embed a read-only diff viewer in a form via the new `diff` node — unified and split layouts (local toggle), syntax highlighting and virtualization for large diffs. The plugin supplies the diffed hunks; updating them live is a `patch` away.
- The plugin form `tree` node gains a dynamic "data tree" mode — `lazy` children fetched on expand (the row fires a scoped `on_expand`, the plugin fills it with a `patch`), a scoped `on_select`, full keyboard navigation (arrows / Home / End / Enter), and row virtualization for large trees. Static trees are unchanged.
- Plugin form value slots can opt into *scoped dispatch*: a `select` `actions.change`, a leaf `field`, or a `vec_field` whose change targets a `dispatch` object (action or command) now ships a compact `{ node_id, slot, value, state? }` payload instead of the whole form, tracked per node so concurrent edits don't block each other. Declare `scope_state` to include a slice of the opaque form state. Bare-string actions are unchanged.
- Plugins can patch an open form's node tree granularly via `arbor.ui.form.patch(ops)` — merge props, append children, or remove nodes addressed by stable `id` without re-mounting — and mutate a single slice of the opaque form state via `arbor.ui.form.set_state_path(segments, value)` (passing `nil` deletes the key).
- Plugins can invoke each other's commands. A command registered as `invocable` can be fired by another plugin via `arbor.command.fire("<owner>::<id>")` or a form button's `dispatch = { kind = "command", … }`, gated by the new `command_invoke` permission plus the command's declared `required` tier.
- Plugins can invoke host built-in commands the same way — `arbor:git.commit|push|fetch|pull|branch_create|checkout|branch_delete|stage_all|unstage_all` (require `git = "write"`), plus `arbor:repo.refresh` and `arbor:app.open_settings` (no tier). Closed by default and gated by `command_invoke` + the declared tier; git commands target `ctx.tab_id` or the active tab.
- IntelliJ-style "compact middle packages" for file trees — *Settings → Interface → Compact file tree folders* collapses chains of single-child directories into a single row across the Files sidebar, the Stage area tree, and the commit detail file list. Also exposed as a Command Palette toggle. Conflict file lists always compact regardless of the setting.
- Markdown editor with Obsidian-style live preview — open any `.md` /
  `.markdown` file via the Files sidebar context menu and edit it
  in-place. Markdown markup is concealed per inline component: only
  the element under the cursor reveals its raw syntax (e.g. editing
  one `**bold**` word doesn't reveal the sibling `*italic*` on the
  same line). Fenced code blocks are syntax-highlighted through Prism
  so the rendering matches DiffViewer and blame. The eye button in the
  header toggles read-only; `Ctrl+S` saves.
- Shortcut **F11** toggles the full-screen diff overlay for the currently
  visible diff (stage panel, commit detail, MR detail). Press again from
  inside the overlay to dismiss.
- Command Palette verbs *Linear Issue* and *Jira Issue* — cross-tab
  ticket search that opens the detail modal pinned to the picked
  provider, visible only when signed in to that tracker. Same `#` / `~`
  query prefixes as the Issues sidebar; no per-project scoping applied.
- Minimize button on long-form dialogs (MR detail, Issue detail): parks
  a "reopen" shortcut in the status-bar dock so you can roam to other
  tabs / workspaces and pick the workflow back up later. Click a chip
  to switch to the original tab and re-open the dialog; ✕ to discard.
  Surviving workspace and tab switches comes from the action-based
  design — the chip outlives the modal component, so unsubmitted text
  and scroll position aren't preserved across the round trip. The cap
  (default 5, max 20) lives under *Settings → Appearance → Minimized
  dialogs cap*.
- Markdown editor inline media: `![alt](url)` references render as a
  real `<img>` (or `<video controls>` / `<audio controls>` based on
  the file extension — `.mp4` / `.webm` / `.mov`, `.mp3` / `.wav` /
  `.m4a`). URLs can be `http(s):`, `data:`, or filesystem paths
  relative to the markdown file (resolved through Tauri's asset
  protocol). Bare URLs on their own line are recognised too. GitHub
  user-attachments video URLs render as an "Open in browser" card
  (their signed-redirect chain can't be followed from an embedded
  WebView), so the system browser handles playback while the rest of
  the README stays in the editor. Caret on the source reveals the
  raw `![…](…)` for editing.

### Fixed

- Plugin form `arbor.ui.form.patch` no longer leaves stale children on screen when merging a fresh child subtree into a `section` / `tabs` / `tree_layout` / `wizard` / `form_field` / `tree` row — the renderer iterates children with a keyed `each`, and freshly-emitted children now go through the same auto-ID step the initial tree gets at mount, so the new markup actually replaces the old.
- F5 (refresh) now also reloads the currently visible diff panel, so externally-modified files appear updated without needing to click off and back on the file.
- Plugin forms no longer silently ignore a click on a menu item, list/card row action, or suggest-grid entry while another action is still running — they now show a brief "action already running" notice (buttons already disabled themselves with a spinner).
- Title bar app mark, hamburger menu, and workspace monogram are now the
  same height, so the left cluster reads as one aligned row.
- Plugins sometimes appeared disabled after launch until the Plugin
  Manager *Refresh* button was clicked. Boot now handshakes with the
  frontend before emitting plugin events, serialises IPC against the
  load thread, and fires the same reload signal the manual refresh
  uses — so sidebars / activity-bar items / command-palette verbs
  appear as soon as the splash dismisses.
- Idle CPU/IPC waste from plugins re-publishing identical state. The
  contribution registry now deduplicates writes by value, so polling
  views (running services, status indicators) no longer fan out
  frontend refetches when nothing actually changed.
- Global shortcuts (Ctrl+R, Ctrl+B, Alt+Shift+1…, etc.) no longer leak
  through the full-screen diff overlay or any other modal dialog —
  pressing a bound chord on top of a modal is now a no-op instead of
  firing the underlying action.
- Removing a repository from a workspace via the Workspace Manager now
  also closes its tab when that workspace is active, so reopening the
  workspace later doesn't resurrect the tab for a repo that's no longer
  a member.
- Diff viewer scrolling no longer stutters on multi-thousand-line files —
  off-screen hunks skip the per-scroll layout work, and chunk navigation
  (F3 / Shift+F3) jumps instantly instead of smooth-scrolling through
  every intermediate hunk.
- Windows taskbar icon goes blank after the system resumes from sleep —
  re-applied on every power-resume notification.
- Repository Browser: cloning from the in-app browser sometimes left the
  Clone button stuck in *Cloning…* even after the repo had been cloned;
  the modal now dismisses as soon as the clone itself succeeds and the
  workspace setup runs afterwards.

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
