<!--
  JsonStudioModal — JSON wrapper around the generic `<StudioModal>` shell.

  Phase 3.b.2 ports the JSON modal to the same architecture RON moved to
  in Phase 2B-2.g: the modal is a thin wrapper that owns format-specific
  state (single-doc store, save split, mutations) and renders the shell
  via snippet props. Tree, Inspector, Text (CodeMirror), Diff, Errors and
  Query all flow through their shared studio panes.

  Capabilities exposed to the user (Phase 3.b.2 scope):
    · Tree view with mutations (edit primitives, remove, add field/item,
      duplicate, move, paste-over).
    · Text view via `<StudioTextPane>` (CodeMirror 6 + JSON language) —
      typing pushes through the host's lossless byte-splice editor.
    · Diff view via `<StudioDiffPane>` (text + tree sub-views).
    · Errors view — inline banner with the host's parse error.
    · Inspector + Query right-rail panes.
    · Footer: parse / dirty / saved pill, encoding pill, undo / redo,
      indent picker, Format, Save split.

  NOT in 3.b.2 (deferred):
    · Cross-refs / broken-refs / F12 rename / F13 bulk-edit → Phase 3.c.
    · JSON Schema sidecar → Phase 3.c.
    · JSONC support (`.jsonc` association, comment-preserving editor) →
      Phase 3.d.
    · Multi-tab workspace — JSON Studio remains single-doc by design.
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';
  import {
    FileJson, X, Copy, ListTree, FileText, AlertCircle, GitCompare,
    ChevronRight, ChevronUp, ChevronDown, Replace,
    Pencil, Check, RotateCcw, ClipboardPaste,
    Trash2, Plus, CopyPlus, ArrowUp, ArrowDown,
    Maximize2, Minimize2,
    ListFilter, ScanSearch, Layers,
    Loader2, ChevronsDown, ChevronsUp, Link as LinkIcon,
    BookOpen, ArrowUpRight, FileText as FileTextIcon,
    Wrench,
  } from 'lucide-svelte';
  import Spinner from './ui/Spinner.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import { type MenuItem } from './ContextMenu.svelte';
  import { type RowSnippetCtx } from './ui/Tree.svelte';
  import Dropdown from './ui/Dropdown.svelte';
  import { type TabItem } from './ui/Tabs.svelte';
  import Alert from './ui/Alert.svelte';
  import StudioModal from './studio/StudioModal.svelte';
  import StudioFooterStatus   from './studio/StudioFooterStatus.svelte';
  import StudioFooterRight    from './studio/StudioFooterRight.svelte';
  import StudioBodyBanners    from './studio/StudioBodyBanners.svelte';
  import StudioHeaderUndoRedo from './studio/StudioHeaderUndoRedo.svelte';
  import StudioToolsSidebar   from './studio/StudioToolsSidebar.svelte';
  import {
    INDENT_OPTIONS_WITH_8,
    type StudioFooterDoc,
  } from './studio/studio-footer-types';
  import { basename as fsBasename, fmtBytes as fsFmtBytes } from './studio/helpers';
  import StudioQueryBar from './studio/StudioQueryBar.svelte';
  import StudioTextPane from './studio/StudioTextPane.svelte';
  import StudioDiffPane, { type StudioDiffPaneController } from './studio/StudioDiffPane.svelte';
  import StudioTreePane, { type StudioTreePaneController } from './studio/StudioTreePane.svelte';
  import StudioInspectorPanel, {
    type StudioInspectorPanelController,
  } from './studio/StudioInspectorPanel.svelte';
  import StudioRenameModal from './studio/StudioRenameModal.svelte';
  import StudioRefsPanel from './studio/StudioRefsPanel.svelte';
  import StudioSchemaPanel from './studio/StudioSchemaPanel.svelte';
  import StudioBulkEditModal from './studio/StudioBulkEditModal.svelte';
  import StudioViewSourceModal from './studio/StudioViewSourceModal.svelte';
  import Modal from './Modal.svelte';
  import StateBlock from './ui/StateBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { jsonStudioStore, type JsonNodeKind } from '$lib/stores/json-studio.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import {
    studioBackend,
    type StudioNodeView, type StudioQueryHit, type StudioPrimitiveValue,
    type Schema, type CrateProbe, type TypeSource,
    type ResolvedType, type TypeDef, type VariantDef,
  } from '$lib/ipc/studio-format';
  import type {
    BulkEditOpenDoc, BulkEditResult,
    RenameOpenDoc, RenameResult,
  } from '$lib/types/studio-format';
  import type { CrossRefDef, UsageMatch } from '$lib/ipc/studio';
  // Shared schema-aware walker — serde rename / alias / rename_all /
  // flatten (incl. HashMap<String,V> catch-all).
  import {
    typeAtPath as walkTypeAtPath,
    flattenedStructFields,
  } from '$lib/utils/studio-schema';

  /** Pre-bound JSON backend. Every host IPC for JSON Studio flows
   *  through the unified `studio_*` Tauri commands with `format_id="json"`
   *  baked in. */
  const JSON_BE = studioBackend<JsonNodeKind>('json');

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'query' | 'bindings' | 'schema' | 'tools' | null;

  let viewMode  = $state<ViewMode>('tree');

  const RIGHT_PANE_KEY = 'arbor:json-studio:right-pane';
  function loadRightPane(): RightPane {
    if (typeof localStorage === 'undefined') return 'inspector';
    const v = localStorage.getItem(RIGHT_PANE_KEY) as RightPane;
    return v === 'inspector' || v === 'query' || v === 'bindings' || v === 'schema'
      ? v : 'inspector';
  }
  let rightPane = $state<RightPane>(loadRightPane());

  let studioModal: StudioModal<JsonNodeKind> | undefined = $state();
  let treePane:    StudioTreePaneController<JsonNodeKind, TNode> | undefined = $state();
  let diffPane:    StudioDiffPaneController | undefined = $state();
  let inspectorPanel: StudioInspectorPanelController | undefined = $state();

  function setRightPane(p: RightPane) { studioModal?.setRightPane(p); }

  // ── Tree state ──────────────────────────────────────────────────────────
  type JsonNodeView = StudioNodeView<JsonNodeKind>;
  type JsonQueryHit = StudioQueryHit<JsonNodeKind>;
  type TNode = JsonNodeView & {
    pid:      string;
    children: TNode[] | null;
    loading?: boolean;
  };

  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: JsonNodeView): TNode { return { ...v, pid: pathId(v.path), children: null }; }
  /** JSON has no semantic ordering rule — render children as the
   *  backend emits them (= source order). */
  function sortChildren(_parentKind: JsonNodeKind, kids: TNode[]): TNode[] { return kids; }
  function isContainerKind(k: JsonNodeKind): boolean { return k === 'object' || k === 'array'; }
  function isEditablePrimitive(k: JsonNodeKind): boolean {
    return k === 'string' || k === 'number' || k === 'bool';
  }

  // Bindable tree state — owned by `<StudioTreePane>`; bound here so
  // mutations + Inspector + Query bar all read from the wrapper scope.
  let roots         = $state<TNode[]>([]);
  let expanded      = $state<Set<string>>(new Set());
  let selectedNode  = $state<TNode | null>(null);
  let valueText     = $state<string | null>(null);
  let valueLoading  = $state(false);
  let expandAllBusy = $state(false);

  /** Wrapper-side `selectNode` — forwards to the panel's controller.
   *  Used by `jumpToQueryHit` + post-mutation flows. */
  async function selectNode(node: TNode): Promise<void> {
    await treePane?.selectNode(node);
  }

  async function commitPendingEdit(): Promise<void> {
    if (editingPid && editingPid !== selectedNode?.pid) {
      try { await maybeCommitActiveEdit(); }
      catch { cancelEdit(); }
    }
  }

  // ── Edit pipeline ───────────────────────────────────────────────────────
  let editingPid    = $state<string | null>(null);
  let editLocation  = $state<'tree' | 'detail'>('detail');
  let editBuf       = $state('');
  let editError     = $state<string | null>(null);

  // Inline-editor refs (tree-location). `null` when no inline edit is
  // active. Mirrors RON's `editInlineEl` / `editInlineSelectEl`.
  let editInlineEl:       HTMLInputElement  | undefined = $state();
  let editInlineSelectEl: HTMLSelectElement | undefined = $state();

  function onEditKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault(); e.stopPropagation();
      if (selectedNode && rowEditMode(selectedNode) === 'variant') {
        void commitVariantEdit();
      } else {
        void commitEdit();
      }
    } else if (e.key === 'Escape') {
      e.preventDefault(); e.stopPropagation();
      cancelEdit();
    }
  }

  const EDIT_BANNER_KEY = 'arbor:json-studio:edit-warning-dismissed';
  let editBannerVisible = $state(false);
  function maybeShowEditBanner() {
    if (typeof localStorage === 'undefined') return;
    if (localStorage.getItem(EDIT_BANNER_KEY) !== '1') editBannerVisible = true;
  }
  function dismissEditBanner() {
    editBannerVisible = false;
    try { localStorage.setItem(EDIT_BANNER_KEY, '1'); } catch { /* ignore */ }
  }

  function startEdit(location: 'tree' | 'detail' = 'detail') {
    if (!selectedNode) return;
    // Schema-aware dispatch — enum-typed string nodes go to the
    // variant picker instead of the free-text editor.
    if (selectedNode.kind === 'string' && enumDefAt(selectedNode.path)) {
      startVariantEdit(location);
      return;
    }
    if (!isEditablePrimitive(selectedNode.kind)) return;
    let seed = valueText ?? selectedNode.preview;
    if (selectedNode.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
      // Strip the surrounding quotes for the input — we re-add them
      // through the `string` primitive serialisation. JSON escapes
      // (`\"`, `\\`, …) survive the round-trip via the backend.
      try {
        seed = JSON.parse(seed) as string;
      } catch {
        seed = seed.slice(1, -1);
      }
    }
    editBuf      = seed;
    editError    = null;
    editingPid   = selectedNode.pid;
    editLocation = location;
    maybeShowEditBanner();
    if (location === 'detail') {
      inspectorPanel?.focusEditInput();
    } else {
      // Double microtask — the inline input is rendered inside an
      // `{#if}` branch that only mounts AFTER editingPid + editLocation
      // state writes propagate to the row snippet.
      queueMicrotask(() => queueMicrotask(() => {
        const el = editInlineEl ?? editInlineSelectEl;
        el?.focus();
        if (el instanceof HTMLInputElement) el.select();
      }));
    }
  }

  function cancelEdit() {
    editingPid = null;
    editError  = null;
  }

  async function commitEdit(): Promise<void> {
    if (!selectedNode || !editingPid) return;
    const node = selectedNode;
    // Schema-aware typed-input narrowing: when the schema constrains
    // this position to a specific primitive, reject input that doesn't
    // match (e.g. `1.5` on an `integer` field). Skips when no schema.
    const hint = schema ? primitiveHintAt(node.path) : null;
    const wantInt    = hint === 'integer';
    const wantNum    = hint === 'number';
    const wantBool   = hint === 'boolean';
    const wantString = hint === 'string';

    let value: StudioPrimitiveValue;
    try {
      switch (node.kind) {
        case 'string':
          if (wantBool) {
            const t = editBuf.trim().toLowerCase();
            if (t !== 'true' && t !== 'false') throw new Error('schema: expected boolean');
            value = { type: 'bool', value: t === 'true' };
            break;
          }
          if (wantInt) {
            const n = Number(editBuf.trim());
            if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          if (wantNum) {
            const n = Number(editBuf.trim());
            if (!Number.isFinite(n)) throw new Error('schema: expected number');
            value = { type: 'float', value: n };
            break;
          }
          value = { type: 'string', value: editBuf };
          break;
        case 'bool': {
          const t = editBuf.trim().toLowerCase();
          if (t !== 'true' && t !== 'false') throw new Error('expected "true" or "false"');
          value = { type: 'bool', value: t === 'true' };
          break;
        }
        case 'number': {
          const s = editBuf.trim();
          const n = Number(s);
          if (!Number.isFinite(n)) throw new Error('not a number');
          // Schema integer → reject decimal input.
          if (wantInt) {
            if (!Number.isInteger(n)) throw new Error('schema: expected integer');
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          // Schema string → keep as literal text.
          if (wantString) { value = { type: 'string', value: editBuf }; break; }
          // JSON numbers are unified — `int` vs `float` in the
          // primitive payload only matters as a serialisation hint
          // (preserves trailing `.0` for floats). Use heuristics on
          // the literal: if it has `.` / `e` / `E` it's a float.
          const looksFloat = /[.eE]/.test(s);
          value = looksFloat || wantNum
            ? { type: 'float', value: n }
            : { type: 'int',   value: Math.trunc(n) };
          break;
        }
        default: return;
      }
    } catch (e: any) {
      editError = e?.message ?? String(e);
      return;
    }
    try {
      await jsonStudioStore.mutatePrimitive(node.path, value);
      await refreshAfterMutation(node, /* structural */ false);
      editingPid = null;
      editError  = null;
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
  }

  async function maybeCommitActiveEdit(): Promise<void> {
    if (!editingPid || !selectedNode) return;
    if (rowEditMode(selectedNode) === 'variant') await commitVariantEdit();
    else                                          await commitEdit();
  }

  async function refreshAfterMutation(node: TNode, structural: boolean, removed = false): Promise<void> {
    await treePane?.refreshAfterMutation(node, structural, removed);
  }

  // ── Removability + remove action ────────────────────────────────────────
  function isRemovable(node: TNode | null): boolean {
    if (!node || node.path.length === 0) return false;
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent) return false;
    return parent.kind === 'object' || parent.kind === 'array';
  }

  async function removeSelected(): Promise<void> {
    if (!selectedNode || !isRemovable(selectedNode)) return;
    const node = selectedNode;
    try {
      await jsonStudioStore.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('json-studio: removeAt failed', e);
    }
  }

  // ── Container mutations ─────────────────────────────────────────────────
  async function addItemAction(parent: TNode): Promise<void> {
    if (parent.kind !== 'array') return;
    try {
      await jsonStudioStore.insertItem(parent.path, 'null');
      await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('json-studio: insertItem failed', e);
    }
  }

  async function addFieldAction(parent: TNode, name?: string): Promise<void> {
    if (parent.kind !== 'object') return;
    let key = name ?? '';
    if (!key) {
      const proposed = window.prompt('Field name:', 'new_field');
      if (!proposed) return;
      key = proposed;
    }
    try {
      await jsonStudioStore.insertField(parent.path, key, 'null');
      await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('json-studio: insertField failed', e);
    }
  }

  async function duplicateAction(node: TNode): Promise<void> {
    if (!isRemovable(node)) return;
    try {
      await jsonStudioStore.duplicateAt(node.path);
      const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('json-studio: duplicateAt failed', e);
    }
  }

  async function moveAction(node: TNode, delta: number): Promise<void> {
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent || parent.kind !== 'array') return;
    try {
      await jsonStudioStore.moveItem(node.path, delta);
      await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('json-studio: moveItem failed', e);
    }
  }

  async function pasteOverAction(node: TNode): Promise<void> {
    let text: string;
    try { text = await navigator.clipboard.readText(); }
    catch { uiStore.showToast('Clipboard read denied', 'error'); return; }
    const t = text.trim();
    if (!t) { uiStore.showToast('Clipboard is empty', 'error'); return; }
    try {
      await jsonStudioStore.replaceAt(node.path, t);
      await refreshAfterMutation(node, /* structural */ true);
      maybeShowEditBanner();
    } catch (e: any) {
      uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error');
    }
  }

  // ── F12 — Cross-refs / Rename across project ──────────────────────────
  //
  // Mirrors the RON wrapper's wiring: tree string nodes that are either
  // `id`/`name` definitions or reference fields can be renamed across
  // every JSON file in the project. The host index lives behind the
  // shared `studioStore` (per-kind cache since Phase 3.c).

  /** Strip surrounding `"` from a preview snippet. Returns `null` for
   *  truncated previews (the host emits `"abc…"` for long strings),
   *  so callers don't accidentally rename a half-string. */
  function unquotedString(preview: string): string | null {
    if (preview.length < 2) return null;
    if (!preview.startsWith('"') || !preview.endsWith('"')) return null;
    const inner = preview.slice(1, -1);
    if (inner.endsWith('…')) return null;
    return inner;
  }

  function isDefinitionFieldName(key: string): boolean {
    return key === 'id' || key === 'name';
  }

  /** Built-in JSON ref convention — mirrors `studio::index::matches_reference`
   *  for the FE. Project `.studio.toml` patterns override via the shared
   *  store's `referenceFieldsFor`. */
  function builtinIsReferenceField(key: string): boolean {
    return key === 'target' || key === 'source' || key === 'parent'
        || key === 'owner'  || key === 'prev'   || key === 'next'
        || key.endsWith('_id') || key.endsWith('_ref')
        || key.endsWith('Id')  || key.endsWith('Ref');
  }

  function relPathInRepo(absPath: string | null): string | null {
    if (!absPath) return null;
    const norm = absPath.replace(/\\/g, '/');
    const hit = studioStore.files.find(f => f.absolute_path.replace(/\\/g, '/') === norm);
    return hit ? hit.relative_path : null;
  }

  function isReferenceFieldName(key: string): boolean {
    const repoRel = relPathInRepo(jsonStudioStore.sourcePath);
    const patterns = repoRel ? studioStore.referenceFieldsFor(repoRel) : null;
    if (!patterns) return builtinIsReferenceField(key);
    return patterns.some(p => studioStore.matchesPattern(p, key));
  }

  /** For a string node, work out which key it sits under. A scalar
   *  inside a list inherits the list's key (so `tags: [..., "x"]`
   *  reads through `tags`). Mirrors RON's `refFieldNameForNode`. */
  function refFieldNameForNode(node: TNode): string | null {
    if (node.kind !== 'string') return null;
    const idx = parseInt(node.key, 10);
    if (Number.isInteger(idx) && String(idx) === node.key && node.path.length >= 2) {
      return node.path[node.path.length - 2];
    }
    return node.key;
  }

  function isRenameableTreeNode(n: TNode): boolean {
    if (n.kind !== 'string') return false;
    const v = unquotedString(n.preview);
    if (!v) return false;
    if (isDefinitionFieldName(n.key)) return true;
    const ref = refFieldNameForNode(n);
    return !!ref && isReferenceFieldName(ref);
  }

  function isDefinitionNode(n: TNode): boolean {
    return n.kind === 'string' && isDefinitionFieldName(n.key) && !!unquotedString(n.preview);
  }
  function definitionValue(n: TNode): string | null {
    if (!isDefinitionNode(n)) return null;
    return unquotedString(n.preview);
  }

  let renameModalState = $state<{ oldValue: string } | null>(null);

  function openRenameModal(node: TNode): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Rename across project',
        'No active project — open this JSON file from a project tab to rename across files.',
        'warning',
      );
      return;
    }
    const value = unquotedString(node.preview);
    if (!value) return;
    renameModalState = { oldValue: value };
  }

  function closeRenameModal(): void { renameModalState = null; }

  /** JSON Studio is single-doc by design — the only doc that can be
   *  blocked by the dirty check is the currently-open one. */
  function buildOpenDocsSnapshot(): RenameOpenDoc[] {
    if (!jsonStudioStore.docId) return [];
    return [{
      doc_id:      jsonStudioStore.docId,
      source_path: jsonStudioStore.sourcePath,
      dirty:       jsonStudioStore.dirty,
    }];
  }

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = jsonStudioStore.sourcePath;
    if (!path) return;
    const title = jsonStudioStore.title;
    await jsonStudioStore.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  async function onRenameApplied(result: RenameResult): Promise<void> {
    closeRenameModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    const sp = jsonStudioStore.sourcePath;
    if (sp) {
      const norm = sp.replace(/\\/g, '/').toLowerCase();
      const touched = written.some(p => p.replace(/\\/g, '/').toLowerCase() === norm);
      if (touched) {
        try { await reloadActiveDocFromDisk(); }
        catch (e) { console.warn('rename: active doc reload failed', e); }
      }
    }

    const aTab = tabsStore.activeTabId;
    if (aTab) {
      try { await studioStore.loadCrossRefsForKind(aTab, 'json', true); } catch { /* soft */ }
      try { await studioStore.refreshIndex(aTab); }                      catch { /* soft */ }
    }

    if (failed.length === 0) {
      notificationsStore.add(
        'Rename across project',
        `Renamed in ${written.length} ${written.length === 1 ? 'file' : 'files'}.`,
        'success',
      );
    } else {
      const lines = failed.map(f => `· ${f.absolute_path}: ${f.message}`).join('\n');
      notificationsStore.add(
        'Rename across project',
        `Renamed in ${written.length} ${written.length === 1 ? 'file' : 'files'}, `
          + `but ${failed.length} ${failed.length === 1 ? 'file' : 'files'} could not be written:\n${lines}`,
        'warning',
      );
    }
  }

  /** Used-by jump — same-file paths jump inline; cross-file paths
   *  swap the single JSON doc to the new file. */
  async function jumpToUsage(hit: UsageMatch): Promise<void> {
    const sp = jsonStudioStore.sourcePath;
    const sameFile = sp && hit.absolute_path.replace(/\\/g, '/').toLowerCase()
                       === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(hit.field_path);
      return;
    }
    try {
      await jsonStudioStore.openDoc({ path: hit.absolute_path });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(hit.field_path);
    } catch (e) {
      console.warn('jumpToUsage: open target failed', e);
    }
  }

  async function openDefinition(d: CrossRefDef): Promise<void> {
    const sp = jsonStudioStore.sourcePath;
    const sameFile = sp && d.absolute_path.replace(/\\/g, '/').toLowerCase()
                       === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(d.def_path);
      return;
    }
    try {
      await jsonStudioStore.openDoc({ path: d.absolute_path });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(d.def_path);
    } catch (e) {
      console.warn('openDefinition: open target failed', e);
    }
  }

  // ── F13 — Bulk edit by query ─────────────────────────────────────────
  //
  // The query bar surfaces the `[⚡ Edit]` button when the descriptor's
  // `supports_bulk_edit` is true. Clicking it routes through here; the
  // generic `<StudioBulkEditModal>` owns the preview/apply lifecycle.
  // JSON's descriptor declares `null_handling = native`, so the modal
  // offers `null` as a first-class literal value source.

  let bulkEditModalState = $state<{ query: string } | null>(null);

  function openBulkEditModal(q: string): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Bulk edit by query',
        'No active project — open this JSON file from a project tab to run a bulk edit.',
        'warning',
      );
      return;
    }
    if (!jsonStudioStore.docId) return;
    if (!q) return;
    bulkEditModalState = { query: q };
  }
  function closeBulkEditModal(): void { bulkEditModalState = null; }

  function buildBulkEditOpenDocs(): BulkEditOpenDoc[] {
    return buildOpenDocsSnapshot();
  }

  async function onBulkEditApplied(result: BulkEditResult): Promise<void> {
    closeBulkEditModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    if (result.active_doc_state) {
      try {
        await jsonStudioStore.applyExternalMutate(result.active_doc_state);
        await treePane?.reloadTree();
      } catch (e) {
        console.warn('bulk edit: active-doc sync failed', e);
      }
    } else {
      // Project-wide — JSON Studio is single-doc; reload from disk if
      // the active doc's source got rewritten.
      const sp = jsonStudioStore.sourcePath;
      if (sp) {
        const norm = sp.replace(/\\/g, '/').toLowerCase();
        const touched = written.some(p => p.replace(/\\/g, '/').toLowerCase() === norm);
        if (touched) {
          try { await reloadActiveDocFromDisk(); }
          catch (e) { console.warn('bulk edit: active doc reload failed', e); }
        }
      }
      const aTab = tabsStore.activeTabId;
      if (aTab) {
        try { await studioStore.loadCrossRefsForKind(aTab, 'json', true); } catch { /* soft */ }
        try { await studioStore.refreshIndex(aTab); }                      catch { /* soft */ }
      }
    }

    const appliedTxt = `${result.applied_sites} ${result.applied_sites === 1 ? 'site' : 'sites'}`;
    const skippedTxt = result.skipped_sites > 0
      ? ` (${result.skipped_sites} skipped)`
      : '';
    if (failed.length === 0) {
      notificationsStore.add(
        'Bulk edit',
        result.active_doc_state
          ? `Applied to ${appliedTxt}${skippedTxt} in this doc.`
          : `Applied to ${appliedTxt}${skippedTxt} across ${written.length} ${written.length === 1 ? 'file' : 'files'}.`,
        'success',
      );
    } else {
      const lines = failed.map(f => `· ${f.absolute_path}: ${f.message}`).join('\n');
      notificationsStore.add(
        'Bulk edit',
        `Applied to ${appliedTxt}${skippedTxt} across ${written.length} ${written.length === 1 ? 'file' : 'files'}, `
          + `but ${failed.length} ${failed.length === 1 ? 'file' : 'files'} could not be written:\n${lines}`,
        'warning',
      );
    }
  }

  // ── JSON Schema sidecar ──────────────────────────────────────────────
  //
  // Phase 3.c.b mirror of RON Studio's crate-schema panel, but the
  // source-of-truth is a `*.schema.json` file the user picks. The
  // backend parses it via `jsonc-parser` / `serde_json`, resolves
  // `$ref` chains, and emits the same `Schema` / `CrateProbe` /
  // `TypeSource` shapes the RON path produces — so `<StudioSchemaPanel>`
  // works unchanged.

  let schema:        Schema      | null = $state(null);
  let schemaProbe:   CrateProbe  | null = $state(null);
  let schemaRsPath:  string      | null = $state(null);
  let schemaRootSel: string      | null = $state(null);
  let schemaLoading             = $state(false);
  let schemaError:   string | null = $state(null);

  async function probeSchemaSource(rsPath: string): Promise<void> {
    schemaLoading = true;
    schemaError   = null;
    try {
      const probe = await JSON_BE.schemaProbe(rsPath);
      schemaProbe   = probe;
      schemaRsPath  = rsPath;
      schemaRootSel = probe.root_candidates[0]?.canonical_path ?? null;
      schema        = null;
    } catch (e) {
      schemaError = String(e);
      schemaProbe = null;
      schemaRootSel = null;
    } finally {
      schemaLoading = false;
    }
  }

  function setSchemaRoot(canonical: string): void { schemaRootSel = canonical; }

  async function loadSchemaForRoot(): Promise<void> {
    if (!schemaRsPath || !schemaRootSel) return;
    schemaLoading = true;
    schemaError   = null;
    try {
      schema = await JSON_BE.schemaLoad(schemaRsPath, schemaRootSel);
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }

  function clearSchema(): void {
    schema = null;
    schemaProbe = null;
    schemaRsPath = null;
    schemaRootSel = null;
    schemaError = null;
  }

  // ── View source (JSON Schema fragment) ───────────────────────────────
  let viewSource:    TypeSource | null = $state(null);
  let viewSourceBusy                  = $state(false);
  let viewSourceErr: string | null    = $state(null);

  async function openViewSource(canonical: string): Promise<void> {
    if (!schemaRsPath) return;
    viewSourceBusy = true;
    viewSourceErr  = null;
    viewSource     = null;
    try {
      viewSource = await JSON_BE.schemaViewSource(schemaRsPath, canonical);
    } catch (e) {
      viewSourceErr = String(e);
    } finally {
      viewSourceBusy = false;
    }
  }
  function closeViewSource(): void {
    viewSource = null;
    viewSourceErr = null;
  }

  // ── Schema-aware type walker (parity with RON / TOML) ───────────────
  //
  // Projects `schema.root_type` along the structural path of a JSON
  // node. Used by tree-row type chips, variant picker, typed-input
  // narrowing, and the Inspector's schema strip.

  // Delegates to the shared schema walker (`studio-schema.ts`) so
  // serde rename / alias / rename_all / flatten work uniformly.
  function typeAtPath(path: string[]): ResolvedType | null {
    return walkTypeAtPath(schema, path);
  }

  function enumDefAt(path: string[]): (TypeDef & { kind: 'enum' }) | null {
    if (!schema) return null;
    let ty = typeAtPath(path);
    if (!ty) return null;
    if (ty.kind === 'option') ty = ty.inner;
    if (ty.kind !== 'named') return null;
    const def = schema.types[ty.path];
    if (!def || def.kind !== 'enum') return null;
    return def;
  }

  function primitiveHintAt(path: string[]): string | null {
    let ty = typeAtPath(path);
    if (!ty) return null;
    if (ty.kind === 'option') ty = ty.inner;
    if (ty.kind === 'primitive') return ty.name;
    return null;
  }

  function rowEditMode(node: TNode): 'primitive' | 'variant' | null {
    if (node.kind === 'string') {
      const ed = enumDefAt(node.path);
      if (ed && ed.variants.length > 0 && ed.variants.every(v => v.shape === 'unit')) {
        return 'variant';
      }
    }
    if (isEditablePrimitive(node.kind)) return 'primitive';
    return null;
  }

  function currentVariantTag(node: TNode): string {
    if (node.kind !== 'string') return '';
    const v = unquotedString(node.preview);
    return v ?? '';
  }

  function startVariantEdit(location: 'tree' | 'detail' = 'detail'): void {
    if (!selectedNode) return;
    const ed = enumDefAt(selectedNode.path);
    if (!ed) return;
    editBuf      = currentVariantTag(selectedNode);
    editError    = null;
    editingPid   = selectedNode.pid;
    editLocation = location;
    maybeShowEditBanner();
    queueMicrotask(() => queueMicrotask(() => {
      editInlineSelectEl?.focus();
    }));
  }

  async function commitVariantEdit(): Promise<void> {
    if (!editingPid || !selectedNode) return;
    const node = selectedNode;
    const name = editBuf;
    const current = currentVariantTag(node);
    editingPid = null;
    editError  = null;
    if (!name || name === current) return;
    try {
      await jsonStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
  }

  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || selectedNode.kind !== 'string') return;
    const current = currentVariantTag(selectedNode);
    if (!name || name === current) return;
    const node = selectedNode;
    try {
      await jsonStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
  }

  // Render-helpers shared with the Inspector schema strip + type chips.
  function fmtType(ty: ResolvedType | null): string {
    if (!ty) return '';
    switch (ty.kind) {
      case 'primitive': return ty.name;
      case 'option':    return `Option<${fmtType(ty.inner)}>`;
      case 'vec':       return `Vec<${fmtType(ty.inner)}>`;
      case 'map':       return `Map<${fmtType(ty.key)}, ${fmtType(ty.value)}>`;
      case 'tuple':     return `(${ty.items.map(fmtType).join(', ')})`;
      case 'named':     return ty.path.replace(/^crate::/, '').replace(/^#\//, '');
      case 'external':  return ty.path + ' (external)';
      case 'unknown':   return `? ${ty.hint}`;
    }
  }

  function typeChipClass(ty: ResolvedType | null): string {
    if (!ty) return '';
    switch (ty.kind) {
      case 'primitive': return 'js-type-prim';
      case 'option':    return 'js-type-option';
      case 'vec':       return 'js-type-vec';
      case 'map':       return 'js-type-map';
      case 'tuple':     return 'js-type-tupletype';
      case 'external':  return 'js-type-external';
      case 'unknown':   return 'js-type-unknown';
      default:          return '';
    }
  }

  function namedTypeAt(path: string[]): string | null {
    if (!schema) return null;
    const ty = typeAtPath(path);
    if (!ty) return null;
    const named = ty.kind === 'named' ? ty
                : ty.kind === 'option' && ty.inner.kind === 'named' ? ty.inner
                : null;
    if (!named) return null;
    const p = named.path.replace(/^crate::/, '').replace(/^#\//, '');
    return p.split('/').pop()?.split('::').pop() ?? null;
  }

  function inspectorSchemaTypeInfo(node: TNode) {
    if (!schema) return null;
    const ty = typeAtPath(node.path);
    if (!ty) return null;
    return {
      label:      fmtType(ty),
      isUnknown:  ty.kind === 'unknown',
      isExternal: ty.kind === 'external',
    };
  }

  function inspectorVariantPickerInfo(node: TNode) {
    if (!schema || node.kind !== 'string') return null;
    const def = enumDefAt(node.path);
    if (!def || def.variants.length === 0) return null;
    return {
      enumName:   def.name,
      currentTag: currentVariantTag(node),
      variants:   def.variants.map((v: VariantDef) => ({
        name:   v.name,
        suffix: v.shape === 'unit' ? '' : v.shape === 'tuple' ? '(…)' : ' { … }',
      })),
    };
  }

  function inspectorMissingFields(node: TNode) {
    if (!schema) return [];
    const ty = typeAtPath(node.path);
    if (!ty || ty.kind !== 'named') return [];
    const def = schema.types[ty.path];
    if (!def || def.kind !== 'struct') return [];
    // Include flattened sub-struct fields; match by serialised name OR
    // alias so a doc that hand-typed the Rust ident doesn't double-list.
    const seenSegs = new Set((node.children ?? []).map((c: TNode) => c.key));
    return flattenedStructFields(schema, def)
      .filter(f => !seenSegs.has(f.name) && !(f.aliases ?? []).some(a => seenSegs.has(a)))
      .map(f => ({
        name:       f.name,
        typeLabel:  fmtType(f.ty),
        hasDefault: f.has_default,
      }));
  }

  // ── Cross-reference click affordance (parity with RON Studio) ────────
  type CrossRefEntry = {
    sourcePath: string;
    fileName:   string;
    defPath:    string[];
    title:      string;
  };

  function crossRefsForValue(value: string): CrossRefEntry[] {
    return studioStore.findCrossRefsForKind(value, 'json').map(d => ({
      sourcePath: d.absolute_path,
      fileName:   d.file_name,
      defPath:    (d.def_path && d.def_path.length > 0) ? d.def_path : [d.def_field],
      title:      d.file_name,
    }));
  }

  function crossRefsForNode(node: TNode): CrossRefEntry[] {
    if (node.kind !== 'string') return [];
    const fieldName = refFieldNameForNode(node);
    if (!fieldName) return [];
    if (!isReferenceFieldName(fieldName)) return [];
    const value = unquotedString(node.preview);
    if (!value) return [];
    return crossRefsForValue(value);
  }

  let crossRefPicker = $state<{ x: number; y: number; entries: CrossRefEntry[] } | null>(null);

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() { node.parentNode?.removeChild(node); },
    };
  }

  async function jumpToCrossRef(target: CrossRefEntry): Promise<void> {
    crossRefPicker = null;
    const sp = jsonStudioStore.sourcePath;
    const sameFile = sp && target.sourcePath.replace(/\\/g, '/').toLowerCase()
                      === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(target.defPath);
      return;
    }
    try {
      await jsonStudioStore.openDoc({ path: target.sourcePath });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(target.defPath);
    } catch (e) {
      console.warn('jumpToCrossRef: open target failed', e);
    }
  }

  function onCrossRefClick(entries: CrossRefEntry[], e: MouseEvent): void {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    e.stopPropagation();
    if (entries.length === 1) {
      void jumpToCrossRef(entries[0]);
    } else if (entries.length > 1) {
      crossRefPicker = { x: e.clientX, y: e.clientY, entries };
    }
  }

  // ── Inspector → Tree adapters ──────────────────────────────────────────
  async function copyPathOf(node: TNode): Promise<void> {
    const text = node.path.length === 0 ? '$' : '$.' + node.path.join('.');
    try {
      await navigator.clipboard.writeText(text);
      uiStore.showToast('Path copied', 'success');
    } catch (err) {
      uiStore.showToast(`Copy failed: ${err}`, 'error');
    }
  }
  async function copyValue(): Promise<void> {
    if (valueText == null) return;
    try { await navigator.clipboard.writeText(valueText); } catch {}
  }
  async function inspectorAddField(parent: TNode, name: string): Promise<void> {
    await addFieldAction(parent, name);
  }
  function noopOption(): Promise<void> | void { /* JSON has no Option */ }

  // ── Context menu ────────────────────────────────────────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({ id: 'copy-path',  label: 'Copy path',         icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: 'Copy value (JSON)', icon: Copy,     iconColor: 'var(--text-muted)' });

    // Phase 3.d — stream-mode docs (large files) keep navigation
    // affordances but skip the structural-edit block. Mutations would
    // error host-side anyway; hiding the menu items beats surfacing
    // disabled rows.
    const editable = !jsonStudioStore.streamMode;

    if (editable) {
      const editMode = rowEditMode(node);
      if (editMode === 'variant') {
        items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
        items.push({ id: 'edit-variant', label: 'Change variant…', icon: Replace, iconColor: '#ffc66d', shortcut: 'F2' });
      } else if (editMode === 'primitive') {
        items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
        items.push({ id: 'edit', label: 'Edit value', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
      }
    }

    // Schema "View implementation" — gated on the node resolving to a
    // named type that's part of the loaded schema.
    if (schema) {
      const ty = typeAtPath(node.path);
      let namedPath: string | null = null;
      if (ty?.kind === 'named') namedPath = ty.path;
      else if (ty?.kind === 'option' && ty.inner.kind === 'named') namedPath = ty.inner.path;
      if (namedPath && schema.types[namedPath]) {
        items.push({ id: 'sep-schema', label: '', separator: true } as MenuItem);
        items.push({ id: 'view-impl', label: 'View implementation', icon: BookOpen, iconColor: '#20b2aa' });
      }
    }

    if (editable) {
      items.push({ id: 'sep-mutate', label: '', separator: true } as MenuItem);
      items.push({ id: 'paste', label: 'Paste JSON over value…', icon: ClipboardPaste, iconColor: 'var(--text-muted)' });

      if (node.kind === 'object') {
        items.push({ id: 'add-field', label: 'Add field…', icon: Plus, iconColor: 'var(--success)' });
      } else if (node.kind === 'array') {
        items.push({ id: 'add-item', label: 'Add item', icon: Plus, iconColor: 'var(--success)' });
      }

      const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
      if (parent && (parent.kind === 'object' || parent.kind === 'array')) {
        items.push({ id: 'sep-reorder', label: '', separator: true } as MenuItem);
        items.push({ id: 'duplicate', label: 'Duplicate', icon: CopyPlus, iconColor: 'var(--text-muted)' });
        if (parent.kind === 'array') {
          const idx = parseInt(node.key, 10);
          const total = parent.child_count;
          items.push({ id: 'move-up',   label: 'Move up',   icon: ArrowUp,   iconColor: 'var(--text-muted)',
                       disabled: !Number.isFinite(idx) || idx <= 0 });
          items.push({ id: 'move-down', label: 'Move down', icon: ArrowDown, iconColor: 'var(--text-muted)',
                       disabled: !Number.isFinite(idx) || idx >= total - 1 });
        }
      }
    }

    if (node.child_count > 0) {
      items.push({ id: 'sep-expand', label: '', separator: true } as MenuItem);
      items.push({
        id:        expanded.has(node.pid) ? 'collapse' : 'expand',
        label:     expanded.has(node.pid) ? 'Collapse' : 'Expand',
        icon:      expanded.has(node.pid) ? ChevronUp : ChevronDown,
        iconColor: 'var(--text-muted)',
      });
      items.push({ id: 'expand-all',   label: 'Expand subtree',   icon: Maximize2, iconColor: 'var(--text-muted)' });
      items.push({ id: 'collapse-all', label: 'Collapse subtree', icon: Minimize2, iconColor: 'var(--text-muted)' });
    }

    if (editable && isRemovable(node)) {
      items.push({ id: 'sep-remove', label: '', separator: true } as MenuItem);
      items.push({ id: 'remove', label: 'Remove', icon: Trash2, danger: true });
    }

    // F12 — Rename across project. Gated on (a) an active project tab,
    // (b) the node being a renameable string (definition or reference),
    // (c) the JSON backend declaring the capability. Last gate is
    // belt-and-braces — the descriptor's `supports_rename_reference`
    // is flipped on in Phase 3.c.a but the modal might be embedded
    // elsewhere later.
    if (tabsStore.activeTabId && isRenameableTreeNode(node)) {
      items.push({ id: 'sep-rename', label: '', separator: true } as MenuItem);
      items.push({
        id:        'rename-across-project',
        label:     'Rename across project…',
        icon:      Replace,
        iconColor: '#ffc66d',
      });
    }
    return items;
  }

  async function onContextMenuSelect(id: string, node: TNode): Promise<void> {
    switch (id) {
      case 'copy-path':    await copyPathOf(node);                        break;
      case 'copy-value':   {
        try {
          const v = await JSON_BE.getValue(jsonStudioStore.docId!, node.path);
          await navigator.clipboard.writeText(v);
        } catch { /* ignore */ }
        break;
      }
      case 'edit':         startEdit('tree');                              break;
      case 'edit-variant': startVariantEdit('tree');                       break;
      case 'view-impl':    {
        const ty = typeAtPath(node.path);
        let p: string | null = null;
        if (ty?.kind === 'named') p = ty.path;
        else if (ty?.kind === 'option' && ty.inner.kind === 'named') p = ty.inner.path;
        if (p) void openViewSource(p);
        break;
      }
      case 'paste':        await pasteOverAction(node);                    break;
      case 'add-field':    await addFieldAction(node);                     break;
      case 'add-item':     await addItemAction(node);                      break;
      case 'duplicate':    await duplicateAction(node);                    break;
      case 'move-up':      await moveAction(node, -1);                     break;
      case 'move-down':    await moveAction(node, +1);                     break;
      case 'expand':       expandNode(node, true);                         break;
      case 'collapse':     expandNode(node, false);                        break;
      case 'expand-all':   await treePane?.expandSubtree(node);            break;
      case 'collapse-all': treePane?.collapseSubtree(node);                break;
      case 'remove':       await removeSelected();                         break;
      case 'rename-across-project': openRenameModal(node);                  break;
    }
  }

  function expandNode(node: TNode, want: boolean): void {
    const next = new Set(expanded);
    if (want) next.add(node.pid); else next.delete(node.pid);
    expanded = next;
  }

  // ── Query bar ───────────────────────────────────────────────────────────
  let knownKeys = $state<Set<string>>(new Set());
  function noteKeys(items: { path: string[]; key?: string }[]) {
    if (items.length === 0) return;
    const next = new Set(knownKeys);
    let changed = false;
    for (const it of items) {
      const candidates: string[] = [];
      if (it.key && !/^\d+$/.test(it.key)) candidates.push(it.key);
      for (const seg of it.path) if (!/^\d+$/.test(seg)) candidates.push(seg);
      for (const c of candidates) if (!next.has(c)) { next.add(c); changed = true; }
    }
    if (changed) knownKeys = next;
  }

  let queryBar: { focus(): void; clear(): void; nav(d: number): void; getHitCount(): number } | undefined = $state();
  let query         = $state('');
  let queryHits     = $state<JsonQueryHit[]>([]);
  let queryError    = $state<string | null>(null);
  let querying      = $state(false);
  let currentHitIdx = $state(0);

  /** Auto-open the query sidebar on first non-empty query — but not
   *  again if the user explicitly closed it via the rail button. */
  let queryAutoOpenDismissed = $state(false);
  function onQueryActiveChange(active: boolean): void {
    if (active && rightPane !== 'query' && !queryAutoOpenDismissed) {
      setRightPane('query');
    }
    if (!active) queryAutoOpenDismissed = false;
  }
  function onQueryToggleRightPane(): void {
    studioModal?.toggleRightPane('query');
  }

  function getChildKeysForPath(path: string[]): string[] | null {
    return treePane?.getChildKeysForPath(path) ?? null;
  }
  function ensureChildrenLoadedForPath(path: string[]): void {
    treePane?.ensureChildrenLoadedForPath(path);
  }
  async function jumpToQueryHit(path: string[]): Promise<void> {
    await treePane?.jumpToPath(path);
  }

  // ── Text view ──────────────────────────────────────────────────────────
  // Mirror the store's `current` into a local buffer so the editor can
  // type freely without round-tripping each keystroke through the host.
  // Debounced push at 180ms matches RON. When `current` updates from
  // outside (mutation, undo / redo, save) we sync the buffer back via
  // `syncTextFromStore`.
  let textBuf = $state<string>('');
  let pushTimer: ReturnType<typeof setTimeout> | null = null;

  function syncTextFromStore() {
    textBuf = jsonStudioStore.current;
  }

  function scheduleTextPush() {
    if (pushTimer) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
      void jsonStudioStore.setText(textBuf).then(() => {
        // Tree pulls from the same parse cache, so a structural reload
        // keeps the visible nodes in sync with the typed text. No-op
        // when the doc fails to parse; the user gets the inline parse
        // error pill in the footer instead.
        void treePane?.reloadTree();
      });
    }, 180);
  }

  function onTextInput(next: string) {
    textBuf = next;
    scheduleTextPush();
  }

  // External store changes → editor buffer. Tracks `current` only; the
  // body is wrapped in `untrack` so `textBuf` writes don't add as deps
  // and `bumpDiffRefresh`'s read-modify-write doesn't loop. Diff auto-
  // refresh on `currentText` change is wired inside StudioDiffPane —
  // we don't need to bump a tick here.
  $effect(() => {
    const c = jsonStudioStore.current;
    untrack(() => { textBuf = c; });
  });

  // Doc lifecycle — reset view / query / known keys when docId clears
  // OR a new doc opens.
  $effect(() => {
    const id = jsonStudioStore.docId;
    if (!id) {
      query        = '';
      queryHits    = [];
      queryError   = null;
      currentHitIdx = 0;
      knownKeys    = new Set();
      editingPid   = null;
      editError    = null;
      queryAutoOpenDismissed = false;
      return;
    }
    // Fresh document — Tree is the natural landing surface; keep the
    // shell's auto-flip-to-Errors behaviour intact via parseError.
    viewMode = 'tree';
  });

  // ── Diff view ──────────────────────────────────────────────────────────
  let diffRefreshTick      = $state(0);
  let diffHunkCount        = $state(0);
  let diffTreeChangeCount  = $state(0);
  // `++` is read-modify-write — wrap in `untrack` so any caller that
  // happens to be inside a $effect doesn't accidentally subscribe to
  // `diffRefreshTick` and form a write→read→re-fire loop.
  function bumpDiffRefresh() { untrack(() => { diffRefreshTick++; }); }

  /** View switcher items. Diff badge mirrors the live structural-change
   *  count (or hunk count fallback); Errors carries a sticky '!' chip
   *  the shell flips to red via the `errorBadge` data marker. */
  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: diffTreeChangeCount > 0 ? diffTreeChangeCount
           : diffHunkCount > 0       ? diffHunkCount
           : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse errors',
      disabled: !jsonStudioStore.parseError,
      badge: jsonStudioStore.parseError ? '!' : undefined,
      data: { errorBadge: !!jsonStudioStore.parseError } },
  ]);

  // Cross-ref index — load on modal open + every active-tab change.
  // The ↗ chip on tree rows needs the index populated to decorate the
  // right strings.
  $effect(() => {
    if (!jsonStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, 'json'); });
  });

  // Auto-load schema from the .arbor/studio.toml binding. The host
  // resolves the binding when the doc is opened with a tabId +
  // relativePath; `parse` returns it as `schema_hint`. The store
  // captures it; here we observe and trigger probe/load through the
  // same path as a manual file-picker bind. Mirror of RON's auto-load.
  $effect(() => {
    const hint = jsonStudioStore.schemaHint;
    if (!hint) return;
    if (schema && schemaRsPath === hint.rs_file && schema.root_type === hint.root_type) return;
    void autoLoadSchemaFromHint(hint.rs_file, hint.root_type);
  });
  async function autoLoadSchemaFromHint(rsFile: string, rootCanonical: string): Promise<void> {
    schemaRsPath  = rsFile;
    schema        = null;
    schemaError   = null;
    schemaLoading = true;
    try {
      schemaProbe   = await JSON_BE.schemaProbe(rsFile);
      schemaRootSel = rootCanonical;
      schema        = await JSON_BE.schemaLoad(rsFile, rootCanonical);
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }

  // ── Indent + Format + Convert ───────────────────────────────────────────
  let indentUnit = $state<string>('  ');
  let actionBusy = $state(false);
  let actionError = $state<string | null>(null);
  $effect(() => {
    const id = jsonStudioStore.docId;
    if (!id) return;
    void JSON_BE.getIndent(id).then(s => { if (s) indentUnit = s; }).catch(() => {});
  });

  // ── Footer snapshot (consumed by shared StudioFooter* components) ──────
  const footerDoc: StudioFooterDoc = $derived({
    parseError: jsonStudioStore.parseError ?? null,
    dirty:      jsonStudioStore.dirty,
    sourcePath: jsonStudioStore.sourcePath ?? null,
    encoding:   jsonStudioStore.docId ? jsonStudioStore.encoding : null,
    canUndo:    jsonStudioStore.canUndo,
    canRedo:    jsonStudioStore.canRedo,
    docId:      jsonStudioStore.docId ?? null,
  });
  const selectedFooterPath = $derived<string[] | null>(
    selectedNode && viewMode === 'tree' ? selectedNode.path : null,
  );

  async function setIndentUnit(unit: string): Promise<void> {
    indentUnit = unit;
    const id = jsonStudioStore.docId;
    if (!id) return;
    try { await JSON_BE.setIndent(id, unit); } catch (e) {
      console.warn('json-studio: setIndent failed', e);
    }
  }
  async function runFormat(): Promise<void> {
    const id = jsonStudioStore.docId;
    if (!id || actionBusy || jsonStudioStore.parseError) return;
    actionBusy = true; actionError = null;
    try {
      const formatted = await JSON_BE.format(id);
      await jsonStudioStore.setText(formatted);
      await treePane?.reloadTree();
      bumpDiffRefresh();
    } catch (e: any) {
      actionError = `Format failed: ${e?.message ?? e}`;
    } finally {
      actionBusy = false;
    }
  }

  // ── Save / Save As ─────────────────────────────────────────────────────
  let saving         = $state(false);
  let saveError      = $state<string | null>(null);
  let savePickerOpen = $state(false);
  /** Phase 3.d — gating modal "Questo .json ha commenti, salva come
   *  .jsonc?" Shown when the user clicks Save on a `.json` doc that
   *  has comments / trailing commas. Three outcomes:
   *   - **Save as .jsonc**: rebind source to the `.jsonc` neighbour
   *     and save through normal Save-As pipeline.
   *   - **Strip & save**: call `stripJsoncFeatures()` first, then save.
   *   - **Save anyway**: leave the buffer untouched, write to the
   *     `.json` path even though strict parsers will reject it.
   *   - **Cancel**: dismiss the dialog. */
  let jsoncSavePromptOpen = $state(false);

  async function doSave(): Promise<void> {
    if (!jsonStudioStore.sourcePath) { savePickerOpen = true; return; }
    // Phase 3.d gate: .json with JSONC features → prompt before write.
    if (jsonStudioStore.hasJsoncFeatures && !jsonStudioStore.isJsonc) {
      jsoncSavePromptOpen = true;
      return;
    }
    await runSave();
  }
  async function runSave(): Promise<void> {
    saving = true; saveError = null;
    try {
      await jsonStudioStore.save({ path: null, bindToDoc: false });
      bumpDiffRefresh();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
  async function onSaveAsJsonc(): Promise<void> {
    jsoncSavePromptOpen = false;
    const next = jsonStudioStore.renameSourceToJsonc();
    if (!next) { savePickerOpen = true; return; }
    saving = true; saveError = null;
    try {
      // Save-As writes to the new `.jsonc` path AND rebinds the doc.
      await jsonStudioStore.save({ path: next, bindToDoc: true });
      bumpDiffRefresh();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
  async function onStripAndSave(): Promise<void> {
    jsoncSavePromptOpen = false;
    const ok = await jsonStudioStore.stripJsoncFeatures();
    if (!ok) return;
    await runSave();
  }
  async function onSaveAnyway(): Promise<void> {
    jsoncSavePromptOpen = false;
    await runSave();
  }
  function openSaveAs() { savePickerOpen = true; }
  async function onSaveAsPicked(p: string) {
    savePickerOpen = false;
    saving = true; saveError = null;
    try {
      await jsonStudioStore.save({ path: p, bindToDoc: true });
      bumpDiffRefresh();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }

  // ── Misc ───────────────────────────────────────────────────────────────
  async function close() {
    if (pushTimer) { clearTimeout(pushTimer); pushTimer = null; }
    await jsonStudioStore.closeDoc();
  }

  // fmtBytes / jsBasename moved to shared/studio/helpers.ts.
  const fmtBytes   = fsFmtBytes;
  const jsBasename = fsBasename;

  function kindBadge(k: JsonNodeKind): string {
    switch (k) {
      case 'object': return '{}';
      case 'array':  return '[]';
      case 'string': return '“';
      case 'number': return '#';
      case 'bool':   return '✓';
      case 'null':   return '∅';
    }
  }

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  function onKey(e: KeyboardEvent) {
    if (!jsonStudioStore.open) return;
    const target = e.target as HTMLElement | null;
    const inEditableField = target instanceof HTMLInputElement
                          || target instanceof HTMLTextAreaElement
                          || target instanceof HTMLSelectElement
                          || (target?.closest('.cm-editor') !== null && target?.closest('.cm-editor') !== undefined);

    // Save (Ctrl/Cmd+S)
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === 's' || e.key === 'S')) {
      e.preventDefault(); e.stopPropagation();
      void doSave();
      return;
    }
    // Undo / Redo (skip when CodeMirror has focus — its own history wins,
    // we sync via setText)
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === 'z' || e.key === 'Z')) {
      if (!inEditableField) {
        e.preventDefault();
        void doUndo();
      }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'z' || e.key === 'Z')) {
      if (!inEditableField) {
        e.preventDefault();
        void doRedo();
      }
      return;
    }
    // F3 / Shift+F3 — query / diff navigation
    if (e.key === 'F3') {
      if (viewMode === 'tree')      { e.preventDefault(); queryBar?.nav(e.shiftKey ? -1 : 1); }
      else if (viewMode === 'diff') { e.preventDefault(); diffPane?.nav(e.shiftKey ? -1 : 1); }
      return;
    }
    // F2 — start inline tree edit on selected (variant or primitive)
    if (e.key === 'F2' && viewMode === 'tree' && !inEditableField) {
      if (selectedNode && !editingPid) {
        const mode = rowEditMode(selectedNode);
        if (mode === 'variant') {
          e.preventDefault();
          startVariantEdit('tree');
        } else if (mode === 'primitive') {
          e.preventDefault();
          startEdit('tree');
        }
      }
      return;
    }
    // Delete — remove selected
    if (e.key === 'Delete' && viewMode === 'tree' && !inEditableField) {
      if (isRemovable(selectedNode)) {
        e.preventDefault();
        void removeSelected();
      }
      return;
    }
  }

  async function doUndo() {
    const ok = await jsonStudioStore.undo();
    if (ok) {
      await treePane?.reloadTree();
      bumpDiffRefresh();
    }
  }
  async function doRedo() {
    const ok = await jsonStudioStore.redo();
    if (ok) {
      await treePane?.reloadTree();
      bumpDiffRefresh();
    }
  }

  // Touch tick to silence a Svelte "tick is unused" warning when this
  // file is compiled in isolation — used in `flushAndClose` flow we may
  // wire later.
  void tick;
</script>

<svelte:window on:keydown={onKey} />

<StudioModal
  bind:this={studioModal}
  formatId="json"
  backend={JSON_BE}
  open={jsonStudioStore.open}
  loading={jsonStudioStore.loading}
  loadingLabel="Parsing JSON…"
  errorState={jsonStudioStore.error}
  parseError={jsonStudioStore.parseError}
  hasDoc={!!jsonStudioStore.docId}
  viewItems={viewItems}
  bind:viewMode
  bind:rightPane
  rightPaneStorageKey={RIGHT_PANE_KEY}
  ariaLabel="JSON Studio"
  onClose={close}
>
  {#snippet rightRailButtons()}
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'inspector'}
      onclick={() => studioModal?.toggleRightPane('inspector')}
      use:tooltip={'Inspector — selected node detail (Tree view)'}
      aria-label="Inspector"
      aria-pressed={rightPane === 'inspector'}
    >
      <ScanSearch size={20} />
    </button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'query'}
      onclick={() => {
        if (rightPane === 'query') queryAutoOpenDismissed = true;
        studioModal?.toggleRightPane('query');
      }}
      use:tooltip={query.trim()
        ? `Query results — ${queryHits.length} hit${queryHits.length === 1 ? '' : 's'}`
        : 'Query results — type in the search bar to populate'}
      aria-label="Query results"
      aria-pressed={rightPane === 'query'}
    >
      <ListFilter size={20} />
      {#if queryHits.length > 0}
        <span class="js-rail-count" aria-hidden="true">{queryHits.length >= 100 ? '99+' : queryHits.length}</span>
      {/if}
    </button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'bindings'}
      onclick={() => studioModal?.toggleRightPane('bindings')}
      use:tooltip={'Bindings & broken refs — project-wide cross-references'}
      aria-label="Bindings & broken refs"
      aria-pressed={rightPane === 'bindings'}
    >
      <Layers size={20} />
    </button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'schema'}
      onclick={() => studioModal?.toggleRightPane('schema')}
      use:tooltip={schema
        ? `Schema — ${schema.root_name} (${Object.keys(schema.types).length} types)`
        : 'Schema — bind a JSON Schema file'}
      aria-label="JSON Schema"
      aria-pressed={rightPane === 'schema'}
    >
      <BookOpen size={20} />
    </button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'tools'}
      onclick={() => studioModal?.toggleRightPane('tools')}
      use:tooltip={'Tools — Format / Indent'}
      aria-label="Tools"
      aria-pressed={rightPane === 'tools'}
    >
      <Wrench size={20} />
    </button>
  {/snippet}

  {#snippet headerLeft()}
    <span class="js-header-icon-wrap" aria-hidden="true">
      <FileJson size={14} />
    </span>
    <!-- Undo / redo sit to the LEFT of the title cluster (the modal's
         own tab strip would also live to the right of this). -->
    <StudioHeaderUndoRedo doc={footerDoc} onUndo={doUndo} onRedo={doRedo} />
    <span class="js-title" use:tooltip={jsonStudioStore.sourcePath ?? ''}>
      {jsonStudioStore.title ?? 'JSON Studio'}
      {#if jsonStudioStore.dirty}<span class="js-dirty" use:tooltip={'Unsaved changes'}>●</span>{/if}
    </span>
    {#if jsonStudioStore.sizeBytes != null}
      <span class="js-meta">{fmtBytes(jsonStudioStore.sizeBytes)}</span>
    {/if}
    <div class="js-spacer"></div>
  {/snippet}

  {#snippet footerStatusLeft()}
    <StudioFooterStatus doc={footerDoc} selectedPath={selectedFooterPath} />
  {/snippet}

  {#snippet toolsSidecar()}
    <StudioToolsSidebar
      doc={footerDoc}
      {actionBusy}
      {indentUnit}
      indentOptions={INDENT_OPTIONS_WITH_8}
      indentTooltip="Indent — applied to Format and tree edits"
      formatTooltip="Format — re-emit canonical JSON (loses any non-canonical whitespace)"
      onSetIndent={setIndentUnit}
      onFormat={runFormat}
    />
  {/snippet}

  {#snippet footerRight()}
    <StudioFooterRight
      doc={footerDoc}
      {saving}
      onSave={() => void doSave()}
      onSaveAs={openSaveAs}
    />
  {/snippet}

  {#snippet bodyBanners()}
    <StudioBodyBanners {saveError} {actionError}>
      {#snippet extras()}
        {#if jsonStudioStore.streamMode && !jsonStudioStore.streamBannerDismissed}
          <div class="js-banner-wrap">
            <div class="js-jsonc-banner js-jsonc-banner-info">
              <div class="js-jsonc-banner-text">
                <strong>Large file ({fmtBytes(jsonStudioStore.sizeBytes ?? 0)}):</strong>
                opened in streaming mode. Comments and trailing commas are not
                supported, and structural tree edits are disabled — use the
                Text pane for raw edits.
              </div>
              <div class="js-jsonc-banner-actions">
                <button type="button" class="js-jsonc-btn js-jsonc-btn-ghost"
                  onclick={() => jsonStudioStore.dismissStreamBanner()}>Dismiss</button>
              </div>
            </div>
          </div>
        {/if}
        {#if jsonStudioStore.hasJsoncFeatures
            && !jsonStudioStore.isJsonc
            && !jsonStudioStore.streamMode
            && !jsonStudioStore.bannerDismissed}
          <div class="js-banner-wrap">
            <div class="js-jsonc-banner js-jsonc-banner-warn">
              <div class="js-jsonc-banner-text">
                <strong>This .json file uses JSONC features</strong>
                (comments / trailing commas). Strict JSON parsers will fail
                to read it.
              </div>
              <div class="js-jsonc-banner-actions">
                <button type="button" class="js-jsonc-btn"
                  onclick={() => void onSaveAsJsonc()}>Rename to .jsonc</button>
                <button type="button" class="js-jsonc-btn"
                  onclick={() => void onStripAndSave()}>Strip & save</button>
                <button type="button" class="js-jsonc-btn js-jsonc-btn-ghost"
                  onclick={() => jsonStudioStore.dismissJsoncBanner()}>Dismiss</button>
              </div>
            </div>
          </div>
        {/if}
      {/snippet}
    </StudioBodyBanners>
  {/snippet}

  {#snippet queryBarSlot()}
    <StudioQueryBar
      bind:this={queryBar}
      formatId="json"
      backend={JSON_BE}
      docId={jsonStudioStore.docId}
      visible={viewMode === 'tree' && !jsonStudioStore.parseError}
      placeholder='Query — name (recursive), $.foo.bar, $.arr[0:5], $.users[?@.age > 30]…'
      historyStorageKey="arbor:json-studio:query-history"
      knownKeys={knownKeys}
      getChildKeysForPath={getChildKeysForPath}
      ensureChildrenLoaded={ensureChildrenLoadedForPath}
      onJumpToHit={(path) => void jumpToQueryHit(path)}
      rightPaneOpen={rightPane === 'query'}
      onToggleRightPane={onQueryToggleRightPane}
      onActiveChange={onQueryActiveChange}
      onHits={(hits) => noteKeys(hits)}
      bulkEditEnabled
      onBulkEditRequest={(q) => openBulkEditModal(q)}
      bind:query
      bind:queryHits
      bind:querying
      bind:queryError
      bind:currentHitIdx
    >
      {#snippet kindChip(kind)}
        <span class="js-row-badge js-row-badge-{kind}" use:tooltip={kind}>{kindBadge(kind)}</span>
      {/snippet}
      {#snippet toolbarRight()}
        <button
          type="button"
          class="js-query-tool-btn"
          onclick={() => void treePane?.expandAll()}
          disabled={expandAllBusy}
          use:tooltip={'Recursively load + expand every container (capped at 5000 nodes for large docs)'}
          aria-label="Expand all"
        >{#if expandAllBusy}<Loader2 size={12} class="js-query-spinner" />{:else}<ChevronsDown size={12} />{/if}<span>Expand</span></button>
        <button
          type="button"
          class="js-query-tool-btn"
          onclick={() => treePane?.collapseAll()}
          use:tooltip={'Collapse all (root stays open)'}
          aria-label="Collapse all"
        ><ChevronsUp size={12} /><span>Collapse</span></button>
      {/snippet}
    </StudioQueryBar>
  {/snippet}

  {#snippet bodyMain()}
    {#if viewMode === 'tree'}
      <StudioTreePane
        bind:this={treePane}
        formatId="json"
        backend={JSON_BE}
        docId={jsonStudioStore.docId}
        parseError={jsonStudioStore.parseError}
        bind:roots
        bind:expanded
        bind:selectedNode
        bind:valueText
        bind:valueLoading
        bind:expandAllBusy
        toTree={toTree as any}
        sortChildren={sortChildren as any}
        isContainerKind={isContainerKind}
        getContextMenuItems={ctxItemsFor as any}
        onContextMenuSelect={(id: string, n: any) => onContextMenuSelect(id, n as TNode)}
        {commitPendingEdit}
        showRightBorder={false}
        ariaLabel="JSON document tree"
      >
        {#snippet rowContent({ node }: RowSnippetCtx<any>)}
          {@const n = node as TNode}
          {@const ty = typeAtPath(n.path)}
          {@const namedType = namedTypeAt(n.path)}
          <span class="js-row-badge js-row-badge-{n.kind}" use:tooltip={n.kind}>{kindBadge(n.kind)}</span>
          <span class="js-row-key" class:js-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          <span class="js-row-sep">:</span>
          {#if editingPid === n.pid && editLocation === 'tree'}
            {#if rowEditMode(n) === 'variant'}
              {@const ed = enumDefAt(n.path)}
              {#if ed}
                <select class="js-inline-edit js-inline-edit-variant"
                        bind:this={editInlineSelectEl}
                        bind:value={editBuf}
                        onkeydown={onEditKey}
                        onchange={() => void commitVariantEdit()}
                        onclick={(e) => e.stopPropagation()}
                        onmousedown={(e) => e.stopPropagation()}>
                  {#each ed.variants as v (v.name)}
                    <option value={v.name}>{v.name}</option>
                  {/each}
                </select>
              {/if}
            {:else if n.kind === 'bool'}
              <select class="js-inline-edit"
                      bind:this={editInlineSelectEl}
                      bind:value={editBuf}
                      onkeydown={onEditKey}
                      onclick={(e) => e.stopPropagation()}
                      onmousedown={(e) => e.stopPropagation()}>
                <option value="true">true</option>
                <option value="false">false</option>
              </select>
            {:else}
              <input class="js-inline-edit"
                     bind:this={editInlineEl}
                     bind:value={editBuf}
                     onkeydown={onEditKey}
                     onclick={(e) => e.stopPropagation()}
                     onmousedown={(e) => e.stopPropagation()}
                     spellcheck="false" />
            {/if}
            {#if editError}
              <span class="js-inline-edit-err" use:tooltip={editError}>!</span>
            {/if}
          {:else}
            {@const xrefs = crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="js-row-preview js-row-preview-{n.kind}"
                  class:js-row-preview-editable={rowEditMode(n) !== null}
                  class:js-row-preview-xref={hasX}
                  ondblclick={(e) => {
                    if (!rowEditMode(n)) return;
                    e.preventDefault(); e.stopPropagation();
                    void selectNode(n).then(() => startEdit('tree'));
                  }}
                  onclick={hasX ? ((e) => onCrossRefClick(xrefs, e)) : undefined}
                  use:tooltip={hasX
                    ? (xrefs.length === 1
                        ? `Ctrl+click → ${xrefs[0].title} (${xrefs[0].defPath.join('.')})`
                        : `Ctrl+click → choose between ${xrefs.length} matches`)
                    : (rowEditMode(n) === 'variant'  ? 'Double-click to change variant'
                      : rowEditMode(n) === 'primitive' ? 'Double-click to edit'
                      : '')}
            >{n.preview}{#if hasX}<span class="js-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="js-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}
          {#if n.loading}<Loader2 size={10} class="js-row-loader" />{/if}
          {#if namedType}
            <span class="js-row-named" use:tooltip={fmtType(ty)}>{namedType}</span>
          {:else if ty && ty.kind !== 'named'}
            <span class="js-row-type {typeChipClass(ty)}"
              use:tooltip={fmtType(ty)}
            >{fmtType(ty)}</span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textBuf}
        language="json"
        oninput={onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="json"
        backend={JSON_BE}
        docId={jsonStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={jsonStudioStore.current}
        refreshTick={diffRefreshTick}
        bind:treeChangeCount={diffTreeChangeCount}
        bind:hunkCount={diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- JSON has no variant tags; renderer renders an empty span. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if jsonStudioStore.parseError}
        <div class="js-errors">
          <div class="js-errors-head">
            <AlertCircle size={14} />
            <span>JSON parse error</span>
          </div>
          <pre class="js-errors-body">{jsonStudioStore.parseError}</pre>
          <p class="js-errors-hint">
            Switch to the <strong>Text</strong> tab to fix it. The error will
            clear automatically once the document parses.
          </p>
        </div>
      {:else}
        <div class="js-errors-empty">
          <Check size={16} />
          <span>No parse errors.</span>
        </div>
      {/if}
    {/if}
  {/snippet}

  {#snippet inspectorSidecar()}
    <StudioInspectorPanel
      bind:this={inspectorPanel}
      formatId="json"
      backend={JSON_BE}
      selectedNode={selectedNode as any}
      {valueText}
      {valueLoading}
      {editingPid}
      {editLocation}
      bind:editBuf
      {editError}
      {editBannerVisible}
      kindBadge={kindBadge as any}
      isRemovable={isRemovable as any}
      isEditablePrimitive={isEditablePrimitive as any}
      isBoolKind={(k: JsonNodeKind) => k === 'bool'}
      isContainerKind={isContainerKind as any}
      isDefinitionNode={isDefinitionNode as any}
      definitionValue={definitionValue as any}
      onCopyPath={copyPathOf as any}
      onCopyValue={copyValue}
      onRemove={removeSelected}
      onStartEdit={startEdit}
      onCommitEdit={commitEdit}
      onCancelEdit={cancelEdit}
      onPickVariant={(name: string) => void inspectorPickVariant(name)}
      onAddField={inspectorAddField as any}
      onToggleOption={noopOption}
      onDismissEditBanner={dismissEditBanner}
      onJumpToUsage={jumpToUsage as any}
      onSelectChild={(c) => void selectNode(c as TNode)}
      schemaTypeInfo={inspectorSchemaTypeInfo as any}
      variantPickerInfo={inspectorVariantPickerInfo as any}
      missingFields={inspectorMissingFields as any}
    />
  {/snippet}

  {#snippet querySidecar()}
    <div class="js-panel-head">
      <ListFilter size={13} />
      <span class="js-panel-title">Query results</span>
      {#if queryHits.length > 0}
        <span class="js-panel-count">{queryHits.length}{queryHits.length >= 500 ? '+' : ''}</span>
      {/if}
      <span class="js-spacer"></span>
    </div>
    <div class="js-query-pane-body">
      {#if !query.trim()}
        <p class="js-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if querying && queryHits.length === 0}
        <div class="js-query-pane-status">
          <Spinner size="xs" /> <span>Running query…</span>
        </div>
      {:else if queryError}
        <div class="js-query-pane-error">
          <AlertCircle size={11} /> {queryError}
        </div>
      {:else if queryHits.length === 0}
        <p class="js-query-pane-empty">No matches.</p>
      {:else}
        <div class="js-query-pane-list">
          {#each queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="js-query-pane-card"
              class:active={i === currentHitIdx}
              onclick={() => { currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="js-query-pane-card-head">
                <span class="js-row-badge js-row-badge-{hit.kind}" use:tooltip={hit.kind}>{kindBadge(hit.kind)}</span>
                <span class="js-query-pane-card-idx">#{i + 1}</span>
              </div>
              <div class="js-query-pane-card-path">{hit.path.length === 0 ? '$' : '$.' + hit.path.join('.')}</div>
              {#if hit.preview}
                <div class="js-query-pane-card-preview">{hit.preview}</div>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="json"
      backend={JSON_BE}
      sourcePath={jsonStudioStore.sourcePath}
      onOpenDefinition={openDefinition}
    >
      {#snippet emptyState()}
        <p class="js-bindings-empty">
          Project-wide cross-refs follow the <code>id</code> / <code>name</code>
          convention by default. Open the Schema panel to bind a JSON
          Schema sidecar — schema-derived bindings will appear here as
          they're configured.
        </p>
      {/snippet}
    </StudioRefsPanel>
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId="json"
      backend={JSON_BE}
      {schema}
      {schemaProbe}
      {schemaRsPath}
      {schemaRootSel}
      {schemaLoading}
      {schemaError}
      onProbe={probeSchemaSource}
      onSelectRoot={setSchemaRoot}
      onLoad={loadSchemaForRoot}
      onClear={clearSchema}
      onOpenViewSource={openViewSource}
      pickerTitle="Pick JSON Schema file"
      pickerExtensions={['json', 'schema.json']}
      pickerButtonLabel="Pick .schema.json"
    >
      {#snippet intro()}
        <p class="js-schema-hint">
          Pick a JSON Schema file (<code>*.schema.json</code> or any JSON
          document with a <code>$schema</code> keyword). JSON Studio will
          surface every <code>$defs</code> / <code>definitions</code>
          entry as a root candidate and walk every <code>$ref</code>
          chain to index the reachable types.
        </p>
      {/snippet}
    </StudioSchemaPanel>
  {/snippet}

  {#snippet auxiliary()}
    {#if savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save JSON document as"
        extensions={['json', 'jsonc']}
        initialPath={jsonStudioStore.sourcePath ?? undefined}
        initialFilename={jsBasename(jsonStudioStore.sourcePath) || (jsonStudioStore.isJsonc ? 'document.jsonc' : 'document.json')}
        onConfirm={onSaveAsPicked}
        onCancel={() => savePickerOpen = false}
      />
    {/if}
    {#if jsoncSavePromptOpen}
      <Modal
        onClose={() => jsoncSavePromptOpen = false}
        width="min(520px, 92vw)"
        height="auto"
        padBody={true}
        ariaLabel="Save .json with comments"
      >
        {#snippet header()}
          <h3 style="margin: 0; font-size: 13px;">Save .json with JSONC features</h3>
        {/snippet}
        <div style="display: flex; flex-direction: column; gap: 10px; font-size: 12px; line-height: 1.5; color: var(--text-primary);">
          <p style="margin: 0;">
            This file uses <strong>comments</strong> or <strong>trailing
            commas</strong>. Strict JSON parsers (most build tools,
            <code>json.loads</code>, <code>JSON.parse</code>) will fail to read it.
          </p>
          <p style="margin: 0; color: var(--text-secondary);">Pick how to save:</p>
          <div style="display: flex; flex-direction: column; gap: 6px;">
            <button type="button" class="js-jsonc-btn" style="text-align: left;"
              onclick={() => void onSaveAsJsonc()}>
              <strong>Save as .jsonc</strong>
              <span style="display:block;color:var(--text-secondary);font-size:11px;">Rename the file to <code>.jsonc</code> and keep all JSONC features intact.</span>
            </button>
            <button type="button" class="js-jsonc-btn" style="text-align: left;"
              onclick={() => void onStripAndSave()}>
              <strong>Strip & save</strong>
              <span style="display:block;color:var(--text-secondary);font-size:11px;">Lose comments and trailing commas — pure JSON. Reversible via undo.</span>
            </button>
            <button type="button" class="js-jsonc-btn" style="text-align: left;"
              onclick={() => void onSaveAnyway()}>
              <strong>Save anyway</strong>
              <span style="display:block;color:var(--text-secondary);font-size:11px;">Keep the <code>.json</code> path and the JSONC features. Strict parsers will break.</span>
            </button>
          </div>
          <div style="display: flex; justify-content: flex-end; padding-top: 4px;">
            <button type="button" class="js-jsonc-btn js-jsonc-btn-ghost"
              onclick={() => jsoncSavePromptOpen = false}>Cancel</button>
          </div>
        </div>
      </Modal>
    {/if}
    {#if renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={JSON_BE}
        tabId={tabsStore.activeTabId}
        formatLabel="JSON"
        oldValue={renameModalState.oldValue}
        openDocs={buildOpenDocsSnapshot()}
        onClose={closeRenameModal}
        onApplied={onRenameApplied}
      />
    {/if}

    {#if bulkEditModalState && tabsStore.activeTabId && jsonStudioStore.docId}
      <StudioBulkEditModal
        backend={JSON_BE}
        tabId={tabsStore.activeTabId}
        docId={jsonStudioStore.docId}
        formatLabel="JSON"
        query={bulkEditModalState.query}
        nullPolicy="native"
        openDocs={buildBulkEditOpenDocs()}
        onClose={closeBulkEditModal}
        onApplied={onBulkEditApplied}
      />
    {/if}

    {#if crossRefPicker}
      <div class="js-xref-overlay"
           use:portal
           role="presentation"
           onclick={() => crossRefPicker = null}
           oncontextmenu={(e) => { e.preventDefault(); crossRefPicker = null; }}
      >
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="js-xref-popover"
             style:left="{crossRefPicker.x}px"
             style:top="{crossRefPicker.y}px"
             role="menu"
             onclick={(e) => e.stopPropagation()}
        >
          <div class="js-xref-header">{crossRefPicker.entries.length} matches</div>
          {#each crossRefPicker.entries as entry (entry.sourcePath + entry.defPath.join('\x00'))}
            <button
              type="button"
              class="js-xref-item"
              onclick={() => void jumpToCrossRef(entry)}
            >
              <span class="js-xref-item-icon"><FileTextIcon size={13} /></span>
              <span class="js-xref-item-name">{entry.title}</span>
              <span class="js-xref-item-path">{entry.defPath.join('.')}</span>
              <span class="js-xref-item-open">›</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    {#if viewSource || viewSourceBusy || viewSourceErr}
      <StudioViewSourceModal
        viewSource={viewSource}
        busy={viewSourceBusy}
        err={viewSourceErr}
        language="json"
        loadingLabel="Loading schema fragment…"
        onClose={closeViewSource}
      />
    {/if}
  {/snippet}
</StudioModal>

<style>
  /* Header strip ───────────────────────────────────────────────────── */
  .js-header-icon-wrap { display: inline-flex; align-items: center; color: var(--accent); flex-shrink: 0; }
  .js-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    max-width: 50%;
  }
  .js-dirty {
    color: var(--accent);
    font-size: 14px;
    margin-left: 4px;
    line-height: 1;
  }
  .js-meta {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 2px 6px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .js-spacer { flex: 1; }

  /* Right activity rail buttons (shared `.ab-btn` from app.css; we
     just need the dot/count overlays specific to JSON Studio). */
  .js-rail-count {
    position: absolute;
    bottom: 2px;
    right: 2px;
    background: var(--accent);
    color: var(--bg-base);
    font-size: 9px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 8px;
    line-height: 1;
  }

  /* Footer pill / button / sep styles moved to the shared
     <StudioFooter*> components in `./studio/`. */

  /* Body banners ───────────────────────────────────────────────────── */
  .js-banner-wrap {
    padding: 6px 8px 0 8px;
  }
  /* Phase 3.d — JSONC + stream-mode banners ──────────────────────────── */
  .js-jsonc-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-radius: 4px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    font-size: 12px;
    line-height: 1.4;
  }
  .js-jsonc-banner-warn {
    border-color: var(--warning-border, #b8851a);
    background:   var(--warning-bg, rgba(184, 133, 26, 0.10));
    color:        var(--text-primary);
  }
  .js-jsonc-banner-info {
    border-color: var(--info-border, var(--border-subtle));
    color:        var(--text-secondary);
  }
  .js-jsonc-banner-text { flex: 1; }
  .js-jsonc-banner-actions {
    display: flex;
    gap: 6px;
    flex: 0 0 auto;
  }
  .js-jsonc-btn {
    appearance: none;
    border: 1px solid var(--border-subtle);
    background: var(--bg-base);
    color: var(--text-primary);
    font: inherit;
    font-size: 11px;
    padding: 3px 9px;
    border-radius: 3px;
    cursor: pointer;
  }
  .js-jsonc-btn:hover { background: var(--bg-overlay-strong, var(--bg-overlay)); }
  .js-jsonc-btn-ghost {
    border-color: transparent;
    background: transparent;
    color: var(--text-secondary);
  }
  .js-jsonc-btn-ghost:hover {
    border-color: var(--border-subtle);
    color: var(--text-primary);
  }

  /* Empty-state copy in the Bindings sidecar (Phase 3.c.a — JSON Schema
     sidecar arrives in 3.c.b; for now the panel just surfaces the
     project-wide cross-ref / broken-ref view). */
  .js-bindings-empty {
    font-size: 11px;
    color: var(--text-secondary);
    line-height: 1.45;
    padding: 12px 14px;
    margin: 0;
  }
  .js-bindings-empty code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }

  /* Query toolbar buttons (inside <StudioQueryBar>'s toolbarRight). */
  .js-query-tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 7px;
    border: 1px solid var(--border-subtle);
    background: transparent;
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    font-size: 11px;
    cursor: pointer;
  }
  .js-query-tool-btn:hover { color: var(--text-primary); background: var(--bg-overlay); }
  .js-query-tool-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  :global(.js-query-spinner) { animation: js-spin 1.2s linear infinite; }
  @keyframes js-spin { to { transform: rotate(360deg); } }

  /* Tree row decoration ────────────────────────────────────────────── */
  .js-row-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 16px;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-family: var(--font-code);
    font-weight: 600;
    flex-shrink: 0;
  }
  .js-row-badge-object { background: rgba(77, 120, 204, 0.18); color: #4d78cc; }
  .js-row-badge-array  { background: rgba(77, 120, 204, 0.18); color: #4d78cc; }
  .js-row-badge-string { background: rgba(106, 153, 86, 0.18); color: #6a9956; }
  .js-row-badge-number { background: rgba(204, 120, 50, 0.18); color: #cc7832; }
  .js-row-badge-bool   { background: rgba(204, 120, 50, 0.18); color: #cc7832; }
  .js-row-badge-null   { background: var(--bg-overlay);        color: var(--text-muted); }

  .js-row-key {
    color: var(--text-primary);
    font-weight: 500;
  }
  .js-row-key-index {
    color: var(--text-muted);
    font-family: var(--font-code);
  }
  .js-row-sep {
    color: var(--text-muted);
    margin-right: 2px;
  }
  .js-row-preview {
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .js-row-preview-string { color: #6a9956; }
  .js-row-preview-number { color: #cc7832; }
  .js-row-preview-bool   { color: #cc7832; }
  .js-row-preview-null   { color: var(--text-muted); font-style: italic; }
  :global(.js-row-loader) { color: var(--accent); animation: js-spin 1.2s linear infinite; }

  /* Inline edit (tree-location). Mirror of RON's `.rs-inline-edit`. */
  .js-inline-edit {
    background: var(--bg-base);
    color: var(--text-primary);
    border: 1px solid var(--accent);
    border-radius: 3px;
    padding: 0 6px;
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.4;
    height: 20px;
    min-width: 80px;
    max-width: 320px;
    outline: none;
  }
  .js-inline-edit:focus {
    border-color: var(--accent-strong, var(--accent));
    box-shadow: 0 0 0 2px var(--accent-subtle);
  }
  .js-inline-edit-err {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-error, rgba(255, 90, 80, 0.18));
    color: var(--text-error, #ff6c5c);
    font-size: 11px;
    font-weight: 700;
    margin-left: 4px;
    cursor: help;
  }

  /* Errors view ────────────────────────────────────────────────────── */
  .js-errors {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 24px;
    overflow: auto;
  }
  .js-errors-head {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--error, #e06c75);
    font-size: 13px;
    font-weight: 600;
  }
  .js-errors-body {
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 12px;
    font-family: var(--font-code);
    font-size: 12px;
    color: var(--text-primary);
    white-space: pre-wrap;
    overflow-x: auto;
  }
  .js-errors-hint {
    color: var(--text-muted);
    font-size: 12px;
    margin: 0;
  }
  .js-errors-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 100%;
    color: var(--text-muted);
    font-size: 12px;
  }

  /* Query results sidecar pane ─────────────────────────────────────── */
  .js-panel-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 600;
  }
  .js-panel-title { letter-spacing: 0.5px; text-transform: uppercase; }
  .js-panel-count {
    background: var(--accent-subtle);
    color: var(--accent);
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 700;
  }
  .js-query-pane-body {
    flex: 1;
    overflow: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
  }
  .js-query-pane-empty,
  .js-query-pane-status,
  .js-query-pane-error {
    color: var(--text-muted);
    font-size: 12px;
    padding: 12px;
    text-align: center;
  }
  .js-query-pane-error {
    color: var(--error, #e06c75);
    background: rgba(224, 108, 117, 0.06);
    border: 1px solid var(--error, #e06c75);
    border-radius: var(--radius-sm);
    text-align: left;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .js-query-pane-list { display: flex; flex-direction: column; gap: 4px; }
  .js-query-pane-card {
    text-align: left;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 6px 8px;
    cursor: pointer;
    color: var(--text-primary);
    transition: background var(--transition-fast);
  }
  .js-query-pane-card:hover { background: var(--bg-hover); }
  .js-query-pane-card.active {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }
  .js-query-pane-card-head {
    display: flex; align-items: center; gap: 6px;
    margin-bottom: 4px;
  }
  .js-query-pane-card-idx {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    margin-left: auto;
  }
  .js-query-pane-card-path {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .js-query-pane-card-preview {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Schema sidecar intro + view-source modal ─────────────────────── */
  .js-schema-hint {
    margin: 0 0 8px 0;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .js-schema-hint code {
    font-family: var(--font-code);
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 4px;
  }
  .js-bindings-empty {
    margin: 0;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.45;
    padding: 8px 12px;
  }
  .js-bindings-empty code {
    font-family: var(--font-code);
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 4px;
  }
  .js-source-pre {
    margin: 0;
    padding: 12px 16px;
    overflow: auto;
    height: 100%;
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-primary);
    background: var(--bg-base);
    white-space: pre;
  }
  :global(.js-header-icon) { color: var(--accent); flex-shrink: 0; }

  /* Schema-aware tree decoration (parity with RON / TOML). */
  .js-inline-edit-variant {
    background: var(--bg-base);
    color: var(--syntax-keyword, #cc7832);
    padding-right: 18px;
  }
  .js-row-preview-editable { cursor: text; }

  .js-row-type {
    margin-left: auto;
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 10px;
    padding: 1px 6px;
    background: var(--bg-overlay);
    border-radius: 8px;
    flex-shrink: 0;
  }
  .js-row-type.js-type-prim {
    color:      var(--syntax-type, #61afef);
    background: color-mix(in srgb, var(--syntax-type, #61afef) 14%, var(--bg-overlay));
  }
  .js-row-type.js-type-option {
    color:      var(--syntax-keyword, #d19a66);
    background: color-mix(in srgb, var(--syntax-keyword, #d19a66) 14%, var(--bg-overlay));
  }
  .js-row-type.js-type-vec {
    color:      var(--syntax-function, #c678dd);
    background: color-mix(in srgb, var(--syntax-function, #c678dd) 14%, var(--bg-overlay));
  }
  .js-row-type.js-type-map {
    color:      var(--syntax-char, #56b6c2);
    background: color-mix(in srgb, var(--syntax-char, #56b6c2) 14%, var(--bg-overlay));
  }
  .js-row-type.js-type-tupletype {
    color:      var(--syntax-decimal, #e5c07b);
    background: color-mix(in srgb, var(--syntax-decimal, #e5c07b) 14%, var(--bg-overlay));
  }
  .js-row-type.js-type-unknown {
    color:      var(--warning, #d19a66);
    background: color-mix(in srgb, var(--warning, #d19a66) 18%, transparent);
  }
  .js-row-type.js-type-external {
    color:      var(--text-disabled);
    background: var(--bg-overlay);
    font-style: italic;
  }
  .js-row-named {
    margin-left: auto;
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 600;
    color: var(--syntax-type, var(--accent));
    background: color-mix(in srgb, var(--syntax-type, var(--accent)) 14%, transparent);
    padding: 1px 7px;
    border-radius: 8px;
    flex-shrink: 0;
    letter-spacing: 0.01em;
  }

  .js-row-preview-xref { cursor: pointer; }
  .js-row-xref {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-left: 4px;
    padding: 1px 4px 1px 3px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-radius: 4px;
    line-height: 1;
    opacity: 0.85;
    transition: opacity var(--transition-fast), background var(--transition-fast);
    vertical-align: 2px;
  }
  .js-row-preview-xref:hover .js-row-xref {
    opacity: 1;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .js-row-xref-count {
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 700;
    color: var(--accent);
  }

  .js-xref-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: transparent;
    cursor: default;
  }
  .js-xref-popover {
    position: fixed;
    z-index: 61;
    min-width: 220px; max-width: 380px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    padding: 4px;
    display: flex; flex-direction: column;
    gap: 1px;
  }
  .js-xref-header {
    padding: 4px 8px 6px;
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 2px;
  }
  .js-xref-item {
    display: flex; align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    background: transparent;
    color: var(--text-primary);
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    font-size: 12px;
  }
  .js-xref-item:hover { background: var(--bg-hover); }
  .js-xref-item-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .js-xref-item-name {
    flex: 1;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-ui-sans);
    font-weight: 500;
  }
  .js-xref-item-path {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .js-xref-item-open {
    color: var(--accent);
    font-size: 14px;
    line-height: 1;
    margin-left: 2px;
  }
</style>
