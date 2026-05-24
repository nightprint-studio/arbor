<!--
  TomlStudioModal — TOML wrapper around the generic `<StudioModal>` shell.

  Architecture mirrors YamlStudioModal: all reusable Studio logic lives
  in `./studio/composables/*`; this file only owns the TOML-specific
  bits — container taxonomy (table / inline_table / array /
  array_of_tables), schema-aware primitive narrowing, and the schema
  sidecar's dual source mode (Rust struct OR JSON Schema).

  Capabilities exposed to the user:
    · Tree view with mutations (edit primitives, remove, add field/item,
      duplicate, move, paste-over). F2 inline edit; schema-aware variant
      picker on enum-typed string leaves.
    · Text view via `<StudioTextPane>` (CodeMirror 6 + TOML stream
      parser) — typing pushes through the host's lossless `toml_edit`.
    · Diff + Errors + Inspector + Query + Bindings + Schema sidecars.
    · F12 cross-ref rename + F13 bulk edit (`null_handling = as_delete`).
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';
  import {
    FileText, Copy, ListTree, AlertCircle, GitCompare,
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
  import type { StudioFooterDoc } from './studio/studio-footer-types';
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
  import { tomlStudioStore, type TomlNodeKind } from '$lib/stores/toml-studio.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import {
    studioBackend,
    type StudioNodeView, type StudioPrimitiveValue,
  } from '$lib/ipc/studio-format';
  import {
    typeAtPath as walkTypeAtPath,
    flattenedStructFields,
  } from '$lib/utils/studio-schema';

  /** Pre-bound TOML backend. */
  const TOML_BE = studioBackend<TomlNodeKind>('toml');

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'query' | 'bindings' | 'schema' | 'tools' | null;

  let viewMode = $state<ViewMode>('tree');

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
  type TNode = TomlNodeView & {
    pid:      string;
    children: TNode[] | null;
    loading?: boolean;
  };

  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: TomlNodeView): TNode { return { ...v, pid: pathId(v.path), children: null }; }
  /** TOML preserves source order — render rows as the backend emits. */
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
    if (editPipeline.editingPid && editPipeline.editingPid !== selectedNode?.pid) {
      try { await editPipeline.maybeCommitActiveEdit(selectedNode); }
      catch { editPipeline.cancelEdit(); }
    }
  }

  // ── Edit pipeline ──────────────────────────────────────────────────────
  const editPipeline = useStudioEditPipeline<TomlNodeKind, TNode>({
    formatId: 'toml',
    isEditablePrimitive,
    rowEditMode:   (n) => rowEditMode(n),
    currentVariantTag,
    computeSeed: (n) => {
      let seed = valueText ?? n.preview;
      if (n.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
        try { seed = JSON.parse(seed) as string; }
        catch { seed = seed.slice(1, -1); }
      }
      return seed;
    },
    commit: async (node, draft) => {
      // Schema-aware numeric narrowing — Yaml-style hint vocabulary
      // (Rust primitive idents from the schema walker).
      const hint = studioSchema.schema ? studioSchema.primitiveHintAt(node.path) : null;
      const wantFloat = hint === 'f32' || hint === 'f64' || hint === 'number';
      const wantInt   = hint === 'integer' || (hint != null &&
        (hint.startsWith('i') || hint.startsWith('u')) && hint !== 'isize' && hint !== 'usize') ||
        hint === 'isize' || hint === 'usize';
      const wantBool   = hint === 'bool' || hint === 'boolean';
      const wantString = hint === 'string' || hint === 'String' || hint === '&str' || hint === 'str';

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
            if (wantFloat) {
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
          case 'integer': {
            const s = draft.trim();
            const n = Number(s);
            if (!Number.isFinite(n)) throw new Error('not an integer');
            if (wantFloat)  { value = { type: 'float',  value: n     }; break; }
            if (wantString) { value = { type: 'string', value: draft }; break; }
            if (!Number.isInteger(n) && !/^-?\d+(_\d+)*$/.test(s)) {
              throw new Error('not an integer');
            }
            value = { type: 'int', value: Math.trunc(n) };
            break;
          }
          case 'float': {
            const s = draft.trim();
            const n = Number(s);
            if (!Number.isFinite(n)) throw new Error('not a number');
            if (wantInt) {
              if (!Number.isInteger(n)) throw new Error('schema: expected integer');
              value = { type: 'int', value: Math.trunc(n) };
              break;
            }
            if (wantString) { value = { type: 'string', value: draft }; break; }
            value = { type: 'float', value: n };
            break;
          }
          default: return {};
        }
      } catch (e: any) {
        return { error: e?.message ?? String(e) };
      }
      try {
        await tomlStudioStore.mutatePrimitive(node.path, value);
        await refreshAfterMutation(node, /* structural */ false);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    commitVariant: async (node, tag) => {
      try {
        await tomlStudioStore.mutatePrimitive(node.path, { type: 'string', value: tag });
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
      await tomlStudioStore.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
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
      editPipeline.maybeShowEditBanner();
    } catch (e: any) {
      uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error');
    }
  }

  // ── Cross-refs + F12 rename + F13 bulk edit ────────────────────────────
  //
  // TOML follows the same convention as JSON / RON: `id`/`name` string
  // fields are definitions; `*_id` / `*_ref` / etc. are references
  // (default convention or per-binding patterns from the shared
  // studioStore).

  const crossRefs = useStudioCrossRefs<TomlNodeKind, TNode>({
    formatId: 'toml',
    getSourcePath: () => tomlStudioStore.sourcePath,
    jumpToPath: async (path) => { await treePane?.jumpToPath(path); },
    openExternalDoc: async (absPath, path) => {
      await tomlStudioStore.openDoc({ path: absPath });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(path);
    },
  });

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = tomlStudioStore.sourcePath;
    if (!path) return;
    const title = tomlStudioStore.title;
    await tomlStudioStore.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  const renameBulk = useStudioRenameBulkPipeline<TNode>({
    formatId:        'toml',
    formatLabel:     'TOML',
    getDocId:        () => tomlStudioStore.docId,
    getSourcePath:   () => tomlStudioStore.sourcePath,
    getDirty:        () => tomlStudioStore.dirty,
    getActiveTabId:  () => tabsStore.activeTabId,
    extractRenameValue: (n) => crossRefs.unquotedString(n.preview),
    reloadAfterDiskWrite: async () => { await reloadActiveDocFromDisk(); },
    applyExternalActiveDocState: async (state) => {
      await tomlStudioStore.applyExternalMutate(state);
      await treePane?.reloadTree();
    },
  });

  // ── Schema sidecar + walker + chips + Inspector adapters ──────────────
  // TOML accepts BOTH Rust struct sources (`*.rs`) and JSON Schema
  // (`*.schema.json`). The BE dispatch lives at `crate::toml_studio::
  // schema`; the FE composable is source-agnostic.

  const studioSchema = useStudioSchema<TomlNodeKind, TNode>({
    backend: TOML_BE,
    getSchemaHint: () => tomlStudioStore.schemaHint,
    walkType: walkTypeAtPath,
    flattenedFields: flattenedStructFields,
    cssPrefix: 'ts',
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
  function noopOption(): Promise<void> | void { /* TOML has no Option */ }

  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || selectedNode.kind !== 'string') return;
    const current = currentVariantTag(selectedNode);
    if (!name || name === current) return;
    const node = selectedNode;
    try {
      await tomlStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editPipeline.setEditError(e?.message ?? String(e));
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

    if (studioSchema.schema && studioSchema.typeAtPath(node.path)) {
      const ty = studioSchema.typeAtPath(node.path);
      let namedPath: string | null = null;
      if (ty?.kind === 'named') namedPath = ty.path;
      else if (ty?.kind === 'option' && ty.inner.kind === 'named') namedPath = ty.inner.path;
      if (namedPath && studioSchema.schema.types[namedPath]) {
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
          const v = await TOML_BE.getValue(tomlStudioStore.docId!, node.path);
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
  const queryBarCtl = useStudioQueryBar<TomlNodeKind>({
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
    getStoreCurrent: () => tomlStudioStore.current,
    setText:         (text) => tomlStudioStore.setText(text),
    reloadTree:      async () => { await treePane?.reloadTree(); },
  });
  function bumpDiffRefresh() { textDiff.bumpDiffRefresh(); }

  $effect(() => {
    const id = tomlStudioStore.docId;
    if (!id) {
      queryBarCtl.resetForDocClose();
      editPipeline.cancelEdit();
      return;
    }
    viewMode = 'tree';
  });

  // Cross-ref index — load on modal open + every active-tab change.
  $effect(() => {
    if (!tomlStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, 'toml'); });
  });

  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: textDiff.diffTreeChangeCount > 0 ? textDiff.diffTreeChangeCount
           : textDiff.diffHunkCount > 0       ? textDiff.diffHunkCount
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
  const saveFlow = useStudioSaveFlow({
    getSourcePath: () => tomlStudioStore.sourcePath,
    save:          (opts) => tomlStudioStore.save(opts),
    onSaved:       bumpDiffRefresh,
  });

  // ── Misc ───────────────────────────────────────────────────────────────
  async function close() {
    textDiff.cancelPendingTextPush();
    await tomlStudioStore.closeDoc();
  }

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
  function kindTone(k: TomlNodeKind): StudioKindTone {
    switch (k) {
      case 'table':
      case 'inline_table':
      case 'array':
      case 'array_of_tables': return 'keyword';
      case 'string':          return 'string';
      case 'integer':
      case 'float':
      case 'bool':            return 'number';
      case 'datetime':        return 'type';
    }
  }
  function isBoolKind(k: TomlNodeKind): boolean { return k === 'bool'; }

  const { doUndo, doRedo } = useStudioUndoRedo({
    undo: () => tomlStudioStore.undo(),
    redo: () => tomlStudioStore.redo(),
    reloadTree: async () => { await treePane?.reloadTree(); },
    bumpDiffRefresh,
  });

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  const { onKey } = useStudioGlobalKeys<TomlNodeKind, TNode>({
    isOpen:        () => tomlStudioStore.open,
    doSave:        () => saveFlow.doSave(),
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
        : 'Schema — bind a Rust struct (`.rs`) or JSON Schema file'}
      label="Schema"
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
      saving={saveFlow.saving}
      onSave={() => void saveFlow.doSave()}
      onSaveAs={saveFlow.openSaveAs}
    />
  {/snippet}

  {#snippet bodyBanners()}
    <StudioBodyBanners saveError={saveFlow.saveError} {actionError} />
  {/snippet}

  {#snippet queryBarSlot()}
    <StudioQueryBar
      bind:this={queryBarCtl.queryBar}
      formatId="toml"
      backend={TOML_BE}
      docId={tomlStudioStore.docId}
      visible={viewMode === 'tree' && !tomlStudioStore.parseError}
      placeholder='Query — name (recursive), $.section.key, $.servers[0], …'
      historyStorageKey="arbor:toml-studio:query-history"
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
        <StudioKindBadge label={kindBadge(kind)} tone={kindTone(kind)} tooltip={kind} />
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
          {@const ty = studioSchema.typeAtPath(n.path)}
          {@const namedType = studioSchema.namedTypeAt(n.path)}
          <StudioKindBadge label={kindBadge(n.kind)} tone={kindTone(n.kind)} tooltip={n.kind} />
          <span class="ts-row-key" class:ts-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          <span class="ts-row-sep">=</span>
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
            <span class="ts-row-preview ts-row-preview-{n.kind}"
                  class:ts-row-preview-editable={rowEditMode(n) !== null}
                  class:ts-row-preview-xref={hasX}
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
            >{n.preview}{#if hasX}<span class="ts-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="ts-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}
          {#if n.loading}<Loader2 size={10} class="ts-row-loader" />{/if}
          {#if namedType}
            <span class="ts-row-type-slot">
              <TypePill label={namedType} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {:else if ty && ty.kind !== 'named'}
            <span class="ts-row-type-slot">
              <TypePill label={studioSchema.fmtType(ty)} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textDiff.textBuf}
        language="toml"
        oninput={textDiff.onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="toml"
        backend={TOML_BE}
        docId={tomlStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={tomlStudioStore.current}
        refreshTick={textDiff.diffRefreshTick}
        bind:treeChangeCount={textDiff.diffTreeChangeCount}
        bind:hunkCount={textDiff.diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- TOML has no variant tags. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if tomlStudioStore.parseError}
        <div class="ts-errors-wrap">
          <Alert variant="error" title="TOML parse error">
            <pre class="ts-errors-body">{tomlStudioStore.parseError}</pre>
            <p class="ts-errors-hint">
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
      formatId="toml"
      backend={TOML_BE}
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
    <PanelShell title="Query results" count={queryBarCtl.queryHits.length} class="ts-query-shell">
      {#snippet icon()}<ListFilter size={13} />{/snippet}
    <div class="ts-query-pane-body">
      {#if !queryBarCtl.query.trim()}
        <p class="ts-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if queryBarCtl.querying && queryBarCtl.queryHits.length === 0}
        <div class="ts-query-pane-status">
          <Spinner size="xs" /> <span>Running query…</span>
        </div>
      {:else if queryBarCtl.queryError}
        <div class="ts-query-pane-error">
          <AlertCircle size={11} /> {queryBarCtl.queryError}
        </div>
      {:else if queryBarCtl.queryHits.length === 0}
        <p class="ts-query-pane-empty">No matches.</p>
      {:else}
        <div class="ts-query-pane-list">
          {#each queryBarCtl.queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="ts-query-pane-card"
              class:active={i === queryBarCtl.currentHitIdx}
              onclick={() => { queryBarCtl.currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="ts-query-pane-card-head">
                <StudioKindBadge label={kindBadge(hit.kind)} tone={kindTone(hit.kind)} tooltip={hit.kind} />
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
    </PanelShell>
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="toml"
      backend={TOML_BE}
      sourcePath={tomlStudioStore.sourcePath}
      onOpenDefinition={crossRefs.openDefinition}
    >
      {#snippet emptyState()}
        <p class="ts-bindings-empty">
          Project-wide cross-refs follow the <code>id</code> / <code>name</code>
          convention by default. Custom reference-field patterns live in
          the repo's <code>.arbor/studio.toml</code> bindings.
        </p>
      {/snippet}
    </StudioRefsPanel>
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId="toml"
      backend={TOML_BE}
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
    {#if saveFlow.savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save TOML document as"
        extensions={['toml']}
        initialPath={tomlStudioStore.sourcePath ?? undefined}
        initialFilename={jsBasename(tomlStudioStore.sourcePath) || 'document.toml'}
        onConfirm={saveFlow.onSaveAsPicked}
        onCancel={() => saveFlow.savePickerOpen = false}
      />
    {/if}

    {#if renameBulk.renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={TOML_BE}
        tabId={tabsStore.activeTabId}
        formatLabel="TOML"
        oldValue={renameBulk.renameModalState.oldValue}
        openDocs={renameBulk.buildRenameOpenDocs()}
        onClose={renameBulk.closeRenameModal}
        onApplied={renameBulk.onRenameApplied}
      />
    {/if}

    {#if renameBulk.bulkEditModalState && tabsStore.activeTabId && tomlStudioStore.docId}
      <StudioBulkEditModal
        backend={TOML_BE}
        tabId={tabsStore.activeTabId}
        docId={tomlStudioStore.docId}
        formatLabel="TOML"
        query={renameBulk.bulkEditModalState.query}
        nullPolicy="as_delete"
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
        language={studioSchema.schemaRsPath && /\.rs$/i.test(studioSchema.schemaRsPath) ? 'rust' : 'json'}
        loadingLabel="Loading schema fragment…"
        onClose={studioSchema.closeViewSource}
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

  /* Row content (Tree pane). */
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
  :global(.ts-row-loader) { color: var(--text-muted); flex-shrink: 0; }

  .ts-row-preview-editable { cursor: text; }

  /* Errors view — Alert wrapper + pre/hint styling. */
  .ts-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .ts-errors-body {
    background: var(--bg-overlay);
    color: var(--text-primary);
    padding: 10px;
    border-radius: 4px;
    font-family: var(--font-code);
    font-size: 11px;
    margin: 6px 0 0;
    overflow: auto;
    white-space: pre-wrap;
  }
  .ts-errors-hint {
    color: var(--text-muted);
    font-size: 11px;
    margin: 6px 0 0;
  }

  /* Query pane sidecar. */
  .ts-query-pane-body { padding: 8px; overflow: auto; height: 100%; }
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
  :global(.ts-query-spinner) { animation: ts-spin 1s linear infinite; }
  @keyframes ts-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  /* Schema sidecar. */
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

  .ts-row-type-slot {
    margin-left: auto;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
  }

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

</style>
