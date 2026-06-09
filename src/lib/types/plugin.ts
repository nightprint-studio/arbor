// ── Manifest types ─────────────────────────────────────────────────────────────

/** Generic 3-tier read/write capability. Higher implies lower. */
export type AccessLevel = 'none' | 'read' | 'write';
/** Git capability with an extra `history_rewrite` tier above `write`. */
export type GitLevel    = 'none' | 'read' | 'write' | 'history_rewrite';
/** Terminal capability. `commands` requires `terminal_scope` allowlist. */
export type TerminalLevel = 'none' | 'commands' | 'any';

export interface PluginPermissions {
  network:               string[];
  /** Filesystem level. `read` enables arbor.fs read ops; `write` enables both. */
  fs:                    AccessLevel;
  /**
   * Optional path scope for arbor.fs.*.
   *   - `[]`   (default) → sandboxed to the active repo's directory
   *   - `["*"]`           → unrestricted (any path)
   *   - other absolute paths → allowed in addition to the active repo
   */
  fs_scope:              string[];
  /**
   * Git level. `read` enables arbor.repo.* + arbor.notes.* read ops; `write`
   * enables non-destructive mutations (commit, branch, fetch, push, notes
   * write, clone, stash); `history_rewrite` enables rebase, reset --hard,
   * force-push, amend, filter-branch.
   */
  git:                   GitLevel;
  terminal:              TerminalLevel;
  /** Allowed command basenames when `terminal = "commands"`. */
  terminal_scope:        string[];
  /**
   * env_read accepts:
   *   - `true`  → all environment variables readable
   *   - `false` → os.getenv removed entirely
   *   - `string[]` → allowlist; only listed names return a value
   */
  env_read:              boolean | string[];
  /** Issues (Linear / Jira). `read` → search/get; `write` → transition/comment. */
  issues:                AccessLevel;
  /**
   * Git provider host APIs (GitHub PRs / GitLab MRs / CI runs).
   *   - `read`  → arbor.mr.list, arbor.ci.runs_for_branch and friends.
   *   - `write` → reserved for future mutations (comments, retrigger, …).
   * Tokens stay in the OS keyring; plugins only see resolved data.
   */
  provider:              AccessLevel;
  /** Toolchain manager. `read` → list/active/detect/env; `write` → add/remove/set_active. */
  toolchain:             AccessLevel;
  /** Allow arbor.service.export — register callable services for other plugins. */
  service_export?:       boolean;
  /** Allow arbor.service.call — invoke services exported by other plugins. */
  service_call?:         boolean;
  /** Read other plugins' settings via `arbor.settings.read(plugin, key)`. */
  settings_read_others?: boolean;
  /** Allow `arbor.command.fire` / declarative `kind = "command"` dispatch.
   *  The caller must also hold whatever tier the target command declares as
   *  `required`. Default false. */
  command_invoke?: boolean;
}

/**
 * Spring-style trigger for a plugin schedule. Exactly one of the three
 * variants is produced by `arbor.scheduler.register` in Lua.
 */
export type ScheduleTrigger =
  | { kind: 'fixed_rate';  interval_sec: number }
  | { kind: 'fixed_delay'; delay_sec:    number }
  | { kind: 'cron';        expr:         string };

/**
 * One concrete background schedule registered by a plugin from main.lua.
 * The plugin manifest only opts the feature on/off via `[scheduler] enabled`;
 * the data below comes from `arbor.scheduler.register({ … })`.
 */
export interface PluginSchedule {
  action:              string;
  trigger:             ScheduleTrigger;
  /** Wait this many seconds before the first fire (fixed_rate / fixed_delay only). */
  initial_delay_sec:   number;
  on_load:             boolean;
  /** If true, the scheduler skips firing when the app window is not focused. */
  only_when_focused:   boolean;
}

/**
 * Live status of one schedule registered by a plugin. Combines the static
 * declaration (action / trigger / focus gate / …) with whether the scheduler
 * thread is currently running, so the Plugin Info modal can show a per-action
 * toggle.  Backed by Rust `PluginScheduleStatus` (flattened serde).
 */
export interface PluginScheduleStatus {
  action:              string;
  trigger:             ScheduleTrigger;
  initial_delay_sec:   number;
  on_load:             boolean;
  only_when_focused:   boolean;
  running:             boolean;
}

/** Manifest opt-in for the background scheduler subsystem. */
export interface PluginSchedulerSection {
  enabled: boolean;
}

export interface PluginHooks {
  on_repo_open?:   boolean;
  on_repo_close?:  boolean;
  on_plugin_load?: boolean;
  on_tab_switch?:  boolean;
  on_commit?:      boolean;
  on_push?:        boolean;
  on_checkout?:    boolean;
  on_fetch?:       boolean;
}

/** A single dependency declaration from plugin.toml `[[dependencies]]`. */
export interface PluginDependency {
  name:      string;
  /** Semver requirement, e.g. ">=1.0.0". Empty = any version. */
  version:   string;
  /** If true, a missing or incompatible match is a warning, not an error. */
  optional?: boolean;
}

export interface PluginManifest {
  name:        string;
  version:     string;
  description: string;
  author:      string;
  license?:    string;
  repository?: string;
  keywords?:   string[];
  /** Minimum Arbor app version (semver). Plugins on older builds are rejected. */
  min_arbor_version?: string;
  arbor_api:   number;
  /** Supported operating systems. Empty/missing = cross-platform. */
  os?:         string[];
  entry?:      string;
  /** When true, the Plugin Manager renders an orange EXPERIMENTAL pill on the
   *  row. Use for plugins still iterating on settings / hooks / storage. */
  experimental?: boolean;
  permissions: PluginPermissions;
  hooks:       PluginHooks;
  scheduler?:  PluginSchedulerSection;
  dependencies?: PluginDependency[];
}

// ── PluginInfo — returned by list_plugin_info ─────────────────────────────────

export interface PluginInfo {
  name:        string;
  version:     string;
  description: string;
  author:      string;
  license?:    string;
  repository?: string;
  keywords?:   string[];
  arbor_api:   number;
  enabled:     boolean;
  /** Mirrors `experimental` in plugin.toml — used to render the EXPERIMENTAL pill. */
  experimental?: boolean;
  permissions: PluginPermissions;
  hooks:       PluginHooks;
  scheduler_count:    number;
  schedulers_running: number;
  /** Per-action scheduler status — used by the Plugin Info modal to render a
   *  toggle for each registered schedule. */
  schedules:          PluginScheduleStatus[];
  /** HTML documentation string read from doc_file in plugin.toml, if declared. */
  doc?: string;
  /**
   * Populated when the plugin was skipped because one of its dependencies
   * failed to resolve (missing or incompatible version). When present the
   * plugin is not actually loaded — only shown in the Plugin Manager so the
   * user can diagnose the issue.
   */
  dep_error?: string;
  /** Direct declared dependencies from the manifest. */
  dependencies?: PluginDependency[];
  /** Names of installed plugins (loaded or dormant) that require this one. */
  required_by?: string[];
}

// ── UI Registrations — typed shapes produced by parsers in src/lib/contributions/ ───

export interface PluginContextMenuItem {
  plugin_name: string;
  target:  string;  // "commit" | "branch" | "file"
  label:   string;
  action:  string;
  icon?:   string;
}

export interface PluginMenuItem {
  plugin_name: string;
  label:  string;
  action: string;
  icon?:  string;
}

export interface PluginSidebarSection {
  plugin_name: string;
  /** Unique id within the plugin — key for set_panel_content / panel:open hook. */
  id:          string;
  action:      string;
  label:       string;
  icon?:       string;
  collapsable: boolean;
  /** Which ActivityBar the icon lives in. `"left"` = classic built-in side,
   *  `"right"` = plugin-expansion side. Defaults to `"right"`. */
  side:        'left' | 'right';
  /** `"top"` = opens a side panel next to the ActivityBar.
   *  `"bottom"` = opens the unique bottom panel (shared across both sides). */
  position:    'top' | 'bottom';
  /** Optional hover tooltip override. Falls back to `label` when empty. */
  tooltip?:    string;
  /** How the panel body is rendered.
   *    `"form"` — pushed via set_panel_content (form DSL).
   *    `"tree"` — pushed via `arbor.ui.tree.set` and rendered with
   *               `PluginTreeSidebar.svelte`. The host plugin can also expose
   *               contribution points (toolbar / node_action / decorator /
   *               context_menu / dependency_provider) consumed by the same
   *               component. */
  kind:        'form' | 'tree';
  /** Optional search-row config. When set on a `kind = "tree"` sidebar,
   *  `PluginTreeSidebar` renders a mode toggle (local-filter / remote-search)
   *  in the built-in search input. Plugins use this to let users opt into a
   *  backend wildcard search instead of just filtering already-loaded rows.
   *  Omit to keep the legacy behaviour (local filter only). */
  search?:     PluginSidebarSearch;
}

export interface PluginSidebarSearch {
  /** Which modes are selectable. Order is preserved for the toggle cycle.
   *  Default: `["local"]` (legacy — local filter only). */
  modes:               ('local' | 'remote')[];
  /** Initial mode. Must be one of `modes`. Defaults to `modes[0]`. */
  default?:            'local' | 'remote';
  /** Plugin action fired on Enter while in `"remote"` mode. Receives
   *  `{ pattern: <input text> }` as ctx. Required for remote mode to work. */
  remote_action?:      string;
  /** Placeholder text per mode (falls back to a sensible default). */
  placeholder_local?:  string;
  placeholder_remote?: string;
  /** When true, typing a glob char (`*` or `?`) in local mode surfaces a
   *  one-shot tip suggesting the remote mode. Default: true when `remote`
   *  is one of `modes`, false otherwise. */
  wildcard_hint?:      boolean;
}

/** How much of the body a main-area view occupies.
 *    `"graph"` — replaces the commit-graph area, keeps the tab bar + bottom panel.
 *    `"main"`  — takes over the whole body column (tab bar + bottom hidden). */
export type ViewPlacement = 'graph' | 'main';

/** A plugin-registered main-area view (`arbor.ui.add_view`). Surfaces in the
 *  activity bar; clicking it mounts the view in the body where the commit graph
 *  lives. Body content is pushed via `set_panel_content(<id>, …)` (shares
 *  `PluginPanelContent`) and rendered by the full `FormNodeRenderer`. */
export interface PluginViewSection {
  plugin_name: string;
  /** Unique id within the plugin — key for set_panel_content + on_view_open. */
  id:          string;
  label:       string;
  icon?:       string;
  /** Body footprint. Defaults to `"graph"`. */
  placement:   ViewPlacement;
  /** Optional hover tooltip override. Falls back to `label` when empty. */
  tooltip?:    string;
}

/** Content pushed by a plugin into one of its registered panels, rendered via
 *  the form-DSL renderer. Shape mirrors Rust `PanelContent`. */
export interface PluginPanelContent {
  plugin_name: string;
  panel_id:    string;
  title?:      string;
  /** Form-DSL node tree — same shape consumed by PluginFormModal. */
  nodes:       unknown;
  /** Optional footer action buttons. */
  actions?:    unknown;
}

export interface ComboOption {
  value:  string;
  label:  string;
  group?: string;
  /** Semantic color name or CSS color string — used by profile pill rendering. */
  color?: string;
  /** When true, clicking this option fires the combo's run_action directly
   *  (opens a modal/settings) and does NOT become the persisted selection.
   *  Rendered in a visually separated footer, like "New Workspace" in the
   *  workspace dropdown. */
  action?: boolean;
  /** Lucide icon name (curated subset — see PluginIcon.LUCIDE_MAP). */
  icon?:     string;
  /** Small caption shown below the label. */
  subtitle?: string;
  /** Right-aligned muted text (counts, dates, …). */
  meta?:     string;
  /** When true the option renders disabled and cannot be selected. */
  disabled?: boolean;
}

export type ActivityBarEntry =
  | { kind: 'action';    plugin_name: string; action: string; label: string; icon?: string }
  | { kind: 'combo';     plugin_name: string; id: string; run_icon?: string; run_action: string; select_action?: string; tooltip?: string; options: ComboOption[]; target?: string; variant?: string }
  | { kind: 'separator'; plugin_name: string };

/** A keyboard shortcut registered by a plugin. */
export interface PluginKeybinding {
  plugin_name: string;
  /** Action fired (via fire_plugin_action) when the shortcut triggers. */
  action:      string;
  key:         string;
  ctrl:        boolean;
  shift:       boolean;
  alt:         boolean;
  description: string;
}

/** A command palette entry registered by a plugin via `arbor.command.register`. */
export interface PluginCommand {
  plugin_name:  string;
  /** Unique identifier within the plugin (e.g. "run-tests"). */
  id:           string;
  /** Display title shown in the command palette. */
  title:        string;
  description?: string;
  /** Lucide icon name, e.g. "Play", "GitBranch". */
  icon?:        string;
  /** Group/category label used to section palette results. */
  group?:       string;
}


// ── Dispatch target ───────────────────────────────────────────────────────────

/**
 * Where an actionable node slot routes when triggered. Every clickable slot
 * (button `action`, select `change` action, future `onSelect`/`onEdit`/…)
 * resolves to one of these.
 *
 *   - `action`  — callback to the owning plugin's Lua handler (the only path
 *                 today; `fire_plugin_action`).
 *   - `command` — a registered host/plugin command, capability-gated. Wired
 *                 in a later phase.
 *
 * A bare `action: string` on a node is sugar for `{ kind: 'action', name }`;
 * see `toDispatchTarget` in `form-nodes/dispatch.ts`.
 */
export type DispatchTarget =
  | { kind: 'action';  name: string }
  | { kind: 'command'; id: string; args?: unknown };

/**
 * Granular node-tree patch op (event `plugin:form-update`, `op: "patch"`).
 *
 * Each op targets a node by its stable `id` and applies one mutation in place,
 * without re-mounting the form (sibling to the whole-tree `replace`). A node
 * without a stable id can't be patched (it can still be `replace`d).
 *
 *   - `merge`  — shallow-merge object props onto the node (label, options,
 *                disabled, variant…). Deep edits use `set`.
 *   - `set`    — assign a value at a path of segments *inside* the node
 *                (e.g. `["options", 0, "label"]`). Intermediate objects/arrays
 *                are created as needed.
 *   - `append` — push a child node into an array-valued prop (`to`, default
 *                `"children"`; e.g. `"nodes"` for a tree). The appended node is
 *                normalized (gets an auto id if missing).
 *   - `remove` — splice the targeted node out of its parent array. Removing a
 *                *child* = target that child by its own id with `remove`.
 *
 * Patches mutate the node tree only; field values go via `set_value` and the
 * opaque liveState via `set_state_path`.
 */
export type FormPatchOp =
  | { id: string; merge: Record<string, unknown> }
  | { id: string; set: (string | number)[]; value: unknown }
  | { id: string; append: FormNode; to?: string }
  | { id: string; remove: true };

