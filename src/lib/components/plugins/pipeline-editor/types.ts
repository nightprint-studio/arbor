// Pipeline editor shared types — extracted from PluginPipelineEditor.svelte
// during the Phase 4 god-object refactor. See project_god_objects_refactor.md.
//
// These are the shapes the plugin emits inside `editor` (a FormNode of type
// `pipeline_editor`). The host renderer doesn't validate them; if the plugin
// ships malformed data the editor will silently fall back to empties.

import type { FormNode } from '$lib/types/plugin';

export type Op = {
  kind:     string;
  label:    string;
  icon?:    string;
  summary?: string;
  category?: string;
};

export type OpCategory = {
  id:    string;
  label: string;
  ops:   Op[];
};

export type Step = {
  id: string;
  name: string;
  kind: string;
  allow_failure?: boolean;
  /** Marks a step that has nested steps (typically `if_block`). The
   * editor renders an "open" arrow on the step row that fires the
   * `enter_step` action so the plugin can drill the user into the body. */
  has_body?: boolean;
  /** Optional summary line shown below the step name (e.g. the
   * condition expression for an `if_block` step). */
  summary?: string;
  /** When the step is an `if_block`, the plugin populates `branches`
   * (the if + elif's, in order) so the editor renders them as inline
   * collapsible subgroups underneath the step row. Each branch
   * recursively contains nested `Step`s — the editor reuses the same
   * row snippet at any depth. */
  branches?: Branch[];
  /** Optional `else` branch — same shape as a regular branch, but the
   * editor hides the condition input. Plugins drop this field entirely
   * when no else clause exists. */
  else_branch?: Branch | null;
};

export type Branch = {
  /** Globally-unique id within the editor tree (typically
   * `<step_id>/<branch_idx>` or `<step_id>/else`). */
  id:         string;
  /** "if" / "elif" / "else" — used as the header text. */
  label:      string;
  /** Free-form condition string (the new parser language). Empty for
   * the `else` branch. Edited via the inline `<input>` in the branch
   * header; commits on blur or Enter. */
  expression?: string;
  collapsed?: boolean;
  steps:      Step[];
};

export type Stage = {
  id: string;
  name: string;
  mode?: 'sequential' | 'parallel';
  max_parallel?: number | null;
  steps: Step[];
};

/** One crumb in the drill-down path. Plugins emit the full chain
 * (root → current level) every render; clicking a crumb fires
 * `navigate_to` with its index. The last crumb is the current level
 * and is rendered as plain text (not clickable). */
export type Crumb = { label: string; icon?: string };

export interface EditorProps {
  id?:               string;
  stages:            Stage[];
  operations:        OpCategory[];
  search_query?:     string;
  selected_step_id?: string;
  selected_stage_id?:string;
  /** Path of the currently-selected `if_block` branch (typically
   * `<step_id>/<branch_idx>` or `<step_id>/else`). When set, the editor
   * renders that branch with the accent border so the user knows
   * "the next palette click goes here". The plugin owns the
   * interpretation; the editor only echoes the value back via
   * `select_branch`. */
  selected_branch_id?: string;
  step_detail_form?: FormNode[];
  empty_label?:      string;
  /** Optional drill-down breadcrumb. When present and longer than 1,
   * the editor renders it above the sequence column with click-to-pop
   * navigation via the `navigate_to` action. */
  breadcrumb?:       Crumb[];
  /** When `true`, the "+ stage" button is hidden (typically while
   * drilled into an if-block body, where stages aren't a meaningful
   * unit). Stage-level actions on existing rows are still available. */
  hide_add_stage?:   boolean;
  actions:           Record<string, string>;
}

export type FireAction = (action: string, extra?: Record<string, unknown>) => void;
export type FireKey    = (actionKey: string, extra?: Record<string, unknown>) => void;
