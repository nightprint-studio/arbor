/**
 * Cross-plugin contribution model + tree-kind sidebar types.
 *
 * Plugins can extend each other's UI via the contribution registry: the host
 * plugin declares a *contribution point* (free-form name like
 * `"compile.toolbar"`) and consumers push *contributions* with a payload the
 * host knows how to interpret. The frontend component that renders the host's
 * UI (e.g. `PluginTreeSidebar`) reads the merged list from the store.
 *
 * Tree snapshots back the body of `kind="tree"` sidebars. The host plugin
 * pushes nodes via `arbor.ui.tree.set(sidebar_id, nodes)`; the snapshot is
 * dual-written into the unified contribution registry under the canonical
 * point `"arbor:tree-state"`, and the frontend reads it back through
 * `contributionStore.tree(plugin, sidebar)` on every coalesced
 * `arbor://contributions-changed` event.
 */

export interface PluginContribution {
  /** Plugin that contributed this item. */
  plugin_name: string;
  /** Contribution point (e.g. `"compile.toolbar"`). */
  point:       string;
  /** Unique id within (plugin_name, point). Re-contributing replaces. */
  item_id:     string;
  /** Free-form payload — shape is dictated by the consumer of `point`. */
  payload:     unknown;
  /** Render order hint. Lower = earlier. */
  priority:    number;
  /** Optional gate clause — when set, consumers that pass a `whenContext`
   *  filter this item out unless it matches. Promoted from a payload-level
   *  convention to a typed top-level field in Phase 5. */
  when?:       WhenClause;
  /** When `true`, consumers skip this item at render time but it remains in
   *  the registry — plugins use this to grey out an item without having to
   *  unregister/re-register. */
  disabled?:   boolean;
  /** Optional group label for consumers that bucket contributions
   *  (CommandPalette sections, KeybindingsSection groups, …). */
  group?:      string;
}

/** When-clause filter — mirrors the Rust `WhenClause` struct. The matcher
 *  lives in `src/lib/contributions/when.ts`. */
export interface WhenClause {
  /** Match if the context's `kind` equals this string, or is one of the
   *  values when an array is supplied. */
  kind?:       string | string[];
  /** Match if the context's `data[key]` deep-equals `value`. */
  data_field?: { key: string; value: unknown };
  /** Multi-selection scope for context-menu items:
   *  `true`  → only when |selectedIds| > 1 (multi-mode)
   *  `false` → only when single selection (legacy / default semantics)
   *  unset   → shown in both modes. */
  multi?:      boolean;
}

export interface ContributionPoint {
  plugin_name: string;
  name:        string;
  description?: string;
  /** Documentation only — payloads are NOT validated at runtime. */
  schema?: unknown;
}

/** A single node inside a tree-kind sidebar. Children may be empty. */
export interface TreeNode {
  id:         string;
  label:      string;
  /** Lucide name, emoji, or `"plugin:<plugin>:<icon_id>"` for custom SVG. */
  icon?:      string;
  /** Small text shown right of the label (counts, status). */
  badge?:     string;
  /** Color hint for the badge — picked by the renderer. */
  badge_kind?: 'info' | 'success' | 'warning' | 'error' | 'muted' | 'accent';
  /** Free-form classification used by the host. Common values:
   *    `"section"`         — gray header row, not selectable by default
   *    `"module"`          — Maven module / Cargo crate / npm workspace
   *    `"lifecycle_phase"` — Maven phase, Gradle task, npm script
   *    `"runnable"`        — single executable/binary/example
   *    `"static"`          — informational row
   */
  kind:       string;
  selectable: boolean;
  expanded:   boolean;
  /** Action fired on double-click / Enter. Payload: `{ node_id, data }`. */
  default_action?: string;
  /** Action fired on every single-click selection of the row, right after
   *  the local selection state updates. Payload `{ node_id, data }`. Use it
   *  for "show details on select" UX where waiting for a double-click would
   *  feel laggy. Independent from `default_action` — both can coexist. */
  selection_action?: string;
  /** Free-form data — passed back to the host with every action. */
  data:       unknown;
  /** Phase 6.2 — opt-in: this row becomes an HTML5 drag source. The drop
   *  semantics is owned by the host plugin via `drop_action` on the snapshot. */
  draggable?:  boolean;
  /** Phase 6.2 — opt-in: this row accepts drops. The host receives the
   *  `drop_action` event with `{source_id, source_data, target_id, target_data}`. */
  drop_target?: boolean;
  /** Optional hover tooltip. Useful when the label/badge are truncated. */
  tooltip?: string | null;
  children:   TreeNode[];
}

/** One segment of the breadcrumb band rendered above a tree sidebar.
 *  `action` fires `firePluginAction(<owner>, action, { value: data })` on click;
 *  when `action` is empty the segment is non-interactive (current location). */
export interface BreadcrumbSegment {
  label:    string;
  icon?:    string | null;
  /** Plugin action fired on click. */
  action?:  string | null;
  /** Free-form data forwarded as `ctx.data`. */
  data?:    unknown;
  /** Small uppercase label on the right of the chip. */
  badge?:   string | null;
  /** Hover tooltip. */
  tooltip?: string | null;
}

export interface TreeSnapshot {
  plugin_name: string;
  sidebar_id:  string;
  /** Optional title override; falls back to the section's `label` when empty. */
  title?:      string;
  /** Optional breadcrumb band; rendered between the section header and the tree. */
  breadcrumb?: BreadcrumbSegment[];
  /** When set, the breadcrumb shows a pencil affordance that flips it into a
   *  text input. On Enter the named action fires with `{ path }` in ctx. */
  breadcrumb_edit_action?:      string | null;
  /** Placeholder shown inside the edit input. */
  breadcrumb_edit_placeholder?: string | null;
  /** Phase 6.2 — when set, dropping a draggable row on a drop_target row
   *  fires this plugin action with `{source_id, source_data, target_id, target_data}`. */
  drop_action?: string | null;
  nodes:       TreeNode[];
  /** Monotonic; bumped on every `set`. */
  version:     number;
}

export type IconRegistrySnapshot = Record<string, string>; // "<plugin>:<id>" → raw SVG

// ── Containers (Phase 2 — ContributableModal) ─────────────────────────────

/** Container definition — registered via `arbor.ui.container.register` and
 *  read back by the frontend when an `arbor://container-open` event fires.
 *  `key` is the canonical id `"<plugin>::<id>"` built by the backend. */
export interface ContainerDef {
  key:           string;
  plugin_name:   string;
  id:            string;
  /** Mount type. Phase 2 only ships `"modal"`. */
  kind:          'modal' | 'sidebar';
  /** Layout used inside the modal. */
  layout:        'tree_nav' | 'flat' | 'tabbed';
  title:         string;
  width?:        string;
  /** CSS height. Set this when the modal would otherwise reflow each
   *  time the user switches sections — gives the modal a stable size. */
  height?:       string;
  submit_label?: string;
  cancel_label?: string;
  /** Action fired by the host AFTER per-section saves complete. */
  on_save?:      string;
  /** Host pre-open hook fired ONCE when the modal opens, before categories /
   *  sections are read. Use it to re-contribute fresh state. */
  on_load?:      string;
  /** Override for the category contribution point; defaults to `<key>:category`. */
  category_point?: string;
  /** Override for the section contribution point; defaults to `<key>:section`. */
  section_point?:  string;
}
