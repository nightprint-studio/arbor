<!--
  JsonStudioModal — JSON wrapper around the generic `<StudioModal>` shell.

  Architecture mirrors YamlStudioModal: all reusable Studio logic lives
  in `./studio/composables/*` (edit pipeline, cross-refs, rename +
  bulk-edit, schema sidecar, query bar, text+diff, save flow, undo/redo,
  global keys); this file only owns the JSON-specific bits — JSONC
  banner + save prompt, stream-mode banner, schema-aware primitive
  narrowing, and the JSON Schema sidecar copy.

  Capabilities exposed to the user:
    · Tree view with mutations (edit primitives, remove, add field/item,
      duplicate, move, paste-over).
    · Text view via `<StudioTextPane>` (CodeMirror 6 + JSON language) —
      typing pushes through the host's lossless byte-splice editor.
    · Diff view via `<StudioDiffPane>` (text + tree sub-views).
    · Errors view — inline banner with the host's parse error.
    · Inspector + Query + Bindings + Schema right-rail panes.
    · F12 cross-ref rename modal — context-menu "Rename across project…".
    · F13 bulk edit by query — `[⚡ Edit]` chip in the query bar.
    · JSON Schema sidecar (Phase 3.c.b) — picks a `*.schema.json` and
      decorates the tree with type chips + variant pickers.
    · JSONC support — banner on `.json` with comments + 4-way save
      prompt (rename to .jsonc / strip / save anyway / cancel).
    · Stream-mode banner — large files open read-mostly with structural
      edits disabled.
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';
  import {
    FileJson, Copy, ListTree, FileText, AlertCircle, GitCompare,
    ChevronUp, ChevronDown, Replace,
    Pencil, ClipboardPaste,
    Trash2, Plus, CopyPlus, ArrowUp, ArrowDown,
    Maximize2, Minimize2,
    ListFilter, ScanSearch, Layers,
    Loader2, ChevronsDown, ChevronsUp, Link as LinkIcon,
    BookOpen, ArrowUpRight,
    Wrench,
  } from 'lucide-svelte';
  import Spinner from './ui/Spinner.svelte';
  import PanelShell from './ui/PanelShell.svelte';
  import Alert from './ui/Alert.svelte';
  import StateBlock from './ui/StateBlock.svelte';
  import TypePill from './internal/TypePill.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import { type MenuItem } from './ContextMenu.svelte';
  import { type RowSnippetCtx } from './ui/Tree.svelte';
  import { type TabItem } from './ui/Tabs.svelte';
  import StudioModal from './studio/StudioModal.svelte';
  import StudioRightRailButton from './studio/StudioRightRailButton.svelte';
  import StudioFooterStatus   from './studio/StudioFooterStatus.svelte';
  import StudioFooterRight    from './studio/StudioFooterRight.svelte';
  import StudioBodyBanners    from './studio/StudioBodyBanners.svelte';
  import StudioHeaderUndoRedo from './studio/StudioHeaderUndoRedo.svelte';
  import StudioToolsSidebar   from './studio/StudioToolsSidebar.svelte';
  import {
    INDENT_OPTIONS_WITH_8,
    type StudioFooterDoc,
  } from './studio/studio-footer-types';
  import { basename as fsBasename, fmtBytes as fsFmtBytes, typePillKind } from './studio/helpers';
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
  import StudioKindBadge, { type StudioKindTone } from './studio/StudioKindBadge.svelte';
  import StudioInlineEdit from './studio/StudioInlineEdit.svelte';
  import StudioXrefPicker from './studio/StudioXrefPicker.svelte';
  import Modal from './Modal.svelte';
  import { useStudioEditPipeline }       from './studio/composables/useStudioEditPipeline.svelte';
  import { useStudioCrossRefs }          from './studio/composables/useStudioCrossRefs.svelte';
  import { useStudioRenameBulkPipeline } from './studio/composables/useStudioRenameBulkPipeline.svelte';
  import { useStudioSchema }             from './studio/composables/useStudioSchema.svelte';
  import { useStudioQueryBar }           from './studio/composables/useStudioQueryBar.svelte';
  import { useStudioTextDiff }           from './studio/composables/useStudioTextDiff.svelte';
  import { useStudioSaveFlow }           from './studio/composables/useStudioSaveFlow.svelte';
  import { useStudioUndoRedo }           from './studio/composables/useStudioUndoRedo.svelte';
  import { useStudioGlobalKeys }         from './studio/composables/useStudioGlobalKeys.svelte';
  import { useStudioOutsideEdit }        from './studio/composables/useStudioOutsideEdit.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { jsonStudioStore, type JsonNodeKind } from '$lib/stores/json-studio.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import {
    studioBackend,
    type StudioNodeView, type StudioPrimitiveValue,
  } from '$lib/ipc/studio-format';
  // Shared schema-aware walker — serde rename / alias / rename_all /
  // flatten (incl. HashMap<String,V> catch-all).
  import {
    typeAtPath as walkTypeAtPath,
    flattenedStructFields,
  } from '$lib/utils/studio-schema';

  /** Pre-bound JSON backend. */
  const JSON_BE = studioBackend<JsonNodeKind>('json');

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'query' | 'bindings' | 'schema' | 'tools' | null;

  let viewMode = $state<ViewMode>('tree');

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

  // ── Tree state ─────────────────────────────────────────────────────────
  type JsonNodeView = StudioNodeView<JsonNodeKind>;
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
    if (editPipeline.editingPid && editPipeline.editingPid !== selectedNode?.pid) {
      try { await editPipeline.maybeCommitActiveEdit(selectedNode); }
      catch { editPipeline.cancelEdit(); }
    }
  }

  // ── Edit pipeline ──────────────────────────────────────────────────────
  // State + flow control live in `useStudioEditPipeline`. The wrapper
  // supplies the format-specific commit dispatch + seed computation.

  const editPipeline = useStudioEditPipeline<JsonNodeKind, TNode>({
    formatId: 'json',
    isEditablePrimitive,
    rowEditMode:   (n) => rowEditMode(n),
    currentVariantTag,
    computeSeed: (n) => {
      let seed = valueText ?? n.preview;
      // Strip the surrounding quotes for the input — we re-add them
      // through the `string` primitive serialisation. JSON escapes
      // (`\"`, `\\`, …) survive the round-trip via the backend.
      if (n.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
        try { seed = JSON.parse(seed) as string; }
        catch { seed = seed.slice(1, -1); }
      }
      return seed;
    },
    commit: async (node, draft) => {
      // Schema-aware typed-input narrowing: when the schema constrains
      // this position to a specific primitive, reject input that doesn't
      // match (e.g. `1.5` on an `integer` field). Skips when no schema.
      const hint = studioSchema.schema ? studioSchema.primitiveHintAt(node.path) : null;
      const wantInt    = hint === 'integer';
      const wantNum    = hint === 'number';
      const wantBool   = hint === 'boolean';
      const wantString = hint === 'string';

      let value: StudioPrimitiveValue;
      try {
        switch (node.kind) {
          case 'string':
            if (wantBool) {
              const t = draft.trim().toLowerCase();
              if (t !== 'true' && t !== 'false') throw new Error('schema: expected boolean');
              value = { type: 'bool', value: t === 'true' };
              break;
            }
            if (wantInt) {
              const n = Number(draft.trim());
              if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
              value = { type: 'int', value: Math.trunc(n) };
              break;
            }
            if (wantNum) {
              const n = Number(draft.trim());
              if (!Number.isFinite(n)) throw new Error('schema: expected number');
              value = { type: 'float', value: n };
              break;
            }
            value = { type: 'string', value: draft };
            break;
          case 'bool': {
            const t = draft.trim().toLowerCase();
            if (t !== 'true' && t !== 'false') throw new Error('expected "true" or "false"');
            value = { type: 'bool', value: t === 'true' };
            break;
          }
          case 'number': {
            const s = draft.trim();
            const n = Number(s);
            if (!Number.isFinite(n)) throw new Error('not a number');
            if (wantInt) {
              if (!Number.isInteger(n)) throw new Error('schema: expected integer');
              value = { type: 'int', value: Math.trunc(n) };
              break;
            }
            if (wantString) { value = { type: 'string', value: draft }; break; }
            // JSON numbers are unified — `int` vs `float` in the
            // primitive payload only matters as a serialisation hint
            // (preserves trailing `.0` for floats).
            const looksFloat = /[.eE]/.test(s);
            value = looksFloat || wantNum
              ? { type: 'float', value: n }
              : { type: 'int',   value: Math.trunc(n) };
            break;
          }
          default: return {};
        }
      } catch (e: any) {
        return { error: e?.message ?? String(e) };
      }
      try {
        await jsonStudioStore.mutatePrimitive(node.path, value);
        await refreshAfterMutation(node, /* structural */ false);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    commitVariant: async (node, tag) => {
      try {
        await jsonStudioStore.mutatePrimitive(node.path, { type: 'string', value: tag });
        await refreshAfterMutation(node, /* structural */ false);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    focusInspector: () => inspectorPanel?.focusEditInput(),
  });

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
      await jsonStudioStore.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('json-studio: removeAt failed', e);
    }
  }

  // ── Container mutations ────────────────────────────────────────────────
  async function addItemAction(parent: TNode): Promise<void> {
    if (parent.kind !== 'array') return;
    try {
      await jsonStudioStore.insertItem(parent.path, 'null');
      await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
    } catch (e: any) {
      uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error');
    }
  }

  // ── Cross-refs + F12 rename + F13 bulk edit ───────────────────────────
  //
  // JSON follows the id/name + *_id/*_ref convention; reference-field
  // patterns can be overridden per-binding via the shared `studioStore`
  // (driven by `.arbor/studio.toml`).
  //
  // JSON has first-class null (`null_handling = native`), so the
  // bulk-edit modal uses `nullPolicy = "native"`.

  const crossRefs = useStudioCrossRefs<JsonNodeKind, TNode>({
    formatId: 'json',
    getSourcePath: () => jsonStudioStore.sourcePath,
    jumpToPath: async (path) => { await treePane?.jumpToPath(path); },
    openExternalDoc: async (absPath, path) => {
      await jsonStudioStore.openDoc({ path: absPath });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(path);
    },
  });

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = jsonStudioStore.sourcePath;
    if (!path) return;
    const title = jsonStudioStore.title;
    await jsonStudioStore.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  const renameBulk = useStudioRenameBulkPipeline<TNode>({
    formatId:        'json',
    formatLabel:     'JSON',
    getDocId:        () => jsonStudioStore.docId,
    getSourcePath:   () => jsonStudioStore.sourcePath,
    getDirty:        () => jsonStudioStore.dirty,
    getActiveTabId:  () => tabsStore.activeTabId,
    extractRenameValue: (n) => crossRefs.unquotedString(n.preview),
    reloadAfterDiskWrite: async () => { await reloadActiveDocFromDisk(); },
    applyExternalActiveDocState: async (state) => {
      await jsonStudioStore.applyExternalMutate(state);
      await treePane?.reloadTree();
    },
  });

  // ── Schema sidecar + walker + chips + Inspector adapters ──────────────
  // Owned by `useStudioSchema`. JSON Schema only — the BE picks the
  // source via `*.schema.json` file selection (no Rust crate probe like
  // RON has). The shared walker handles serde rename / alias /
  // rename_all / flatten transparently.

  const studioSchema = useStudioSchema<JsonNodeKind, TNode>({
    backend: JSON_BE,
    getSchemaHint: () => jsonStudioStore.schemaHint,
    walkType: walkTypeAtPath,
    flattenedFields: flattenedStructFields,
    cssPrefix: 'js',
    getSelectedChildKeys: (n) => (n.children ?? []).map((c: TNode) => c.key),
    currentVariantTag: (n) => currentVariantTag(n),
  });

  function rowEditMode(node: TNode): 'primitive' | 'variant' | null {
    if (node.kind === 'string') {
      const ed = studioSchema.enumDefAt(node.path);
      if (ed && ed.variants.length > 0 && ed.variants.every(v => v.shape === 'unit')) {
        return 'variant';
      }
    }
    if (isEditablePrimitive(node.kind)) return 'primitive';
    return null;
  }

  function currentVariantTag(node: TNode): string {
    if (node.kind !== 'string') return '';
    const v = crossRefs.unquotedString(node.preview);
    return v ?? '';
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
  function noopOption(): Promise<void> | void { /* JSON has no Option */ }

  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || selectedNode.kind !== 'string') return;
    const current = currentVariantTag(selectedNode);
    if (!name || name === current) return;
    const node = selectedNode;
    try {
      await jsonStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editPipeline.setEditError(e?.message ?? String(e));
    }
  }

  // ── Context menu ───────────────────────────────────────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({ id: 'copy-path',  label: 'Copy path',         icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: 'Copy value (JSON)', icon: Copy,     iconColor: 'var(--text-muted)' });

    // Phase 3.d — stream-mode docs (large files) keep navigation
    // affordances but skip the structural-edit block.
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

    if (studioSchema.schema) {
      const ty = studioSchema.typeAtPath(node.path);
      let namedPath: string | null = null;
      if (ty?.kind === 'named') namedPath = ty.path;
      else if (ty?.kind === 'option' && ty.inner.kind === 'named') namedPath = ty.inner.path;
      if (namedPath && studioSchema.schema.types[namedPath]) {
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
      if (parent && isContainerKind(parent.kind)) {
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

    if (tabsStore.activeTabId && crossRefs.isRenameableTreeNode(node)) {
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
          await copyToClipboard(v);
        } catch { /* ignore */ }
        break;
      }
      case 'edit':         editPipeline.startEdit(node, 'tree');           break;
      case 'edit-variant': editPipeline.startVariantEdit(node, 'tree');    break;
      case 'view-impl':    {
        const ty = studioSchema.typeAtPath(node.path);
        let p: string | null = null;
        if (ty?.kind === 'named') p = ty.path;
        else if (ty?.kind === 'option' && ty.inner.kind === 'named') p = ty.inner.path;
        if (p) void studioSchema.openViewSource(p);
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
      case 'rename-across-project': renameBulk.openRenameModalForNode(node); break;
    }
  }

  function expandNode(node: TNode, want: boolean): void {
    const next = new Set(expanded);
    if (want) next.add(node.pid); else next.delete(node.pid);
    expanded = next;
  }

  // ── Query bar ──────────────────────────────────────────────────────────
  const queryBarCtl = useStudioQueryBar<JsonNodeKind>({
    getRightPane:    () => rightPane,
    setRightPane:    (p) => setRightPane(p),
    toggleQueryPane: () => studioModal?.toggleRightPane('query'),
  });
  function getChildKeysForPath(path: string[]): string[] | null {
    return treePane?.getChildKeysForPath(path) ?? null;
  }
  function ensureChildrenLoadedForPath(path: string[]): void {
    treePane?.ensureChildrenLoadedForPath(path);
  }
  async function jumpToQueryHit(path: string[]): Promise<void> {
    await treePane?.jumpToPath(path);
  }

  // ── Text + Diff views ──────────────────────────────────────────────────
  const textDiff = useStudioTextDiff({
    getStoreCurrent: () => jsonStudioStore.current,
    setText:         (text) => jsonStudioStore.setText(text),
    reloadTree:      async () => { await treePane?.reloadTree(); },
  });
  /** Local alias so the body still reads `bumpDiffRefresh()` everywhere. */
  function bumpDiffRefresh() { textDiff.bumpDiffRefresh(); }

  $effect(() => {
    const id = jsonStudioStore.docId;
    if (!id) {
      queryBarCtl.resetForDocClose();
      editPipeline.cancelEdit();
      return;
    }
    viewMode = 'tree';
  });

  // Cross-ref index — load on modal open + every active-tab change.
  $effect(() => {
    if (!jsonStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, 'json'); });
  });

  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: textDiff.diffTreeChangeCount > 0 ? textDiff.diffTreeChangeCount
           : textDiff.diffHunkCount > 0       ? textDiff.diffHunkCount
           : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse errors',
      disabled: !jsonStudioStore.parseError,
      badge: jsonStudioStore.parseError ? '!' : undefined,
      data: { errorBadge: !!jsonStudioStore.parseError } },
  ]);

  // ── Indent + Format ────────────────────────────────────────────────────
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
  //
  // JSON layers a JSONC gate on top of the shared save flow: if the
  // active doc is `.json` AND has JSONC features (comments / trailing
  // commas), surface the prompt modal instead of writing through. The
  // prompt's four outcomes call into the shared `saveFlow` path with
  // the right setup (rename to .jsonc / strip features / save anyway).

  const saveFlow = useStudioSaveFlow({
    getSourcePath: () => jsonStudioStore.sourcePath,
    save:          (opts) => jsonStudioStore.save(opts),
    onSaved:       bumpDiffRefresh,
  });

  let jsoncSavePromptOpen = $state(false);

  async function onSaveRequested(): Promise<void> {
    if (!jsonStudioStore.sourcePath) { saveFlow.openSaveAs(); return; }
    if (jsonStudioStore.hasJsoncFeatures && !jsonStudioStore.isJsonc) {
      jsoncSavePromptOpen = true;
      return;
    }
    await saveFlow.doSave();
  }
  async function onSaveAsJsonc(): Promise<void> {
    jsoncSavePromptOpen = false;
    const next = jsonStudioStore.renameSourceToJsonc();
    if (!next) { saveFlow.openSaveAs(); return; }
    // Save-As writes to the new `.jsonc` path AND rebinds the doc.
    await saveFlow.onSaveAsPicked(next);
  }
  async function onStripAndSave(): Promise<void> {
    jsoncSavePromptOpen = false;
    const ok = await jsonStudioStore.stripJsoncFeatures();
    if (!ok) return;
    await saveFlow.doSave();
  }
  async function onSaveAnyway(): Promise<void> {
    jsoncSavePromptOpen = false;
    await saveFlow.doSave();
  }

  // ── Misc ───────────────────────────────────────────────────────────────
  async function close() {
    textDiff.cancelPendingTextPush();
    await jsonStudioStore.closeDoc();
  }

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
  function kindTone(k: JsonNodeKind): StudioKindTone {
    switch (k) {
      case 'object':
      case 'array':  return 'type';
      case 'string': return 'string';
      case 'number': return 'number';
      case 'bool':   return 'keyword';
      case 'null':   return 'muted';
    }
  }
  function isBoolKind(k: JsonNodeKind): boolean { return k === 'bool'; }

  const { doUndo, doRedo } = useStudioUndoRedo({
    undo: () => jsonStudioStore.undo(),
    redo: () => jsonStudioStore.redo(),
    reloadTree: async () => { await treePane?.reloadTree(); },
    bumpDiffRefresh,
  });

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  const { onKey } = useStudioGlobalKeys<JsonNodeKind, TNode>({
    isOpen:        () => jsonStudioStore.open,
    doSave:        onSaveRequested,
    doUndo,
    doRedo,
    getViewMode:     () => viewMode,
    getSelectedNode: () => selectedNode,
    getEditingPid:   () => editPipeline.editingPid,
    startEdit:        (n, loc) => editPipeline.startEdit(n, loc),
    startVariantEdit: (n, loc) => editPipeline.startVariantEdit(n, loc),
    rowEditMode,
    isRemovable,
    removeSelected,
    getQueryBarController: () => queryBarCtl.queryBar,
    getDiffPaneController: () => diffPane,
  });

  // Pointer-down outside the active inline tree edit = implicit commit.
  useStudioOutsideEdit({ editPipeline, getSelectedNode: () => selectedNode });

  void tick;
