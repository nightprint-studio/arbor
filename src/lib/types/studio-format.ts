// Format-agnostic Studio types shared across every Studio modal
// (RON / JSON / TOML / YAML / .properties).
//
// Mirrors `studio::format::types` on the BE. Kept separate from
// `$lib/ipc/studio-format.ts` so anything that just needs the *shape*
// (modals, components, stores) can import from `$lib/types/...`
// without pulling the IPC layer.

// ── F12 — Cross-reference rename refactor ──────────────────────────

export type RenameSiteScope = 'definition' | 'reference' | 'key';

export interface RenameSite {
  absolute_path: string;
  relative_path: string;
  file_name:     string;
  /** AST path of the value node (matches what `studio_get_value` expects). */
  field_path:    string[];
  /** Key the value lives under. For `.properties` `Key` scope this is the dotted key itself. */
  key_name:      string;
  scope:         RenameSiteScope;
  /** Short snippet of the matched line for the preview UI. May be empty. */
  preview:       string;
}

export interface RenameDirtyBlocker {
  doc_id:      string;
  source_path: string | null;
}

export interface RenameCollision {
  absolute_path: string;
  relative_path: string;
  field_path:    string[];
  key_name:      string;
}

/** Open-doc snapshot the FE feeds to the BE for dirty-checking. */
export interface RenameOpenDoc {
  doc_id:      string;
  source_path: string | null;
  dirty:       boolean;
}

export interface RenamePreview {
  sites:           RenameSite[];
  dirty_blockers:  RenameDirtyBlocker[];
  collisions:      RenameCollision[];
}

export interface RenameFailure {
  absolute_path: string;
  message:       string;
}

export interface RenameResult {
  written_files: string[];
  failed_files:  RenameFailure[];
}

// ── F13 — Query-driven bulk edit (mini-expression language) ────────

export type BulkEditAction = 'set' | 'delete';

export type BulkEditScope  = 'active_doc' | 'project_wide';

/** Typed literal used when `value_source.kind === 'literal'`. */
export type BulkEditLiteral =
  | { type: 'string'; value: string  }
  | { type: 'number'; value: number  }
  | { type: 'bool';   value: boolean }
  | { type: 'null' };

/** Where the new value comes from for `set`. */
export type BulkEditValueSource =
  | { kind: 'literal';    literal: BulkEditLiteral }
  | { kind: 'expression'; source: string };

/** One row in the preview list. */
export interface BulkEditSite {
  absolute_path: string;
  relative_path: string;
  file_name:     string;
  field_path:    string[];
  /** Opaque per-format kind — see `FormatDescriptor.kind_palette`. */
  kind:          string;
  /** Preview of the current value at this site (`"goblin"`, `42`…). */
  old_preview:   string;
  /** Preview of the computed new value. Empty when `will_skip`. */
  new_preview:   string;
  /** `true` when this site will be skipped at apply time. */
  will_skip:     boolean;
  /** Human-readable reason for the skip. Empty when not skipped. */
  skip_reason?:  string;
}

/** Open-doc snapshot — same shape as F12, reused here. */
export type BulkEditOpenDoc = RenameOpenDoc;

/** Output of `bulk_edit_preview`. */
export interface BulkEditPreview {
  sites:             BulkEditSite[];
  dirty_blockers:    RenameDirtyBlocker[];
  /** Top-level compile error for `value_source.kind === 'expression'`.
   *  Surface as a banner; per-site eval errors land inside `sites`
   *  as `will_skip + skip_reason`. */
  expression_error:  string | null;
}

export interface BulkEditFailure {
  absolute_path: string;
  message:       string;
}

/** Output of `bulk_edit_apply`. */
export interface BulkEditResult {
  written_files:     string[];
  failed_files:      BulkEditFailure[];
  applied_sites:     number;
  skipped_sites:     number;
  /** Post-mutation snapshot for the active doc (`scope === 'active_doc'`).
   *  Same shape as `studio_apply_mutation`'s result so the FE pipes it
   *  through `applyMutateResult`. `null` for `project_wide` scope. */
  active_doc_state:  StudioMutateResultLike | null;
}

/** Mirror of `studio::format::types::MutateResult`. Kept local to the
 *  types module so the bulk-edit shape is self-contained. */
export interface StudioMutateResultLike {
  text:        string;
  parse_error: string | null;
  root_kind:   string | null;
  child_count: number;
  can_undo:    boolean;
  can_redo:    boolean;
  /** Phase 3.d (JSON Studio). Other formats always emit `false`. */
  has_jsonc_features: boolean;
}