// ── Plugin form config — emitted via Tauri event "plugin:form" ────────────────

// ─── Visibility conditions ────────────────────────────────────────────────────

/** Condition targeting a single field value */
export interface FieldCondition {
  field: string;
  eq?:       unknown;
  neq?:      unknown;
  gt?:       number;
  lt?:       number;
  gte?:      number;
  lte?:      number;
  in?:       unknown[];
  nin?:      unknown[];
  in_values?: unknown[];   // alias for `in` (avoids Lua reserved word)
}

export type FormCondition =
  | FieldCondition
  | { and: FormCondition[] }
  | { or:  FormCondition[] }
  | { not: FormCondition  };

// ─── Shared node base ─────────────────────────────────────────────────────────

export interface FormNodeBase {
  id?:      string;
  show_if?: FormCondition;
  style?:   string;
  class?:   string;
}

export type FormFieldValue = string | number | boolean;

// ─── Field nodes (contribute to submitted values) ─────────────────────────────

interface FormFieldBase extends FormNodeBase {
  name:      string;
  label?:    string;
  hint?:     string;
  required?: boolean;
  readonly?: boolean;
  /** Render in compact mode: a 3-column grid `label · control · pill`,
   *  with the label aligned to the control's baseline and the pill
   *  right-aligned. Pairs with `pill` / `pill_kind`. Designed for
   *  inspector-style data cards. */
  compact?:  boolean;
  /** Small uppercase pill rendered after the control. Free-form text; if
   *  it maps to a known kind ("vec3", "u32", "enum", "handle", …) it
   *  picks the curated palette colour automatically. */
  pill?:     string;
  /** Override the pill palette explicitly. Useful when the label is
   *  custom and `pill` alone wouldn't map to a known kind. */
  pill_kind?: string;
  /** Highlight tone for the row when the value changed since last frame
   *  / since last commit, etc. Renders a coloured strip on the left. */
  highlight?: boolean;
  /** Opt-in scoped commit slot. When set, the field's change is dispatched
   *  through the *scoped* channel — payload `{ node_id, slot, value, state? }`
   *  instead of the whole form — and may target a command. A node must carry
   *  a stable `id` to be a useful scoped target. Honoured today by the leaf
   *  `field` node (and `vec_field`); other value fields use the legacy path.
   *  Leave unset to keep the existing behaviour. */
  dispatch?:    DispatchTarget;
  /** Keys of the opaque form state to include (as `state`) in a scoped
   *  payload. Default: none (the scoped channel ships no state). */
  scope_state?: string[];
}

export interface FormFieldText extends FormFieldBase {
  type:         'text' | 'password' | 'email' | 'url';
  placeholder?: string;
  default?:     string;
  /** Regex pattern for inline validation (Lua pattern on the backend, JS regex on frontend). */
  pattern?:     string;
  pattern_hint?: string;
  /** Padding / font-size tier — matches the shared `<Input size>` widget.
   *  Default `'md'`. Use `'sm'` for dense filter rows. */
  size?:        'sm' | 'md' | 'lg';
  /** Leading Lucide icon name (rendered inside the input chrome). */
  icon?:        string;
  /** Trailing Lucide icon name. Mutually visible with `clearable` — the
   *  clear-× takes precedence when the value is non-empty. */
  icon_end?:    string;
  /** Leading text affix (e.g. `"$"`, `"https://"`, `"@"`). Muted, non-editable. */
  prefix?:      string;
  /** Trailing text affix (e.g. `"kg"`, `"%"`, `".com"`). Muted, non-editable. */
  suffix?:      string;
  /** Show a × button while the field has a value. Default `false`. */
  clearable?:   boolean;
  /** Fire a slot live on each keystroke (debounced), without waiting for
   *  Submit. Same shape as `FormFieldSelect.actions.change`: a bare string
   *  keeps the legacy whole-form payload; a `DispatchTarget` object goes
   *  scoped (`{ node_id, slot: 'change', value, state? }`) and can target a
   *  command. `scope_state` (on the field base) declares the state slice. */
  actions?:     { change?: string | DispatchTarget };
  /** Debounce window in ms for `actions.change` (trailing-edge). Default
   *  `250`. Use `0` to fire on every input. */
  debounce_ms?: number;
}

export interface FormFieldTextarea extends FormFieldBase {
  type:         'textarea';
  placeholder?: string;
  default?:     string;
  rows?:        number;
  /** Fire a slot live on every input (debounced). Same shape as
   *  `FormFieldText.actions.change`. */
  actions?:     { change?: string | DispatchTarget };
  /** Debounce window in ms for `actions.change` (trailing-edge). Default `250`. */
  debounce_ms?: number;
}

/**
 * Click-to-edit single-line field. Renders the current value as a clickable
 * label; activating it (click / Enter / Space) swaps in the host's
 * `<InlineEdit>` widget — `Enter` commits, `Esc` reverts, and the explicit
 * check / X buttons mirror those keys. There is no blur-commits behaviour;
 * dismissing focus reverts the in-progress edit. Use this for header titles,
 * row names, or anywhere a text input would be too noisy.
 */
export interface FormFieldInlineEdit extends FormFieldBase {
  type:                 'inline_edit';
  default?:             string;
  /** Placeholder inside the editing input. */
  placeholder?:         string;
  /** Text shown when the value is empty in display mode. Default `'—'`. */
  display_placeholder?: string;
  size?:                'sm' | 'md';
  maxlength?:           number;
  /** Block commit when the value is empty after trimming (default true). */
  require_value?:       boolean;
}

export interface FormFieldNumber extends FormFieldBase {
  type:        'number';
  default?:    number;
  min?:        number;
  max?:        number;
  step?:       number;
  placeholder?: string;
  /** Padding tier — matches the shared `<NumberStepper size>` widget. Default `'md'`. */
  size?:       'sm' | 'md' | 'lg';
  /** Leading Lucide icon name (rendered inside the stepper chrome). */
  icon?:       string;
  /** Trailing Lucide icon name (between the digits and the stepper column). */
  icon_end?:   string;
  /** Leading text affix (e.g. `"$"`). */
  prefix?:     string;
  /** Trailing text affix (e.g. `"kg"`, `"%"`, `"ms"`). */
  suffix?:     string;
  /** Fire a slot live on every keystroke / stepper click (debounced).
   *  Same shape as `FormFieldText.actions.change`. */
  actions?:     { change?: string | DispatchTarget };
  /** Debounce window in ms for `actions.change` (trailing-edge). Default `250`. */
  debounce_ms?: number;
}

export interface FormFieldRange extends FormFieldBase {
  type:          'range';
  default?:      number;
  min?:          number;
  max?:          number;
  step?:         number;
  show_value?:   boolean;
  value_format?: string;
  /** Fire a slot live as the user drags the slider (debounced). Same shape
   *  as `FormFieldText.actions.change`. */
  actions?:      { change?: string | DispatchTarget };
  /** Debounce window in ms for `actions.change` (trailing-edge). Default `250`. */
  debounce_ms?:  number;
}

export interface FormFieldCheckbox extends FormFieldBase {
  type:     'checkbox';
  label:    string;
  default?: boolean;
  /** Fire a slot every time the user flips the box (not deferred to submit).
   *  Same shape as `FormFieldSelect.actions.change`: a bare string keeps the
   *  legacy whole-form payload; a `DispatchTarget` object goes scoped. */
  actions?: { change?: string | DispatchTarget };
}

/**
 * iOS-style on/off switch. Like `checkbox` but rendered as a toggle. Use this
 * when the field semantically toggles a feature on/off (eg. "Enable foo");
 * use `checkbox` when the field expresses agreement / acknowledgment.
 */
export interface FormFieldToggle extends FormFieldBase {
  type:         'toggle';
  label?:       string;
  description?: string;
  default?:     boolean;
  size?:        'sm' | 'md' | 'lg';
  /** Fire a slot every time the user flips the switch (not deferred to submit).
   *  Same shape as `FormFieldSelect.actions.change`. */
  actions?:     { change?: string | DispatchTarget };
}

/** Shorthand element allowed inside radio / autocomplete / table cell option lists.
 *  A bare string is auto-expanded to { value, label } (label is capitalised). */
export type FormOptionInput =
  | string
  | { value: string; label: string; disabled?: boolean; description?: string };

/** Selectable option inside a `select` / `multiselect` field. Item form. */
export interface FormSelectOptionItem {
  value:        string;
  label:        string;
  /** Small text shown under the label. */
  description?: string;
  /** Lucide icon name (curated subset — see PluginIcon.LUCIDE_MAP). */
  icon?:        string;
  /** Right-aligned muted text (counts, dates, …). */
  meta?:        string;
  disabled?:    boolean;
}

/** Group header inside a `select` / `multiselect` option list. */
export interface FormSelectOptionGroup {
  group:              string;
  items:              FormSelectOption[];
  collapsible?:       boolean;
  default_collapsed?: boolean;
}

/** Decorative separator inside a `select` / `multiselect` option list. */
export interface FormSelectOptionSeparator {
  separator: true;
  label?:    string;
}

/** Rich option shape accepted by `select` / `multiselect`. Bare strings and
 *  legacy `{ value, label }` entries continue to work — the new entries
 *  (group / separator / icon / meta) are purely additive. */
export type FormSelectOption =
  | string
  | FormSelectOptionItem
  | FormSelectOptionGroup
  | FormSelectOptionSeparator;

export interface FormFieldSelect extends FormFieldBase {
  type:           'select';
  default?:       string;
  options:        FormSelectOption[];
  /** Show a search input above the items. Default: auto-on if list > 12. */
  searchable?:    boolean;
  /** Trigger placeholder when nothing is selected. */
  placeholder?:   string;
  /** Empty-state message (no items match / list empty). */
  empty_message?: string;
  /** Show a × button inside the trigger when a value is selected. Clicking
   *  it resets the field to the empty string and fires `actions.change`
   *  (when present) so live consumers see the cleared state. Default false. */
  clearable?:     boolean;
  /** Live action slots. `change` fires on every selection (not just Submit).
   *  A string keeps the legacy whole-form payload; a `DispatchTarget` object
   *  goes scoped (`{ node_id, slot:'change', value, state? }`) and can target
   *  a command. `scope_state` (on the field base) declares the state slice. */
  actions?:       { change?: string | DispatchTarget };
}

/** Multi-value variant of `select`. Stored as `string[]`. */
export interface FormFieldMultiselect extends FormFieldBase {
  type:           'multiselect';
  default?:       string[];
  options:        FormSelectOption[];
  searchable?:    boolean;
  placeholder?:   string;
  empty_message?: string;
  /** Min selected count (validation). */
  min?:           number;
  /** Max selected count (validation). */
  max?:           number;
  /** Show a × button inside the trigger when at least one option is
   *  selected. Clicking it resets the field to an empty array. Default
   *  false. */
  clearable?:     boolean;
}

export interface FormFieldRadio extends FormFieldBase {
  type:     'radio';
  default?: string;
  options:  FormOptionInput[];
  inline?:  boolean;
  /** Visual style. `radio` = classic radio dots (default).
   *  `segment` = pill-style toggle bar (IntelliJ / studio segmented control).
   *  `card`    = description+title cards. Use `segment` for compact mode
   *  switches (View: Tree / Raw, etc.). */
  appearance?: 'radio' | 'segment' | 'card';
  /** Size hint, honoured by `segment` and `card`. Default: `md`. */
  size?:    'sm' | 'md' | 'lg';
  /** Fire a slot every time the user picks a different option (not deferred
   *  to submit). Same shape as `FormFieldSelect.actions.change`. */
  actions?: { change?: string | DispatchTarget };
}

/** File / folder picker — opens the existing FileExplorerModal on click. */
export interface FormFieldFile extends FormFieldBase {
  type:        'file';
  /** "file" picks an existing file, "folder" picks a directory, "save" picks an output path. */
  pick_mode?:  'file' | 'folder' | 'save';
  /** File extension filter (without the dot, e.g. ["json", "yaml"]). Only honoured in "file"/"save" mode. */
  extensions?: string[];
  /** Placeholder shown when the path is empty. */
  placeholder?: string;
  default?:    string;
}

/**
 * Autocomplete with static or dynamic options.
 * - If `source_action` is set, Arbor fires that action on the plugin each time
 *   the user types, with `{ query, state }`. The plugin responds by calling
 *   `arbor.ui.set_autocomplete_options(id, options)` (may include `group`).
 * - Otherwise filters the static `options` list by fuzzy match.
 * The field `id` (required) is how the plugin identifies the autocomplete
 * when updating options.
 */
export interface FormFieldAutocomplete extends FormFieldBase {
  type:           'autocomplete';
  name:           string;
  /** Required — used as the autocomplete's dispatch identifier. */
  id:             string;
  label?:         string;
  placeholder?:   string;
  default?:       string;
  options?:       FormOptionInput[];
  /** Plugin action fired on each input change. */
  source_action?: string;
  /** Allow submitting values that aren't in the options list. Default: true. */
  free_form?:     boolean;
  /** Debounce for source_action in ms. Default: 150. */
  debounce_ms?:   number;
}

/**
 * Git branch picker — same chrome as the host's `<BranchSelect>` widget
 * (monospace dropdown trigger, search input above the menu past the
 * `search_threshold`, sticky entry for a value not in the list). Submitted
 * as the picked branch name (`string`).
 *
 * The plugin owns the branch list — pass it explicitly via `branches`.
 * Typical use: call `arbor.repo.branches()` (requires `git = "read"`) on
 * form open, map `b.name`, and supply the array. The host does not
 * auto-load or watch the active repo's branches; if the underlying list
 * changes, push it back with `arbor.ui.form.patch` (`merge = { branches }`).
 */
export interface FormFieldBranchSelect extends FormFieldBase {
  type:              'branch_select';
  /** Available branches to pick from. */
  branches:          string[];
  default?:          string;
  /** Trigger placeholder when no branch is selected. Default `'— pick a branch —'`. */
  placeholder?:      string;
  /** Render the trigger as a loading shell (label "Loading…", disabled). */
  loading?:          boolean;
  /** Show a search input above the menu once `branches.length` exceeds this.
   *  Default `12`. */
  search_threshold?: number;
}

/** Tag / chip input — submitted as string[]. */
export interface FormFieldTags extends FormFieldBase {
  type:        'tags';
  name:        string;
  label?:      string;
  placeholder?: string;
  default?:    string[];
  /** Allowlist — if provided, only these values can be added (acts like a multi-select). */
  suggestions?: string[];
  /** Maximum number of tags. */
  max?:        number;
}

/** One item in a tree row's right-click context menu. Render order matches
 *  the array order; mix interactive items with `separator`/`header` rows.
 *  Each item carries its own dispatch — falls back to the tree-level
 *  `on_context_menu` slot when neither `action` nor `dispatch` is set. */
