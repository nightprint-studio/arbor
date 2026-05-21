// Unified Studio Format IPC layer — one set of Tauri commands shared
// across every format (RON / JSON / TOML / YAML / .properties).
//
// See FROZEN F17 in `memory/project_studio_multi_format.md` for the
// design contract. The short version: format-specific Tauri commands
// are forbidden — every call dispatches through the host's
// `StudioRegistry`, with `formatId` as the first argument.
//
// `studioBackend(formatId)` returns an object that pre-binds the
// format id so call-sites read naturally (`backend.getRoot(docId)`).
//
// (Sibling `$lib/ipc/studio.ts` is the Studio **sidebar/index** IPC —
//  different surface area, unrelated.)

import { invoke } from '@tauri-apps/api/core';
import type {
  BulkEditAction,
  BulkEditOpenDoc,
  BulkEditPreview,
  BulkEditResult,
  BulkEditScope,
  BulkEditSite,
  BulkEditValueSource,
  RenameOpenDoc,
  RenamePreview,
  RenameResult,
  RenameSite,
} from '$lib/types/studio-format';

// ──────────────────────────────────────────────────────────────────────
// Format id
// ──────────────────────────────────────────────────────────────────────

export type StudioFormat = 'ron' | 'json' | 'toml' | 'yaml' | 'properties';

// ──────────────────────────────────────────────────────────────────────
// Format descriptor — mirror of `studio/format/descriptor.rs`
// ──────────────────────────────────────────────────────────────────────

export type IconRef =
  | { type: 'iconify';    name: string }
  | { type: 'inline_svg'; svg:  string };

export type NullPolicy        = 'native' | 'as_delete' | 'ask_user' | 'not_supported';
export type QuerySyntax       = 'json_path';
export type CrossRefScope     = 'value' | 'key';
export type SchemaSourceKind  = 'rust_struct' | 'json_schema' | 'java_class';
export type SaveWarningKind   = 'lossy_comments' | 'jsonc_comments_in_json';
export type KindTone          =
  | 'neutral' | 'info' | 'accent' | 'success' | 'warning' | 'error' | 'muted';

export interface KindStyle {
  label: string;
  tone:  KindTone;
  icon?: string;
}

export interface FormatDescriptor {
  id:                          string;
  label:                       string;
  file_extensions:             string[];
  icon:                        IconRef;

  supports_lossless_edit:      boolean;
  supports_comments:           boolean;
  supports_anchors:            boolean;
  null_handling:               NullPolicy;

  supports_streaming_mode:     boolean;
  streaming_threshold_kb:      number | null;
  streaming_setting_key:       string | null;

  query_syntax:                QuerySyntax;

  cross_ref_default_fields:    string[];
  cross_ref_scopes:            CrossRefScope[];

  schema_sources:              SchemaSourceKind[];

  kind_palette:                Record<string, KindStyle>;

  save_warnings:               SaveWarningKind[];
  save_behavior_setting_key:   string | null;

  convert_to_json_supported:   boolean;
  supports_external_files:     boolean;

  /** F12 — `true` when the backend implements `rename_preview` /
   *  `rename_apply`. The FE gates the "Rename across project…"
   *  context-menu item on this. Never probes by attempting. */
  supports_rename_reference:   boolean;

  /** F13 — `true` when the backend implements `bulk_edit_preview` /
   *  `bulk_edit_apply`. The FE gates the `[⚡ Edit]` button on this. */
  supports_bulk_edit:          boolean;
}

// ──────────────────────────────────────────────────────────────────────
// Doc lifecycle types — mirror `studio/format/types.rs`
// ──────────────────────────────────────────────────────────────────────

export type SchemaHintOrigin = 'directive' | 'sidecar';

export interface SchemaHint {
  rs_file:   string;
  root_type: string;
  origin:    SchemaHintOrigin;
}

/** Encoding metadata for a doc (FROZEN F16). Mirrors
 *  `studio::format::types::EncodingInfo` on the BE side. `label` is the
 *  canonical name reported by `encoding_rs` (`"UTF-8"`,
 *  `"windows-1252"`, `"UTF-16LE"`, …); `had_bom` records whether the
 *  open buffer began with a BOM so save can re-prepend it. */
