# Changelog

All notable changes to Arbor are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- File Explorer: open the built-in explorer in its own dedicated window — a standalone, frameless explorer rather than the whole app — from the Command Palette ("Open File Explorer in New Window"), the system-tray menu ("Open File Explorer"), or via an opt-in system-wide **Ctrl+Shift+E** shortcut (enable it in Settings → File Explorer) that fires even when Arbor isn't focused. Re-summoning focuses the existing window instead of opening a second one.
- File Explorer: TortoiseGit-style git awareness — files and folders inside a repo now show a status overlay badge (modified, staged, untracked, deleted, renamed, conflicted; ignored items are dimmed), with folders rolling up to their strongest descendant state, and the footer shows the current branch with ahead/behind counts. Right-click adds inline **Stage**, **Unstage**, **Discard changes** (now behind a confirmation), and **Add to .gitignore**, plus **Open in Arbor** which brings the main window forward and opens the repo so the heavy operations (diff, log, blame, commit) happen in Arbor's full git UI.
- File Explorer: a folder that is itself a git repository is now flagged when browsing its parent — a branch chip in details view (corner badge in icon views) shows the repo and its current branch even when the folder you're in isn't a repo. Repos already registered in Arbor are highlighted, with coloured workspace dots; the Info panel adds a Repository section listing the branch, whether it's registered in Arbor, which workspaces hold it, and an Open in Arbor action.
- File Explorer: a **Changes** panel in the right rail lists the repo's staged and unstaged files at a glance, each with its status; clicking a file jumps to it in the list, and Open in Arbor hands off to the full git UI.
- File Explorer: **switch branch** without leaving the explorer — click the footer branch chip (or right-click a repo → Switch branch…) for a filterable, keyboard-first branch picker (type to filter, ↑/↓ to move, Enter to switch). Uses a safe checkout that refuses to overwrite uncommitted changes. Switch branch and **Open in Arbor** are now also on the context menu of a folder that is itself a repository, so you can act on a project straight from its parent folder.
- File Explorer: a built-in **Settings** page that swaps the explorer body in place (browser-style) — open it by typing `arbor://settings` in the address bar, the sidebar Settings item, or **Ctrl+,**. It tunes the git-awareness and global-shortcut switches, the default view / sort / startup folder, show-hidden and recursive-search, all persisted to your Arbor config; the two host-level switches also appear in Settings → File Explorer.
- File Explorer: a setting to **always open a new window** instead of focusing the existing one — the shortcut and the Command Palette action honour it (default: reuse / focus the single window).
- File Explorer: the sidebar sections (Library / Recents / Favourites / Devices / Projects) can be **reordered and hidden** — manage them in the explorer's settings page, or right-click a section header in the sidebar to hide it. Added a configurable **max recent folders** (1–50).
- File Explorer: the global shortcut is now **rebindable** — click the chord in either settings location to record a new combination (the change registers immediately; an invalid or already-claimed combo is rejected and the previous one kept). Added a **default sort + direction** and an **on-open** preference (Overview vs the last folder), plus **Reset** actions to clear the per-folder view memory, recent folders, or the sidebar/panel layout.
- Issue tracker & PR/MR: images embedded in a ticket description or comment now render inline (previously stripped). Sources are fetched through the provider's authenticated path so screenshots on private Linear / Jira issues and private GitHub / GitLab PRs/MRs resolve instead of breaking; the token is only ever sent to that provider's own host. Click an image (or focus + Enter) to open a full-size preview with zoom and ← / → paging; Esc closes.
- Worktrees: create a new worktree straight from a right-click — on any commit in the graph ("New worktree here…", starting a branch or detached HEAD from that commit) or on any branch in the Branches & Stashes sidebar ("Create worktree…", pre-selecting that branch).
- Plugin form-DSL: `card_grid` layout node — responsive auto-fit grid of cards that wraps to multiple rows when narrow. Pass `min_card` (default `"280px"`) to control the wrap threshold and `gap` for spacing. Use for dashboard layouts of `section variant="component"` or `info_card` children — unlike `card_row` (single flex row), `card_grid` wraps.
- Plugin form-DSL: `property_grid` display node — read-only-first reflection grid (dense `label → value` rows, right-aligned type pills, nested-struct indentation, lock glyphs for immutable fields, per-row click-to-edit). Rows support `value_tone` (code-editor value colouring), `copyable` (hover copy glyph) and collapsible group rows. Editing reuses the existing field editors via each row's `edit_node`. Generic — for any structured-data inspector (config dumps, JSON, ECS reflection, API responses).
- Plugin form-DSL: `chip_bar` gains `tint_inactive` — colours inactive chips by their `tone` so a filter bar reads like a legend before selection. `tabs` tab items now honour `badge` (count/warning), `disabled`, and `tooltip`.
- Plugin form-DSL: `tabs` gains `lazy` — mounts only the active panel (inactive panels render nothing until selected), so a modal with one heavy tab (a large syntax-highlighted dump, hundreds of cards) no longer pays for every panel up-front.
- Plugin form-DSL: Studio-shaped sidecars can anchor to the left — `sidecars[id].side = "left"` renders the pane before the body (bordered on its right), so it sits beside a left-side activity bar instead of always sliding in from the right.
- Plugin form-DSL: `tree` rows gain inline editing — a leaf with an `edit_node` swaps its value cell for that field editor when clicked (every existing editor works: text / number / select / vec / color), committing through the editor's own action. Leaf rows also render a right-aligned `value_display` + `value_tone` and a kind-coloured type `pill` (via the shared `TypePill`) for a dense, colourful "key: value" source-tree look. The `expanded = true` flag now actually expands the whole tree on mount (it was previously ignored by the dynamic tree).
- Plugin form-DSL: `tree` gains `fill` (grow to fill the parent flex column and own the only scroll region — for a full-height tree in a modal body without a double scrollbar) and tree rows gain `pill_tone` (explicit semantic tone for the type pill — `accent` / `info` / `success` / `warning` / `error` / `muted` — for provenance/state badges that aren't a value-kind) plus `icon_color` (tint a row's icon with any CSS colour, so deep trees can colour group headers per-category instead of reading monochrome).
- Plugin form-DSL: `tree` gains `path_query` — JSONPath-style navigation in the search box. A query starting with `$` (e.g. `$.category.crate.Component`) prefix-matches its segments as an ordered subsequence of each node's ancestor labels instead of substring-filtering; matches are navigable with F3 / Shift+F3 (and ↑/↓ / Enter from the input), a hit counter shows in the search row, and a results rail listing the hits opens beside the tree (click to jump). Plain (non-`$`) queries still substring-filter as before.

