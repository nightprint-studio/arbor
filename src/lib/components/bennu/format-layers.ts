/**
 * The formatter style's layers, as pure functions — no store, no IPC, so the precedence rules the
 * settings page shows can be tested on their own.
 *
 * The backend resolves the same order (`FormatStyle::layered`):
 * `defaults ← global ← detected (if auto_detect) ← .editorconfig (if use_editorconfig) ← project`.
 * The page repeats it locally so an edit is reflected at once, without a round trip per keystroke.
 */
import type { FormatKey, FormatLayer, FormatPatch, FormatStyle } from '$lib/types/bennu-format';

/** The defaults, mirrored from `FormatStyle::default()`. The backend's answer wins whenever there
 *  is one; this is what the page draws before it has heard, and what the tests compare against. */
export const DEFAULT_FORMAT_STYLE: FormatStyle = {
  indent_width: 4,
  use_tabs: false,
  continuation_indent: 8,
  indent_case_body: true,
  align_wrapped_to_paren: false,
  max_blank_lines: 1,
  blank_lines_after_header: 0,
  blank_lines_before_member: 0,
  brace_type: 'preserve',
  brace_method: 'preserve',
  brace_block: 'preserve',
  brace_continuation: 'preserve',
  force_braces: false,
  normalize_spacing: false,
  space_before_control_paren: true,
  space_before_call_paren: false,
  space_before_lbrace: true,
  space_around_binary: true,
  space_around_assign: true,
  space_around_ternary: true,
  space_around_arrow: true,
  space_after_comma: true,
  space_before_comma: false,
  space_after_cast: true,
  space_within_parens: false,
  space_within_brackets: false,
  space_within_array_braces: false,
  space_within_angles: false,
  space_around_foreach_colon: true,
  line_width: 120,
  wrap_parameters: 'preserve',
  wrap_arguments: 'preserve',
  wrap_chain: 'preserve',
  chain_min_calls: 3,
  chain_keep_first_call: true,
  wrap_binary: 'preserve',
  wrap_ternary: 'preserve',
  wrap_clauses: 'preserve',
  wrap_array_init: 'preserve',
  operator_on_next_line: true,
  closing_paren_on_new_line: false,
  break_after_open_paren: false,
  organize_imports: false,
  import_groups: ['java', 'javax', '*'],
  blank_line_between_import_groups: true,
  static_imports: 'first',
  wildcard_threshold: 0,
  static_wildcard_threshold: 0,
  annotations_type: 'preserve',
  annotations_method: 'preserve',
  annotations_field: 'preserve',
  align_block_comments: false,
  space_after_line_comment: false,
  wrap_javadoc: false,
  ensure_final_newline: true,
  line_ending: 'auto',
  format_on_save: false,
  use_editorconfig: true,
  auto_detect: false,
};

/** Every layer the page knows about for the project at hand. */
export interface StyleLayers {
  defaults: FormatStyle;
  global: FormatStyle;
  detected: FormatPatch | null;
  editorconfig: FormatPatch | null;
  project: FormatPatch | null;
}

/** Value equality for a style value: scalars by `===`, the one list type element by element. */
export function sameValue(a: unknown, b: unknown): boolean {
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((x, i) => x === b[i]);
  }
  return a === b;
}

/** The keys on which `style` differs from `defaults` — the minimal override that reproduces it. */
export function diffFromDefaults(style: FormatStyle, defaults: FormatStyle = DEFAULT_FORMAT_STYLE): FormatPatch {
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(defaults) as FormatKey[]) {
    if (!sameValue(style[key], defaults[key])) out[key] = style[key];
  }
  return out as FormatPatch;
}

/** `base` with each layer laid over it, first to last (later wins). A layer names only the keys it sets. */
export function mergeLayers(base: FormatStyle, ...layers: (FormatPatch | null | undefined)[]): FormatStyle {
  const out: Record<string, unknown> = { ...base };
  for (const layer of layers) {
    if (!layer) continue;
    for (const [key, value] of Object.entries(layer)) {
      if (value !== undefined) out[key] = value;
    }
  }
  return out as unknown as FormatStyle;
}

/** The two behaviour flags that decide whether the detected / `.editorconfig` layers count. They are
 *  read from the layers the user controls, because those layers cannot set them. */
function flagsOf(l: StyleLayers): { autoDetect: boolean; useEditorconfig: boolean } {
  const own = mergeLayers(l.defaults, l.global, l.project);
  return { autoDetect: own.auto_detect, useEditorconfig: own.use_editorconfig };
}

/** The style that actually formats: all the layers that count, laid over the defaults. */
export function effectiveStyle(l: StyleLayers): FormatStyle {
  const { autoDetect, useEditorconfig } = flagsOf(l);
  return mergeLayers(
    l.defaults,
    l.global,
    autoDetect ? l.detected : null,
    useEditorconfig ? l.editorconfig : null,
    l.project,
  );
}

/** The style as it would be without the project's own override — what clearing an override falls back to. */
export function styleBelowProject(l: StyleLayers): FormatStyle {
  return effectiveStyle({ ...l, project: null });
}

/** Which layer a key's effective value comes from. `global` means the user's own non-default choice;
 *  `default` that nobody changed it. */
export function originOf(key: FormatKey, l: StyleLayers): FormatLayer {
  const { autoDetect, useEditorconfig } = flagsOf(l);
  if (l.project && key in l.project) return 'project';
  if (useEditorconfig && l.editorconfig && key in l.editorconfig) return 'editorconfig';
  if (autoDetect && l.detected && key in l.detected) return 'detected';
  return sameValue(l.global[key], l.defaults[key]) ? 'default' : 'global';
}