export interface EncodingInfo {
  label:   string;
  had_bom: boolean;
}

export interface StudioParseResult {
  doc_id:      string;
  size_bytes:  number;
  source_path: string | null;
  original:    string;
  parse_error: string | null;
  root_kind:   string | null;
  child_count: number;
  schema_hint: SchemaHint | null;
  encoding:    EncodingInfo;
  /** Phase 3.d (JSON Studio): `true` when the backend decided to open
   *  this doc in stream mode (size ≥ format's streaming threshold).
   *  Stream-mode docs disable structural mutations — the modal hides
   *  edit affordances + shows a "Large file" banner. Other backends
   *  default to `false` until they wire their own streaming path. */
  stream_mode:        boolean;
  /** Phase 3.d (JSON Studio): `true` when the doc was opened from a
   *  `.jsonc` file. Drives banner copy — `.jsonc + features` is
   *  expected, `.json + features` surfaces the rename/strip prompt. */
  is_jsonc:           boolean;
  /** Phase 3.d (JSON Studio): `true` when the current buffer contains
   *  comments or trailing commas. */
  has_jsonc_features: boolean;
}

export interface StudioUpdateResult {
  parse_error: string | null;
  root_kind:   string | null;
  child_count: number;
  can_undo:    boolean;
  can_redo:    boolean;
  /** Phase 3.d (JSON Studio). Other formats always emit `false`. */
  has_jsonc_features: boolean;
}

export interface StudioMutateResult {
  text:        string;
  parse_error: string | null;
  root_kind:   string | null;
  child_count: number;
  can_undo:    boolean;
  can_redo:    boolean;
  /** Phase 3.d (JSON Studio). Other formats always emit `false`. */
  has_jsonc_features: boolean;
}

/** A tree-pane row. `kind` is an **opaque format-specific** string
 *  — the per-format `FormatDescriptor.kind_palette` tells the UI how
 *  to render it. The generic type parameter `TKind` lets callers
 *  narrow this to a format-specific union at the call site without
 *  losing the wire-level `string`. */
export interface StudioNodeView<TKind extends string = string> {
  key:         string;
  path:        string[];
  kind:        TKind;
  preview:     string;
  child_count: number;
  variant_tag: string | null;
}

export interface StudioQueryHit<TKind extends string = string> {
  path:        string[];
  kind:        TKind;
  preview:     string;
  variant_tag: string | null;
}

export interface StudioDocSnapshot<TKind extends string = string> {
  doc_id:      string;
  source_path: string | null;
  size_bytes:  number;
  original:    string;
  current:     string;
  parse_error: string | null;
  root_kind:   TKind | null;
  child_count: number;
  can_undo:    boolean;
  can_redo:    boolean;
  indent:      string;
}

export interface StudioFileEntry {
  absolute_path: string;
  relative_path: string;
  name:          string;
  size_bytes:    number;
}

// ──────────────────────────────────────────────────────────────────────
// Diff types — format-agnostic (RON-shaped today, future-compat).
// ──────────────────────────────────────────────────────────────────────

export type DiffLineKind = 'context' | 'add' | 'del';

export interface DiffLine {
  kind:     DiffLineKind;
  old_line: number | null;
  new_line: number | null;
  text:     string;
}

export interface DiffHunk {
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  lines:     DiffLine[];
}

export type DiffStatus = 'unchanged' | 'added' | 'removed' | 'modified' | 'partial';

export interface DiffTreeNode<TKind extends string = string> {
  key:             string;
  path:            string[];
  status:          DiffStatus;
  kind_before:     TKind | null;
  kind_after:      TKind | null;
  preview_before:  string | null;
  preview_after:   string | null;
  tag_before:      string | null;
  tag_after:       string | null;
  children:        DiffTreeNode<TKind>[];
  change_count:    number;
}