### Changed

- File Explorer: git awareness (status overlays, repo markers, the Changes panel, branch switching) is now **off by default** behind a master switch — when off the explorer issues no git checks, so plain browsing stays fast. Turn it on in Settings → File Explorer or the explorer's own settings page. The system-wide Ctrl+Shift+E shortcut is likewise off by default now and enabled in the same place.
- Git Blame: large files now show a real progress bar (lines attributed / total, plus the commit the walk is on) instead of an indeterminate spinner — blame streams incrementally as it walks history. Falls back to the previous one-shot load when no `git` binary is available.
- `bevy-brp` plugin: entity inspector rebuilt as a Studio-shaped modal — an editable component tree grouped three levels deep (**category → crate → component**), with per-category & per-component icons, count badges, kind-coloured type pills, and a **`bevy` badge** on first-party engine crates so you can tell engine components from your own. Single-field components (newtypes like `Health(f32)`) render their value inline on the component row; richer ones expand to nested `key: value` rows. Click a value to edit it inline (commits live via the Bevy Remote Protocol). Toolbar adds a **Show: All / Project (non-Bevy) / Bevy / Errors only** filter, **Collapse / Expand all**, **Refresh**, and **Copy entity as JSON**; right-click a component for **Copy type path / Open docs.rs / Remove component**. Category headers are colour-tinted per category. The search box also takes a JSONPath-style query — start it with `$` (e.g. `$.gameplay.rt_engine.ActionAbilities`) to jump to a component, with a hit counter, **F3 / Shift+F3** to cycle matches, and a results rail beside the tree. The tree is virtualized for entities with many components; plain queries match category, crate, component and field names. Computed components (GlobalTransform, …) are read-only.
- Type pills: `f32` / `f64` now read violet (continuous) while integers stay gold (discrete), so the two scalar families are tellable apart at a glance across every plugin that uses them.