export interface FormTreeMenuItem {
  /** Stable id surfaced in the dispatched payload as `item_id`. When omitted
   *  the widget synthesises a positional id (`__item_<index>`); set it
   *  explicitly when the same handler dispatches multiple item kinds. */
  id?:        string;
  /** Label text. Omit (+ `separator = true`) to render a divider. */
  label?:     string;
  /** Icon name (curated Lucide subset — see PLUGIN_ICONS). */
  icon?:      string;
  /** Legacy slot — sugar for `dispatch = { kind: 'action', name: action }`. */
  action?:    string;
  /** Explicit dispatch target — takes precedence over `action`. */
  dispatch?:  DispatchTarget;
  /** Style the row as destructive (red). */
  danger?:    boolean;
  /** Render the row disabled (no hover, no click). */
  disabled?:  boolean;
  /** Render a separator line. Alternative: omit `label` + `action`. */
  separator?: boolean;
  /** Render a non-clickable section header (bold, small caps). */
  header?:    boolean;
}

/** A single node in the tree selector. */
export interface FormTreeNode {
  value:    string;
  label:    string;
  /** Optional child nodes. */
  children?: FormTreeNode[];
  /** Non-selectable group header (still expandable). */
  group?:   boolean;
  /** Icon name (subset of Lucide — see docs). */
  icon?:    string;
  /** Explicit colour for the row icon (any CSS colour). Use to tint group
   *  headers per-category so a deep tree doesn't read monochrome. */
  icon_color?: string;
  /** Small inline pill badge shown after the label (e.g. "Tomcat"). */
  tag?:     string;
  /** Colour variant for `tag`. */
  tag_variant?: 'neutral' | 'ok' | 'warn' | 'error' | 'accent' | 'dev' | 'prod' | 'test';
  /** Optional dim caption under the label. */
  description?: string;
  /** Stable id for granular patching. When a `lazy` node is expanded the widget
   *  ships this id in the `on_expand` payload so the plugin can target it with
   *  `arbor.ui.form.patch` (merge/append children, clear `loading`). */
  id?:          string;
  /** Advertise (lazy) children before they are loaded: shows an expander and,
   *  on first expand, fires `on_expand`. The row may carry an empty `children`. */
  has_children?: boolean;
  /** Show a spinner on this row (e.g. while children are being fetched). The
   *  plugin clears it — usually with the same patch that appends the children. */
  loading?:     boolean;
  /** Per-row drag-drop override. When the tree's `reorderable = true`, every
   *  non-group row is draggable by default; set `false` here to pin a row. */
  draggable?:   boolean;
  /** Per-row drop-target override. When the tree's `reorderable = true`,
   *  every row accepts drops by default; set `false` to refuse drops on this
   *  row (e.g. for read-only sections inside an otherwise mutable tree). */
  drop_target?: boolean;
  /** Per-row context menu — wins over the tree-level `menu_items` when set.
   *  Empty array suppresses the menu for this row even if the tree has one. */
  menu_items?:  FormTreeMenuItem[];
  /** Inline editor for a leaf row. When set (and the row is not a `group`),
   *  activating the row swaps its value area for this field node, rendered
   *  through the normal node dispatcher — so every existing editor (`text`,
   *  `number`, `select`, `toggle`, `vec_field`, `color`…) works verbatim. The
   *  editor's own `actions.change` / dispatch fires the mutation on commit;
   *  the tree just toggles the read ⇄ edit view. Mirrors `PropertyRow.edit_node`. */
  edit_node?:   FormNode;
  /** Right-aligned display value for a leaf (shown before the type pill, dim
   *  monospace). Distinct from `value` (the node's selection key) and
   *  `description` (which sits under the label). Use for the "key: value"
   *  source-tree look. */
  value_display?: string;
  /** Code-editor colour tone for `value_display` (number / string / enum /
   *  bool / entity / handle / accent / warn / muted). Default: inherits. */
  value_tone?:    string;
  /** Type pill rendered after the value (kind-coloured via the shared
   *  `TypePill` — richer than the flat `tag`). Use for reflection/source
   *  trees where each leaf has a type (`Vec3`, `u32`, `enum`, …). */
  pill?:          string;
  /** Colour bucket for `pill` (defaults to `pill`). See `TypePill` `kind`. */
  pill_kind?:     string;
  /** Explicit semantic tone for `pill` (`accent` / `info` / `success` /
   *  `warning` / `error` / `muted`). Wins over `pill_kind` — use for badges
   *  that aren't a value-kind (e.g. a "bevy" provenance badge on a group). */
  pill_tone?:     'accent' | 'info' | 'success' | 'warning' | 'error' | 'muted';
}

/**
 * Hierarchical selector. Values are stored as the selected node's `value` when
 * `multi = false`, or as `string[]` when `multi = true`.
 */
export interface FormFieldTree extends FormFieldBase {
  type:        'tree';
  name:        string;
  label?:      string;
  nodes:       FormTreeNode[];
  multi?:      boolean;
  default?:    string | string[];
  /** Expand the whole tree on open. Default: false. */
  expanded?:   boolean;
  /** Render with border + rounded corners + inner padding + capped height.
   *  Default: false (plain, blends with parent — useful inside tree_layout nav). */
  bordered?:   boolean;
  /** Max-height when `bordered = true`. Default: "300px". */
  max_height?: string;
  /**
   * Plugin action fired whenever the user selects a (non-group) tree node.
   * The ctx passed to the handler includes the current form state plus
   * `value` — the newly selected node's `value`. Use this to drive master/
   * detail layouts where selecting a row must rebuild the right-hand side.
   * Legacy whole-form payload — prefer the scoped `on_select` below.
   */
  change_action?: string;

  // ── Dynamic ("data tree") opt-ins — additive; the static tree above is
  //    unchanged when these are absent. ──

  /** Enable lazy children: expanding a row that has `has_children` but no
   *  loaded `children` fires `on_expand` and shows a spinner until a patch
   *  fills them in. Without this the tree is fully static (today's behaviour). */
  lazy?:        boolean;
  /** Scoped slot fired when a (lazy) row is expanded. Ships `{ id, value, path }`
   *  of the expanded row so the plugin can patch its children by id. Bare action
   *  string or an explicit `DispatchTarget`. `scope_state` rides along. */
  on_expand?:   string | DispatchTarget;
  /** Scoped slot fired on selection change. Ships the newly selected value
   *  (string, or string[] in multi mode). Bare action string or a
   *  `DispatchTarget`. Coexists with `change_action`; when both are set
   *  `on_select` wins. */
  on_select?:   string | DispatchTarget;
  /** Scoped slot fired as the (virtualized) viewport scrolls. Ships
   *  `{ start, end, total }` row indices so a plugin can fetch by window. */
  on_scroll_range?: string | DispatchTarget;
  /** Window the rows when the flattened, currently-expanded tree exceeds this
   *  many rows (default 400). Below the threshold every row is rendered. */
  virtualize_threshold?: number;
  /** Fixed row height (px) used for the virtualized window (default 24). */
  row_height?:  number;
  /** Optional fixed viewport height (px or CSS length); falls back to
   *  `max_height`. Useful for virtualized trees. */
  height?:      number | string;
  /** Make the tree grow to fill the remaining height of its parent flex
   *  column and own the only scroll region (no `max_height` / fixed
   *  `height`). Use in a flush modal body so the tree — not the form body —
   *  scrolls, avoiding a double scrollbar. Default: false. */
  fill?:        boolean;
  /** Render an inline filter input at the top of the tree. Matches `label`
   *  and `description` case-insensitively, dims non-matching ancestors,
   *  auto-expands subtrees that contain matches, and highlights the matched
   *  substring. Local UI state — no plugin round-trip. */
  searchable?:  boolean;
  /** Placeholder for the filter input. Default `"Filter…"`. */
  search_placeholder?: string;
  /** Enable JSONPath-style navigation in the search box: when the query starts
   *  with `$` (e.g. `$.gameplay.rt_engine.ActionAbilities`) the substring
   *  filter is replaced by path matching against the node-label hierarchy —
   *  segments are prefix-matched as an ordered subsequence of each node's
   *  ancestor labels. Matches are navigable with F3 / Shift+F3 (and ↑/↓ /
   *  Enter from the input), a hit counter shows in the search row, and a
   *  results rail listing the hits opens beside the tree. Plain (non-`$`)
   *  queries still substring-filter as usual. Requires `searchable`. */
  path_query?:  boolean;
  /** Enable HTML5 drag-drop reorder among rows. Group rows (expansion
   *  headers) are non-draggable by default; per-row overrides via
   *  `tnode.draggable` / `tnode.drop_target`. Drop landing zone resolves
   *  from the cursor's vertical position over the target row:
   *    · top third  → `before` (sibling above)
   *    · middle third → `inside` (child) — only on group/expandable rows
   *    · bottom third → `after`  (sibling below)
   *  Leaves only emit `before` / `after`. */
  reorderable?: boolean;
  /** Scoped slot fired when a reorder completes. Payload:
   *    `{ source: { id?, value, path }, target: { id?, value, path },
   *       position: "before" | "inside" | "after" }`
   *  The plugin mutates its model and patches the tree's `nodes`. */
  on_reorder?:  string | DispatchTarget;
  /** Default right-click context menu items. Per-row `tnode.menu_items`
   *  wins when set. Each item carries its own `action` / `dispatch` — the
   *  tree-level `on_context_menu` slot is the fallback handler for items
   *  without one (use it to fan out by `item_id` in a single handler). */
  menu_items?:      FormTreeMenuItem[];
  /** Fallback scoped slot fired when a context-menu item without its own
   *  `action`/`dispatch` is picked. Payload: `{ item_id, value, path }`. */
  on_context_menu?: string | DispatchTarget;
}

/** Column definition for the table field. */
export interface FormTableColumn {
  /** Key in each row object. */
  key:          string;
  label:        string;
  /** Cell editor type. Default: "text". */
  type?:        'text' | 'number' | 'checkbox' | 'select';
  /** For type="select" — options list (supports bare-string shortcut). */
  options?:     FormOptionInput[];
  placeholder?: string;
  /** CSS width (e.g. "120px", "2fr"). */
  width?:       string;
  /** Render this column as display-only — the cell shows the row's value
   *  formatted by `type` (text, number, checked glyph, select label) but
   *  cannot be edited. Independent of the table-level `readonly`; useful
   *  for "id" / "computed" / "owner" columns sat next to editable ones. */
  readonly?:    boolean;
  /** Cell content alignment. Default: `"left"` for text/select, `"center"`
   *  for checkbox, `"right"` for number. */
  align?:       'left' | 'center' | 'right';
}

/** A per-row action button rendered in the table's trailing column.
 *  Each button fires its own dispatch with payload
 *  `{ row_index, row, action_id }`. */
export interface FormTableRowAction {
  /** Stable id surfaced in the dispatched payload as `action_id`. When
   *  omitted the widget synthesises a positional id (`__action_<index>`). */
  id?:        string;
  /** Lucide icon name (curated subset — see PLUGIN_ICONS). */
  icon?:      string;
  /** Tooltip / aria-label. */
  label?:     string;
  /** Style the row button as destructive (red on hover). */
  danger?:    boolean;
  /** Legacy slot — sugar for `dispatch = { kind: 'action', name: action }`.
   *  Fires the owning plugin's handler with payload
   *  `{ row_index, row, action_id }`. */
  action?:    string;
  /** Explicit dispatch target. Takes precedence over `action`. */
  dispatch?:  DispatchTarget;
  /** Render the button disabled. */
  disabled?:  boolean;
}

/** Tabular input — submitted as Array<Record<string, unknown>>. */
export interface FormFieldTable extends FormFieldBase {
  type:         'table';
  name:         string;
  label?:       string;
  columns:      FormTableColumn[];
  default?:     Record<string, unknown>[];
  /** Minimum number of rows (cannot delete below this). */
  min_rows?:    number;
  /** Maximum number of rows (hides the Add button when reached). */
  max_rows?:    number;
  /** Label for the Add button. Default: "+ Add row". */
  add_label?:   string;
  /** Per-row action buttons rendered in the trailing column, before the
   *  built-in trash. Each carries its own dispatch with payload
   *  `{ row_index, row, action_id }`. */
  row_actions?:   FormTableRowAction[];
  /** Hide the built-in row delete (trash) button — useful when a
   *  `row_actions` entry takes over the destructive role. */
  hide_delete?:   boolean;
  /** Hide the built-in "+ Add row" button — useful when the table's rows
   *  are derived from an external source (and only certain columns are
   *  user-editable), or when row creation is handled by a plugin action
   *  outside the table. */
  hide_add?:      boolean;
  /** Make the header stick to the top of the rows region — keeps column
   *  labels visible while scrolling. Pairs naturally with `max_height`. */
  sticky_header?: boolean;
  /** CSS max-height for the rows region (e.g. `"260px"`, `"40vh"`). When
   *  set, the rows scroll vertically while the Add button stays anchored
   *  below the scroll area. */
  max_height?:    string;
}

/** ISO-formatted date, e.g. "2026-04-20". */
export interface FormFieldDate extends FormFieldBase {
  type:     'date';
  default?: string;
  min?:     string;
  max?:     string;
}

/** Local datetime, e.g. "2026-04-20T14:30" (no timezone suffix). */
export interface FormFieldDateTime extends FormFieldBase {
  type:     'datetime';
  default?: string;
  min?:     string;
  max?:     string;
}

/** Time of day, e.g. "14:30". */
export interface FormFieldTime extends FormFieldBase {
  type:     'time';
  default?: string;
  min?:     string;
  max?:     string;
}

export interface FormFieldColor extends FormFieldBase {
  type:     'color';
  default?: string;
}

/** Key-value pair editor. Submitted value is a Record<string, string>. */
export interface FormFieldKvList extends FormFieldBase {
  type:               'kv_list';
  key_placeholder?:   string;
  value_placeholder?: string;
  default?:           Record<string, string>;
}

/** Multi-line code/text editor (CodeMirror 6). Value-bearing: the current
 *  document is submitted as `values[name]`; the host can push new content via
 *  `set_value`. On top of the whole-form model it can emit *scoped* events
 *  (`{ node_id, slot, value, state? }`) on the high-frequency channel:
 *  `on_edit` (debounced, slot `edit`, value = full text) and `on_select`
 *  (slot `select`, value = `{ from, to, text }`). Both slots accept a legacy
 *  action string or a `DispatchTarget` (so an edit/selection can drive a
 *  command). `scope_state` (on the field base) declares the state slice that
 *  rides along. */
/** One diagnostic / lint marker driven by the plugin. Address a range with
 *  document offsets (`from`/`to`, UTF-16 code units, CodeMirror native) or
 *  with a 1-based `line` for a whole-line marker. Out-of-range positions are
 *  clamped; entries with no addressable range are silently dropped. */
export interface FormEditorDiagnostic {
  /** Document offset of the marker start. */
  from?:     number;
  /** Document offset of the marker end (defaults to `from` when omitted). */
  to?:       number;
  /** 1-based line number — used when `from`/`to` are absent. */
  line?:     number;
  severity:  'error' | 'warning' | 'info' | 'hint';
  message:   string;
  /** Optional short identifier of the producer (shown in the tooltip). */
  source?:   string;
}

