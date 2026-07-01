/**
 * studio-config — the format-specific extension contract consumed by the
 * generic `<Studio>` component.
 *
 * Every per-format wrapper (JSON / YAML / TOML / .properties) builds one
 * `StudioConfig<TKind, TNode>` describing the bits that genuinely differ
 * between formats, then renders `<Studio {config} …/>` with a few
 * `{#snippet}` extension points for the divergent UI (JSONC banner,
 * YAML↔properties converter, …). The generic component owns ALL the
 * shared chrome, markup, CSS and composable wiring.
 *
 * The lambdas here are the SAME store-bound accessors the composables
 * already accept — `<Studio>` distributes them to
 * `useStudioEditPipeline`, `useStudioSchema`, `useStudioCrossRefs`,
 * `useStudioRenameBulkPipeline`, `useStudioQueryBar`, `useStudioTextDiff`,
 * `useStudioSaveFlow`, `useStudioUndoRedo`, `useStudioGlobalKeys`,
 * `useStudioOutsideEdit` — so the migration is mechanical: move each
 * wrapper's existing config object into one `StudioConfig` and delete the
 * markup. RON is a partial consumer and keeps its own modal.
 */

import type { MenuItem } from '../shared/ContextMenu.svelte';
import type { StudioKindTone } from './StudioKindBadge.svelte';
import type { IndentChoice } from './studio-footer-types';
import type {
  StudioBackend, StudioNodeView, StudioPrimitiveValue,
  Schema, ResolvedType, TypeDef, SchemaHint, EncodingInfo,
} from '$lib/ipc/studio/studio-format';
import type { StudioFileKind } from '$lib/ipc/studio/studio';
import type { BulkEditResult } from '$lib/types/studio/studio-format';

/** A tree node — the projected `StudioNodeView` decorated with the
 *  per-row UI fields the tree pane attaches. Identical across formats. */
export type StudioTreeNode<TKind extends string> = StudioNodeView<TKind> & {
  pid:      string;
  children: StudioTreeNode<TKind>[] | null;
  loading?: boolean;
};

/** Reactive snapshot of the per-format store that `<Studio>` reads for
 *  chrome (header / footer / banners / gating). All getters — the
 *  wrapper points each at its own store. */
export interface StudioStoreView {
  readonly open:       boolean;
  readonly loading:    boolean;
  readonly error:      string | null;
  readonly parseError: string | null;
  readonly docId:      string | null;
  readonly sourcePath: string | null;
  readonly title:      string | null;
  readonly dirty:      boolean;
  readonly current:    string;
  readonly canUndo:    boolean;
  readonly canRedo:    boolean;
  readonly encoding:   EncodingInfo | null;
  readonly sizeBytes:  number | null;
}

/** Result of a primitive commit attempt — `{ error }` keeps the editor
 *  open and surfaces the message (matches `useStudioEditPipeline`). */
export interface StudioCommitResult { error?: string }

export interface StudioConfig<TKind extends string, TNode extends StudioTreeNode<TKind>> {
  // ── Identity / static metadata ──────────────────────────────────────
  formatId:     StudioFileKind;
  backend:      StudioBackend<TKind>;
  formatLabel:  string;          // 'JSON' / '.properties'
  ariaLabel:    string;          // 'JSON Studio'
  defaultTitle: string;          // shown when the doc has no title
  loadingLabel: string;          // 'Parsing JSON…'
  rightPaneKey: string;          // localStorage key
  queryHistoryKey: string;
  queryPlaceholder: string;
  saveExtensions:  string[];
  saveDefaultName: string;
  separator:    string;          // ':' or '='
  indentOptions?: IndentChoice[];
  nullPolicy:   'native' | 'as_delete' | 'ask_user';
  indentTooltip: string;
  formatTooltip: string;
  schemaPickerTitle:  string;
  schemaPickerButton: string;
  schemaPickerExts:   string[];
  schemaRailTooltipEmpty: string;
  schemaRailLabel:    string;
  schemaCssPrefix:    string;    // 'js' / 'ys' / 'ts' / 'ps'
  treeErrorMessage?:  string;
  copyValueLabel:     string;    // 'Copy value (JSON)'
  pasteLabel:         string;    // 'Paste JSON over value…'
  /** Kind-badge style: `tinted` (JSON/TOML) vs italic-null (YAML/Props). */
  kindBadgeStyle: 'tinted' | 'italic-null';