### Fixed

- Graph: "Show Commits Touching File" no longer races with itself — starting a filter and clearing it (or starting a second one) before the first response arrives no longer lets the stale filtered result overwrite the live graph.
- Sidebar: the Pull / Merge Requests panel now refreshes automatically after creating, merging, or closing a PR/MR instead of holding on to the pre-mutation list until the next tab switch or manual refresh.
- CI panel: the runs list now refreshes after a push (or other ref change) on the active repo while the CI tab is open, so a freshly-triggered remote pipeline shows up without waiting for the poll timer.
- Setting up auto-merge on a PR/MR with conflicts now shows a clear "resolve conflicts first" message instead of a generic provider error string, on both GitHub and GitLab.
- Git Blame modal no longer gets stuck on the loading spinner — the load lifecycle now reliably clears the spinner when blame finishes, falls back to an actionable "timed out" message with Retry if a very deep history takes too long, and virtualises the row list so large files render instantly. Stale responses from a previous file are discarded when the modal is reopened on a different path mid-load.
- Command Palette: branch and tag pickers now colour the entry to match the graph (lane colour for branches, tag accent for tags) and use a monospace title, so refs read as identifiers at a glance.
- Graph: branches column drops the `origin/` (and other remote-name) prefix from remote refs — the Globe icon already says "remote", and the freed characters keep the meaningful suffix visible on a narrow column. Full ref name still shows in the tooltip and on copy.
- Create PR (GitHub) now refreshes an expired OAuth token and retries automatically, matching every other GitHub call — previously the first PR after expiry surfaced a raw 401.
- Plugin form-DSL: a full-height `tree` in a modal body no longer shows a double vertical scrollbar — with `fill`, the tree owns the only scroll region instead of nesting its own scroll inside the form body's. The `bevy-brp` inspector adopts it.
- Plugin form-DSL: the inline `tree` row editor now closes when you click or tab anywhere outside it — including non-focusable areas like the tree background, which previously left a native `<select>` / `<input>` focused so the editor stayed open. (Closes on `Esc` and the × button too.)
- Tooltips no longer crash the UI when their trigger is re-rendered or unmounted mid-update (e.g. a plugin sidebar repainting during a live reconnect) — the tooltip-store write is now deferred out of Svelte's reactive flush, so it can't trip a `state_unsafe_mutation` error that previously broke rendering and froze the window.
- Plugin form-DSL: a large `code` node no longer freezes the UI — above ~40k characters the block renders as plain monospace text instead of running Prism syntax-highlighting (which tokenises synchronously and bloats the DOM).
- `bevy-brp` plugin: opening an entity with many components is no longer sluggish, and the JSON tab + card expand/collapse no longer stutter — the inspector tabs are lazy (only the visible panel is in the DOM) and the JSON tab now uses the virtualised Studio CodeMirror editor (read-only) instead of a fully-rendered highlighted block, so even a huge entity dump stays instant.