// ──────────────────────────────────────────────────────────────────────
// Mutation payload — tagged union dispatched by `studio_apply_mutation`.
// Per-format backends destructure and delegate to native helpers.
// ──────────────────────────────────────────────────────────────────────

export type StudioPrimitiveValue =
  | { type: 'bool';   value: boolean }
  | { type: 'int';    value: number  }
  | { type: 'float';  value: number  }
  | { type: 'string'; value: string  }
  | { type: 'char';   value: string  };

export type StudioMutation =
  | { kind: 'set_primitive';    path: string[]; value: StudioPrimitiveValue }
  | { kind: 'toggle_option';    path: string[] }
  | { kind: 'replace_at';       path: string[]; text: string }
  | { kind: 'remove_at';        path: string[] }
  | { kind: 'insert_field';     path: string[]; name: string; text: string }
  | { kind: 'insert_item';      path: string[]; text: string }
  | { kind: 'insert_map_entry'; path: string[]; key_text: string; val_text: string }
  | { kind: 'duplicate_at';     path: string[] }
  | { kind: 'move_item';        path: string[]; delta: number };

// ──────────────────────────────────────────────────────────────────────
// Schema types — `.rs`-source introspection lives in `ron_studio` but
// the data shapes are format-agnostic (reused by any format that binds
// to a Rust struct: RON today, TOML/JSON tomorrow).
// ──────────────────────────────────────────────────────────────────────

export type ResolvedType =
  | { kind: 'primitive'; name: string }
  | { kind: 'option';    inner: ResolvedType }
  | { kind: 'vec';       inner: ResolvedType }
  | { kind: 'map';       key: ResolvedType; value: ResolvedType }
  | { kind: 'tuple';     items: ResolvedType[] }
  | { kind: 'named';     path: string }
  | { kind: 'external';  path: string }
  | { kind: 'unknown';   hint: string };

export type VariantShape = 'unit' | 'tuple' | 'struct';

export interface FieldDef {
  /** Serialised name (after `#[serde(rename = "...")]` or container
   *  `rename_all`). The Rust ident sits in `aliases` when it differs. */
  name:             string;
  ty:               ResolvedType;
  /** Additional accepted names from `#[serde(alias = "...")]` plus the
   *  original Rust ident when distinct from `name`. Defaults to `[]`
   *  for older payloads (BE may omit when empty). */
  aliases?:         string[];
  has_default:      boolean;
  skip_if_default:  boolean;
  flatten:          boolean;
}

export interface VariantDef {
  name:    string;
  shape:   VariantShape;
  fields:  FieldDef[];
}

export type TypeDef =
  | { kind: 'struct'; name: string; fields: FieldDef[]; tuple_like: boolean }
  | { kind: 'enum';   name: string; variants: VariantDef[] }
  | { kind: 'alias';  name: string; target: ResolvedType };

export interface SchemaStats {
  resolved: number;
  external: number;
  unknown:  number;
}

export interface Schema {
  root_type:        string;
  root_name:        string;
  crate_manifest:   string;
  crate_name:       string;
  types:            Record<string, TypeDef>;
  stats:            SchemaStats;
}

export type CandidateKind = 'struct' | 'enum';

export interface RootCandidate {
  name:           string;
  canonical_path: string;
  kind:           CandidateKind;
}

export interface CrateProbe {
  crate_manifest:  string;
  crate_name:      string;
  root_candidates: RootCandidate[];
}

export interface TypeSource {
  canonical_path: string;
  name:           string;
  kind:           CandidateKind;
  source:         string;
}

// ──────────────────────────────────────────────────────────────────────
// Raw IPC wrappers — every call takes `formatId` as the first arg.
// ──────────────────────────────────────────────────────────────────────

export const studioListFormats = (): Promise<FormatDescriptor[]> =>
  invoke<FormatDescriptor[]>('studio_list_formats');

export const studioDescribe = (formatId: StudioFormat): Promise<FormatDescriptor> =>
  invoke<FormatDescriptor>('studio_describe', { formatId });