/** One static completion item supplied by the plugin. */
export interface FormEditorCompletion {
  label:    string;
  /** Short detail shown on the right of the label (e.g. `"keyword"`). */
  detail?:  string;
  /** Longer description shown in a side panel. */
  info?:    string;
  /** CodeMirror completion `type` (`keyword`, `variable`, `function`, …)
   *  — drives the icon shown in the popup. */
  type?:    string;
  /** Text to insert when this entry is picked. Defaults to `label`. */
  apply?:   string;
  /** Score boost (positive = higher in the list). */
  boost?:   number;
}

/** One snippet template supplied by the plugin. Uses CodeMirror's
 *  `${1:placeholder}` syntax for tab stops. */
export interface FormEditorSnippet {
  label:    string;
  /** Snippet body. `${1:name}` defines tab stop 1 with default `"name"`. */
  template: string;
  detail?:  string;
  info?:    string;
  type?:    string;
  boost?:   number;
}

export interface FormFieldEditor extends FormFieldBase {
  type:          'editor';
  /** Initial document (used when `values[name]` is otherwise unset). */
  default?:      string;
  /** Syntax language: `json | toml | yaml | ron | properties | plain`.
   *  Unknown ids fall back to `plain`. */
  language?:     string;
  /** Editor box height — a px number or any CSS length. Default `240`. */
  height?:       number | string;
  /** Show the line-number gutter (default `true`). */
  line_numbers?: boolean;
  /** Highlight the active line (default `true`). */
  active_line?:  boolean;
  /** Plugin-supplied diagnostics — gutter markers, squiggles, and hover
   *  tooltips. Reactive: patching the array via `arbor.ui.form.patch{ … }`
   *  re-renders the markers. */
  diagnostics?:  FormEditorDiagnostic[];
  /** Force the lint gutter on/off. Defaults to "on when `diagnostics` is non-empty". */
  lint_gutter?:  boolean;
  /** Static completion items merged into the autocomplete popup. */
  completions?:  FormEditorCompletion[];
  /** Static snippets merged into the autocomplete popup (each one expands
   *  into the editor with tab stops at `${1:…}` placeholders). */
  snippets?:     FormEditorSnippet[];
  /** Scoped, debounced slot fired on content edit. Payload value = full text. */
  on_edit?:      string | DispatchTarget;
  /** Debounce (ms) for `on_edit`. Default `300`. */
  debounce_ms?:  number;
  /** Scoped slot fired on selection change (cursor moves / range selects).
   *  Payload value = `{ from, to, text }` (document offsets + selected text). */
  on_select?:    string | DispatchTarget;
}

/** One line inside a {@link FormDiffHunk}. The plugin supplies the diffed
 *  content; line numbers are auto-filled from the hunk's `old_start` /
 *  `new_start` when omitted. */
export interface FormDiffLine {
  kind:        'context' | 'added' | 'removed';
  content:     string;
  /** Optional explicit old-side line number (auto-counted when omitted). */
  old_lineno?: number;
  /** Optional explicit new-side line number (auto-counted when omitted). */
  new_lineno?: number;
}

/** A contiguous block of diff lines. `header` and the start offsets are
 *  optional — the widget synthesises a `@@ … @@` header and counts line
 *  numbers from `old_start` / `new_start` (default 1) when they're absent. */
export interface FormDiffHunk {
  header?:     string;
  old_start?:  number;
  new_start?:  number;
  lines:       FormDiffLine[];
}

/** Read-only diff viewer. Display-only (NOT value-bearing): it carries
 *  pre-diffed hunks supplied by the plugin and reuses the same renderer as the
 *  app's git diff (unified + split, syntax highlight, virtualization for large
 *  diffs). Address it by a stable `id` to swap its `hunks` live via the
 *  `patch` op (`merge`). */
export interface FormNodeDiff extends FormNodeBase {
  type:    'diff';
  hunks:   FormDiffHunk[];
  label?:  string;
  hint?:   string;
  /** Filename used to pick the syntax-highlight grammar and shown in the
   *  header. */
  path?:     string;
  /** Previous path when the file was renamed (shown as `old → new`). */
  old_path?: string;
  /** Override the highlight grammar explicitly (e.g. `"rust"`, `"json"`).
   *  Takes precedence over the extension derived from `path`. */
  language?: string;
  /** Initial layout. Default `"unified"`. A local toggle switches it unless
   *  `hide_mode_toggle` is set. */
  mode?:    'unified' | 'split';
  /** Hide the local unified/split toggle. Default `false`. */
  hide_mode_toggle?: boolean;
  /** Wrap long lines (unified only — split keeps per-column horizontal
   *  scroll). Default `false`. */
  word_wrap?: boolean;
  /** Viewer height — a px number or any CSS length. Default `"320px"`. */
  height?:  number | string;
  /** Text shown when there are no hunks/lines. Default `"No changes"`. */
  empty_text?: string;
  /** Total-line count above which the virtualized renderer kicks in.
   *  Default `600`. */
  virtualize_threshold?: number;
}

export type FormFieldNode =
  | FormFieldText
  | FormFieldTextarea
  | FormFieldInlineEdit
  | FormFieldNumber
  | FormFieldRange
  | FormFieldCheckbox
  | FormFieldToggle
  | FormFieldSelect
  | FormFieldMultiselect
  | FormFieldRadio
  | FormFieldColor
  | FormFieldKvList
  | FormFieldEditor
  | FormFieldDate
  | FormFieldDateTime
  | FormFieldTime
  | FormFieldFile
  | FormFieldAutocomplete
  | FormFieldTags
  | FormFieldBranchSelect
  | FormFieldTree
  | FormFieldTable;

// ─── Layout & decoration nodes ────────────────────────────────────────────────

export interface FormNodeContainer extends FormNodeBase {
  type:      'container';
  children:  FormNode[];
  columns?:  number | string;
  gap?:      number | string;
}

export interface FormNodeRow extends FormNodeBase {
  type:     'row';
  children: FormNode[];
  gap?:     number | string;
  align?:   'start' | 'center' | 'end' | 'stretch';
  wrap?:    boolean;
}

export interface FormNodeSection extends FormNodeBase {
  type:         'section';
  title?:       string;
  description?: string;
  children:     FormNode[];
  collapsible?: boolean;
  collapsed?:   boolean;
  /** Render with card chrome (dark title bar, border, bg-base background). */
  card?:        boolean;
  /** Visual variant for `card` mode. Default: standard.
   *  - `"component"` — IntelliJ-style data card: status dot, two-tone
   *    `namespace::Name` title with namespace dimmed, dense 2-column body
   *    grid, and `header_actions` rendered as round ghost icons. */
  variant?:     'component';
  /** When `variant = "component"`, the small dot before the title.
   *  Tone picks the colour. Defaults to a muted/idle look when absent. */
  status_dot?:  { tone?: 'success' | 'info' | 'warning' | 'error' | 'muted' | 'accent'; tooltip?: string };
  /** When `variant = "component"`, the small text rendered dim just under
   *  the title (full type path, asset uri, etc.). */
  subtitle?:    string;
  /** Right-aligned ghost icon buttons in the header. Each entry fires a
   *  plugin action when clicked. */
  header_actions?: {
    icon:     string;
    tooltip?: string;
    action:   string;
    extra?:   Record<string, unknown>;
    variant?: 'default' | 'danger';
    disabled?: boolean;
  }[];
  /** Counter pill shown in card title (e.g. number of installed items). */
  count?:       number;
  /** Action fired when the + button in the card title is clicked. */
  add_action?:  string;
  /** Dense layout — children are laid out in a 2-column grid (1 column at
   *  narrow widths). Designed to pair with `variant = "component"` for
   *  IntelliJ-like inspector cards. */
  dense?:       boolean;
}

export interface FormNodeSeparator extends FormNodeBase {
  type:   'separator';
  label?: string;
}

export interface FormNodeParagraph extends FormNodeBase {
  type:     'paragraph';
  content:  string;
  variant?: 'normal' | 'muted' | 'heading' | 'caption';
}

export interface FormNodeAlert extends FormNodeBase {
  type:     'alert';
  /** Title rendered bold above the body (optional). Survives `collapsible`
   *  collapse so the user always sees something to click on. */
  title?:   string;
  text:     string;
  variant?: 'info' | 'warning' | 'error' | 'success';
  /**
   * Visual mode.
   *   - `banner` (default) — full-width tinted block with leading icon (Alert.svelte).
   *     Use for transient app messages (save / error / loading) that need to
   *     stand out at the top of a form or panel.
   *   - `inline` — compact callout with a coloured leading bar (Callout.svelte).
   *     Use for in-document hints, onboarding notes, and "by-the-way" call-outs
   *     embedded in body copy. `variant = "error"` maps to the danger styling,
   *     `variant = "success"` maps to the tip styling.
   */
  style?:   'banner' | 'inline';
  /** Show an × button on the right. Click hides the alert locally — no
   *  plugin round-trip. To bring it back, re-render the node via patch. */
  dismissable?: boolean;
  /** Show a chevron toggle that hides the body text. The title (and the
   *  underlying widget chrome) stays visible. */
  collapsible?: boolean;
  /** Start in the collapsed state. Only meaningful when `collapsible` is true. */
  collapsed?: boolean;
}

export interface FormNodeCode extends FormNodeBase {
  type:      'code';
  text:      string;
  language?: string;
  /** Show a floating Copy button (top-right) that copies `text` on click. */
  copy?:     boolean;
  /** Override the toast shown after a successful copy. */
  toast?:    string;
}

export interface FormNodeButton extends FormNodeBase {
  type:         'button';
  /** Label text. Optional when `icon_only = true`. */
  label?:       string;
  /** Legacy slot — sugar for `dispatch = { kind: 'action', name: action }`.
   *  Fires the owning plugin's handler. */
  action:       string;
  /** Explicit dispatch target. When set, takes precedence over `action` —
   *  lets a button invoke a registered command (`kind: 'command'`) instead of
   *  the owning plugin's handler. */
  dispatch?:    DispatchTarget;
  variant?:     'default' | 'primary' | 'danger' | 'ghost';
  close_after?: boolean;
  disabled?:    boolean;
  /** Extra data merged into the action payload alongside form values. Useful for item-specific actions in cfg_list / card_row. */
  extra?:       Record<string, unknown>;
  /** Optional Lucide icon shown before the label. */
  icon?:        string;
  /** Optional Lucide icon shown after the label (e.g. chevron, external-link).
   *  Suppressed when `icon_only = true`. */
  icon_end?:    string;
  /** Hide the label and render only the icon. */
  icon_only?:   boolean;
  /** Visual size — mirrors `shared/ui/Button` sizes. Default `'sm'`. */
  size?:        'xs' | 'sm' | 'md' | 'lg';
  /** Stretch to full container width (centered label). */
  block?:       boolean;
  /** Optional CSS colour override (hex, `var(--…)`, `color-mix(...)`). Applied
   *  to background for `variant = 'primary'`, to text/border for `'ghost'` /
   *  `'danger'`. Foreground auto-picks black/white via `oklch` for brand fills. */
  color?:       string;
  /** Tooltip on hover (useful for icon-only buttons). */
  tooltip?:     string;
}

/** Option inside a `menu_button` dropdown. */
export interface FormMenuOption {
  /** Label text. Omit together with `action` to render as a separator. */
  label?:    string;
  icon?:     string;
  action?:   string;
  /** Extra data merged into the action payload. */
  extra?:    Record<string, unknown>;
  variant?:  'default' | 'danger';
  disabled?: boolean;
  /** Render bold non-clickable section header instead of a selectable item. */
  heading?:  boolean;
  /** Render as a separator line. Alternative: omit `label` + `action`. */
  separator?: boolean;
}

/**
 * Button that opens a dropdown menu on click. Each option fires its own action
 * (with optional `extra`). Useful for IntelliJ-style "+▾" new-config menus.
 */
export interface FormNodeMenuButton extends FormNodeBase {
  type:          'menu_button';
  /** Label text (omit together with `icon_only = true` for icon-only button). */
  label?:        string;
  icon?:         string;
  tooltip?:      string;
  variant?:      'default' | 'primary' | 'danger' | 'ghost';
  disabled?:     boolean;
  /** Hide the label and render only the icon (+ chevron). */
  icon_only?:    boolean;
  /** Show a chevron after the label. Default: true. */
  show_chevron?: boolean;
  options:       FormMenuOption[];
}

/**
 * Two-column layout with a navigation panel on the left and a content panel on
 * the right. Typical pattern: `nav_children` = `[toolbar, tree]`; `content_children`
 * = `[sections gated with show_if]`. Works in any form (not just `sidebar = true`).
 */
/**
 * Dedicated pipeline / workflow editor (3-column palette · sequence · detail).
 * Backed by `PluginPipelineEditor.svelte` — far more compact than the generic
 * form primitives, and tuned for the "select step / mutate / re-render" loop
 * that workflow editors need.
 *
 * The plugin supplies the full in-memory profile state plus the list of
 * operations available in the palette. The component resolves selection and
 * palette search internally and emits structural mutations via plugin actions
 * (add_stage, add_step, select_step, move_*, remove_*, etc.).
 *
 * Typical use: one top-level `pipeline_editor` inside a tab — the surrounding
 * form handles Save / Cancel as usual.
 */
// ─── Dashboard widgets (generic, plugin-renderable) ───────────────────────────

/** Single tile inside a `<counter_grid>`. */
export interface CounterGridItem {
  /** Stable identifier — surfaced as `{ key }` in the `select` action payload. */
  key:    string;
  /** Header label (rendered upper-case). */
  label:  string;
  /** Primary value. Numbers render as-is; strings pass through. */
  value:  number | string;
  /** Optional muted sub-line under the value (delta, age, units…). */
  hint?:  string;
  /** Accent colour — CSS expression (`"var(--severity-high)"`, `"#f97316"`). */
  color?: string;
  /** Lucide icon name (curated subset — see PLUGIN_ICONS). */
  icon?:  string;
  /** When true (or when `value === 0`), the tile is dimmed and unclickable. */
  empty?: boolean;
}

/**
 * Responsive grid of KPI counter tiles. Each tile shows a coloured label, a
 * large primary value, and an optional hint line. Domain-agnostic — the
 * security dashboard's severity grid is one wrapper; any plugin can build
 * its own (build status totals, repo counts, …).
 *
 * `actions.select` fires with `{ key }` when a non-empty tile is clicked.
 */
export interface FormNodeCounterGrid extends FormNodeBase {
  type:       'counter_grid';
  items:      CounterGridItem[];
  /** Min tile width in px (CSS `minmax(N, 1fr)`). Default 120. */
  min_width?: number;
  /** Grid gap in px. Default 8. */
  gap?:       number;
  /** Outer padding (CSS). Default `'12px'`. */
  padding?:   string;
  /** Supported keys: `select`. Payload: `{ key }`. */
  actions?:   Record<string, string>;
}