  // ── Store snapshot + lifecycle ──────────────────────────────────────
  store: StudioStoreView;
  closeDoc(): Promise<void>;
  openDoc(opts: { path: string; title?: string | null }): Promise<void>;
  undo(): Promise<boolean>;
  redo(): Promise<boolean>;
  setText(text: string): Promise<void>;
  save(opts: { path: string | null; bindToDoc: boolean }): Promise<void>;
  /** Persist the server-returned bulk-edit active-doc state in place. */
  applyExternalMutate(state: NonNullable<BulkEditResult['active_doc_state']>): Promise<void>;

  // ── Structured mutations ────────────────────────────────────────────
  mutatePrimitive(path: string[], v: StudioPrimitiveValue): Promise<void>;
  removeAt(path: string[]): Promise<void>;
  insertField(path: string[], key: string, snippet: string): Promise<void>;
  insertItem(path: string[], snippet: string): Promise<void>;
  duplicateAt(path: string[]): Promise<void>;
  moveItem(path: string[], delta: number): Promise<void>;
  replaceAt(path: string[], snippet: string): Promise<void>;
  /** Snippet text for a freshly-inserted field / array item. May depend
   *  on the parent kind (TOML arrays vs array-of-tables differ). */
  newFieldSnippet: (parentKind: TKind) => string;
  newItemSnippet:  (parentKind: TKind) => string;

  // ── Kind metadata ───────────────────────────────────────────────────
  kindBadge:   (k: TKind) => string;
  kindTone:    (k: TKind) => StudioKindTone;
  isBoolKind:  (k: TKind) => boolean;
  isContainerKind:     (k: TKind) => boolean;
  isEditablePrimitive: (k: TKind) => boolean;
  isPromotableNull?:   (k: TKind) => boolean;
  /** Object-like container (shows "Add field…"). Default: kind === 'object'. */
  isObjectLike?: (k: TKind) => boolean;
  /** Array-like container (shows "Add item" + Move up/down). Default: kind === 'array'. */
  isArrayLike?:  (k: TKind) => boolean;

  // ── Tree projection ─────────────────────────────────────────────────
  sortChildren: (parentKind: TKind, kids: TNode[]) => TNode[];

  // ── Edit pipeline lambdas ───────────────────────────────────────────
  computeSeed: (node: TNode, valueText: string | null) => string;
  commit:      (node: TNode, draft: string, ctx: StudioCtx<TKind, TNode>) => Promise<StudioCommitResult>;

  // ── Schema walker ───────────────────────────────────────────────────
  getSchemaHint:   () => SchemaHint | null;
  walkType:        (schema: Schema | null, path: string[]) => ResolvedType | null;
  flattenedFields: (schema: Schema, def: TypeDef & { kind: 'struct' }) => any[];

  // ── Cross-ref overrides (Properties widens the defaults) ────────────
  unquotedString?:        (preview: string) => string | null;
  isDefinitionFieldName?: (key: string) => boolean;
  isReferenceFieldName?:  (key: string) => boolean;

  // ── Variant / definition adapters (need live composable handles) ────
  currentVariantTag: (node: TNode, ctx: StudioCtx<TKind, TNode>) => string;
  extractRenameValue: (node: TNode, ctx: StudioCtx<TKind, TNode>) => string | null;
  isDefinitionNode:  (node: TNode, ctx: StudioCtx<TKind, TNode>) => boolean;
  definitionValue:   (node: TNode, ctx: StudioCtx<TKind, TNode>) => string | null;

  // ── Capability gates ────────────────────────────────────────────────
  /** Whether structural editing is allowed (JSON stream-mode disables). */
  editingEnabled?: () => boolean;
  /** Override the "Copy path" clipboard text (.properties strips the
   *  `$value` sentinel). Default = `$.a.b` from the raw path. */
  copyPathText?: (path: string[]) => string;
  /** Custom save trigger — JSON's JSONC gate overrides this. Default =
   *  the shared save flow's doSave. */
  onSaveRequested?: () => Promise<void> | void;
}

/** Live composable handles passed into the config lambdas that need them
 *  (commit narrowing, variant tag, definition detection). `<Studio>`
 *  builds one of these once the composables exist and threads it in. */
export interface StudioCtx<TKind extends string, TNode extends StudioTreeNode<TKind>> {
  schema:          () => Schema | null;
  primitiveHintAt: (path: string[]) => string | null;
  enumDefAt:       (path: string[]) => (TypeDef & { kind: 'enum' }) | null;
  unquotedString:  (preview: string) => string | null;
  isDefinitionFieldName: (key: string) => boolean;
  refresh:         (node: TNode, structural: boolean, removed?: boolean) => Promise<void>;
  setEditError:    (msg: string | null) => void;
}

export type { MenuItem, IndentChoice };