export interface StudioParseArgs {
  text?:         string;
  path?:         string;
  /** Active tab id — paired with `relativePath` enables the cfg-keyed
   *  schema-binding fallback for files outside the repo's walk-up
   *  reach. */
  tabId?:        string;
  relativePath?: string;
}

export const studioParse = (
  formatId: StudioFormat,
  args:     StudioParseArgs,
): Promise<StudioParseResult> =>
  invoke<StudioParseResult>('studio_parse', { formatId, ...args });

export const studioClose = (formatId: StudioFormat, docId: string): Promise<void> =>
  invoke('studio_close', { formatId, docId });

export const studioGetEncoding = (
  formatId: StudioFormat,
  docId:    string,
): Promise<EncodingInfo> =>
  invoke<EncodingInfo>('studio_get_encoding', { formatId, docId });

export const studioSetText = (
  formatId: StudioFormat,
  docId:    string,
  text:     string,
): Promise<StudioUpdateResult> =>
  invoke<StudioUpdateResult>('studio_set_text', { formatId, docId, text });

export const studioGetRoot = <TKind extends string = string>(
  formatId: StudioFormat,
  docId:    string,
): Promise<StudioNodeView<TKind> | null> =>
  invoke<StudioNodeView<TKind> | null>('studio_get_root', { formatId, docId });

export const studioGetChildren = <TKind extends string = string>(
  formatId: StudioFormat,
  docId:    string,
  path:     string[],
): Promise<StudioNodeView<TKind>[]> =>
  invoke<StudioNodeView<TKind>[]>('studio_get_children', { formatId, docId, path });

export const studioGetValue = (
  formatId: StudioFormat,
  docId:    string,
  path:     string[],
): Promise<string> =>
  invoke<string>('studio_get_value', { formatId, docId, path });

export const studioQuery = <TKind extends string = string>(
  formatId: StudioFormat,
  docId:    string,
  expr:     string,
): Promise<StudioQueryHit<TKind>[]> =>
  invoke<StudioQueryHit<TKind>[]>('studio_query', { formatId, docId, expr });

export const studioRawOriginal = (formatId: StudioFormat, docId: string): Promise<string> =>
  invoke<string>('studio_raw_original', { formatId, docId });

export const studioRawCurrent = (formatId: StudioFormat, docId: string): Promise<string> =>
  invoke<string>('studio_raw_current', { formatId, docId });

export const studioFormat = (formatId: StudioFormat, docId: string): Promise<string> =>
  invoke<string>('studio_format', { formatId, docId });

export const studioToJson = (formatId: StudioFormat, docId: string): Promise<string> =>
  invoke<string>('studio_to_json', { formatId, docId });

export const studioFromJson = (
  formatId: StudioFormat,
  docId:    string,
  jsonText: string,
): Promise<string> =>
  invoke<string>('studio_from_json', { formatId, docId, jsonText });

export const studioGetIndent = (formatId: StudioFormat, docId: string): Promise<string> =>
  invoke<string>('studio_get_indent', { formatId, docId });

export const studioSetIndent = (
  formatId: StudioFormat,
  docId:    string,
  indent:   string,
): Promise<void> =>
  invoke('studio_set_indent', { formatId, docId, indent });

export const studioApplyMutation = (
  formatId: StudioFormat,
  docId:    string,
  mutation: StudioMutation,
): Promise<StudioMutateResult> =>
  invoke<StudioMutateResult>('studio_apply_mutation', { formatId, docId, mutation });

/** Phase 3.d — re-emit the doc with format-specific extras stripped.
 *  JSON Studio: drops comments + trailing commas (lossy). Backends
 *  without this support return `Unsupported`; callers gate on
 *  descriptor.save_warnings before invoking. */
export const studioStripFeatures = (
  formatId: StudioFormat,
  docId:    string,
): Promise<StudioMutateResult> =>
  invoke<StudioMutateResult>('studio_strip_features', { formatId, docId });

export const studioDiff = (
  formatId: StudioFormat,
  docId:    string,
): Promise<DiffHunk[]> =>
  invoke<DiffHunk[]>('studio_diff', { formatId, docId });