/** Coloured zone inside a `<score_gauge>`. */
export interface ScoreGaugeSegment {
  from:  number;
  to:    number;
  color: string;
}

/**
 * Semi-circle gauge for a single bounded value. Coloured `segments` define
 * the band palette; the needle rotates to the interpolated value. Display-
 * only — no actions in v1.
 */
export interface FormNodeScoreGauge extends FormNodeBase {
  type:         'score_gauge';
  value:        number;
  /** Default 0. */
  min?:         number;
  /** Default 100. */
  max?:         number;
  segments?:    ScoreGaugeSegment[];
  /** Sub-label rendered under the numeric value. */
  label?:       string;
  size?:        'sm' | 'md' | 'lg';
  /** Override the needle / value text colour. Defaults to the segment colour at `value`. */
  value_color?: string;
}

/** One series inside a `<time_series_chart>`. */
export interface TimeSeriesSeriesDef {
  id:     string;
  label:  string;
  color:  string;
  /** `x` is an ISO-8601 string (parsed as `Date`) when `x_kind = 'time'`, a number otherwise. */
  points: Array<{ x: string | number; y: number }>;
}

/**
 * Multi-series line chart with a time-aware x-axis. Hover-guide, tooltip,
 * and an optional legend are baked in. Display-only — no actions in v1.
 */
export interface FormNodeTimeSeriesChart extends FormNodeBase {
  type:             'time_series_chart';
  series:           TimeSeriesSeriesDef[];
  /** `'time'` (default) or `'linear'`. */
  x_kind?:          'time' | 'linear';
  /** Body height in px. Default 220. */
  height?:          number;
  /** Default true. */
  show_legend?:     boolean;
  /** Default true — force-include zero on the y-axis. */
  y_include_zero?:  boolean;
}

/** Column definition inside a `<data_table>`. */
export interface DataTableColumnDef {
  key:    string;
  label:  string;
  /** CSS width — `'120px'`, `'1fr'`, `'minmax(80px, 1fr)'`. Default `'1fr'`. */
  width?: string;
  align?: 'left' | 'center' | 'right';
  /** Cell rendering. Default `'text'`. */
  kind?:  'text' | 'code' | 'pill' | 'datetime' | 'age';
  /** Pill background colour (CSS expression). Used when `kind = 'pill'`. Per-row override: `_<key>_color`. */
  color?: string;
  sortable?: boolean;
}

/**
 * Sortable, optionally clickable data table. Cells render according to the
 * column's `kind`. Sorting is client-side and stable across re-renders.
 *
 * `actions.row_click` fires with `{ row_id, row }` when a row is clicked.
 */
export interface FormNodeDataTable extends FormNodeBase {
  type:          'data_table';
  columns:       DataTableColumnDef[];
  rows:          Array<Record<string, unknown>>;
  /** Field used as the row id in Svelte keys and `row_click` payloads. Default `'id'`. */
  row_key?:      string;
  /** When set, the body scrolls inside this height (px). */
  height?:       number;
  initial_sort?: { key: string; dir: 'asc' | 'desc' };
  /** Plain text shown when `rows` is empty. */
  empty?:        string;
  /** Supported keys: `row_click`. Payload: `{ row_id, row }`. */
  actions?:      Record<string, string>;
}

/** Single dropdown filter inside a `<filter_bar>`. */
export interface FilterBarFilterDef {
  /** Stable id; surfaced as the key in the emitted `filters` map. */
  id:          string;
  label:       string;
  /** Lucide icon name (curated subset — see PLUGIN_ICONS). */
  icon?:       string;
  options:     Array<{ value: string; label: string; color?: string }>;
  /** `'multi'` (default) accepts any subset; `'single'` clears the others on select. */
  mode?:       'single' | 'multi';
  /** When true the dropdown gets an inline filter input. Default false. */
  searchable?: boolean;
  /** Wider dropdown panel. */
  wide?:       boolean;
  /** Default selection. */
  default?:    string[];
}

/**
 * Search input + N chip dropdowns. State is `{ search, filters }`. When
 * `name` is set, the form value tracks this object; the bar also fires
 * `actions.change` (real-time) with the latest value in `extra` so the
 * plugin can re-fetch / re-render without round-tripping through submit.
 *
 * Display-only otherwise — no validation, no required.
 */
export interface FormNodeFilterBar extends FormNodeBase {
  type:        'filter_bar';
  /** Optional field name — when set the value is collected into form values. */
  name?:       string;
  /** Initial value (when `name` is set, also used as the default). */
  default?:    { search?: string; filters?: Record<string, string[]> };
  /** Search input config. Set to `null` / omit to hide the search input. */
  search?:     { placeholder?: string; show_regex?: boolean } | null;
  filters?:    FilterBarFilterDef[];
  /** Outer padding (CSS). Default `'8px'`. */
  padding?:    string;
  /** Supported keys: `change`. Payload: `{ value: { search, filters } }`. */
  actions?:    Record<string, string>;
}

export interface FormNodePipelineEditor extends FormNodeBase {
  type:                'pipeline_editor';
  /** Ordered list of stages + their steps. */
  stages:              Array<{
    id: string;
    name: string;
    mode?: 'sequential' | 'parallel';
    max_parallel?: number | null;
    steps: Array<{
      id:   string;
      name: string;
      kind: string;
      allow_failure?: boolean;
    }>;
  }>;
  /** Palette entries grouped by category, in display order. */
  operations:          Array<{
    id: string;
    label: string;
    ops: Array<{ kind: string; label: string; icon?: string; summary?: string }>;
  }>;
  /** Initial search query (editor keeps its own live value afterwards). */
  search_query?:       string;
  selected_step_id?:   string;
  selected_stage_id?:  string;
  /** Form nodes rendered inside the detail pane for the selected step. */
  step_detail_form?:   FormNode[];
  /** Placeholder shown in the detail pane when no step is selected. */
  empty_label?:        string;
  /**
   * Plugin action names emitted for each interaction. The payload always
   * includes the relevant id(s) — e.g. `{ stage_id, step_id }` for step ops.
   * Supported keys (all optional): add_stage, add_step, select_step,
   * remove_step, duplicate_step, move_step_up, move_step_down,
   * remove_stage, move_stage_up, move_stage_down, edit_stage, search_changed.
   */
  actions:             Record<string, string>;
}

export interface FormNodeTreeLayout extends FormNodeBase {
  type:             'tree_layout';
  /** Left-panel nodes (toolbar + tree, typically). */
  nav_children:     FormNode[];
  /** Right-panel nodes (form content, typically gated with show_if). */
  content_children: FormNode[];
  /**
   * Left-panel width.
   *  - Without `nav_resizable`: any CSS length ("240px", "20rem", "30%"…).
   *  - With `nav_resizable`: parsed as pixels (string `"NNNpx"` or number);
   *    used as the initial width when no stored preference exists. Default 240.
   */
  nav_width?:       string | number;
  /**
   * When true, renders a toggle in the top-right of the nav (and a thin rail
   * on the content side when collapsed) so the user can hide the sidebar and
   * reclaim horizontal space. State persists in localStorage under
   * `arbor:tree-layout-collapsed:<id>` (only persisted when `id` is set on the
   * node). Default: false (no toggle, same as prior behaviour).
   */
  nav_collapsible?:       boolean;
  /**
   * Initial collapsed state when the form first opens. Ignored if
   * localStorage already has a preference for this `id`. Default: false.
   */
  nav_collapsed_default?: boolean;
  /**
   * When true, renders a drag handle on the right edge of the nav so the
   * user can resize the sidebar (clamped to `nav_min_width` / `nav_max_width`).
   * Arrow keys nudge by 8px (Shift = 32px). Width persists in localStorage
   * under `arbor:tree-layout-nav-w:<id>` when an `id` is set on the node
   * (anonymous nav stays resizable but the size is session-only).
   * Default: false.
   */
  nav_resizable?:         boolean;
  /**
   * Minimum width when `nav_resizable` is on. Pixels (number or `"NNNpx"`).
   * Default: 160.
   */
  nav_min_width?:         string | number;
  /**
   * Maximum width when `nav_resizable` is on. Pixels (number or `"NNNpx"`).
   * Default: 480.
   */
  nav_max_width?:         string | number;
}

/** Plain label — static text, no field. Alias for a minimal paragraph. */
export interface FormNodeLabel extends FormNodeBase {
  type:     'label';
  text:     string;
  variant?: 'normal' | 'muted' | 'caption';
}

/** Horizontal rule divider without a label. */
export interface FormNodeDivider extends FormNodeBase {
  type: 'divider';
}

/**
 * Branch form content on the current value of a sibling field. Fields inside
 * non-matching cases are not rendered (and their initial values are not
 * re-collected on switch — they stay as declared).
 */
export interface FormNodeSwitch extends FormNodeBase {
  type:    'switch';
  /** Name of the field whose value drives the branch. */
  field:   string;
  /** Case lookup: keys are possible values of `field`. */
  cases:   Record<string, FormNode[]>;
  /** Rendered when no case matches. */
  default?: FormNode[];
}

export interface FormTab {
  id:       string;
  label:    string;
  /** Optional Lucide icon name shown before the label. */
  icon?:    string;
  /** Nav group header label for sidebar mode. Tabs with the same group are grouped together. */
  group?:   string;
  children: FormNode[];
  /**
   * When true, the tab's panel is rendered without the default padding and
   * gap. Use this for tabs that ship a full-bleed component (e.g. a
   * `pipeline_editor`) that already handles its own inner spacing.
   */
  flush?:   boolean;
  /** Small badge text shown after the label (counts, warnings). */
  badge?:   string;
  /** Variant hint for the badge color. */
  badge_kind?: 'info' | 'success' | 'warning' | 'error' | 'muted' | 'accent';
  /** Disable the tab — dimmed, not selectable. */
  disabled?: boolean;
  /** Optional dim subtitle shown under the label — useful when the label
   *  is a short alias and the full identifier (type path, etc.) should
   *  stay visible without truncation. */
  meta?:    string;
  /** Tooltip on the nav item — typical use is showing the full type path
   *  when the visible label is a short alias. */
  tooltip?: string;
}

/** One step of a wizard. */
export interface FormWizardStep {
  id:           string;
  label:        string;
  description?: string;
  icon?:        string;
  children:     FormNode[];
}

/**
 * Multi-step wizard layout. Replaces the submit button with a Back/Next pair
 * while stepping through; the final step shows the normal Submit button.
 * All fields across all steps are collected into the submit payload.
 */
export interface FormNodeWizard extends FormNodeBase {
  type:        'wizard';
  steps:       FormWizardStep[];
  /** Start step id (default: first step). */
  start_step?: string;
  /** Label for the Next button (default: "Next"). */
  next_label?: string;
  /** Label for the Back button (default: "Back"). */
  back_label?: string;
}

/**
 * Tabbed layout container. All fields inside every tab are always collected
 * for submission — inactive tabs are just visually hidden, not removed.
 */
export interface FormNodeTabs extends FormNodeBase {
  type:         'tabs';
  tabs:         FormTab[];
  default_tab?: string;
  /** Render only the active tab's panel; inactive panels render nothing
   *  until selected (re-mounted on each switch). Use for heavy panels — a
   *  large syntax-highlighted code dump, hundreds of cards — where mounting
   *  every panel up-front bloats the DOM and stalls interaction. Field values
   *  are still collected from every tab regardless (collection walks the node
   *  tree, not the DOM), so submit is unaffected. Default: false. */
  lazy?:        boolean;
  /**
   * When set, the active tab id is mirrored to `localStorage[persist_key]`
   * — the user's selection survives the modal closing and reopening.
   * Ignored when set on a `tabs` rendered as sidebar nav (the sidebar
   * has its own selection model via `default_tab` + show_if).
   *
   * Doubles as the cross-renderer sync key: two `tabs` widgets in the same
   * modal that share a `persist_key` (typically one `strip_only` in
   * `header.centre` and one full-content in `nodes`) read and write the
   * same in-memory slot, so clicking a tab on one updates the other in
   * lock-step.
   */
  persist_key?: string;
  /**
   * When true, render only the tab strip — skip the per-tab panel divs
   * entirely. Designed for the "view-mode switcher in `header.centre`"
   * pattern: the strip lives in the header for the Studio-shaped chrome
   * look, while a second `tabs` widget in the body (same `persist_key`,
   * `panels_only = true`) renders the panel content. Without `strip_only`,
   * putting `tabs` in `header.centre` would draw the active tab's children
   * INSIDE the header strip — almost never what you want.
   *
   * Plain `strip_only = true` without a matching body `tabs` (or any other
   * widget reacting to the same shared state) gives a header strip that
   * only flips its own active highlight — inert until something else
   * subscribes to the same `persist_key`.
   */
  strip_only?: boolean;
  /**
   * When true, render only the per-tab panel divs — skip the tab strip.
   * Mirror of `strip_only`: typically paired with a `strip_only` tabs in
   * `header.centre` so the body shows panels without a duplicate strip
   * sitting between the header strip and the active panel. Both widgets
   * must share the same `persist_key` for the strip to drive the body.
   */
  panels_only?: boolean;
  /** Sidebar mode only — show a filter input at the top of the nav that
   *  case-insensitively matches `label`, `group` and `meta` against the
   *  user's query. Tabs that don't match are hidden; empty groups
   *  collapse. */
  nav_search?:  boolean;
  /** Placeholder text for the nav filter input. Default: "Search…". */
  nav_search_placeholder?: string;
  /** Sidebar mode only — small heading line shown above the nav (e.g. a
   *  count or a subtitle for the current selection). */
  nav_header?:  string;
  /** Sidebar mode only — small caption line shown below the nav (e.g.
   *  a count of hidden items). */
  nav_footer?:  string;
}

/** Two-column label+control row inside a card section. */
export interface FormNodeCardRow extends FormNodeBase {
  type:         'card_row';
  label?:       string;
  description?: string;
  children:     FormNode[];
}

/** Responsive card grid container.
 *
 *  Lays out `children` in an auto-fit grid where each cell is at least
 *  `min_card` wide (default `280px`) and expands to fill the available
 *  width. Children are typically `section variant="component"` cards or
 *  `info_card`s. Unlike `card_row`, the grid wraps to multiple rows when
 *  there isn't enough width for all cards side-by-side. */
export interface FormNodeCardGrid extends FormNodeBase {
  type:       'card_grid';
  /** Minimum card width before wrapping (e.g. `"280px"`, `"22ch"`).
   *  Default: `"280px"`. */
  min_card?:  string;
  /** Gap between cards (e.g. `"8px"`). Default: `"8px"`. */
  gap?:       string;
  children:   FormNode[];
}

/** One row in a `property_grid`. Read-only by default: renders
 *  `label` on the left, the formatted `value` on the right with an optional
 *  type `pill`. A row is either a leaf (has `value`) or a group (`children`,
 *  for nested structs / arrays). */
