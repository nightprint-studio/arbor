/**
 * useStudioEditPipeline — owns the edit-pipeline state shared by every
 * Studio modal: which node is being edited, where (tree row vs Inspector
 * detail panel), the draft text, the inline input/select refs, and the
 * "edits aren't auto-saved" banner.
 *
 * Format-specific bits (seed computation, kind→PrimitiveValue dispatch,
 * variant tag extraction) live in the consumer-supplied `config`.
 * The composable orchestrates: it routes Enter / Escape, decides
 * primitive vs variant commit, focuses the right input, and clears
 * editingPid on success.
 */

import type { StudioPrimitiveValue } from '$lib/ipc/studio-format';

export type EditLocation = 'tree' | 'detail';

export interface CommitResult {
  /** Set on failure — keeps `editingPid` open and shows the inline `!` chip. */
  error?: string;
}

export interface EditPipelineConfig<TKind extends string, TNode> {
  /** Used to build localStorage key for the dismissed-banner pref. */
  formatId: string;
  /** Whether a row's kind supports the F2/double-click primitive editor. */
  isEditablePrimitive: (kind: TKind) => boolean;
  /** YAML's `null` leaf — promotable via `replace_at` snippet. */
  isPromotableNull?: (kind: TKind) => boolean;
  /** 'primitive' for regular edit, 'variant' for enum-typed string nodes,
   *  `null` otherwise. */
  rowEditMode: (node: TNode) => 'primitive' | 'variant' | null;
  /** Seed text shown in the input when edit starts. Wrapper unquotes
   *  format-specific syntax (YAML "…", RON char escapes, …) — typically
   *  closes over the wrapper's `valueText` $state. */
  computeSeed: (node: TNode) => string;
  /** Commit a primitive draft. Returns `{ error }` on failure so the
   *  composable can keep `editingPid` open and surface the message. */
  commit: (node: TNode, draft: string) => Promise<CommitResult>;
  /** Commit a variant pick (enum-typed strings). */
  commitVariant: (node: TNode, tag: string) => Promise<CommitResult>;
  /** Read the current variant tag for the inline `<select>`'s initial value. */
  currentVariantTag: (node: TNode) => string;
  /** Optional — called when starting an edit with `location === 'detail'`
   *  so the Inspector panel can focus its own bound `<input>`. */
  focusInspector?: () => void;
}

export interface EditPipeline<TKind extends string, TNode> {
  // State (readonly via getters; `editBuf` is bindable through set/get pair).
  readonly editingPid: string | null;
  readonly editLocation: EditLocation;
  editBuf: string;
  readonly editError: string | null;
  readonly editBannerVisible: boolean;

  // `bind:this` targets — getter/setter pair so consumers can write
  // `bind:this={pipeline.editInlineEl}`.
  editInlineEl:       HTMLInputElement  | undefined;
  editInlineSelectEl: HTMLSelectElement | undefined;

  // Lifecycle.
  startEdit(node: TNode | null, location?: EditLocation): void;
  startVariantEdit(node: TNode | null, location?: EditLocation): void;
  cancelEdit(): void;
  /** Route Enter → commit (variant-aware) / Escape → cancel. */
  onEditKey(e: KeyboardEvent, node: TNode | null): void;
  /** Commit whatever is open without checking the key — used when the
   *  selection moves while an edit is active. Awaited callers may ignore
   *  the returned promise. */
  maybeCommitActiveEdit(node: TNode | null): Promise<void>;
  /** Force-commit the current primitive draft (e.g. inline `<select>`
   *  `onchange`). */
  runCommit(node: TNode): Promise<void>;
  runCommitVariant(node: TNode): Promise<void>;

  // Banner.
  maybeShowEditBanner(): void;
  dismissEditBanner(): void;

  /** Force the inline `!` error chip — used by callers that mutate
   *  outside the normal start→commit flow (e.g. Inspector variant picker). */
  setEditError(msg: string | null): void;
}

const _PRIMITIVE_VALUE_PHANTOM: StudioPrimitiveValue | null = null;
void _PRIMITIVE_VALUE_PHANTOM;