export const studioTreeDiff = <TKind extends string = string>(
  formatId: StudioFormat,
  docId:    string,
): Promise<DiffTreeNode<TKind>> =>
  invoke<DiffTreeNode<TKind>>('studio_tree_diff', { formatId, docId });

export const studioUndo = (formatId: StudioFormat, docId: string): Promise<StudioMutateResult> =>
  invoke<StudioMutateResult>('studio_undo', { formatId, docId });

export const studioRedo = (formatId: StudioFormat, docId: string): Promise<StudioMutateResult> =>
  invoke<StudioMutateResult>('studio_redo', { formatId, docId });

export const studioHistoryState = (
  formatId: StudioFormat,
  docId:    string,
): Promise<[boolean, boolean]> =>
  invoke<[boolean, boolean]>('studio_history_state', { formatId, docId });

export const studioSnapshot = <TKind extends string = string>(
  formatId: StudioFormat,
  docId:    string,
): Promise<StudioDocSnapshot<TKind>> =>
  invoke<StudioDocSnapshot<TKind>>('studio_snapshot', { formatId, docId });

export const studioSourcePath = (
  formatId: StudioFormat,
  docId:    string,
): Promise<string | null> =>
  invoke<string | null>('studio_source_path', { formatId, docId });

export interface StudioSaveArgs {
  docId:       string;
  path:        string;
  contents:    string;
  bindToDoc:   boolean;
}

export const studioSave = (formatId: StudioFormat, args: StudioSaveArgs): Promise<void> =>
  invoke('studio_save', {
    formatId,
    docId:      args.docId,
    path:       args.path,
    contents:   args.contents,
    bindToDoc:  args.bindToDoc,
  });

export const studioListFiles = (
  formatId: StudioFormat,
  folder:   string,
): Promise<StudioFileEntry[]> =>
  invoke<StudioFileEntry[]>('studio_list_files', { formatId, folder });

export const studioSchemaProbe = (
  formatId: StudioFormat,
  source:   string,
): Promise<CrateProbe> =>
  invoke<CrateProbe>('studio_schema_probe', { formatId, source });

export const studioSchemaLoad = (
  formatId:       StudioFormat,
  source:         string,
  rootCanonical:  string,
): Promise<Schema> =>
  invoke<Schema>('studio_schema_load', { formatId, source, rootCanonical });

export const studioSchemaViewSource = (
  formatId:       StudioFormat,
  source:         string,
  canonicalPath:  string,
): Promise<TypeSource> =>
  invoke<TypeSource>('studio_schema_view_source', { formatId, source, canonicalPath });

// ── F12 — Cross-reference rename refactor ─────────────────────────────

export interface StudioRenamePreviewArgs {
  /** Active tab id — the BE resolves the repo root from it. */
  tabId:        string;
  oldValue:     string;
  /** When provided, the preview surfaces collisions where this value
   *  already exists as a definition (FROZEN F12 sticky warning). */
  newValueHint?: string | null;
  /** Open-doc snapshot used for the dirty-blocker check. */
  openDocs:     RenameOpenDoc[];
}

export const studioRenamePreview = (
  formatId: StudioFormat,
  args:     StudioRenamePreviewArgs,
): Promise<RenamePreview> =>
  invoke<RenamePreview>('studio_rename_preview', {
    formatId,
    tabId:        args.tabId,
    oldValue:     args.oldValue,
    newValueHint: args.newValueHint ?? null,
    openDocs:     args.openDocs,
  });

export interface StudioRenameApplyArgs {
  tabId:    string;
  oldValue: string;
  newValue: string;
  /** Site list returned by `studioRenamePreview`, possibly pruned by
   *  the user via per-site skip checkboxes. */
  sites:    RenameSite[];
  openDocs: RenameOpenDoc[];
}

export const studioRenameApply = (
  formatId: StudioFormat,
  args:     StudioRenameApplyArgs,
): Promise<RenameResult> =>
  invoke<RenameResult>('studio_rename_apply', {
    formatId,
    tabId:    args.tabId,
    oldValue: args.oldValue,
    newValue: args.newValue,
    sites:    args.sites,
    openDocs: args.openDocs,
  });