export interface PropertyRow {
  /** Stable id — required when the row is editable so the grid can track
   *  which row is open for editing. Falls back to `label` + index. */
  id?:        string;
  label:      string;
  /** Pre-formatted display string (e.g. `"[ 4.50, 0.00, -2.25 ]"`, `"82"`,
   *  `"Job::Legionary"`). The plugin owns formatting — the grid never
   *  interprets the raw value. */
  value?:     string;
  /** Syntax-highlight colour for the value text, code-editor style. When
   *  omitted the value uses the default primary colour. */
  value_tone?: 'number' | 'string' | 'enum' | 'bool' | 'entity' | 'handle' | 'muted' | 'warn' | 'accent';
  /** Type pill rendered right-aligned (e.g. `"u32"`, `"Vec3"`, `"enum"`). */
  pill?:      string;
  /** Pill colour bucket — defaults to `pill`. Mirrors `TypePill`'s `kind`. */
  pill_kind?: string;
  pill_tooltip?: string;
  /** Tooltip on the value cell (typical use: the full / untruncated value). */
  tooltip?:   string;
  /** Dim the value (e.g. `None` / null / default). */
  muted?:     boolean;
  /** Click-to-copy the value text (client-side, no plugin round-trip).
   *  Shows a copy glyph on row hover. */
  copyable?:  boolean;
  /** Immutable field — shows a lock glyph and suppresses editing even when
   *  `edit_node` is present. */
  locked?:    boolean;
  /** Nested struct / array rows — rendered indented under this row. */
  children?:  PropertyRow[];
  /** Group rows only: render a chevron that folds the children. */
  collapsible?: boolean;
  /** Group rows only: initial open state when `collapsible` (default open). */
  open?:      boolean;
  /** When present (and not `locked`), the row gains a hover pencil; clicking
   *  it swaps the value cell for this node rendered inline (a `field`,
   *  `vec_field`, `color`, `select`, … — the grid delegates to the normal
   *  node dispatcher, so all existing editors work unchanged). The node's own
   *  `action` / `dispatch` fires the mutation on commit. */
  edit_node?: FormNode;
}

/** Read-only-first property / reflection grid.
 *
 *  Renders a dense, IntelliJ-inspector-style list of `label → value` rows
 *  with right-aligned type pills, nested-struct indentation, lock glyphs for
 *  immutable fields, and optional per-row click-to-edit (via `edit_node`).
 *
 *  Generic — any plugin inspecting structured data (config dumps, JSON,
 *  ECS reflection, API responses) can use it. The plugin formats the values
 *  and supplies the editor nodes; the grid owns only the layout and the
 *  read-only ⇄ edit toggle. */
export interface FormNodePropertyGrid extends FormNodeBase {
  type:    'property_grid';
  rows:    PropertyRow[];
  /** Empty-state text shown when `rows` is empty. Default: `"(no fields)"`. */
  empty?:  string;
}

export interface CfgListItemTag {
  text:     string;
  variant?: 'neutral' | 'ok' | 'warn' | 'error' | 'accent' | 'dev' | 'prod' | 'test';
}

export interface CfgListItem {
  id:             string;
  label:          string;
  active?:        boolean;
  tags?:          CfgListItemTag[];
  /** Action fired with `{ id }` payload when the edit (pencil) button is clicked. */
  edit_action?:   string;
  /** Action fired with `{ id }` payload when the delete (trash) button is clicked. */
  delete_action?: string;
}

/** Config list — rows with active-state dot, name, tags, and hover edit/delete buttons. */
export interface FormNodeCfgList extends FormNodeBase {
  type:  'cfg_list';
  items: CfgListItem[];
}

export interface SuggestItem {
  name:    string;
  cmd?:    string;
  /** Tag shown alongside the name (e.g. "prod"). */
  tag?:    string;
  /** Action fired with `{ name, cmd }` when "Add configuration" is clicked. */
  action?: string;
}

/** 2-column grid of suggestion cards with an "Add configuration" link each. */
export interface FormNodeSuggestGrid extends FormNodeBase {
  type:  'suggest_grid';
  items: SuggestItem[];
}

/**
 * Vertical labeled wrapper around any child nodes — same look as the
 * `<FormField>` Svelte widget used by host modals (label on top, control(s)
 * below, optional hint/error). Use this when you want to apply the standard
 * field chrome around custom content (a `button`, a `copy_link`, a row of
 * controls) or to enrich a single field with a leading icon, an action
 * button next to the label, or a description above the control.
 *
 * For a plain text input the existing field types (`text`, `select`, …)
 * already render their own label — `form_field` is mainly for non-field
 * content, mixed-control layouts, or labels that need the icon / actions /
 * description affordances those types don't expose.
 */
export interface FormNodeFormField extends FormNodeBase {
  type:           'form_field';
  /** Label text. Omit together with `icon` and `actions` to render without the label row. */
  label?:         string;
  /** Small muted text after the label (e.g. "(optional)"). */
  optional_text?: string;
  /** Show a red asterisk after the label. */
  required?:      boolean;
  /** Description shown between label and content (sentence-case secondary). */
  description?:   string;
  /** Hint shown below the content, muted. */
  hint?:          string;
  /** Error shown below the content. Replaces hint when present. */
  error?:         string;
  /** Lucide icon name shown before the label text. */
  icon?:          string;
  /** Right-aligned action node(s) on the same row as the label (typically `button` nodes). */
  actions?:       FormNode[];
  /** Body content rendered below the label. */
  children:       FormNode[];
  /** htmlFor target on the underlying <label>. */
  for?:           string;
}

// ─── Hero header card (entity-style: avatar + title + meta + actions) ────────

export type InfoCardBadgeKind = 'info' | 'success' | 'warning' | 'error' | 'accent' | 'muted';

export interface InfoCardBadge {
  text: string;
  kind?: InfoCardBadgeKind;
}

export interface InfoCardMeta {
  /** Optional ALL-CAPS label rendered dim in front of the value. */
  label?: string;
  /** Value rendered in the mono palette. */
  value:  string;
  /** Hover tooltip — typical use is showing the full type path when the
   *  value column shows a shortened alias. */
  tooltip?: string;
}

export interface InfoCardAction {
  /** Lucide icon name. */
  icon:    string;
  label?:  string;
  tooltip?: string;
  variant?: 'default' | 'primary' | 'danger';
  disabled?: boolean;
  /** Plugin action fired on click. */
  action:  string;
  /** Extra data merged into the action payload. */
  extra?:  Record<string, unknown>;
}

/**
 * Hero header card. Use as the FIRST node of a tab body, panel section or
 * modal to anchor "what am I looking at" context — title, status pill,
 * type badges, key:value meta pills, and a row of action icons.
 */
export interface FormNodeInfoCard extends FormNodeBase {
  type:        'info_card';
  title:       string;
  subtitle?:   string;
  /** Either a Lucide icon name OR a 1-2 letter monogram (e.g. `"M"`).
   *  Mutually exclusive; pick one. */
  icon?:       string;
  monogram?:   string;
  /** Avatar accent override — defaults to `--accent`. */
  accent?:     string;
  /** Right-aligned status pill next to the title. */
  status?:     { text: string; kind?: InfoCardBadgeKind };
  badges?:     InfoCardBadge[];
  meta?:       InfoCardMeta[];
  actions?:    InfoCardAction[];
  /** Card chrome tone. Defaults to `'elevated'`. Use `'flat'` when nesting
   *  inside another elevated surface. */
  variant?:    'elevated' | 'flat' | 'subtle';
  /** Show the 1px border. Defaults to `true`. */
  bordered?:   boolean;
}

// ─── Filter / category chips ────────────────────────────────────────────────

export type ChipTone = 'accent' | 'info' | 'success' | 'warning' | 'error' | 'muted' | 'neutral';

export interface ChipItem {
  id:    string;
  label: string;
  count?: number;
  tone?:  ChipTone;
  icon?:  string;
  tooltip?: string;
  disabled?: boolean;
}

/**
 * Horizontal pill selector. The current selection is exposed as a
 * regular form value (so it can be read in submit and echoed back
 * through `liveState`). In multi mode the value is a `string[]`,
 * otherwise a single `string`.
 *
 * Use as a filter row above a list of `section` cards — the typical
 * pattern is to gate the sections with `show_if = { field, value }`
 * so flipping a chip narrows the visible cards without a round-trip.
 */
export interface FormNodeChipBar extends FormNodeBase {
  type:     'chip_bar';
  /** Field name — selection is stored in `values[name]`. */
  name:     string;
  /** Default-selected id(s). */
  default?: string | string[];
  multi?:   boolean;
  size?:    'sm' | 'md';
  /** Tint inactive chips by their `tone` too (coloured text + border), so the
   *  bar reads like a legend before selection. Default: false (neutral until
   *  selected). */
  tint_inactive?: boolean;
  /** When set, also fires this action with `{ name, value }` whenever the
   *  selection changes (useful when no parent uses `show_if`). */
  action?:  string;
  items:    ChipItem[];
}

// ─── breadcrumb ───────────────────────────────────────────────────────────────
/**
 * Display-only horizontal trail of chip-style segments. Useful as a path
 * indicator in plugin views / studio-like modals. Clicking an interactive
 * segment fires `action` with `{ value, index }` merged into the payload.
 */
export interface FormNodeBreadcrumbSegment {
  label:        string;
  /** Lucide name, emoji, or `plugin:<plugin>:<icon_id>` reference. */
  icon?:        string;
  /** Tiny pill rendered after the label (e.g. "current"). */
  badge?:       string;
  tooltip?:     string;
  /** When false, segment is dimmed and not clickable. Default: true. */
  interactive?: boolean;
  /** Opaque value echoed back to the plugin in the action payload. */
  value?:       string | number;
}

export interface FormNodeBreadcrumb extends FormNodeBase {
  type:        'breadcrumb';
  segments:    FormNodeBreadcrumbSegment[];
  /** Soft cap on visible segments; middle collapses to ellipsis. Default: 6. */
  max?:        number;
  /** Fired when an interactive segment is clicked. Payload merges
   *  `{ value, index, label }`. */
  action?:     string;
  /** When true a pencil button appears on the right; double-clicking the
   *  trail also enters edit mode. Submitted path is sent via `commit_action`
   *  as `{ path }`. */
  editable?:        boolean;
  edit_value?:      string;
  edit_placeholder?: string;
  commit_action?:   string;
}

// ─── url_block ────────────────────────────────────────────────────────────────
/**
 * Monospace readable display for a URL or any opaque identifier the user
 * needs to verify verbatim. Wraps long values; never truncates with
 * ellipsis. `copyable` adds a small copy-to-clipboard button.
 */
export interface FormNodeUrlBlock extends FormNodeBase {
  type:      'url_block';
  value:     string;
  label?:    string;
  copyable?: boolean;
}

// ─── monogram ─────────────────────────────────────────────────────────────────
/**
 * 1-2 letter monogram tile used to brand workspaces / projects / plugins.
 * For person identity use the `avatar` node (coming separately). When
 * `initials` is omitted the renderer derives them from `name`.
 */
export interface FormNodeMonogram extends FormNodeBase {
  type:      'monogram';
  /** Used for the tooltip and as the source for auto-derived initials. */
  name:      string;
  /** Override the auto-derived initials (e.g. show just one letter). */
  initials?: string;
  /** Any CSS color or `var(--…)` reference; defaults to `var(--accent)`. */
  color?:    string;
  /** Pixel size of the shorter edge. Reasonable range: 12-48. */
  size?:     number;
  variant?:  'square' | 'circle' | 'outline' | 'dot';
  /** Greyed-out look — used to indicate disabled/unavailable items. */
  disabled?: boolean;
  /** Foreground override (for square/circle/outline). */
  fg?:       string;
  /** Tooltip override; falls back to `name`. */
  tooltip?:  string;
}

// ─── state_block ──────────────────────────────────────────────────────────────
/**
 * Centered block-level status message for a content pane (loading / error /
 * empty / success). Stretches to fill its parent unless `fill = false`.
 */
export interface FormNodeStateBlock extends FormNodeBase {
  type:    'state_block';
  tone?:   'loading' | 'error' | 'success' | 'info' | 'neutral';
  label?:  string;
  /** When `tone === "loading"`, shows a built-in spinner instead of the
   *  default tone icon. Other tones ignore it. */
  spinner?: boolean;
  /** Override the default tone icon (Lucide name). */
  icon?:    string;
  fill?:    boolean;
}

// ─── step_indicator ───────────────────────────────────────────────────────────
/**
 * Wizard-style step navigation breadcrumb. Distinct from the `wizard`
 * container node — this one is a pure VISUAL indicator without the
 * children-routing. Useful for plugin onboarding / setup screens that
 * own their step navigation.
 */
export interface FormNodeStepIndicatorStep {
  id:    string;
  label: string;
  /** Optional Lucide icon name shown in pending/active state. */
  icon?: string;
}

export interface FormNodeStepIndicator extends FormNodeBase {
  type:    'step_indicator';
  steps:   FormNodeStepIndicatorStep[];
  current: string;
  layout?: 'horizontal' | 'vertical';
  size?:   'sm' | 'md';
  variant?: 'flat' | 'pill';
  separator?: boolean;
  collapse_labels?: boolean;
  /** Fired with `{ id, index }` when the user clicks a step. Click-back
   *  is allowed for done + active steps by default. */
  action?: string;
}

// ─── status_list ──────────────────────────────────────────────────────────────
/**
 * Itemised "preview before bulk action" panel — header with summary pills,
 * scrollable body of rows, optional footnote. Display-only; the plugin
 * recomputes `items` and patches the node when the underlying state changes.
 */
export interface FormNodeStatusListChip {
  severity: 'block' | 'warn' | 'info' | 'success';
  text:     string;
  /** Lucide icon name shown before the text. */
  icon?:    string;
}

export interface FormNodeStatusListItem {
  id:    string;
  label: string;
  chips: FormNodeStatusListChip[];
}

export interface FormNodeStatusList extends FormNodeBase {
  type:           'status_list';
  items:          FormNodeStatusListItem[];
  /** Total considered (≥ items with chips). Drives the "N of M" header. */
  total_count?:   number;
  scanning?:      boolean;
  scanning_label?: string;
  clean_label?:   string;
  /** Drives the default header/clean copy. */
  noun?:          { singular: string; plural: string };
  footnote?:      string;
  /** Pixel cap on the scrolling list. Default: 160. */
  max_list_height?: number;
}

// ─── copy_button ──────────────────────────────────────────────────────────────
/**
 * Click-to-copy button with chrome (border, hover). Mirrors the host's
 * `<CopyButton>` widget. Distinct from `copy_link` — `copy_link` is a
 * subtle inline pseudo-link with a glyph; `copy_button` is a standalone
 * action button (icon square or icon + label).
 *
 * The value is copied client-side via the browser clipboard API — no
 * plugin action round-trip. Pass an absolute string in `value`; for
 * computed values (rare), copy via a plugin action instead.
 */
