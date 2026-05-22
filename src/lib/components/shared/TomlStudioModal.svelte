<!--
  TomlStudioModal — TOML wrapper around the generic `<StudioModal>` shell.

  Phase 4.b ports a fully-editable TOML modal onto the same architecture
  JSON / RON moved to: thin wrapper that owns format-specific state
  (single-doc store, save split, mutations) and renders the shell via
  snippet props. Tree, Inspector, Text (CodeMirror), Diff, Errors and
  Query all flow through the shared studio panes.

  Capabilities exposed to the user (Phase 4.b scope):
    · Tree view with mutations (edit primitives, remove, add field/item,
      duplicate, move, paste-over).
    · Text view via `<StudioTextPane>` (CodeMirror 6 + TOML stream
      parser) — typing pushes through the host's lossless `toml_edit`
      editor.
    · Diff view via `<StudioDiffPane>` (text + tree sub-views).
    · Errors view — inline banner with the host's parse error.
    · Inspector + Query right-rail panes.
    · Footer: parse / dirty / saved pill, encoding pill, undo / redo,
      indent picker, Format, Save split.

  NOT in 4.b (deferred):
    · Cross-refs / broken-refs / F12 rename / F13 bulk-edit → Phase 4.c.
    · Schema sidecar (Rust struct + JSON Schema) → Phase 4.c.
    · Multi-tab workspace — TOML Studio remains single-doc by design.
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';
  import {
    FileText, Copy, ListTree, AlertCircle, GitCompare,
    ChevronUp, ChevronDown, Replace,
    Pencil, Check, ClipboardPaste,
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
  import type { StudioFooterDoc } from './studio/studio-footer-types';
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
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tomlStudioStore, type TomlNodeKind } from '$lib/stores/toml-studio.svelte';
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

  /** Pre-bound TOML backend. */
  const TOML_BE = studioBackend<TomlNodeKind>('toml');

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'query' | 'bindings' | 'schema' | 'tools' | null;

  let viewMode  = $state<ViewMode>('tree');

  const RIGHT_PANE_KEY = 'arbor:toml-studio:right-pane';
  function loadRightPane(): RightPane {
    if (typeof localStorage === 'undefined') return 'inspector';
    const v = localStorage.getItem(RIGHT_PANE_KEY) as RightPane;
    return v === 'inspector' || v === 'query' || v === 'bindings' || v === 'schema'
      ? v : 'inspector';
  }
  let rightPane = $state<RightPane>(loadRightPane());

  let studioModal: StudioModal<TomlNodeKind> | undefined = $state();
  let treePane:    StudioTreePaneController<TomlNodeKind, TNode> | undefined = $state();
  let diffPane:    StudioDiffPaneController | undefined = $state();
  let inspectorPanel: StudioInspectorPanelController | undefined = $state();

  function setRightPane(p: RightPane) { studioModal?.setRightPane(p); }

  // ── Tree state ─────────────────────────────────────────────────────────
  type TomlNodeView = StudioNodeView<TomlNodeKind>;
  type TomlQueryHit = StudioQueryHit<TomlNodeKind>;
  type TNode = TomlNodeView & {
    pid:      string;
    children: TNode[] | null;
    loading?: boolean;
  };

  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: TomlNodeView): TNode { return { ...v, pid: pathId(v.path), children: null }; }
  /** TOML has no semantic ordering rule — render children as the
   *  backend emits them (= source order). */
  function sortChildren(_parentKind: TomlNodeKind, kids: TNode[]): TNode[] { return kids; }
  function isContainerKind(k: TomlNodeKind): boolean {
    return k === 'table' || k === 'inline_table' || k === 'array' || k === 'array_of_tables';
  }
  function isEditablePrimitive(k: TomlNodeKind): boolean {
    return k === 'string' || k === 'integer' || k === 'float' || k === 'bool';
  }

  let roots         = $state<TNode[]>([]);
  let expanded      = $state<Set<string>>(new Set());
  let selectedNode  = $state<TNode | null>(null);
  let valueText     = $state<string | null>(null);
  let valueLoading  = $state(false);
  let expandAllBusy = $state(false);

  async function selectNode(node: TNode): Promise<void> {
    await treePane?.selectNode(node);
  }

  async function commitPendingEdit(): Promise<void> {
    if (editingPid && editingPid !== selectedNode?.pid) {
      try { await maybeCommitActiveEdit(); }
      catch { cancelEdit(); }
    }
  }

  // ── Edit pipeline ──────────────────────────────────────────────────────
  let editingPid    = $state<string | null>(null);
  let editLocation  = $state<'tree' | 'detail'>('detail');
  let editBuf       = $state('');
  let editError     = $state<string | null>(null);

  // Inline-editor refs (tree-location). Mirror of RON's
  // `editInlineEl` / `editInlineSelectEl`.
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

  const EDIT_BANNER_KEY = 'arbor:toml-studio:edit-warning-dismissed';
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
    // Schema-aware dispatch — enum-typed string nodes route to the
    // variant picker; everything else stays on the free-text editor.
    if (selectedNode.kind === 'string' && enumDefAt(selectedNode.path)) {
      startVariantEdit(location);
      return;
    }
    if (!isEditablePrimitive(selectedNode.kind)) return;
    let seed = valueText ?? selectedNode.preview;
    if (selectedNode.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
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
      // `{#if}` branch that mounts AFTER editingPid + editLocation
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
    // Schema-aware numeric narrowing: if a node is currently typed as
    // `integer` but the schema declares this position as a float, allow
    // the user to write `1.5` without parse failure. And vice versa —
    // an integer-typed schema position rejects a decimal even if TOML
    // would happily accept a float there. Skips when schema isn't bound.
    const hint = schema ? primitiveHintAt(node.path) : null;
    const wantFloat = hint === 'f32' || hint === 'f64' || hint === 'number';
    const wantInt   = hint === 'integer' || (hint != null &&
      (hint.startsWith('i') || hint.startsWith('u')) && hint !== 'isize' && hint !== 'usize') ||
      hint === 'isize' || hint === 'usize';
    const wantBool   = hint === 'bool' || hint === 'boolean';
    const wantString = hint === 'string' || hint === 'String' || hint === '&str' || hint === 'str';

    let value: StudioPrimitiveValue;
    try {
      // Enum-of-strings (handled by variant editor, but if we land here
      // anyway treat the buf as a free-text string value).
      switch (node.kind) {
        case 'string':
          if (wantBool) {
            const t = editBuf.trim().toLowerCase();
            if (t !== 'true' && t !== 'false') throw new Error('schema: expected boolean');
            value = { type: 'bool', value: t === 'true' };
            break;
          }
          if (wantInt) {
            const s = editBuf.trim();
            const n = Number(s);
            if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          if (wantFloat) {
            const s = editBuf.trim();
            const n = Number(s);
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
        case 'integer': {
          const s = editBuf.trim();
          const n = Number(s);
          if (!Number.isFinite(n)) throw new Error('not an integer');
          // Float schema → promote to float (allows decimal input).
          if (wantFloat) { value = { type: 'float', value: n }; break; }
          // Schema explicitly wants string → keep the literal text.
          if (wantString) { value = { type: 'string', value: editBuf }; break; }
          if (!Number.isInteger(n) && !/^-?\d+(_\d+)*$/.test(s)) {
            throw new Error('not an integer');
          }
          value = { type: 'int', value: Math.trunc(n) };
          break;
        }
        case 'float': {
          const s = editBuf.trim();
          const n = Number(s);
          if (!Number.isFinite(n)) throw new Error('not a number');
          // Schema explicitly wants integer → reject a decimal.
          if (wantInt) {
            if (!Number.isInteger(n)) throw new Error('schema: expected integer');
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          if (wantString) { value = { type: 'string', value: editBuf }; break; }
          value = { type: 'float', value: n };
          break;
        }
        default: return;
      }
    } catch (e: any) {
      editError = e?.message ?? String(e);
      return;
    }
    try {
      await tomlStudioStore.mutatePrimitive(node.path, value);
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

  // ── Removability + remove action ───────────────────────────────────────
  function isRemovable(node: TNode | null): boolean {
    if (!node || node.path.length === 0) return false;
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent) return false;
    return isContainerKind(parent.kind);
  }

  async function removeSelected(): Promise<void> {
    if (!selectedNode || !isRemovable(selectedNode)) return;
    const node = selectedNode;
    try {
      await tomlStudioStore.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('toml-studio: removeAt failed', e);
    }
  }

  // ── Container mutations ────────────────────────────────────────────────
  async function addItemAction(parent: TNode): Promise<void> {
    if (parent.kind !== 'array' && parent.kind !== 'array_of_tables') return;
    // For plain arrays, push a stringy placeholder; for array-of-tables,
    // push an empty inline table.
    const snippet = parent.kind === 'array' ? '""' : '{}';
    try {
      await tomlStudioStore.insertItem(parent.path, snippet);
      await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('toml-studio: insertItem failed', e);
    }
  }

  async function addFieldAction(parent: TNode, name?: string): Promise<void> {
    if (parent.kind !== 'table' && parent.kind !== 'inline_table') return;
    let key = name ?? '';
    if (!key) {
      const proposed = window.prompt('Field name:', 'new_field');
      if (!proposed) return;
      key = proposed;
    }
    try {
      await tomlStudioStore.insertField(parent.path, key, '""');
      await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('toml-studio: insertField failed', e);
    }
  }

  async function duplicateAction(node: TNode): Promise<void> {
    if (!isRemovable(node)) return;
    try {
      await tomlStudioStore.duplicateAt(node.path);
      const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('toml-studio: duplicateAt failed', e);
    }
  }

  async function moveAction(node: TNode, delta: number): Promise<void> {
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent || (parent.kind !== 'array' && parent.kind !== 'array_of_tables')) return;
    try {
      await tomlStudioStore.moveItem(node.path, delta);
      await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('toml-studio: moveItem failed', e);
    }
  }

  async function pasteOverAction(node: TNode): Promise<void> {
    let text: string;
    try { text = await navigator.clipboard.readText(); }
    catch { uiStore.showToast('Clipboard read denied', 'error'); return; }
    const t = text.trim();
    if (!t) { uiStore.showToast('Clipboard is empty', 'error'); return; }
    try {
      await tomlStudioStore.replaceAt(node.path, t);
      await refreshAfterMutation(node, /* structural */ true);
      maybeShowEditBanner();
    } catch (e: any) {
      uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error');
    }
  }

  // ── F12 — Cross-refs / Rename across project ──────────────────────────
  //
  // TOML follows the same convention as JSON / RON: `id`/`name` string
  // fields are definitions; `*_id` / `*_ref` / `target` / etc. are
  // references (default convention or per-binding patterns from the
  // shared studioStore).

  function unquotedString(preview: string): string | null {
    // Tree previews wrap strings in `"…"`. Truncated previews end in
    // `…` — skip those so we don't rename a half-string.
    if (preview.length < 2) return null;
    if (!preview.startsWith('"') || !preview.endsWith('"')) return null;
    const inner = preview.slice(1, -1);
    if (inner.endsWith('…')) return null;
    return inner;
  }

  function isDefinitionFieldName(key: string): boolean {
    return key === 'id' || key === 'name';
  }

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
    const repoRel = relPathInRepo(tomlStudioStore.sourcePath);
    const patterns = repoRel ? studioStore.referenceFieldsFor(repoRel) : null;
    if (!patterns) return builtinIsReferenceField(key);
    return patterns.some(p => studioStore.matchesPattern(p, key));
  }

  /** For a string node, work out which key it sits under. A scalar
   *  inside a list inherits the list's key (so `tags = ["x"]` reads
   *  through `tags`). */
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

  let renameModalState = $state<{ oldValue: string } | null>(null);

  function openRenameModal(node: TNode): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Rename across project',
        'No active project — open this TOML file from a project tab to rename across files.',
        'warning',
      );
      return;
    }
    const value = unquotedString(node.preview);
    if (!value) return;
    renameModalState = { oldValue: value };
  }

  function closeRenameModal(): void { renameModalState = null; }

  function buildOpenDocsSnapshot(): RenameOpenDoc[] {
    if (!tomlStudioStore.docId) return [];
    return [{
      doc_id:      tomlStudioStore.docId,
      source_path: tomlStudioStore.sourcePath,
      dirty:       tomlStudioStore.dirty,
    }];
  }

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = tomlStudioStore.sourcePath;
    if (!path) return;
    const title = tomlStudioStore.title;
    await tomlStudioStore.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  async function onRenameApplied(result: RenameResult): Promise<void> {
    closeRenameModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    const sp = tomlStudioStore.sourcePath;
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
      try { await studioStore.loadCrossRefsForKind(aTab, 'toml', true); } catch { /* soft */ }
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

  async function jumpToUsage(hit: UsageMatch): Promise<void> {
    const sp = tomlStudioStore.sourcePath;
    const sameFile = sp && hit.absolute_path.replace(/\\/g, '/').toLowerCase()
                       === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(hit.field_path);
      return;
    }
    try {
      await tomlStudioStore.openDoc({ path: hit.absolute_path });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(hit.field_path);
    } catch (e) {
      console.warn('jumpToUsage: open target failed', e);
    }
  }

  async function openDefinition(d: CrossRefDef): Promise<void> {
    const sp = tomlStudioStore.sourcePath;
    const sameFile = sp && d.absolute_path.replace(/\\/g, '/').toLowerCase()
                       === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(d.def_path);
      return;
    }
    try {
      await tomlStudioStore.openDoc({ path: d.absolute_path });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(d.def_path);
    } catch (e) {
      console.warn('openDefinition: open target failed', e);
    }
  }

  // ── F13 — Bulk edit by query ─────────────────────────────────────────
  //
  // Query bar surfaces `[⚡ Edit]` because the descriptor's
  // `supports_bulk_edit` is true (Phase 4.c.b.1). TOML's descriptor
  // declares `null_handling = AsDelete`, so the modal warns the user
  // that writing `null` removes the key (no native null in TOML).

  let bulkEditModalState = $state<{ query: string } | null>(null);

  function openBulkEditModal(q: string): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Bulk edit by query',
        'No active project — open this TOML file from a project tab to run a bulk edit.',
        'warning',
      );
      return;
    }
    if (!tomlStudioStore.docId) return;
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
        await tomlStudioStore.applyExternalMutate(result.active_doc_state);
        await treePane?.reloadTree();
      } catch (e) {
        console.warn('bulk edit: active-doc sync failed', e);
      }
    } else {
      // Project-wide — TOML Studio is single-doc; reload from disk if
      // the active doc's source got rewritten.
      const sp = tomlStudioStore.sourcePath;
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
        try { await studioStore.loadCrossRefsForKind(aTab, 'toml', true); } catch { /* soft */ }
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

  // ── Schema sidecar (RustStruct + JsonSchema) ─────────────────────────
  //
  // Phase 4.c.b.2 — TOML can bind to a Rust crate (`*.rs` source from
  // any crate Cargo.toml ancestor) OR a JSON Schema file. The backend
  // dispatches on the source path's extension. Both walkers produce the
  // same `Schema` / `CrateProbe` / `TypeSource` shapes so `<StudioSchemaPanel>`
  // and the schema-aware tree editor consume them uniformly.

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
      const probe = await TOML_BE.schemaProbe(rsPath);
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
      schema = await TOML_BE.schemaLoad(schemaRsPath, schemaRootSel);
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

  // ── View source (Rust struct or JSON Schema fragment) ───────────────
  let viewSource:    TypeSource | null = $state(null);
  let viewSourceBusy                  = $state(false);
  let viewSourceErr: string | null    = $state(null);

  async function openViewSource(canonical: string): Promise<void> {
    if (!schemaRsPath) return;
    viewSourceBusy = true;
    viewSourceErr  = null;
    viewSource     = null;
    try {
      viewSource = await TOML_BE.schemaViewSource(schemaRsPath, canonical);
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

  // ── Schema-aware type walker ────────────────────────────────────────
  //
  // Mirrors RON Studio's `typeAtPath` / `enumDefAt` pair but adapted to
  // TOML's container kinds (`table`/`inline_table` → struct, `array` /
  // `array_of_tables` → vec). The walker projects through `serde_json`-
  // style structural paths (which is exactly how the BE addresses nodes
  // for queries + cross-refs).
  //
  // Schema-aware inline editing uses this to drive two affordances:
  //   1. Enum-of-strings → `<select>` instead of free-text input
  //      (rowEditMode = 'variant').
  //   2. Numeric narrowing — a primitive `integer` schema rejects a
  //      decimal input even if TOML would accept it as a float.

  // Delegates to the shared schema walker (`studio-schema.ts`) so
  // serde rename / alias / rename_all / flatten work uniformly across
  // every studio modal.
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

  /** A schema-typed primitive narrows the editor: integer fields take
   *  int only, float fields take float, etc. Returns the primitive
   *  hint name when the schema constrains this path. */
  function primitiveHintAt(path: string[]): string | null {
    let ty = typeAtPath(path);
    if (!ty) return null;
    if (ty.kind === 'option') ty = ty.inner;
    if (ty.kind === 'primitive') return ty.name;
    return null;
  }

  /** What kind of inline editor does the row need? `primitive` =
   *  free-text (with type narrowing); `variant` = `<select>` populated
   *  by the enum's unit variants (TOML supports only string-shaped
   *  enums — Rust/JSON Schema unit variants reduce to "the string is
   *  this constant"). */
  function rowEditMode(node: TNode): 'primitive' | 'variant' | null {
    // String values whose schema position is a unit-only enum →
    // variant picker. Rust crate enums + JSON Schema string-enums
    // both reduce to TypeDef::Enum with unit variants in our shared
    // schema shape.
    if (node.kind === 'string') {
      const ed = enumDefAt(node.path);
      if (ed && ed.variants.length > 0 && ed.variants.every(v => v.shape === 'unit')) {
        return 'variant';
      }
    }
    if (isEditablePrimitive(node.kind)) return 'primitive';
    return null;
  }

  /** Current variant tag for an enum-typed string node — for TOML this
   *  is just the unquoted string value. */
  function currentVariantTag(node: TNode): string {
    if (node.kind !== 'string') return '';
    const v = unquotedString(node.preview);
    return v ?? '';
  }

  function startVariantEdit(location: 'tree' | 'detail' = 'detail') {
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

  async function commitVariantEdit() {
    if (!editingPid || !selectedNode) return;
    const node = selectedNode;
    const name = editBuf;
    const current = currentVariantTag(node);
    editingPid = null;
    editError  = null;
    if (!name || name === current) return;
    try {
      await tomlStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
  }

  // Inspector → Tree adapters ──────────────────────────────────────────
  async function copyPathOf(node: TNode): Promise<void> {
    const text = node.path.length === 0 ? '$' : '$.' + node.path.join('.');
    await copyToClipboard(text, { successToast: 'Path copied', errorToast: true });
  }
  async function copyValue(): Promise<void> {
    if (valueText == null) return;
    await copyToClipboard(valueText);
  }
  async function inspectorAddField(parent: TNode, name: string): Promise<void> {
    await addFieldAction(parent, name);
  }
  function noopOption(): Promise<void> | void { /* TOML has no Option */ }

  /** Inspector variant pick — TOML enum values are strings; mutate the
   *  primitive directly. */
  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || selectedNode.kind !== 'string') return;
    const current = currentVariantTag(selectedNode);
    if (!name || name === current) return;
    const node = selectedNode;
    try {
      await tomlStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
  }

  // Render-helpers used by the Inspector schema strip + variant picker.
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

  // ── Tree-row type chips (parity with RON Studio) ─────────────────────
  //
  // Three layers stacked at the row's right edge:
  //   · `namedType` — short last-segment of a `named` resolved type
  //     (`I18nLanguages` for `crate::I18nLanguages`, `Server` for
  //     `#/$defs/Server`). Shown in accent colour.
  //   · `ty` chip — `fmtType(ty)` for non-named types (primitive / vec /
  //     map / option / tuple). Colour-coded by kind.
  //   · (suppressed when nothing's loaded; the row falls back to the
  //     existing kind badge + key + value layout.)

  function typeChipClass(ty: ResolvedType | null): string {
    if (!ty) return '';
    switch (ty.kind) {
      case 'primitive': return 'ts-type-prim';
      case 'option':    return 'ts-type-option';
      case 'vec':       return 'ts-type-vec';
      case 'map':       return 'ts-type-map';
      case 'tuple':     return 'ts-type-tupletype';
      case 'external':  return 'ts-type-external';
      case 'unknown':   return 'ts-type-unknown';
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

  // ── Cross-reference click affordance (parity with RON Studio) ────────
  //
  // String nodes that match a reference-field convention (`*_id`,
  // `*_ref`, etc.) AND whose value appears in the project-wide cross-ref
  // index get a small ↗ arrow + Ctrl/Cmd+click to jump. TOML Studio is
  // single-doc so cross-file jumps swap the doc via the same path used
  // by F12 `jumpToUsage`.

  type CrossRefEntry = {
    sourcePath: string;
    fileName:   string;
    defPath:    string[];
    title:      string;
  };

  function crossRefsForValue(value: string): CrossRefEntry[] {
    return studioStore.findCrossRefsForKind(value, 'toml').map(d => ({
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

  /** Tiny Svelte action that re-parents the node to `document.body` on
   *  mount and removes it on destroy. The cross-ref picker needs to
   *  escape the modal's transformed stack so `position: fixed`
   *  coordinates stay viewport-relative. */
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() { node.parentNode?.removeChild(node); },
    };
  }

  async function jumpToCrossRef(target: CrossRefEntry): Promise<void> {
    crossRefPicker = null;
    const sp = tomlStudioStore.sourcePath;
    const sameFile = sp && target.sourcePath.replace(/\\/g, '/').toLowerCase()
                      === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(target.defPath);
      return;
    }
    try {
      await tomlStudioStore.openDoc({ path: target.sourcePath });
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

  // ── Context menu ───────────────────────────────────────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({ id: 'copy-path',  label: 'Copy path',         icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: 'Copy value (TOML)', icon: Copy,     iconColor: 'var(--text-muted)' });

    const editMode = rowEditMode(node);
    if (editMode === 'variant') {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
      items.push({ id: 'edit-variant', label: 'Change variant…', icon: Replace, iconColor: '#ffc66d', shortcut: 'F2' });
    } else if (editMode === 'primitive') {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
      items.push({ id: 'edit', label: 'Edit value', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
    }

    // Schema "View implementation" — show only when the selected node
    // resolves to a named type that's part of the loaded schema.
    if (schema && typeAtPath(node.path)) {
      const ty = typeAtPath(node.path);
      let namedPath: string | null = null;
      if (ty?.kind === 'named') namedPath = ty.path;
      else if (ty?.kind === 'option' && ty.inner.kind === 'named') namedPath = ty.inner.path;
      if (namedPath && schema.types[namedPath]) {
        items.push({ id: 'sep-schema', label: '', separator: true } as MenuItem);
        items.push({ id: 'view-impl', label: 'View implementation', icon: BookOpen, iconColor: '#20b2aa' });
      }
    }

    items.push({ id: 'sep-mutate', label: '', separator: true } as MenuItem);
    items.push({ id: 'paste', label: 'Paste TOML over value…', icon: ClipboardPaste, iconColor: 'var(--text-muted)' });

    if (node.kind === 'table' || node.kind === 'inline_table') {
      items.push({ id: 'add-field', label: 'Add field…', icon: Plus, iconColor: 'var(--success)' });
    } else if (node.kind === 'array' || node.kind === 'array_of_tables') {
      items.push({ id: 'add-item', label: 'Add item', icon: Plus, iconColor: 'var(--success)' });
    }

    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (parent && isContainerKind(parent.kind)) {
      items.push({ id: 'sep-reorder', label: '', separator: true } as MenuItem);
      items.push({ id: 'duplicate', label: 'Duplicate', icon: CopyPlus, iconColor: 'var(--text-muted)' });
      if (parent.kind === 'array' || parent.kind === 'array_of_tables') {
        const idx = parseInt(node.key, 10);
        const total = parent.child_count;
        items.push({ id: 'move-up',   label: 'Move up',   icon: ArrowUp,   iconColor: 'var(--text-muted)',
                     disabled: !Number.isFinite(idx) || idx <= 0 });
        items.push({ id: 'move-down', label: 'Move down', icon: ArrowDown, iconColor: 'var(--text-muted)',
                     disabled: !Number.isFinite(idx) || idx >= total - 1 });
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

    if (isRemovable(node)) {
      items.push({ id: 'sep-remove', label: '', separator: true } as MenuItem);
      items.push({ id: 'remove', label: 'Remove', icon: Trash2, danger: true });
    }

    // F12 — Rename across project. Gated on an active project tab +
    // the node being a renameable string (definition or reference).
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
          const v = await TOML_BE.getValue(tomlStudioStore.docId!, node.path);
          await copyToClipboard(v);
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

  // ── Query bar ──────────────────────────────────────────────────────────
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
  let queryHits     = $state<TomlQueryHit[]>([]);
  let queryError    = $state<string | null>(null);
  let querying      = $state(false);
  let currentHitIdx = $state(0);

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
  let textBuf = $state<string>('');
  let pushTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleTextPush() {
    if (pushTimer) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
      void tomlStudioStore.setText(textBuf).then(() => {
        void treePane?.reloadTree();
      });
    }, 180);
  }

  function onTextInput(next: string) {
    textBuf = next;
    scheduleTextPush();
  }

  // External store changes → editor buffer. `untrack` to avoid forming a
  // write→read loop with `bumpDiffRefresh`'s read-modify-write.
  $effect(() => {
    const c = tomlStudioStore.current;
    untrack(() => { textBuf = c; });
  });

  // Doc lifecycle — reset view / query / known keys when docId clears
  // OR a new doc opens.
  $effect(() => {
    const id = tomlStudioStore.docId;
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
    viewMode = 'tree';
  });

  // Cross-ref index — load on modal open + every active-tab change.
  // Parity with RON Studio: the ↗ chip on tree rows needs the index
  // populated to know which strings to decorate.
  $effect(() => {
    if (!tomlStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, 'toml'); });
  });

  // Auto-load schema from the .arbor/studio.toml binding. The host
  // resolves the binding when the doc is opened with tabId +
  // relativePath; `parse` returns it as `schema_hint`. Mirror of RON's
  // auto-load. TOML accepts both .rs and .schema.json as source.
  $effect(() => {
    const hint = tomlStudioStore.schemaHint;
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
      schemaProbe   = await TOML_BE.schemaProbe(rsFile);
      schemaRootSel = rootCanonical;
      schema        = await TOML_BE.schemaLoad(rsFile, rootCanonical);
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }

  // ── Diff view ──────────────────────────────────────────────────────────
  let diffRefreshTick      = $state(0);
  let diffHunkCount        = $state(0);
  let diffTreeChangeCount  = $state(0);
  function bumpDiffRefresh() { untrack(() => { diffRefreshTick++; }); }

  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: diffTreeChangeCount > 0 ? diffTreeChangeCount
           : diffHunkCount > 0       ? diffHunkCount
           : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse errors',
      disabled: !tomlStudioStore.parseError,
      badge: tomlStudioStore.parseError ? '!' : undefined,
      data: { errorBadge: !!tomlStudioStore.parseError } },
  ]);

  // ── Indent + Format ────────────────────────────────────────────────────
  let indentUnit = $state<string>('  ');
  let actionBusy = $state(false);
  let actionError = $state<string | null>(null);
  $effect(() => {
    const id = tomlStudioStore.docId;
    if (!id) return;
    void TOML_BE.getIndent(id).then(s => { if (s) indentUnit = s; }).catch(() => {});
  });

  // ── Footer snapshot (consumed by shared StudioFooter* components) ──────
  const footerDoc: StudioFooterDoc = $derived({
    parseError: tomlStudioStore.parseError ?? null,
    dirty:      tomlStudioStore.dirty,
    sourcePath: tomlStudioStore.sourcePath ?? null,
    encoding:   tomlStudioStore.docId ? tomlStudioStore.encoding : null,
    canUndo:    tomlStudioStore.canUndo,
    canRedo:    tomlStudioStore.canRedo,
    docId:      tomlStudioStore.docId ?? null,
  });
  const selectedFooterPath = $derived<string[] | null>(
    selectedNode && viewMode === 'tree' ? selectedNode.path : null,
  );

  async function setIndentUnit(unit: string): Promise<void> {
    indentUnit = unit;
    const id = tomlStudioStore.docId;
    if (!id) return;
    try { await TOML_BE.setIndent(id, unit); } catch (e) {
      console.warn('toml-studio: setIndent failed', e);
    }
  }
  async function runFormat(): Promise<void> {
    const id = tomlStudioStore.docId;
    if (!id || actionBusy || tomlStudioStore.parseError) return;
    actionBusy = true; actionError = null;
    try {
      const formatted = await TOML_BE.format(id);
      await tomlStudioStore.setText(formatted);
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

  async function doSave(): Promise<void> {
    if (!tomlStudioStore.sourcePath) { savePickerOpen = true; return; }
    await runSave();
  }
  async function runSave(): Promise<void> {
    saving = true; saveError = null;
    try {
      await tomlStudioStore.save({ path: null, bindToDoc: false });
      bumpDiffRefresh();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
  function openSaveAs() { savePickerOpen = true; }
  async function onSaveAsPicked(p: string) {
    savePickerOpen = false;
    saving = true; saveError = null;
    try {
      await tomlStudioStore.save({ path: p, bindToDoc: true });
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
    await tomlStudioStore.closeDoc();
  }

  // fmtBytes / jsBasename moved to shared/studio/helpers.ts.
  const fmtBytes   = fsFmtBytes;
  const jsBasename = fsBasename;

  function kindBadge(k: TomlNodeKind): string {
    switch (k) {
      case 'table':           return '{}';
      case 'inline_table':    return '{ }';
      case 'array':           return '[]';
      case 'array_of_tables': return '[[]]';
      case 'string':          return '“';
      case 'integer':         return '#';
      case 'float':           return '⊘';
      case 'bool':            return '✓';
      case 'datetime':        return '🕒';
    }
  }
  function isBoolKind(k: TomlNodeKind): boolean { return k === 'bool'; }

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  function onKey(e: KeyboardEvent) {
    if (!tomlStudioStore.open) return;
    const target = e.target as HTMLElement | null;
    const inEditableField = target instanceof HTMLInputElement
                          || target instanceof HTMLTextAreaElement
                          || target instanceof HTMLSelectElement
                          || (target?.closest('.cm-editor') !== null && target?.closest('.cm-editor') !== undefined);

    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === 's' || e.key === 'S')) {
      e.preventDefault(); e.stopPropagation();
      void doSave();
      return;
    }
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
    if (e.key === 'F3') {
      if (viewMode === 'tree')      { e.preventDefault(); queryBar?.nav(e.shiftKey ? -1 : 1); }
      else if (viewMode === 'diff') { e.preventDefault(); diffPane?.nav(e.shiftKey ? -1 : 1); }
      return;
    }
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
    if (e.key === 'Delete' && viewMode === 'tree' && !inEditableField) {
      if (isRemovable(selectedNode)) {
        e.preventDefault();
        void removeSelected();
      }
      return;
    }
  }

  async function doUndo() {
    const ok = await tomlStudioStore.undo();
    if (ok) {
      await treePane?.reloadTree();
      bumpDiffRefresh();
    }
  }
  async function doRedo() {
    const ok = await tomlStudioStore.redo();
    if (ok) {
      await treePane?.reloadTree();
      bumpDiffRefresh();
    }
  }

  void tick;
</script>

<svelte:window on:keydown={onKey} />

<StudioModal
  bind:this={studioModal}
  formatId="toml"
  backend={TOML_BE}
  open={tomlStudioStore.open}
  loading={tomlStudioStore.loading}
  loadingLabel="Parsing TOML…"
  errorState={tomlStudioStore.error}
  parseError={tomlStudioStore.parseError}
  hasDoc={!!tomlStudioStore.docId}
  viewItems={viewItems}
  bind:viewMode
  bind:rightPane
  rightPaneStorageKey={RIGHT_PANE_KEY}
  ariaLabel="TOML Studio"
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
        <span class="ts-rail-count" aria-hidden="true">{queryHits.length >= 100 ? '99+' : queryHits.length}</span>
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
        : 'Schema — bind a Rust struct (`.rs`) or JSON Schema file'}
      aria-label="Schema"
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
    <span class="ts-header-icon-wrap" aria-hidden="true">
      <FileText size={14} />
    </span>
    <StudioHeaderUndoRedo doc={footerDoc} onUndo={doUndo} onRedo={doRedo} />
    <span class="ts-title" use:tooltip={tomlStudioStore.sourcePath ?? ''}>
      {tomlStudioStore.title ?? 'TOML Studio'}
      {#if tomlStudioStore.dirty}<span class="ts-dirty" use:tooltip={'Unsaved changes'}>●</span>{/if}
    </span>
    {#if tomlStudioStore.sizeBytes != null}
      <span class="ts-meta">{fmtBytes(tomlStudioStore.sizeBytes)}</span>
    {/if}
    <div class="ts-spacer"></div>
  {/snippet}

  {#snippet footerStatusLeft()}
    <StudioFooterStatus doc={footerDoc} selectedPath={selectedFooterPath} />
  {/snippet}

  {#snippet toolsSidecar()}
    <StudioToolsSidebar
      doc={footerDoc}
      {actionBusy}
      {indentUnit}
      indentTooltip="Indent — informational; toml_edit owns per-table decor"
      formatTooltip="Format — re-emit through toml_edit (may normalise trailing newline / whitespace)"
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
    <StudioBodyBanners {saveError} {actionError} />
  {/snippet}

  {#snippet queryBarSlot()}
    <StudioQueryBar
      bind:this={queryBar}
      formatId="toml"
      backend={TOML_BE}
      docId={tomlStudioStore.docId}
      visible={viewMode === 'tree' && !tomlStudioStore.parseError}
      placeholder='Query — name (recursive), $.section.key, $.servers[0], …'
      historyStorageKey="arbor:toml-studio:query-history"
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
        <span class="ts-row-badge ts-row-badge-{kind}" use:tooltip={kind}>{kindBadge(kind)}</span>
      {/snippet}
      {#snippet toolbarRight()}
        <button
          type="button"
          class="ts-query-tool-btn"
          onclick={() => void treePane?.expandAll()}
          disabled={expandAllBusy}
          use:tooltip={'Recursively load + expand every container'}
          aria-label="Expand all"
        >{#if expandAllBusy}<Loader2 size={12} class="ts-query-spinner" />{:else}<ChevronsDown size={12} />{/if}<span>Expand</span></button>
        <button
          type="button"
          class="ts-query-tool-btn"
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
        formatId="toml"
        backend={TOML_BE}
        docId={tomlStudioStore.docId}
        parseError={tomlStudioStore.parseError}
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
        ariaLabel="TOML document tree"
      >
        {#snippet rowContent({ node }: RowSnippetCtx<any>)}
          {@const n = node as TNode}
          {@const ty = typeAtPath(n.path)}
          {@const namedType = namedTypeAt(n.path)}
          <span class="ts-row-badge ts-row-badge-{n.kind}" use:tooltip={n.kind}>{kindBadge(n.kind)}</span>
          <span class="ts-row-key" class:ts-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          <span class="ts-row-sep">=</span>
          {#if editingPid === n.pid && editLocation === 'tree'}
            {#if rowEditMode(n) === 'variant'}
              {@const ed = enumDefAt(n.path)}
              {#if ed}
                <select class="ts-inline-edit ts-inline-edit-variant"
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
              <select class="ts-inline-edit"
                      bind:this={editInlineSelectEl}
                      bind:value={editBuf}
                      onkeydown={onEditKey}
                      onclick={(e) => e.stopPropagation()}
                      onmousedown={(e) => e.stopPropagation()}>
                <option value="true">true</option>
                <option value="false">false</option>
              </select>
            {:else}
              <input class="ts-inline-edit"
                     bind:this={editInlineEl}
                     bind:value={editBuf}
                     onkeydown={onEditKey}
                     onclick={(e) => e.stopPropagation()}
                     onmousedown={(e) => e.stopPropagation()}
                     spellcheck="false" />
            {/if}
            {#if editError}
              <span class="ts-inline-edit-err" use:tooltip={editError}>!</span>
            {/if}
          {:else}
            {@const xrefs = crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="ts-row-preview ts-row-preview-{n.kind}"
                  class:ts-row-preview-editable={rowEditMode(n) !== null}
                  class:ts-row-preview-xref={hasX}
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
            >{n.preview}{#if hasX}<span class="ts-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="ts-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}
          {#if n.loading}<Loader2 size={10} class="ts-row-loader" />{/if}
          {#if namedType}
            <span class="ts-row-named" use:tooltip={fmtType(ty)}>{namedType}</span>
          {:else if ty && ty.kind !== 'named'}
            <span class="ts-row-type {typeChipClass(ty)}"
              use:tooltip={fmtType(ty)}
            >{fmtType(ty)}</span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textBuf}
        language="toml"
        oninput={onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="toml"
        backend={TOML_BE}
        docId={tomlStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={tomlStudioStore.current}
        refreshTick={diffRefreshTick}
        bind:treeChangeCount={diffTreeChangeCount}
        bind:hunkCount={diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- TOML has no variant tags. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if tomlStudioStore.parseError}
        <div class="ts-errors">
          <div class="ts-errors-head">
            <AlertCircle size={14} />
            <span>TOML parse error</span>
          </div>
          <pre class="ts-errors-body">{tomlStudioStore.parseError}</pre>
          <p class="ts-errors-hint">
            Switch to the <strong>Text</strong> tab to fix it. The error will
            clear automatically once the document parses.
          </p>
        </div>
      {:else}
        <div class="ts-errors-empty">
          <Check size={16} />
          <span>No parse errors.</span>
        </div>
      {/if}
    {/if}
  {/snippet}

  {#snippet inspectorSidecar()}
    <StudioInspectorPanel
      bind:this={inspectorPanel}
      formatId="toml"
      backend={TOML_BE}
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
      isBoolKind={isBoolKind as any}
      isContainerKind={isContainerKind as any}
      isDefinitionNode={((n: TNode) =>
        n.kind === 'string' && isDefinitionFieldName(n.key) && !!unquotedString(n.preview)) as any}
      definitionValue={((n: TNode) => {
        if (n.kind !== 'string' || !isDefinitionFieldName(n.key)) return null;
        return unquotedString(n.preview);
      }) as any}
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
    <div class="ts-panel-head">
      <ListFilter size={13} />
      <span class="ts-panel-title">Query results</span>
      {#if queryHits.length > 0}
        <span class="ts-panel-count">{queryHits.length}{queryHits.length >= 500 ? '+' : ''}</span>
      {/if}
      <span class="ts-spacer"></span>
    </div>
    <div class="ts-query-pane-body">
      {#if !query.trim()}
        <p class="ts-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if querying && queryHits.length === 0}
        <div class="ts-query-pane-status">
          <Spinner size="xs" /> <span>Running query…</span>
        </div>
      {:else if queryError}
        <div class="ts-query-pane-error">
          <AlertCircle size={11} /> {queryError}
        </div>
      {:else if queryHits.length === 0}
        <p class="ts-query-pane-empty">No matches.</p>
      {:else}
        <div class="ts-query-pane-list">
          {#each queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="ts-query-pane-card"
              class:active={i === currentHitIdx}
              onclick={() => { currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="ts-query-pane-card-head">
                <span class="ts-row-badge ts-row-badge-{hit.kind}" use:tooltip={hit.kind}>{kindBadge(hit.kind)}</span>
                <span class="ts-query-pane-card-idx">#{i + 1}</span>
              </div>
              <div class="ts-query-pane-card-path">{hit.path.length === 0 ? '$' : '$.' + hit.path.join('.')}</div>
              {#if hit.preview}
                <div class="ts-query-pane-card-preview">{hit.preview}</div>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="toml"
      backend={TOML_BE}
      sourcePath={tomlStudioStore.sourcePath}
      onOpenDefinition={openDefinition}
    >
      {#snippet emptyState()}
        <p class="ts-bindings-empty">
          Project-wide cross-refs follow the <code>id</code> / <code>name</code>
          convention by default. Custom reference-field patterns live
          in the repo's <code>.studio.toml</code> bindings.
        </p>
      {/snippet}
    </StudioRefsPanel>
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId="toml"
      backend={TOML_BE}
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
      pickerTitle="Pick schema source (.rs or .schema.json)"
      pickerExtensions={['rs', 'json', 'schema.json']}
      pickerButtonLabel="Pick schema file"
    >
      {#snippet intro()}
        <p class="ts-schema-hint">
          Pick a schema source for this TOML document:
          a Rust source file (<code>*.rs</code>) from a crate that
          deserialises this TOML via <code>serde</code>, or a JSON Schema
          file (<code>*.schema.json</code>). TOML Studio surfaces every
          struct/enum (Rust) or <code>$defs</code> entry (JSON Schema)
          as a root candidate.
        </p>
      {/snippet}
    </StudioSchemaPanel>
  {/snippet}

  {#snippet auxiliary()}
    {#if savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save TOML document as"
        extensions={['toml']}
        initialPath={tomlStudioStore.sourcePath ?? undefined}
        initialFilename={jsBasename(tomlStudioStore.sourcePath) || 'document.toml'}
        onConfirm={onSaveAsPicked}
        onCancel={() => savePickerOpen = false}
      />
    {/if}

    {#if renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={TOML_BE}
        tabId={tabsStore.activeTabId}
        formatLabel="TOML"
        oldValue={renameModalState.oldValue}
        openDocs={buildOpenDocsSnapshot()}
        onClose={closeRenameModal}
        onApplied={onRenameApplied}
      />
    {/if}

    {#if bulkEditModalState && tabsStore.activeTabId && tomlStudioStore.docId}
      <StudioBulkEditModal
        backend={TOML_BE}
        tabId={tabsStore.activeTabId}
        docId={tomlStudioStore.docId}
        formatLabel="TOML"
        query={bulkEditModalState.query}
        nullPolicy="as_delete"
        openDocs={buildBulkEditOpenDocs()}
        onClose={closeBulkEditModal}
        onApplied={onBulkEditApplied}
      />
    {/if}

    {#if crossRefPicker}
      <div class="ts-xref-overlay"
           use:portal
           role="presentation"
           onclick={() => crossRefPicker = null}
           oncontextmenu={(e) => { e.preventDefault(); crossRefPicker = null; }}
      >
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="ts-xref-popover"
             style:left="{crossRefPicker.x}px"
             style:top="{crossRefPicker.y}px"
             role="menu"
             onclick={(e) => e.stopPropagation()}
        >
          <div class="ts-xref-header">{crossRefPicker.entries.length} matches</div>
          {#each crossRefPicker.entries as entry (entry.sourcePath + entry.defPath.join('\x00'))}
            <button
              type="button"
              class="ts-xref-item"
              onclick={() => void jumpToCrossRef(entry)}
            >
              <span class="ts-xref-item-icon"><FileTextIcon size={13} /></span>
              <span class="ts-xref-item-name">{entry.title}</span>
              <span class="ts-xref-item-path">{entry.defPath.join('.')}</span>
              <span class="ts-xref-item-open">›</span>
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
        language={schemaRsPath && /\.rs$/i.test(schemaRsPath) ? 'rust' : 'json'}
        loadingLabel="Loading schema fragment…"
        onClose={closeViewSource}
      />
    {/if}
  {/snippet}
</StudioModal>

<style>
  /* Header ─────────────────────────────────────────────────────────── */
  .ts-header-icon-wrap { display: inline-flex; align-items: center; color: var(--accent); flex-shrink: 0; }
  .ts-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    max-width: 50%;
  }
  .ts-dirty {
    color: var(--accent);
    font-size: 14px;
    margin-left: 4px;
    line-height: 1;
  }
  .ts-meta {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 2px 6px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .ts-spacer { flex: 1; }

  /* Right activity rail counters. */
  .ts-rail-count {
    position: absolute;
    bottom: 2px;
    right: 2px;
    background: var(--accent);
    color: var(--bg-base);
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
    padding: 1px 3px;
    border-radius: 6px;
    min-width: 12px;
    text-align: center;
  }

  /* Footer pill / button / sep styles moved to the shared
     <StudioFooter*> components in `./studio/`. */

  /* Row content (Tree pane). */
  .ts-row-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
  }
  .ts-row-badge-table,
  .ts-row-badge-inline_table { color: var(--syntax-keyword, #cc7832); }
  .ts-row-badge-array,
  .ts-row-badge-array_of_tables { color: var(--syntax-keyword, #cc7832); }
  .ts-row-badge-string   { color: var(--syntax-string, #6a9956); }
  .ts-row-badge-integer,
  .ts-row-badge-float    { color: var(--syntax-number, #9876aa); }
  .ts-row-badge-bool     { color: var(--syntax-number, #9876aa); }
  .ts-row-badge-datetime { color: var(--syntax-type, #4d78cc); }
  .ts-row-key {
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11px;
    white-space: nowrap;
  }
  .ts-row-key-index {
    color: var(--text-muted);
    font-style: italic;
  }
  .ts-row-sep {
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 11px;
    margin: 0 4px;
  }
  .ts-row-preview {
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }
  .ts-row-preview-string { color: var(--syntax-string, #6a9956); }
  .ts-row-preview-integer,
  .ts-row-preview-float  { color: var(--syntax-number, #9876aa); }
  .ts-row-loader { color: var(--text-muted); flex-shrink: 0; }

  /* Inline edit (tree-location). Mirror of RON's `.rs-inline-edit`. */
  .ts-inline-edit {
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
  .ts-inline-edit:focus {
    border-color: var(--accent-strong, var(--accent));
    box-shadow: 0 0 0 2px var(--accent-subtle);
  }
  .ts-inline-edit-err {
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

  /* Errors view. */
  .ts-errors {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    overflow: auto;
  }
  .ts-errors-head {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-error, #ff6c5c);
    font-size: 12px;
    font-weight: 600;
  }
  .ts-errors-body {
    background: var(--bg-overlay);
    color: var(--text-primary);
    padding: 10px;
    border-radius: 4px;
    font-family: var(--font-code);
    font-size: 11px;
    margin: 0;
    overflow: auto;
    white-space: pre-wrap;
  }
  .ts-errors-hint {
    color: var(--text-muted);
    font-size: 11px;
    margin: 0;
  }
  .ts-errors-empty {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 16px;
    color: var(--text-secondary);
    font-size: 12px;
  }

  /* Query pane sidecar. */
  .ts-panel-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-secondary);
  }
  .ts-panel-title { font-weight: 600; color: var(--text-primary); }
  .ts-panel-count {
    background: var(--bg-overlay);
    color: var(--text-muted);
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
  }
  .ts-query-pane-body {
    padding: 8px;
    overflow: auto;
    height: 100%;
  }
  .ts-query-pane-empty,
  .ts-query-pane-status,
  .ts-query-pane-error {
    color: var(--text-muted);
    font-size: 11px;
    padding: 8px;
    margin: 0;
    line-height: 1.5;
  }
  .ts-query-pane-error { color: var(--text-error, #ff6c5c); display: inline-flex; align-items: center; gap: 4px; }
  .ts-query-pane-list { display: flex; flex-direction: column; gap: 4px; }
  .ts-query-pane-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    border-radius: 4px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11px;
    cursor: pointer;
    text-align: left;
  }
  .ts-query-pane-card:hover { background: var(--bg-hover); }
  .ts-query-pane-card.active {
    border-color: var(--accent);
    background: var(--bg-hover);
  }
  .ts-query-pane-card-head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ts-query-pane-card-idx { color: var(--text-muted); }
  .ts-query-pane-card-path { color: var(--text-primary); }
  .ts-query-pane-card-preview { color: var(--text-secondary); }

  .ts-query-tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    height: 22px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border-radius: 4px;
    font-size: 10px;
    cursor: pointer;
  }
  .ts-query-tool-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .ts-query-tool-btn:disabled {
    color: var(--text-disabled);
    cursor: not-allowed;
  }
  .ts-bindings-empty {
    color: var(--text-muted);
    font-size: 11px;
    padding: 12px;
    margin: 0;
    line-height: 1.5;
  }
  .ts-bindings-empty code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .ts-query-spinner {
    animation: ts-spin 1s linear infinite;
  }
  @keyframes ts-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  /* Schema sidecar (Phase 4.c.b.2) ─────────────────────────────────── */
  .ts-schema-hint {
    color: var(--text-secondary);
    font-size: 11px;
    line-height: 1.5;
    margin: 0;
  }
  .ts-schema-hint code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .ts-source-pre {
    margin: 0;
    padding: 16px;
    overflow: auto;
    background: var(--bg-base);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.5;
    height: 100%;
    box-sizing: border-box;
  }
  :global(.ts-header-icon) { color: var(--accent); flex-shrink: 0; }

  /* Variant editor (schema-aware enum picker). */
  .ts-inline-edit-variant {
    background: var(--bg-base);
    color: var(--syntax-keyword, #cc7832);
    padding-right: 18px;
  }
  /* Visual hint that a row's preview is double-click-editable when
     schema has narrowed the type (primitive or variant). */
  .ts-row-preview-editable { cursor: text; }

  /* Type chips at row's right edge (parity with RON). */
  .ts-row-type {
    margin-left: auto;
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 10px;
    padding: 1px 6px;
    background: var(--bg-overlay);
    border-radius: 8px;
    flex-shrink: 0;
  }
  .ts-row-type.ts-type-prim {
    color:      var(--syntax-type, #61afef);
    background: color-mix(in srgb, var(--syntax-type, #61afef) 14%, var(--bg-overlay));
  }
  .ts-row-type.ts-type-option {
    color:      var(--syntax-keyword, #d19a66);
    background: color-mix(in srgb, var(--syntax-keyword, #d19a66) 14%, var(--bg-overlay));
  }
  .ts-row-type.ts-type-vec {
    color:      var(--syntax-function, #c678dd);
    background: color-mix(in srgb, var(--syntax-function, #c678dd) 14%, var(--bg-overlay));
  }
  .ts-row-type.ts-type-map {
    color:      var(--syntax-char, #56b6c2);
    background: color-mix(in srgb, var(--syntax-char, #56b6c2) 14%, var(--bg-overlay));
  }
  .ts-row-type.ts-type-tupletype {
    color:      var(--syntax-decimal, #e5c07b);
    background: color-mix(in srgb, var(--syntax-decimal, #e5c07b) 14%, var(--bg-overlay));
  }
  .ts-row-type.ts-type-unknown {
    color:      var(--warning, #d19a66);
    background: color-mix(in srgb, var(--warning, #d19a66) 18%, transparent);
  }
  .ts-row-type.ts-type-external {
    color:      var(--text-disabled);
    background: var(--bg-overlay);
    font-style: italic;
  }
  .ts-row-named {
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

  /* Cross-ref ↗ arrow + Ctrl+click affordance. */
  .ts-row-preview-xref { cursor: pointer; }
  .ts-row-xref {
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
  .ts-row-preview-xref:hover .ts-row-xref {
    opacity: 1;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .ts-row-xref-count {
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 700;
    color: var(--accent);
  }

  /* Cross-ref multi-match picker (portaled to body). */
  .ts-xref-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: transparent;
    cursor: default;
  }
  .ts-xref-popover {
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
  .ts-xref-header {
    padding: 4px 8px 6px;
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 2px;
  }
  .ts-xref-item {
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
  .ts-xref-item:hover { background: var(--bg-hover); }
  .ts-xref-item-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .ts-xref-item-name {
    flex: 1;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-ui-sans);
    font-weight: 500;
  }
  .ts-xref-item-path {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .ts-xref-item-open {
    color: var(--accent);
    font-size: 14px;
    line-height: 1;
    margin-left: 2px;
  }
</style>