// ── F13 — Query-driven bulk edit ──────────────────────────────────────

export interface StudioBulkEditPreviewArgs {
  /** Active tab id — resolves the repo root for `project_wide`. */
  tabId:        string;
  /** Active doc id — required for `active_doc` scope, ignored for
   *  `project_wide`. */
  docId:        string;
  scope:        BulkEditScope;
  query:        string;
  action:       BulkEditAction;
  /** `null` for `delete`; required for `set`. */
  valueSource:  BulkEditValueSource | null;
  /** Open-doc snapshot — used for the dirty-blocker check
   *  (`project_wide` only). */
  openDocs:     BulkEditOpenDoc[];
}

export const studioBulkEditPreview = (
  formatId: StudioFormat,
  args:     StudioBulkEditPreviewArgs,
): Promise<BulkEditPreview> =>
  invoke<BulkEditPreview>('studio_bulk_edit_preview', {
    formatId,
    tabId:       args.tabId,
    docId:       args.docId,
    scope:       args.scope,
    query:       args.query,
    action:      args.action,
    valueSource: args.valueSource,
    openDocs:    args.openDocs,
  });

export interface StudioBulkEditApplyArgs {
  tabId:       string;
  docId:       string;
  scope:       BulkEditScope;
  action:      BulkEditAction;
  valueSource: BulkEditValueSource | null;
  /** Site list returned by `studioBulkEditPreview`, possibly pruned
   *  via per-site skip checkboxes. */
  sites:       BulkEditSite[];
  openDocs:    BulkEditOpenDoc[];
}

export const studioBulkEditApply = (
  formatId: StudioFormat,
  args:     StudioBulkEditApplyArgs,
): Promise<BulkEditResult> =>
  invoke<BulkEditResult>('studio_bulk_edit_apply', {
    formatId,
    tabId:       args.tabId,
    docId:       args.docId,
    scope:       args.scope,
    action:      args.action,
    valueSource: args.valueSource,
    sites:       args.sites,
    openDocs:    args.openDocs,
  });

// ──────────────────────────────────────────────────────────────────────
// Backend factory — pre-binds the format id so callers read naturally.
//
// Usage:
//   const ron = studioBackend('ron');                     // generic
//   const ron = studioBackend<RonNodeKind>('ron');        // typed kind
//   const root = await ron.getRoot(docId);                // typed result
//
// Thin sugar — callers that prefer raw `studio*` helpers can ignore it.
// ──────────────────────────────────────────────────────────────────────

export interface StudioBackend<TKind extends string = string> {
  readonly formatId: StudioFormat;

  describe():                                            Promise<FormatDescriptor>;
  parse(args: StudioParseArgs):                          Promise<StudioParseResult>;
  close(docId: string):                                  Promise<void>;
  getEncoding(docId: string):                            Promise<EncodingInfo>;
  setText(docId: string, text: string):                  Promise<StudioUpdateResult>;
  getRoot(docId: string):                                Promise<StudioNodeView<TKind> | null>;
  getChildren(docId: string, path: string[]):            Promise<StudioNodeView<TKind>[]>;
  getValue(docId: string, path: string[]):               Promise<string>;
  query(docId: string, expr: string):                    Promise<StudioQueryHit<TKind>[]>;
  rawOriginal(docId: string):                            Promise<string>;
  rawCurrent(docId: string):                             Promise<string>;
  format(docId: string):                                 Promise<string>;
  toJson(docId: string):                                 Promise<string>;
  fromJson(docId: string, jsonText: string):             Promise<string>;
  getIndent(docId: string):                              Promise<string>;
  setIndent(docId: string, indent: string):              Promise<void>;
  applyMutation(docId: string, m: StudioMutation):       Promise<StudioMutateResult>;
  stripFeatures(docId: string):                          Promise<StudioMutateResult>;
  diff(docId: string):                                   Promise<DiffHunk[]>;
  treeDiff(docId: string):                               Promise<DiffTreeNode<TKind>>;
  undo(docId: string):                                   Promise<StudioMutateResult>;
  redo(docId: string):                                   Promise<StudioMutateResult>;
  historyState(docId: string):                           Promise<[boolean, boolean]>;
  snapshot(docId: string):                               Promise<StudioDocSnapshot<TKind>>;
  sourcePath(docId: string):                             Promise<string | null>;
  save(args: StudioSaveArgs):                            Promise<void>;
  listFiles(folder: string):                             Promise<StudioFileEntry[]>;
  schemaProbe(source: string):                           Promise<CrateProbe>;
  schemaLoad(source: string, rootCanonical: string):     Promise<Schema>;
  schemaViewSource(source: string, canon: string):       Promise<TypeSource>;