export interface FormNodeCopyButton extends FormNodeBase {
  type:           'copy_button';
  /** The string copied to the clipboard on click. */
  value:          string;
  /** `icon` (default) renders a 22×22 square icon-only button; `inline`
   *  renders a leading icon + label. */
  variant?:       'icon' | 'inline';
  /** Inline label (default "Copy"). Ignored when `variant === "icon"`. */
  label?:         string;
  /** Inline label shown in the success state (default "Copied"). */
  copied_label?:  string;
  /** Tooltip text. Default "Copy to clipboard". */
  tooltip?:       string;
  /** Toast text shown on successful copy. Omit to suppress the toast. */
  toast_success?: string;
  /** Show a generic error toast on copy failure. Default true. */
  show_error_toast?: boolean;
}

// ─── experimental_badge ───────────────────────────────────────────────────────
/**
 * Small "Experimental" pill — flag features that are still being shaped.
 * Soft amber→coral gradient with a flask icon. Designed for modal headers
 * (`md`) and list rows (`sm`).
 */
export interface FormNodeExperimentalBadge extends FormNodeBase {
  type:         'experimental_badge';
  /** Tooltip title. Default "Experimental". */
  title?:       string;
  /** Longer description shown under the tooltip title. */
  description?: string;
  /** `md` (default) for modal headers; `sm` for list rows. */
  size?:        'sm' | 'md';
  /** Override the visible label. Default "Experimental". */
  label?:       string;
}

// ─── section_header ───────────────────────────────────────────────────────────
/**
 * Standalone section title bar — just the headline + optional secondary
 * description, without wrapping any children. Distinct from the `section`
 * container which has its own body. Use `section_header` to anchor a
 * region whose body is laid out by sibling nodes (e.g. a settings page
 * where the heading sits above a free-form layout).
 */
export interface FormNodeSectionHeader extends FormNodeBase {
  type:         'section_header';
  title:        string;
  description?: string;
}

// ─── filter_button ────────────────────────────────────────────────────────────
/**
 * Action-only chip-style filter button. Renders the same pill chrome as the
 * built-in `<FilterButton>` widget (rounded, accent when active, optional
 * count badge), but clicking it fires a plugin action instead of opening a
 * dropdown panel.
 *
 * Not value-bearing: the active/inactive look is driven by the `active` flag
 * (and `count > 0`) in the node config, which the plugin flips at runtime via
 * `arbor.ui.form.patch({ id = "…", merge = { active = … } })`. This keeps
 * ephemeral filter state out of `values` — the plugin owns it.
 */
// ─── bottom_panel_header ──────────────────────────────────────────────────────
/**
 * Title bar styled like the host's `<BottomPanelHeader>` — the chrome that
 * sits at the top of a bottom-docked panel (build output, run console, …).
 * Standalone header: icon + uppercase title + count badge + optional inline
 * `children` (status / tab strip) + right-aligned action slot + a mac-style
 * close affordance when `close_action` is set.
 *
 * Distinct from `panel_shell`: that one is a full panel wrapper with body
 * and footer; `bottom_panel_header` is just the header bar — pair it with
 * sibling layout nodes when the host owns the body.
 */
export interface FormNodeBottomPanelHeader extends FormNodeBase {
  type:        'bottom_panel_header';
  title?:      string;
  /** Lucide icon name shown in the accent slot to the left of the title. */
  icon?:       string;
  /** Optional count badge after the title (visible when > 0). */
  count?:      number;
  /** Inline content placed after the title and before the spacer. Use for
   *  status lines / breadcrumb / tab strips. */
  children?:   FormNode[];
  /** Right-aligned action nodes, just before the close button. Typically
   *  `button` / `menu_button` with `class = "ps-btn"`. */
  actions?:    FormNode[];
  /** When set, renders the close button on the far right; clicking fires
   *  this plugin action. When omitted, the close button is hidden. */
  close_action?: string;
}

// ─── panel_shell ──────────────────────────────────────────────────────────────
/**
 * Panel chrome wrapper — same look as the host's `<PanelShell>` widget used
 * by every sidebar / main panel (issues, branches, reflog, plugin panels …).
 * Renders:
 *   · a header row (icon + uppercase title + count badge + right-aligned
 *     action buttons),
 *   · an optional toolbar row below the header (filters / tabs / search …),
 *   · a scrollable body that hosts the form children,
 *   · an optional fixed footer.
 *
 * Useful inside an `arbor.ui.add_view` body or any plugin modal that wants
 * to mirror the IntelliJ-style panel chrome rather than the looser form
 * default flow. Display-only (not value-bearing) — child nodes carry their
 * own values like anywhere else.
 *
 * Pick `variant` to switch chrome:
 *   - `"plain"` (default) — transparent header, no border around the panel
 *     (blends inside a modal / parent surface).
 *   - `"plugin"` — floating-card look: elevated header bar, rounded outer
 *     border, body rendered as a `--bg-base` inset card. Equivalent of the
 *     `plugin-panel-shell` class the Plugin Manager and view body use.
 */
export interface FormNodePanelShell extends FormNodeBase {
  type:        'panel_shell';
  title:       string;
  /** Lucide icon name shown in the accent slot to the left of the title. */
  icon?:       string;
  /** Optional count badge after the title (> 0 to be visible). */
  count?:      number;
  /** Right-aligned action nodes on the header row (typically `button` /
   *  `menu_button` with `class = "ps-btn"`). */
  actions?:    FormNode[];
  /** Optional second-row content below the header — search input, filter
   *  chips, tab bar, etc. */
  toolbar?:    FormNode[];
  /** Main body. Scrolls inside the panel when overflow exceeds the body
   *  height (unless `scrollable = false`). */
  children:    FormNode[];
  /** Optional fixed footer below the scrollable body. */
  footer?:     FormNode[];
  /** Body scrolls. Default `true`. */
  scrollable?: boolean;
  /** Skip the default header (when an outer chrome owns the title bar).
   *  Default `false`. */
  hide_header?: boolean;
  /** Visual variant. Default `"plain"`. */
  variant?:    'plain' | 'plugin';
}

export interface FormNodeFilterButton extends FormNodeBase {
  type:     'filter_button';
  label:    string;
  /** Lucide icon name shown before the label. */
  icon?:    string;
  /** Numeric badge shown after the label; > 0 also forces the active look
   *  unless `active` is set explicitly. */
  count?:   number;
  /** Active-state override. When unset, falls back to `count > 0`. */
  active?:  boolean;
  /** Plugin action fired on click. Merges `extra` into the payload. */
  action:   string;
  /** Extra data merged into the action payload. */
  extra?:   Record<string, unknown>;
}

// ─── color_swatch ─────────────────────────────────────────────────────────────
/**
 * Display-only colour swatch — chip-only or labelled card row. Mirrors the
 * host's `<ColorSwatch>` widget used in the Marketplace palette and
 * theme-preview surfaces. Distinct from the value-bearing `color` field
 * (HTML5 colour input) — `color_swatch` is presentational only: the plugin
 * supplies the `color` value (any CSS expression — hex, `rgb()`, `var(--…)`,
 * `color-mix(...)`) and the chip renders accordingly. To edit a swatch, pair
 * it with a sibling `color` field and patch the swatch's `color` from the
 * field's `change` action.
 *
 * When `label` is set the widget renders as a labelled card row
 * `[chip] Label   #caption`; when `label` is absent only the chip is
 * rendered (use this inside a custom grid where the label lives elsewhere).
 * Set `glyph` (a single character like `"#"`, `"n"`, `"T"`) to render a
 * centred marker instead of a colour fill — useful when the swatch doubles
 * as a typed-token indicator.
 */
export interface FormNodeColorSwatch extends FormNodeBase {
  type:        'color_swatch';
  /** Any CSS colour value — hex, `rgb()`, `var(--token)`, `color-mix(...)`, … */
  color:       string;
  /** Display name. When set, renders as a labelled card row; when absent,
   *  only the chip is rendered. */
  label?:      string;
  /** Right-hand caption in labelled mode. Defaults to the raw `color`. */
  caption?:    string;
  /** Hide the caption in labelled mode. */
  no_caption?: boolean;
  /** Chip width/height in px. Defaults to 18 (labelled) / 22 (chip-only). */
  chip_size?:  number;
  /** Tooltip override; defaults to the colour value. */
  tooltip?:    string;
  /** Single-character marker shown instead of the colour fill. Used for
   *  non-colour tokens (e.g. `"#"` for lengths, `"n"` for numbers,
   *  `"T"` for typography). */
  glyph?:      string;
}

// ─── kbd ──────────────────────────────────────────────────────────────────────
/**
 * Display-only keybinding badge — same chrome as the host's `<Kbd>` widget
 * used throughout Shortcuts / Command Palette / footer hints. Renders a
 * boxed monospace badge per chord part (`box`, default) or plain inline
 * monospace text (`inline`, IntelliJ-menu style).
 *
 * Resolution priority: `action` (looked up live in the user's
 * keybindings — remaps in Settings flow through) → `binding` (explicit
 * `{ key, modifiers }` object) → `keys` (array of chord parts like
 * `["Ctrl", "K"]`) → `label` (single string, split on `+`).
 *
 * When `action` resolves to nothing (action not registered, or its binding
 * is cleared) the widget renders nothing — safe to drop next to a label
 * without guarding it.
 */
export interface FormNodeKbd extends FormNodeBase {
  type:     'kbd';
  /** Built-in or plugin-registered action id; resolved live against the
   *  user's keybindings. */
  action?:  string;
  /** Explicit keybinding object: `{ key, ctrl?, shift?, alt? }`. Wins over
   *  `keys` / `label` when set. */
  binding?: { key: string; ctrl?: boolean; shift?: boolean; alt?: boolean };
  /** Single label like `"Ctrl+K"`; split on `+` if `keys` isn't supplied. */
  label?:   string;
  /** Explicit chord parts. Wins over `label`. */
  keys?:    string[];
  /** Badge size. Default `"md"`. */
  size?:    'sm' | 'md';
  /** Visual tone. Default `"default"`. `"accent"` tints with `--accent`;
   *  `"muted"` drops the chrome to a dim hint. */
  tone?:    'default' | 'accent' | 'muted';
  /** `"box"` (default) → boxed `<kbd>` badges; `"inline"` → plain monospace
   *  text without border / bg (IntelliJ-menu style). */
  variant?: 'box' | 'inline';
}

// ─── type_pill ────────────────────────────────────────────────────────────────
/**
 * Display-only uppercase type pill — same chrome as the host's `<TypePill>`
 * widget used in component cards (Bevy-style reflection panels) and field
 * rows. Tags a value with a one-word type hint (`Vec3`, `Quat`, `u32`,
 * `enum`, `Handle`, …) without taking real estate.
 *
 * Two ways to drive the colour:
 *   · `kind` picks from a curated palette keyed by the type bucket
 *     (numeric / vector / bool / enum / handle / entity / option /
 *      string / array / struct / unknown). Case-insensitive.
 *   · `tone` is the explicit semantic override (`accent`, `info`,
 *      `success`, `warning`, `error`, `muted`) — wins over `kind`.
 */
export interface FormNodeTypePill extends FormNodeBase {
  type:     'type_pill';
  /** Visible text. When omitted, the resolved `kind` is shown as-is. */
  label?:   string;
  /** Curated kind — picks a palette. Case-insensitive; unknown values
   *  fall through to a neutral / dim look. */
  kind?:    string;
  /** Explicit tone override. Wins over `kind`. */
  tone?:    'accent' | 'info' | 'success' | 'warning' | 'error' | 'muted';
  /** Tooltip on hover. */
  tooltip?: string;
}

// ─── encoding_pill ────────────────────────────────────────────────────────────
/**
 * Display-only charset indicator — same chrome as the host's
 * `<EncodingPill>` used in the diff toolbar / file-list rows. Renders the
 * encoding label inside a small monospace pill; warning-tinted when
 * `overridden` is true to surface that the user pinned a non-auto value.
 *
 * Presentational only — the plugin owns the encoding string and the
 * override flag. Pair with a sibling `select` field to let the user pick a
 * charset, then patch the pill to reflect the choice.
 */
export interface FormNodeEncodingPill extends FormNodeBase {
  type:        'encoding_pill';
  /** Encoding label currently in effect (e.g. `"UTF-8"`, `"windows-1252"`). */
  encoding:    string;
  /** True when the user has pinned a non-auto encoding — drives the
   *  warning tint. */
  overridden?: boolean;
  /** Compact 14px variant for cramped headers. Default false. */
  compact?:    boolean;
}

// ─── avatar ───────────────────────────────────────────────────────────────────
/**
 * Display-only round avatar — same chrome as the host's `<Avatar>` widget
 * used for committer rows / reviewer chips. Derives initials from `name`
 * (first letter of the first two words) and a stable hue from the bytes of
 * `email` (preferred) or `name`. Tooltip is `name` + optional `email`
 * description. Distinct from `monogram`, which is square / outline-styled
 * and meant for workspaces and plugins (entities), not people.
 */
export interface FormNodeAvatar extends FormNodeBase {
  type:    'avatar';
  /** Display name — also the source of the initials when no other text is
   *  supplied. */
  name?:   string;
  /** Email address — preferred hue source; appears in the tooltip
   *  description. */
  email?:  string;
  /** Avatar diameter in px. Default 24. */
  size?:   number;
}

// ─── brand_icon / brand_tile ─────────────────────────────────────────────────
/** Provider brand identifier shared by `brand_icon` and `brand_tile`. */
export type ProviderBrandName = 'github' | 'gitlab' | 'bitbucket' | 'linear' | 'jira';

/**
 * Display-only monochrome brand glyph — same chrome as the host's
 * `<BrandIcon>`. Renders the canonical `simple-icons` mark in
 * `currentColor`, so it inherits the surrounding text colour (sidebar /
 * toolbar / activity bar). Use this when a coloured brand square would
 * clash with the rest of the icon set; for owned-swatch surfaces (auth
 * tiles, settings cards, welcome screens) use `brand_tile` instead.
 */
export interface FormNodeBrandIcon extends FormNodeBase {
  type:    'brand_icon';
  /** Provider brand to render. */
  brand:   ProviderBrandName;
  /** Pixel size of the glyph. Default 20. */
  size?:   number;
  /** Override the title attribute / tooltip (defaults to the capitalised
   *  brand name). */
  title?:  string;
}

/**
 * Display-only branded square tile — same chrome as the host's
 * `<BrandTile>`. Composes the canonical `simple-icons` mark on the brand's
 * hard-coded background colour (`#24292e` GitHub, `#fc6d26` GitLab,
 * `#0052cc` Bitbucket / Jira, `#5e6ad2` Linear) with a fixed bright
 * foreground — brand contrast does NOT borrow theme tokens. Use this for
 * auth tiles, settings provider cards, welcome screens. For monochrome
 * brand marks that inherit the surrounding text colour (activity bar,
 * inline glyphs) use `brand_icon` instead.
 */
