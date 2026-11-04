/**
 * The Java formatter's style — a mirror of `FormatStyle` in
 * `crates/products/bennu/editing/intentions/src/format/style.rs`. The wire names are the field names
 * and the enums travel as snake_case strings; change one side and the other in the same turn.
 */

/** Where a `{` goes relative to the construct that opens it. */
export type BraceStyle = 'preserve' | 'same_line' | 'next_line';
/** What to do with the line breaks inside one kind of list or expression. */
export type WrapPolicy = 'preserve' | 'never' | 'fill' | 'chop' | 'always';
/** Where the annotations of a declaration sit relative to the declaration itself. */
export type AnnotationPlacement = 'preserve' | 'own_line' | 'same_line';
/** Where `static` imports go in the import block. */
export type StaticImports = 'first' | 'last' | 'mixed';
/** The line ending written. */
export type LineEnding = 'auto' | 'lf' | 'crlf';

export interface FormatStyle {
  // Indentation
  indent_width: number;
  use_tabs: boolean;
  continuation_indent: number;
  indent_case_body: boolean;
  align_wrapped_to_paren: boolean;
  // Blank lines
  max_blank_lines: number;
  blank_lines_after_header: number;
  blank_lines_before_member: number;
  // Braces
  brace_type: BraceStyle;
  brace_method: BraceStyle;
  brace_block: BraceStyle;
  brace_continuation: BraceStyle;
  force_braces: boolean;
  // Spacing (only when `normalize_spacing` is on)
  normalize_spacing: boolean;
  space_before_control_paren: boolean;
  space_before_call_paren: boolean;
  space_before_lbrace: boolean;
  space_around_binary: boolean;
  space_around_assign: boolean;
  space_around_ternary: boolean;
  space_around_arrow: boolean;
  space_after_comma: boolean;
  space_before_comma: boolean;
  space_after_cast: boolean;
  space_within_parens: boolean;
  space_within_brackets: boolean;
  space_within_array_braces: boolean;
  space_within_angles: boolean;
  space_around_foreach_colon: boolean;
  // Wrapping
  line_width: number;
  wrap_parameters: WrapPolicy;
  wrap_arguments: WrapPolicy;
  wrap_chain: WrapPolicy;
  chain_min_calls: number;
  chain_keep_first_call: boolean;
  wrap_binary: WrapPolicy;
  wrap_ternary: WrapPolicy;
  wrap_clauses: WrapPolicy;
  wrap_array_init: WrapPolicy;
  operator_on_next_line: boolean;
  closing_paren_on_new_line: boolean;
  break_after_open_paren: boolean;
  // Imports
  organize_imports: boolean;
  import_groups: string[];
  blank_line_between_import_groups: boolean;
  static_imports: StaticImports;
  wildcard_threshold: number;
  static_wildcard_threshold: number;
  // Annotations
  annotations_type: AnnotationPlacement;
  annotations_method: AnnotationPlacement;
  annotations_field: AnnotationPlacement;
  // Comments
  align_block_comments: boolean;
  space_after_line_comment: boolean;
  wrap_javadoc: boolean;
  // File
  ensure_final_newline: boolean;
  line_ending: LineEnding;
  // Behaviour (not part of the output)
  format_on_save: boolean;
  use_editorconfig: boolean;
  auto_detect: boolean;
}

export type FormatKey = keyof FormatStyle;

/** A layer that names only the keys it sets. */
export type FormatPatch = Partial<FormatStyle>;

/** What `bennu_format_settings` answers: every layer, so the page can say where a value comes from. */
export interface FormatSettings {
  defaults: FormatStyle;
  /** The full global style (defaults ← what the user saved). */
  global: FormatStyle;
  /** The sparse override saved in `<repo>/.arbor/bennu/config.toml [format]`; `null` without one. */
  project: FormatPatch | null;
  /** defaults ← global ← detected (if auto_detect) ← editorconfig (if use_editorconfig) ← project. */
  effective: FormatStyle;
  editorconfig: FormatPatch | null;
  detected: FormatPatch | null;
}

/** One measurement behind a detected value: what the project does, and how consistently. */
export interface FormatEvidence {
  key: string;
  value: unknown;
  /** 0..1 — the fraction of samples that agree. */
  share: number;
  samples: number;
}

export interface FormatDetection {
  patch: FormatPatch;
  evidence: FormatEvidence[];
  /** How many files were measured. */
  files: number;
}

/** The layers a value can come from, lowest first. `default` and `global` are not badged. */
export type FormatLayer = 'default' | 'global' | 'detected' | 'editorconfig' | 'project';