  // F12 — gated by `descriptor.supports_rename_reference`.
  renamePreview(args: StudioRenamePreviewArgs):          Promise<RenamePreview>;
  renameApply(args: StudioRenameApplyArgs):              Promise<RenameResult>;

  // F13 — gated by `descriptor.supports_bulk_edit`.
  bulkEditPreview(args: StudioBulkEditPreviewArgs):      Promise<BulkEditPreview>;
  bulkEditApply(args: StudioBulkEditApplyArgs):          Promise<BulkEditResult>;
}

export function studioBackend<TKind extends string = string>(
  formatId: StudioFormat,
): StudioBackend<TKind> {
  return {
    formatId,
    describe:         ()                          => studioDescribe(formatId),
    parse:            (args)                      => studioParse(formatId, args),
    close:            (docId)                     => studioClose(formatId, docId),
    getEncoding:      (docId)                     => studioGetEncoding(formatId, docId),
    setText:          (docId, text)               => studioSetText(formatId, docId, text),
    getRoot:          (docId)                     => studioGetRoot<TKind>(formatId, docId),
    getChildren:      (docId, path)               => studioGetChildren<TKind>(formatId, docId, path),
    getValue:         (docId, path)               => studioGetValue(formatId, docId, path),
    query:            (docId, expr)               => studioQuery<TKind>(formatId, docId, expr),
    rawOriginal:      (docId)                     => studioRawOriginal(formatId, docId),
    rawCurrent:       (docId)                     => studioRawCurrent(formatId, docId),
    format:           (docId)                     => studioFormat(formatId, docId),
    toJson:           (docId)                     => studioToJson(formatId, docId),
    fromJson:         (docId, jsonText)           => studioFromJson(formatId, docId, jsonText),
    getIndent:        (docId)                     => studioGetIndent(formatId, docId),
    setIndent:        (docId, indent)             => studioSetIndent(formatId, docId, indent),
    applyMutation:    (docId, m)                  => studioApplyMutation(formatId, docId, m),
    stripFeatures:    (docId)                     => studioStripFeatures(formatId, docId),
    diff:             (docId)                     => studioDiff(formatId, docId),
    treeDiff:         (docId)                     => studioTreeDiff<TKind>(formatId, docId),
    undo:             (docId)                     => studioUndo(formatId, docId),
    redo:             (docId)                     => studioRedo(formatId, docId),
    historyState:     (docId)                     => studioHistoryState(formatId, docId),
    snapshot:         (docId)                     => studioSnapshot<TKind>(formatId, docId),
    sourcePath:       (docId)                     => studioSourcePath(formatId, docId),
    save:             (args)                      => studioSave(formatId, args),
    listFiles:        (folder)                    => studioListFiles(formatId, folder),
    schemaProbe:      (source)                    => studioSchemaProbe(formatId, source),
    schemaLoad:       (source, rootCanonical)     => studioSchemaLoad(formatId, source, rootCanonical),
    schemaViewSource: (source, canon)             => studioSchemaViewSource(formatId, source, canon),
    renamePreview:    (args)                      => studioRenamePreview(formatId, args),
    renameApply:      (args)                      => studioRenameApply(formatId, args),
    bulkEditPreview:  (args)                      => studioBulkEditPreview(formatId, args),
    bulkEditApply:    (args)                      => studioBulkEditApply(formatId, args),
  };
}
