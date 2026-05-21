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
}

export interface FormFieldText extends FormFieldBase {
  type:         'text' | 'password' | 'email' | 'url';
  placeholder?: string;
  default?:     string;
  /** Regex pattern for inline validation (Lua pattern on the backend, JS regex on frontend). */
  pattern?:     string;
  pattern_hint?: string;
}

export interface FormFieldTextarea extends FormFieldBase {
  type:         'textarea';
  placeholder?: string;
  default?:     string;
  rows?:        number;
}

export interface FormFieldNumber extends FormFieldBase {
  type:     'number';
  default?: number;
  min?:     number;
  max?:     number;
  step?:    number;
}

export interface FormFieldRange extends FormFieldBase {
  type:          'range';
  default?:      number;
  min?:          number;
  max?:          number;
  step?:         number;
  show_value?:   boolean;
  value_format?: string;
}

export interface FormFieldCheckbox extends FormFieldBase {
  type:     'checkbox';
  label:    string;
  default?: boolean;
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
}

export interface FormFieldRadio extends FormFieldBase {
  type:     'radio';
  default?: string;
  options:  FormOptionInput[];
  inline?:  boolean;
}

/** File / folder picker — opens the existing FilePickerModal on click. */
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
  /** Small inline pill badge shown after the label (e.g. "Tomcat"). */
  tag?:     string;
  /** Colour variant for `tag`. */
  tag_variant?: 'neutral' | 'ok' | 'warn' | 'error' | 'accent' | 'dev' | 'prod' | 'test';
  /** Optional dim caption under the label. */
  description?: string;
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
   */
  change_action?: string;
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

export type FormFieldNode =
  | FormFieldText
  | FormFieldTextarea
  | FormFieldNumber
  | FormFieldRange
  | FormFieldCheckbox
  | FormFieldToggle
  | FormFieldSelect
  | FormFieldMultiselect
  | FormFieldRadio
  | FormFieldColor
  | FormFieldKvList
  | FormFieldDate
  | FormFieldDateTime
  | FormFieldTime
  | FormFieldFile
  | FormFieldAutocomplete
  | FormFieldTags
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
  text:     string;
  variant?: 'info' | 'warning' | 'error' | 'success';
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
  action:       string;
  variant?:     'default' | 'primary' | 'danger' | 'ghost';
  close_after?: boolean;
  disabled?:    boolean;
  /** Extra data merged into the action payload alongside form values. Useful for item-specific actions in cfg_list / card_row. */
  extra?:       Record<string, unknown>;
  /** Optional Lucide icon shown before the label. */
  icon?:        string;
  /** Hide the label and render only the icon. */
  icon_only?:   boolean;
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
  /** Left-panel width. Default: "240px". */
  nav_width?:       string;
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
  /** When set, also fires this action with `{ name, value }` whenever the
   *  selection changes (useful when no parent uses `show_if`). */
  action?:  string;
  items:    ChipItem[];
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
  | FormNodeChipBar;

export type FormNode = FormFieldNode | FormLayoutNode;

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