export interface FormNodeBrandTile extends FormNodeBase {
  type:      'brand_tile';
  /** Provider brand to render. */
  brand:     ProviderBrandName;
  /** Pixel size of the inner glyph. Default 20. */
  size?:     number;
  /** Pixel size of the outer square. Defaults to `max(size + 16, 36)`. */
  tile_size?: number;
  /** Greyed-out look — used to indicate disabled / unavailable items. */
  disabled?: boolean;
  /** Override the title attribute / tooltip (defaults to the capitalised
   *  brand name). */
  title?:    string;
}

// ─── provider_user_badge ──────────────────────────────────────────────────────
/**
 * Display-only two-line user identity row — same chrome as the host's
 * `<ProviderUserBadge>` used in the provider settings cards (GitHub /
 * GitLab / Linear / Jira account rows). Avatar (or circled monogram of the
 * first initial when no URL) + primary name line + optional secondary line
 * (email / @handle / domain). When `copyable` is true (the default) both
 * lines are click-to-copy with a hover affordance and a transient ✓
 * confirmation.
 *
 * Presentational only — the plugin owns the data. Pair with `arbor.http.*`
 * to fetch the user, then push the value into the form. To make the badge
 * non-interactive set `copyable = false`.
 */
export interface FormNodeProviderUserBadge extends FormNodeBase {
  type:        'provider_user_badge';
  /** Primary line — typically display name or login. */
  name:        string;
  /** Avatar URL — falls back to a circled monogram of the first initial. */
  avatar_url?: string;
  /** Secondary line — email, domain, @handle, … */
  secondary?:  string;
  /** When true (default), clicking the name / secondary copies it to the
   *  clipboard. */
  copyable?:   boolean;
}

// ─── tooltip ──────────────────────────────────────────────────────────────────
/**
 * Wraps one or more child nodes with a hover/focus tooltip — same singleton
 * popover the host uses everywhere (smart placement, viewport-aware flipping,
 * keyboard focus, optional shortcut hint, optional Markdown body). The wrapper
 * is purely behavioural: it renders an extra element around its children so
 * the tooltip action has somewhere to attach.
 *
 * Display defaults to `"inline"` (a `<span>` with `display: inline-block`) —
 * ideal for wrapping a `button`, `monogram`, `copy_button`, `icon`, badge, or
 * any other inline-sized widget. Set `display = "block"` to render a `<div>`
 * wrapper instead — needed when wrapping a block-level subtree like a
 * `section`, `panel_shell`, `info_card`, or any node that paints with its own
 * width / margins.
 *
 * The tooltip is fired by hover and by keyboard focus on any focusable
 * descendant (buttons, links, focusable inputs). Children render at their
 * normal size; the wrapper adds no chrome of its own.
 */
export interface FormNodeTooltip extends FormNodeBase {
  type:         'tooltip';
  children:     FormNode[];
  /** Primary tooltip text. Required — the wrapper is a no-op when empty. */
  content:      string;
  /** Secondary line shown dimmer / smaller under `content`. */
  description?: string;
  /** Keyboard shortcut hint rendered as `<kbd>` chips. Either a "+"-joined
   *  string (`"Ctrl+K"`) or an explicit array (`["Ctrl", "K"]`). */
  shortcut?:    string | string[];
  /** Preferred side; the renderer auto-flips on viewport collision. Default
   *  `"auto"` (top, then bottom / right / left). */
  placement?:   'top' | 'bottom' | 'left' | 'right' | 'auto';
  /** Open delay in ms on hover. Default 350. Focus opens immediately. */
  delay?:       number;
  /** Distance in px between the trigger and the tooltip. Default 8. */
  offset?:      number;
  /** Max width in px. Default 320. */
  max_width?:   number;
  /** Max height in px; longer content is clipped with a fade. Default 280. */
  max_height?:  number;
  /** Render `content` as sanitised Markdown. Default false (plain text). */
  markdown?:    boolean;
  /** Wrapper element. `"inline"` (default) renders a `<span>` with
   *  `display: inline-block`; `"block"` renders a `<div>` and is required when
   *  wrapping a block-level subtree (section, panel_shell, info_card, …). */
  display?:     'inline' | 'block';
}

export type FormLayoutNode =
  | FormNodeContainer
  | FormNodeRow
  | FormNodeSection
  | FormNodeSeparator
  | FormNodeParagraph
  | FormNodeAlert
  | FormNodeCode
  | FormNodeButton
  | FormNodeLabel
  | FormNodeDivider
  | FormNodeSwitch
  | FormNodeTabs
  | FormNodeWizard
  | FormNodeCardRow
  | FormNodeCardGrid
  | FormNodePropertyGrid
  | FormNodeCfgList
  | FormNodeSuggestGrid
  | FormNodeMenuButton
  | FormNodeTreeLayout
  | FormNodePipelineEditor
  | FormNodeCounterGrid
  | FormNodeScoreGauge
  | FormNodeTimeSeriesChart
  | FormNodeDataTable
  | FormNodeFilterBar
  | FormNodeFormField
  | FormNodeInfoCard
  | FormNodeChipBar
  | FormNodeBreadcrumb
  | FormNodeUrlBlock
  | FormNodeMonogram
  | FormNodeStateBlock
  | FormNodeStepIndicator
  | FormNodeStatusList
  | FormNodeCopyButton
  | FormNodeExperimentalBadge
  | FormNodeSectionHeader
  | FormNodeFilterButton
  | FormNodePanelShell
  | FormNodeBottomPanelHeader
  | FormNodeTooltip
  | FormNodeColorSwatch
  | FormNodeKbd
  | FormNodeTypePill
  | FormNodeEncodingPill
  | FormNodeAvatar
  | FormNodeBrandIcon
  | FormNodeBrandTile
  | FormNodeProviderUserBadge
  | FormNodeDiff;

export type FormNode = FormFieldNode | FormLayoutNode;

// ─── Studio-shaped modal sub-configs ──────────────────────────────────────────

/**
 * Icon shown next to the title in the modal header. Exactly one variant must be
 * present — `lucide` (Lucide icon name), `brand` (provider brand id), or
 * `image` (URL: `file://`, `data:`, `https://`). Raw SVG markup is intentionally
 * not accepted to avoid XSS surface from plugin-supplied HTML.
 */
export type FormHeaderIcon =
  | { lucide: string }
  | { brand:  ProviderBrandName }
  | { image:  string };

/**
 * Optional header zone for `arbor.ui.form{...}`. When set, replaces the default
 * `<ModalHeader>` chrome (plugin tag + title + close button) with a richer
 * header that mirrors the Studio modals (icon + title + meta + left/centre/
 * right snippet zones). Subkey defaults all reduce to "render nothing".
 */
export interface FormHeaderCfg {
  /** Pictogram before the title. See FormHeaderIcon for accepted variants. */
  icon?:     FormHeaderIcon;
  /** Secondary single-line caption shown next to the title (muted). */
  subtitle?: string;
  /** Render a `●` dirty marker after the title when true. */
  dirty?:    boolean;
  /** Tooltip on the title (typically the full file path). */
  tooltip?:  string;
  /** Right-aligned meta pill rendered after the title (e.g. "12.4 KB · 412 lines"). */
  size_meta?: string;
  /** FormNodes rendered after the title cluster, before the centre zone. */
  left?:     FormNode[];
  /** FormNodes rendered in the centre — typically a `tabs` for view-mode switching. */
  centre?:   FormNode[];
  /** FormNodes rendered before the host-owned close button. */
  right?:    FormNode[];
  /** When set, render an ExperimentalBadge next to the title. */
  experimental?: { description: string };
}

/**
 * One item in the modal's right (or left) activity bar. Activity-bar items
 * are ROUTING-ONLY — clicking one opens / focuses the sidecar with the same
 * `id`. Items that need to fire an action (Open file…, Save As…) belong in
 * `header.left` / `header.right` as `button` FormNodes.
 */
export type FormActivityBarItem =
  | {
      /** Stable id; must match a key in `sidecars`. */
      id:       string;
      /** Lucide icon name. */
      icon:     string;
      /** Sidecar label (shown as tooltip and aria-label). */
      label:    string;
      /** Optional override tooltip (defaults to `label`). */
      tooltip?: string;
      /** Numeric badge shown on the icon (omit / 0 = hidden). */
      count?:   number;
      /** Accent dot for "has unread content / dirty pane". */
      dot?:     boolean;
      /** Override the badge / dot tone. */
      tone?:    'info' | 'success' | 'warning' | 'error' | 'accent' | 'muted';
      disabled?: boolean;
    }
  | {
      /** Thin separator line between groups in the bar. */
      separator: true;
    };

export interface FormActivityBarCfg {
  /** Which side of the modal the bar lives on. Default: 'right'. */
  side?:    'left' | 'right' | 'both';
  /** Items when `side` is 'left' or 'right'. */
  items?:   FormActivityBarItem[];
  /** Items when `side` is 'both'. */
  left_items?:  FormActivityBarItem[];
  right_items?: FormActivityBarItem[];
  /** Which item id is active on first mount (no localStorage history). */
  default?: string;
  /**
   * When set, the active sidecar id is mirrored to `localStorage[storage_key]`
   * — the user's selection survives across modal opens.
   */
  storage_key?: string;
  /** When true, one item is always selected (cannot close to null). Default: false. */
  always_open?: boolean;
}

/**
 * Sidecar pane keyed by activity-bar item id. Rendered as an animated slide-in
 * panel to the right (or left) of the body. The pane is a FormNode subtree —
 * value-bearing nodes participate in the same submit payload as the body
 * (collisions on `name` are a plugin error: last-write-wins + warning).
 */
export interface FormSidecarCfg {
  /** Pixel width of the pane. Default: 320. */
  width?:    number;
  /** Optional header line above the pane contents. */
  title?:    string;
  /**
   * Which edge the pane slides in from. Default: `'right'` (back-compat with
   * the RON/JSON-style sidecars). Use `'left'` to anchor the pane next to a
   * left-side activity bar (e.g. an entity navigator) — it renders before the
   * main body and borders on its right edge.
   */
  side?:     'left' | 'right';
  /** Pane contents as FormNodes. */
  children:  FormNode[];
}

/**
 * Modal footer override. Each zone is a FormNode list rendered horizontally;
 * unset zones fall through to the default chrome (Submit/Cancel + wizard).
 */
export interface FormFooterCfg {
  /** Left status row — typically `state_block_pill` + `breadcrumb`. */
  status?: FormNode[];
  /** Centre — typically undo/redo + Format/Convert tool buttons. */
  center?: FormNode[];
  /**
   * Right — replaces the default Submit/Cancel/wizard cluster. Pass an empty
   * array to render no right-side controls.
   */
  right?:  FormNode[];
}

/**
 * Optional full-body fallback state — when any subkey is set, the body
 * `nodes` are hidden and the matching block is rendered instead. Flip in/out
 * live with `arbor.ui.form.set_state_block(name, cfg)`.
 */
export interface FormStateBlockCfg {
  /** Spinner overlay with a label. Equivalent to top-level `loading = true`. */
  loading?: { label?: string };
  /** Error block with a tone="error" StateBlock. */
  error?:   { label: string };
  /** Empty-doc state with optional CTA. */
  empty?:   {
    title?:      string;
    body?:       string;
    cta_label?:  string;
    cta_action?: string;
  };
}

// ─── Top-level config ─────────────────────────────────────────────────────────

export interface PluginFormConfig {
  plugin_name:  string;
  title:        string;
  description?: string;
  nodes:        FormNode[];
  css?:         string;
  submit_label?:  string;
  submit_action:  string;
  cancel_label?:  string;
  cancel_action?: string;
  width?:         string;
  height?:        string;
  /** Enable two-column sidebar layout. The first root `tabs` node becomes the left nav. */
  sidebar?:       boolean;
  /**
   * Studio-shaped header (icon + title + meta + left/centre/right zones).
   * When absent, the default `<ModalHeader>` (plugin tag + title) is used.
   */
  header?:        FormHeaderCfg;
  /**
   * Right (or left, or both) activity bar with routing-only items. Each item's
   * `id` must match a key in `sidecars`.
   */
  activity_bar?:  FormActivityBarCfg;
  /**
   * Sidecar panes keyed by activity-bar item id. Each pane is a FormNode
   * subtree; its value-bearing nodes participate in the modal's shared values
   * payload (collisions on `name` across regions are a plugin error).
   */
  sidecars?:      Record<string, FormSidecarCfg>;
  /**
   * Three-zone footer (status / center / right). Unset zones fall through to
   * the default Submit/Cancel/wizard chrome.
   */
  footer?:        FormFooterCfg;
  /**
   * Optional fallback state(s) shown in place of the body — loading / error /
   * empty. Mutually exclusive at render time (first set wins). Live-updatable
   * via `arbor.ui.form.set_state_block(name, cfg)`.
   */
  state_block?:   FormStateBlockCfg;
  /**
   * When true, the modal stays open after the submit handler runs. Use
   * this when the submit triggers a follow-up flow (file picker, confirm
   * dialog, second form) and you want the original form to remain on
   * screen until the flow completes — typically the plugin then calls
   * `arbor.ui.form.close()` on success. Default: false (form closes on
   * submit, current behaviour).
   */
  keep_open?:     boolean;
  /**
   * Opaque state table echoed back unchanged as `ctx.state` in submit and
   * button-action handlers. Not rendered in the form. Use instead of hidden fields.
   */
  state?: Record<string, unknown>;
  /**
   * When true, the modal renders a translucent overlay with a centered
   * spinner above the form body — useful for plugins that fan out to the
   * network after opening the form (e.g. fetching per-repo summaries
   * before the dashboard has data to draw). Toggle live by passing
   * `loading` inside `arbor.ui.form.replace({ loading, nodes })`, or by
   * the focused `arbor.ui.form.set_loading(...)` API which only updates
   * the overlay without re-rendering the node tree (preferred for
   * per-step progress ticks during a fan-out loop).
   */
  loading?: boolean;
  /** Custom label for the loading overlay. Defaults to "Loading…". Update
   *  live via `arbor.ui.form.set_loading({ loading = true, label = "..." })`. */
  loading_label?: string;
  /** Hide the Submit button in the footer (e.g. read-only inspector forms
   *  that only need a Close button). */
  hide_submit?:   boolean;
  /** Hide the Cancel button in the footer. */
  hide_cancel?:   boolean;
}

// ── Confirm dialog — emitted via "plugin:confirm" ────────────────────────────

export interface PluginConfirmConfig {
  plugin_name:      string;
  message:          string;
  confirm_label?:   string;
  confirm_variant?: 'default' | 'primary' | 'danger' | 'ghost';
  confirm_action:   string;
  cancel_action?:   string;
  state?:           Record<string, unknown>;
}