export function useStudioEditPipeline<TKind extends string, TNode extends { pid: string; kind: TKind; path: string[]; preview: string; key: string }>(
  config: EditPipelineConfig<TKind, TNode>,
): EditPipeline<TKind, TNode> {
  let editingPid    = $state<string | null>(null);
  let editLocation  = $state<EditLocation>('detail');
  let editBuf       = $state('');
  let editError     = $state<string | null>(null);

  let editInlineEl:       HTMLInputElement  | undefined = $state();
  let editInlineSelectEl: HTMLSelectElement | undefined = $state();

  const EDIT_BANNER_KEY = `arbor:${config.formatId}-studio:edit-warning-dismissed`;
  let editBannerVisible = $state(false);

  function maybeShowEditBanner() {
    if (typeof localStorage === 'undefined') return;
    if (localStorage.getItem(EDIT_BANNER_KEY) !== '1') editBannerVisible = true;
  }
  function dismissEditBanner() {
    editBannerVisible = false;
    try { localStorage.setItem(EDIT_BANNER_KEY, '1'); } catch { /* ignore */ }
  }

  function focusInlineEditor() {
    queueMicrotask(() => queueMicrotask(() => {
      const el = editInlineEl ?? editInlineSelectEl;
      el?.focus();
      if (el instanceof HTMLInputElement) el.select();
    }));
  }

  function startEdit(node: TNode | null, location: EditLocation = 'detail') {
    if (!node) return;
    // Variant-typed strings reroute to the variant picker.
    if (config.rowEditMode(node) === 'variant') {
      startVariantEdit(node, location);
      return;
    }
    const canPrimitive = config.isEditablePrimitive(node.kind);
    const canNull      = config.isPromotableNull?.(node.kind) ?? false;
    if (!canPrimitive && !canNull) return;
    editBuf      = config.computeSeed(node);
    editError    = null;
    editingPid   = node.pid;
    editLocation = location;
    maybeShowEditBanner();
    if (location === 'detail') {
      config.focusInspector?.();
    } else {
      focusInlineEditor();
    }
  }

  function startVariantEdit(node: TNode | null, location: EditLocation = 'detail') {
    if (!node) return;
    editBuf      = config.currentVariantTag(node);
    editError    = null;
    editingPid   = node.pid;
    editLocation = location;
    maybeShowEditBanner();
    if (location === 'detail') {
      config.focusInspector?.();
    } else {
      queueMicrotask(() => queueMicrotask(() => { editInlineSelectEl?.focus(); }));
    }
  }

  function cancelEdit() {
    editingPid = null;
    editError  = null;
  }

  async function runCommit(node: TNode): Promise<void> {
    if (!editingPid) return;
    try {
      const r = await config.commit(node, editBuf);
      if (r.error) { editError = r.error; return; }
      editingPid = null;
      editError  = null;
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
  }

  async function runCommitVariant(node: TNode): Promise<void> {
    if (!editingPid) return;
    const current = config.currentVariantTag(node);
    const name    = editBuf;
    // Clear editing state up front to match the original behaviour:
    // the variant pick happens via a select, which fires onchange before
    // any user-visible "still editing" hint.
    editingPid = null;
    editError  = null;
    if (!name || name === current) return;
    try {
      const r = await config.commitVariant(node, name);
      if (r.error) editError = r.error;
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
  }

  function onEditKey(e: KeyboardEvent, node: TNode | null) {
    if (e.key === 'Enter') {
      e.preventDefault(); e.stopPropagation();
      if (!node) return;
      if (config.rowEditMode(node) === 'variant') void runCommitVariant(node);
      else                                         void runCommit(node);
    } else if (e.key === 'Escape') {
      e.preventDefault(); e.stopPropagation();
      cancelEdit();
    }
  }

  async function maybeCommitActiveEdit(node: TNode | null): Promise<void> {
    if (!editingPid || !node) return;
    if (config.rowEditMode(node) === 'variant') await runCommitVariant(node);
    else                                         await runCommit(node);
  }

  return {
    get editingPid()        { return editingPid; },
    get editLocation()      { return editLocation; },
    get editBuf()           { return editBuf; },
    set editBuf(v: string)  { editBuf = v; },
    get editError()         { return editError; },
    get editBannerVisible() { return editBannerVisible; },

    get editInlineEl()                              { return editInlineEl; },
    set editInlineEl(v: HTMLInputElement | undefined) { editInlineEl = v; },
    get editInlineSelectEl()                              { return editInlineSelectEl; },
    set editInlineSelectEl(v: HTMLSelectElement | undefined) { editInlineSelectEl = v; },

    startEdit,
    startVariantEdit,
    cancelEdit,
    onEditKey,
    maybeCommitActiveEdit,
    runCommit,
    runCommitVariant,
    maybeShowEditBanner,
    dismissEditBanner,
    setEditError(msg: string | null) { editError = msg; },
  };
}