</script>

<svelte:window onkeydown={onKey} />

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
    <StudioRightRailButton
      icon={ScanSearch}
      active={rightPane === 'inspector'}
      tooltip="Inspector — selected node detail (Tree view)"
      label="Inspector"
      onClick={() => studioModal?.toggleRightPane('inspector')}
    />
    <StudioRightRailButton
      icon={ListFilter}
      active={rightPane === 'query'}
      tooltip={queryBarCtl.query.trim()
        ? `Query results — ${queryBarCtl.queryHits.length} hit${queryBarCtl.queryHits.length === 1 ? '' : 's'}`
        : 'Query results — type in the search bar to populate'}
      label="Query results"
      onClick={queryBarCtl.onQueryToggleRightPane}
      count={queryBarCtl.queryHits.length}
    />
    <StudioRightRailButton
      icon={Layers}
      active={rightPane === 'bindings'}
      tooltip="Bindings & broken refs — project-wide cross-references"
      label="Bindings & broken refs"
      onClick={() => studioModal?.toggleRightPane('bindings')}
    />
    <StudioRightRailButton
      icon={BookOpen}
      active={rightPane === 'schema'}
      tooltip={studioSchema.schema
        ? `Schema — ${studioSchema.schema.root_name} (${Object.keys(studioSchema.schema.types).length} types)`
        : 'Schema — bind a JSON Schema file'}
      label="JSON Schema"
      onClick={() => studioModal?.toggleRightPane('schema')}
    />
    <StudioRightRailButton
      icon={Wrench}
      active={rightPane === 'tools'}
      tooltip="Tools — Format / Indent"
      label="Tools"
      onClick={() => studioModal?.toggleRightPane('tools')}
    />
  {/snippet}

  {#snippet headerLeft()}
    <span class="js-header-icon-wrap" aria-hidden="true">
      <FileJson size={14} />
    </span>
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
      saving={saveFlow.saving}
      onSave={() => void onSaveRequested()}
      onSaveAs={saveFlow.openSaveAs}
    />
  {/snippet}

  {#snippet bodyBanners()}
    <StudioBodyBanners saveError={saveFlow.saveError} {actionError}>
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
      bind:this={queryBarCtl.queryBar}
      formatId="json"
      backend={JSON_BE}
      docId={jsonStudioStore.docId}
      visible={viewMode === 'tree' && !jsonStudioStore.parseError}
      placeholder='Query — name (recursive), $.foo.bar, $.arr[0:5], $.users[?@.age > 30]…'
      historyStorageKey="arbor:json-studio:query-history"
      knownKeys={queryBarCtl.knownKeys}
      getChildKeysForPath={getChildKeysForPath}
      ensureChildrenLoaded={ensureChildrenLoadedForPath}
      onJumpToHit={(path) => void jumpToQueryHit(path)}
      rightPaneOpen={rightPane === 'query'}
      onToggleRightPane={queryBarCtl.onQueryToggleRightPane}
      onActiveChange={queryBarCtl.onQueryActiveChange}
      onHits={(hits) => queryBarCtl.noteKeys(hits)}
      bulkEditEnabled
      onBulkEditRequest={(q) => renameBulk.openBulkEditModal(q)}
      bind:query={queryBarCtl.query}
      bind:queryHits={queryBarCtl.queryHits}
      bind:querying={queryBarCtl.querying}
      bind:queryError={queryBarCtl.queryError}
      bind:currentHitIdx={queryBarCtl.currentHitIdx}
    >
      {#snippet kindChip(kind)}
        <StudioKindBadge label={kindBadge(kind)} tone={kindTone(kind)} tinted tooltip={kind} />
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
          {@const ty = studioSchema.typeAtPath(n.path)}
          {@const namedType = studioSchema.namedTypeAt(n.path)}
          <StudioKindBadge label={kindBadge(n.kind)} tone={kindTone(n.kind)} tinted tooltip={n.kind} />
          <span class="js-row-key" class:js-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          <span class="js-row-sep">:</span>
          {#if editPipeline.editingPid === n.pid && editPipeline.editLocation === 'tree'}
            {#if rowEditMode(n) === 'variant'}
              {@const ed = studioSchema.enumDefAt(n.path)}
              {#if ed}
                <StudioInlineEdit
                  mode="select"
                  variant
                  bind:value={editPipeline.editBuf}
                  options={ed.variants.map(v => ({ value: v.name }))}
                  onPick={() => void editPipeline.runCommitVariant(n)}
                  onCancel={() => editPipeline.cancelEdit()}
                  errorMsg={editPipeline.editError}
                />
              {/if}
            {:else if n.kind === 'bool'}
              <StudioInlineEdit
                mode="select"
                bind:value={editPipeline.editBuf}
                options={[{ value: 'true' }, { value: 'false' }]}
                onPick={() => void editPipeline.runCommit(n)}
                onCancel={() => editPipeline.cancelEdit()}
                errorMsg={editPipeline.editError}
              />
            {:else}
              <StudioInlineEdit
                mode="input"
                bind:value={editPipeline.editBuf}
                bind:inputEl={editPipeline.editInlineEl}
                onkeydown={(e) => editPipeline.onEditKey(e, n)}
                errorMsg={editPipeline.editError}
              />
            {/if}
          {:else}
            {@const xrefs = crossRefs.crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="js-row-preview js-row-preview-{n.kind}"
                  class:js-row-preview-editable={rowEditMode(n) !== null}
                  class:js-row-preview-xref={hasX}
                  ondblclick={(e) => {
                    if (!rowEditMode(n)) return;
                    e.preventDefault(); e.stopPropagation();
                    void selectNode(n).then(() => editPipeline.startEdit(n, 'tree'));
                  }}
                  onclick={hasX ? ((e) => crossRefs.onCrossRefClick(xrefs, e)) : undefined}
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
            <span class="js-row-type-slot">
              <TypePill label={namedType} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {:else if ty && ty.kind !== 'named'}
            <span class="js-row-type-slot">
              <TypePill label={studioSchema.fmtType(ty)} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textDiff.textBuf}
        language="json"
        oninput={textDiff.onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="json"
        backend={JSON_BE}
        docId={jsonStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={jsonStudioStore.current}
        refreshTick={textDiff.diffRefreshTick}
        bind:treeChangeCount={textDiff.diffTreeChangeCount}
        bind:hunkCount={textDiff.diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- JSON has no variant tags; renderer renders an empty span. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if jsonStudioStore.parseError}
        <div class="js-errors-wrap">
          <Alert variant="error" title="JSON parse error">
            <pre class="js-errors-body">{jsonStudioStore.parseError}</pre>
            <p class="js-errors-hint">
              Switch to the <strong>Text</strong> tab to fix it. The error will
              clear automatically once the document parses.
            </p>
          </Alert>
        </div>
      {:else}
        <StateBlock tone="success" label="No parse errors." />
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
      editingPid={editPipeline.editingPid}
      editLocation={editPipeline.editLocation}
      bind:editBuf={editPipeline.editBuf}
      editError={editPipeline.editError}
      editBannerVisible={editPipeline.editBannerVisible}
      kindBadge={kindBadge as any}
      isRemovable={isRemovable as any}
      isEditablePrimitive={isEditablePrimitive as any}
      isBoolKind={isBoolKind as any}
      isContainerKind={isContainerKind as any}
      isDefinitionNode={((n: TNode) =>
        n.kind === 'string' && crossRefs.isDefinitionFieldName(n.key) && !!crossRefs.unquotedString(n.preview)) as any}
      definitionValue={((n: TNode) => {
        if (n.kind !== 'string' || !crossRefs.isDefinitionFieldName(n.key)) return null;
        return crossRefs.unquotedString(n.preview);
      }) as any}
      onCopyPath={copyPathOf as any}
      onCopyValue={copyValue}
      onRemove={removeSelected}
      onStartEdit={(loc?: 'tree' | 'detail') => editPipeline.startEdit(selectedNode, loc)}
      onCommitEdit={() => selectedNode ? editPipeline.runCommit(selectedNode) : Promise.resolve()}
      onCancelEdit={editPipeline.cancelEdit}
      onPickVariant={(name: string) => void inspectorPickVariant(name)}
      onAddField={inspectorAddField as any}
      onToggleOption={noopOption}
      onDismissEditBanner={editPipeline.dismissEditBanner}
      onJumpToUsage={crossRefs.jumpToUsage as any}
      onSelectChild={(c) => void selectNode(c as TNode)}
      schemaTypeInfo={studioSchema.inspectorSchemaTypeInfo as any}
      variantPickerInfo={studioSchema.inspectorVariantPickerInfo as any}
      missingFields={studioSchema.inspectorMissingFields as any}
    />
  {/snippet}

  {#snippet querySidecar()}
    <PanelShell title="Query results" count={queryBarCtl.queryHits.length} class="js-query-shell">
      {#snippet icon()}<ListFilter size={13} />{/snippet}
    <div class="js-query-pane-body">
      {#if !queryBarCtl.query.trim()}
        <p class="js-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if queryBarCtl.querying && queryBarCtl.queryHits.length === 0}
        <div class="js-query-pane-status">
          <Spinner size="xs" /> <span>Running query…</span>
        </div>
      {:else if queryBarCtl.queryError}
        <div class="js-query-pane-error">
          <AlertCircle size={11} /> {queryBarCtl.queryError}
        </div>
      {:else if queryBarCtl.queryHits.length === 0}
        <p class="js-query-pane-empty">No matches.</p>
      {:else}
        <div class="js-query-pane-list">
          {#each queryBarCtl.queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="js-query-pane-card"
              class:active={i === queryBarCtl.currentHitIdx}
              onclick={() => { queryBarCtl.currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="js-query-pane-card-head">
                <StudioKindBadge label={kindBadge(hit.kind)} tone={kindTone(hit.kind)} tinted tooltip={hit.kind} />
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
    </PanelShell>
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="json"
      backend={JSON_BE}
      sourcePath={jsonStudioStore.sourcePath}
      onOpenDefinition={crossRefs.openDefinition}
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
      schema={studioSchema.schema}
      schemaProbe={studioSchema.schemaProbe}
      schemaRsPath={studioSchema.schemaRsPath}
      schemaRootSel={studioSchema.schemaRootSel}
      schemaLoading={studioSchema.schemaLoading}
      schemaError={studioSchema.schemaError}
      onProbe={studioSchema.probeSchemaSource}
      onSelectRoot={studioSchema.setSchemaRoot}
      onLoad={studioSchema.loadSchemaForRoot}
      onClear={studioSchema.clearSchema}
      onOpenViewSource={studioSchema.openViewSource}
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
    {#if saveFlow.savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save JSON document as"
        extensions={['json', 'jsonc']}
        initialPath={jsonStudioStore.sourcePath ?? undefined}
        initialFilename={jsBasename(jsonStudioStore.sourcePath) || (jsonStudioStore.isJsonc ? 'document.jsonc' : 'document.json')}
        onConfirm={saveFlow.onSaveAsPicked}
        onCancel={() => saveFlow.savePickerOpen = false}
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
    {#if renameBulk.renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={JSON_BE}
        tabId={tabsStore.activeTabId}
        formatLabel="JSON"
        oldValue={renameBulk.renameModalState.oldValue}
        openDocs={renameBulk.buildRenameOpenDocs()}
        onClose={renameBulk.closeRenameModal}
        onApplied={renameBulk.onRenameApplied}
      />
    {/if}

    {#if renameBulk.bulkEditModalState && tabsStore.activeTabId && jsonStudioStore.docId}
      <StudioBulkEditModal
        backend={JSON_BE}
        tabId={tabsStore.activeTabId}
        docId={jsonStudioStore.docId}
        formatLabel="JSON"
        query={renameBulk.bulkEditModalState.query}
        nullPolicy="native"
        openDocs={renameBulk.buildBulkEditOpenDocs()}
        onClose={renameBulk.closeBulkEditModal}
        onApplied={renameBulk.onBulkEditApplied}
      />
    {/if}

    <StudioXrefPicker
      picker={crossRefs.crossRefPicker}
      portal={crossRefs.portal}
      onPick={(entry) => void crossRefs.jumpToCrossRef(entry)}
      onDismiss={crossRefs.dismissPicker}
    />

    {#if studioSchema.viewSource || studioSchema.viewSourceBusy || studioSchema.viewSourceErr}
      <StudioViewSourceModal
        viewSource={studioSchema.viewSource}
        busy={studioSchema.viewSourceBusy}
        err={studioSchema.viewSourceErr}
        language="json"
        loadingLabel="Loading schema fragment…"
        onClose={studioSchema.closeViewSource}
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

  /* Empty-state copy in the Bindings sidecar. */
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

  /* Errors view — Alert wrapper + pre/hint styling. */
  .js-errors-wrap { padding: 24px; overflow: auto; height: 100%; }
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
    margin: 6px 0 0;
  }
  .js-errors-hint {
    color: var(--text-muted);
    font-size: 12px;
    margin: 6px 0 0;
  }

  /* Query results sidecar pane ─────────────────────────────────────── */
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

  /* Schema sidecar intro. */
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
  :global(.js-header-icon) { color: var(--accent); flex-shrink: 0; }

  /* Schema-aware tree decoration (parity with RON / TOML). */
  .js-row-preview-editable { cursor: text; }

  .js-row-type-slot {
    margin-left: auto;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
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

</style>