- Plugin form-DSL: sidecars now stay mounted (and slide in on activity-bar selection) even when the body is replaced by a `state_block` (loading / error / empty). Previously sidecars only rendered in the populated state, so clicking an activity-bar item during the empty-state CTA flow highlighted the icon but showed nothing on the side. Plugin authors should author sidecar children that handle the no-document case gracefully (e.g. a `state_block` "Open a file to enable filtering" + a CTA `button`).
- Plugin form-DSL: `tabs` gains `strip_only` and `panels_only` props plus cross-renderer sync via `persist_key`. `strip_only = true` renders only the tab pill strip (no panel divs) — designed for the Studio-shaped "view switcher in `header.centre`" pattern; `panels_only = true` is its mirror for the body half. Two `tabs` widgets in the same modal sharing a `persist_key` now read and write the same in-memory `$state` slot, so clicking a tab on one updates the other in lock-step — without that, two widgets in different `FormNodeRenderer` regions (header.centre vs nodes) drifted because each renderer kept its own `activeTabMap`.
- Plugin form-DSL: Studio-shaped modal chrome — `arbor.ui.form{...}` now accepts five new top-level subkeys that mirror the host's Studio modals (RON / JSON / TOML / YAML / .properties). `header = { icon = { lucide|brand|image }, subtitle, dirty, tooltip, size_meta, experimental, left, centre, right }` swaps the plain title bar for a Studio-style strip with optional file-icon, view-mode tabs in the centre, and free-form left / right zones; `activity_bar = { side, items, default, storage_key, always_open }` adds a routing-only IDE-style rail of icon buttons, each pointing at a named pane; `sidecars = { <id> = { width, title, children } }` provides those panes as full-bleed FormNode subtrees that slide in / out with width animation (always mounted, so field values survive close + reopen); `footer = { status, center, right }` replaces the default Submit / Cancel with three composable zones (status pills on the left, tool buttons in the middle, custom CTA cluster on the right); `state_block = { loading?, error?, empty? }` substitutes the body with a loading spinner / error block / empty-state card without re-rendering the form tree. `arbor.ui.form.set_sidecar(id|nil)` and `arbor.ui.form.set_state_block(name, cfg?)` toggle these live. Pairs with `tabs.persist_key` (new): when set, the active tab id is mirrored to `localStorage[persist_key]` so the user's view-mode pick survives reopening. Cross-region `show_if` is not supported in v1 — each region's renderer keeps its own field values; `name` collisions across regions are a plugin error (last-write-wins + console warning).
- Plugin form-DSL: `editor` field gains plugin-driven `diagnostics`, `completions`, and `snippets`. `diagnostics = { { from, to, severity, message, source? }, … }` (or `{ line = N, severity, message }` for whole-line markers) drives squiggles, gutter chips and hover tooltips — patchable live via `arbor.ui.form.patch{…}` so a parse / lint loop can stream results in. `completions` merges static items into the autocomplete popup (label + detail + info + type icon + optional `apply` text). `snippets` uses CodeMirror's `${1:name}` tab-stop syntax — picking the snippet expands into the editor with the cursor at the first stop. Triggered with Ctrl-Space, and fires automatically while typing identifier characters.
- Plugin form-DSL: six new display-only nodes — `breadcrumb`, `url_block`, `monogram`, `state_block`, `step_indicator`, `status_list`. They forward 1:1 to the corresponding shared widgets (`Breadcrumb`, `UrlBlock`, `Monogram`, `StateBlock`, `StepIndicator`, `StatusList`) so plugins can build IntelliJ-grade panels without hand-rolling chrome. Each has a `FormBuilder` chainable helper and full EmmyLua type defs in the SDK.
- Plugin form-DSL: three more display-only nodes — `copy_button` (standalone click-to-copy button with chrome, distinct from the inline `copy_link`), `experimental_badge` (amber→coral pill for in-flight features), and `section_header` (headline + secondary description, no body — for free-form layouts where the body is laid out by siblings).
- Plugin form-DSL: `filter_button` action-only chip-style button — same pill chrome as the host filter chips (rounded outline, accent when active, optional count badge), fires a plugin action on click; the active look is owned upstream and toggled via `arbor.ui.form.patch`, so filter state stays out of `values`.
- Plugin form-DSL: `panel_shell` chrome wrapper — same look as the host's `<PanelShell>` (icon + uppercase title + count badge + right-aligned actions + optional toolbar row + scrollable body + fixed footer). `variant = "plugin"` enables the floating-card chrome the Plugin Manager and `arbor.ui.add_view` bodies use; the standard `.ps-btn` header-button class is available to plugins.
- Plugin form-DSL: `bottom_panel_header` — header bar (no body) styled like the host's bottom-docked panels (build output / run console). Icon + uppercase title + count + inline children + right-aligned actions + optional mac-style close button driven by a plugin action.
- Plugin form-DSL: `tooltip` wrapper node — attaches the host's singleton hover/focus tooltip (smart placement, viewport-aware flipping, keyboard focus, optional shortcut hint, optional Markdown body) to one or more child nodes. `display = "inline"` (default) wraps inline-sized widgets like a `button` / `monogram` / `copy_button`; `display = "block"` is the opt-in for wrapping a block-level subtree (`section`, `panel_shell`, `info_card`). `FormBuilder:tooltip(cfg)` chainable helper plus full EmmyLua type defs.
- Plugin form-DSL: `color_swatch` display-only node — chip-only or labelled card row (`[chip] Label   #caption`). Accepts any CSS colour expression (hex, `rgb()`, `var(--…)`, `color-mix(...)`), an optional single-character `glyph` for typed-token indicators, and a `chip_size` override. Distinct from the value-bearing `color` field (HTML5 picker) — `color_swatch` is presentational; pair it with a sibling `color` field + a `patch` to make it editable. `FormBuilder:color_swatch(cfg)` chainable helper plus EmmyLua type defs.
- Plugin form-DSL: four more display-only nodes — `kbd` (keybinding badge with live lookup against the user's keybindings, same chrome as Shortcuts and Command Palette hints), `type_pill` (uppercase one-word type hint with curated palette by `kind` or explicit `tone`), `encoding_pill` (charset indicator with override tint, mirrors the diff toolbar), and `avatar` (round, initials + stable hue-from-email — distinct from `monogram` which is for entities, `avatar` is for people). `FormBuilder:kbd`, `:type_pill`, `:encoding_pill`, `:avatar` chainable helpers plus full EmmyLua type defs.
- Plugin form-DSL: three provider-flavoured display-only nodes — `brand_icon` (monochrome `simple-icons` glyph in `currentColor`, for activity bar / sidebar / inline use), `brand_tile` (branded square tile with the brand's hard-coded background colour and a fixed bright foreground, for auth tiles / settings cards / welcome screens), and `provider_user_badge` (two-line user identity row with avatar + name + secondary line, click-to-copy on both lines). All three accept the `github` / `gitlab` / `bitbucket` / `linear` / `jira` brand set used by the host. `FormBuilder:brand_icon`, `:brand_tile`, `:provider_user_badge` chainable helpers plus EmmyLua type defs.
- Plugin form-DSL: `alert` gains a `style` prop. `style = "banner"` (default) keeps the existing full-width tinted block; `style = "inline"` renders the in-document `Callout` (left-bar accent, bold title) designed to embed inside body copy / docs.
- Plugin form-DSL: `alert` gains `title` (bold heading rendered above the body, survives collapse), `dismissable` (× button that hides the alert locally — no plugin round-trip), and `collapsible` + `collapsed` (chevron toggle that hides the body text while keeping the title clickable).
- Plugin form-DSL: `branch_select` value-bearing field — git branch picker with the same chrome as the host's `<BranchSelect>` widget (monospace trigger, search input above the menu past `search_threshold`, sticky entry for a value not in the list). Plugin owns the list (`branches = …` — typically `arbor.repo.branches()` mapped to `.name`); submits the picked branch as a string. `FormBuilder:branch_select(name|cfg, opts?)` chainable helper plus full EmmyLua type defs.
- Plugin form-DSL: `inline_edit` value-bearing field — click-to-edit single-line input. Renders the current value as a clickable label; activating it swaps in the host's `<InlineEdit>` widget (Enter commits, Esc reverts, ✓ / ✕ buttons mirror those keys). No blur-commit, so dismissing focus reverts the draft. Use for header titles, row names, or anywhere a full text input would be too noisy.
- Plugin form-DSL: `button` gains `size` (`xs` / `sm` (default) / `md` / `lg`), `icon_end` (trailing Lucide glyph — chevron, external-link), `block` (full-width, centred label), and `color` (CSS override applied to the fill for `variant = "primary"` and to the text for `ghost` / `danger`; brand fills auto-pick black/white via oklch so themes with light brand tokens stay readable). Existing buttons keep their previous look — the new size defaults to `sm` which matches the legacy baseline.
- Plugin form-DSL: `text` / `password` / `email` / `url` and `number` gain `icon` (leading Lucide glyph), `icon_end` (trailing glyph), `prefix` / `suffix` (muted text affixes for units / sigils — `"$"`, `"kg"`, `"ms"`, `"https://"`, …), and `size` (`sm` / `md` / `lg`); text fields also accept `clearable` to show a × while the field has a value. The text branch now renders through the shared `<Input>` widget so plugin forms pick up the same chrome and focus styling as the rest of the app, and the number branch wires the same affordances onto `<NumberStepper>`.
- Plugin form-DSL: `select` and `multiselect` gain a `clearable` prop. When the field has a value, the trigger shows an × the user can click to reset it (single → `""`, multi → `[]`). On `select`, clearing also fires `actions.change` so live consumers see the cleared state. Plugin SDK now also ships EmmyLua type defs for `arbor.FormFieldSelect` / `arbor.FormFieldMultiselect` (previously unblessed in `sdk.d.lua`) and a `FormBuilder:multiselect(name|cfg, opts?)` chainable helper to match.
- Plugin form-DSL: `arbor.ui.form.set_value` / `set_options` / `set_disabled` now accept a cfg-table call shape `{ id | name, <payload_key> }` alongside the legacy positional `(name, payload)` form. Targeting by `id` matches what `patch` already uses, so plugins that track node ids don't need a parallel "field names" table — the host resolves id → field name by walking the node tree. Passing a name / id that doesn't match any current field logs a warning in the host devtools console (the write still goes through) so typos surface immediately. The positional form is preserved unchanged for the common shortcut case.
- Plugin form-DSL: `table` gains per-column `readonly` (display-only cells — plain text / checked glyph / select label, sat next to editable columns), `row_actions` (per-row icon buttons in the trailing column with their own `action` / `dispatch`, payload `{ row_index, row, action_id }`), `hide_delete` / `hide_add` (drop the built-in trash and / or "+ Add row" buttons when row creation or removal goes through a separate plugin flow), and `sticky_header` + `max_height` (column labels stay pinned while a bounded rows region scrolls; the Add button stays anchored below the scroll viewport).
- Plugin form-DSL: `tree_layout` gains `nav_resizable` — when on, a drag handle appears on the right edge of the rail; width is clamped between `nav_min_width` / `nav_max_width` (default 160 / 480 px), persists per `id` in localStorage, and arrow keys nudge by 8 px (Shift = 32 px). Stacks with `nav_collapsible` so the user can both hide the rail and tune its width.
- Plugin form-DSL: `tree` gains three opt-in affordances. `searchable = true` renders an inline filter input above the tree — matches `label` + `description` case-insensitively, auto-expands ancestors of matches, highlights the matched substring, dims ancestor-only rows; local state, no plugin round-trip. `reorderable = true` enables HTML5 drag-drop reorder among rows; the cursor's vertical zone over the target row picks `before` / `inside` / `after`, and the scoped `on_reorder` slot ships `{ source, target, position }` so the plugin can mutate its model. `menu_items` (tree-wide) and per-row `tnode.menu_items` add a right-click context menu — each item carries its own `action` / `dispatch`, falling back to the tree-level `on_context_menu` slot (`{ item_id, value, path }`) for items without one. Per-row `tnode.draggable` / `tnode.drop_target` opt individual rows out of reorder.
- Plugin SDK: EmmyLua type defs for `alert`, `info_card`, `chip_bar`, and `form_field` (previously rendered but unblessed in `sdk.d.lua`) — autocomplete now covers the full form-DSL surface. `FormBuilder` gains `:alert`, `:info_card`, `:chip_bar`, `:copy_button`, `:experimental_badge`, and `:section_header` chainable helpers.
- Plugin form-DSL: `info_card` gains `variant` (`elevated` / `flat` / `subtle`) and `bordered` props — the card now shares the host `Card` chrome, so plugins can nest a hero header inside another elevated surface (set `variant = "flat"`) without doubling-up borders or backgrounds.
- Plugin form-DSL: `checkbox`, `toggle`, and `radio` nodes accept the same `actions = { change = "…" }` slot that `select` already had, so plugins can react to a boolean flip or option pick without waiting for Submit. The slot also accepts a `DispatchTarget` for scoped per-node dispatch.
- Plugin form-DSL: `text` / `password` / `email` / `url` / `textarea` / `number` / `range` fields now accept the same `actions = { change = "…" }` slot, trailing-edge debounced by `debounce_ms` (default 250ms). Filters / live-search / scrub-driven previews no longer need a separate Apply button — the value dispatches as the user types or drags, with the latest value winning during a debounce window. Accepts either a legacy action string or a `DispatchTarget` (scoped per-node, can target a command).
- Plugin form-DSL: `radio` gains an `appearance` prop — `"segment"` renders a pill-style toggle bar (IntelliJ studio-style View switcher), `"card"` renders title+description cards, `"radio"` (the default) keeps classic radio dots.
- "What's New" modal auto-opens once after every upgrade, sourced from the in-app `CHANGELOG.md` and grouped by category. Reachable any time from the Command Palette (*Show What's New*), the About panel, or the Getting Started doc. Fresh installs stay silent — the dialog only pops after a real version bump.
- Command Palette → *Show Active Schedules*: read-only modal listing every currently-registered timer (plugin actions, marketplace auto-refresh, …) grouped by namespace, with trigger cadence, enabled state, and focus-gating / fire-on-load flags.
- Commit graph splits into six fully reorderable columns — Graph, Branches / Tags, Subject, Author, Date, Hash. Any column (including Graph) can be dragged to any position; edge-dragging resizes; right-click offers hide / show / reset. The Graph column itself is adaptive (auto-sizes to the lanes, capped by its stored width which acts as a maximum). Drag uses a mouse-pointer pattern (no native HTML5 DnD) so the forbidden cursor never shows up. Branch and tag chips live in their own dedicated column. Layout is persisted host-wide in `~/.config/arbor/graph_columns.toml`, separate from the main settings file.

### Changed

- `properties-studio-lite` plugin restructured around the Studio-shaped form chrome (header / activity_bar / sidecars / footer / state_block) as an end-to-end smoke-test of the new surface. Three commands: *Open Properties Studio (Lite)…* opens the modal in its empty state with a CTA, *Open .properties (Lite Studio)…* still drives the picker-first flow, *Paste .properties (Lite Studio)…* unchanged. The body switches between Tree / Text / Diff / Errors via persisted view-mode tabs; the right activity bar routes between Inspector / Query / Bindings / Schema / Tools sidecars; the footer carries a SAVED / MODIFIED pill, undo / redo / Format stubs, and a primary Save button.
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
