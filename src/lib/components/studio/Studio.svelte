<!--
  Studio — the generic per-format Studio modal.

  Promotes the shared chrome shell (`<StudioModal>`) into a complete,
  format-agnostic editor: it instantiates ALL ten Studio composables
  ONCE here (edit pipeline, cross-refs, rename+bulk, schema, query bar,
  text/diff, save, undo/redo, global keys, outside-edit), owns every
  standard markup block (rail buttons, header, footer, tools, query bar,
  tree + row, text/diff/errors body, inspector / query / bindings /
  schema sidecars, save-as picker, rename + bulk modals, xref picker,
  view-source modal) and all the CSS (single `st-` prefix).

  Everything format-specific is supplied by the `config` prop
  (`StudioConfig`) — lambdas + static knobs — and the optional snippet
  props for genuinely-divergent UI: `bannersExtras` (JSONC / stream /
  parse-warning banners), `toolsExtras` (YAML↔properties converter),
  `errorsBody` (per-format Errors-view copy), `bindingsEmpty` /
  `schemaIntro` / `querySidecarEmpty` (sidecar copy), `headerIcon` (file
  icon), `auxiliaryExtras` (extra modals — JSONC save prompt), and
  `rowSelfMarker` (.properties `$value` self marker).

  RON is NOT a consumer (workspace tabs + Option splitting + variant
  tags make it too special) — it keeps its own modal but shares the
  composables.
-->
<script lang="ts" generics="TKind extends string, TNode extends StudioTreeNode<TKind>">
  import type { Snippet } from 'svelte';
  import { tick, untrack } from 'svelte';
  import {
    Copy, ListTree, FileText, AlertCircle, GitCompare,
    ChevronUp, ChevronDown, Replace,
    Pencil, ClipboardPaste,
    Trash2, Plus, CopyPlus, ArrowUp, ArrowDown,
    Maximize2, Minimize2,
    ListFilter, ScanSearch, Layers,
    Loader2, ChevronsDown, ChevronsUp, Link as LinkIcon,
    BookOpen, ArrowUpRight,
    Wrench,
  } from 'lucide-svelte';
  import Spinner from '../shared/ui/Spinner.svelte';
  import PanelShell from '../shared/ui/PanelShell.svelte';
  import Alert from '../shared/ui/Alert.svelte';
  import StateBlock from '../shared/ui/StateBlock.svelte';
  import TypePill from '../shared/internal/TypePill.svelte';
  import FileExplorerModal from '../sitta/FileExplorerModal.svelte';
  import { type MenuItem } from '../shared/ContextMenu.svelte';
  import { type RowSnippetCtx } from '../shared/ui/Tree.svelte';
  import { type TabItem } from '../shared/ui/Tabs.svelte';
  import StudioModal from './StudioModal.svelte';
  import StudioRightRailButton from './StudioRightRailButton.svelte';
  import StudioFooterStatus   from './StudioFooterStatus.svelte';
  import StudioFooterRight    from './StudioFooterRight.svelte';
  import StudioBodyBanners    from './StudioBodyBanners.svelte';
  import StudioHeaderUndoRedo from './StudioHeaderUndoRedo.svelte';
  import StudioToolsSidebar   from './StudioToolsSidebar.svelte';
  import type { StudioFooterDoc } from './studio-footer-types';
  import { basename as fsBasename, fmtBytes as fsFmtBytes, typePillKind } from './helpers';
  import StudioQueryBar from './StudioQueryBar.svelte';
  import StudioTextPane from './StudioTextPane.svelte';
  import StudioDiffPane, { type StudioDiffPaneController } from './StudioDiffPane.svelte';
  import StudioTreePane, { type StudioTreePaneController } from './StudioTreePane.svelte';
  import StudioInspectorPanel, { type StudioInspectorPanelController } from './StudioInspectorPanel.svelte';
  import StudioRenameModal from './StudioRenameModal.svelte';
  import StudioRefsPanel from './StudioRefsPanel.svelte';
  import StudioSchemaPanel from './StudioSchemaPanel.svelte';
  import StudioBulkEditModal from './StudioBulkEditModal.svelte';
  import StudioViewSourceModal from './StudioViewSourceModal.svelte';
  import StudioKindBadge from './StudioKindBadge.svelte';
  import StudioInlineEdit from './StudioInlineEdit.svelte';
  import StudioXrefPicker from './StudioXrefPicker.svelte';
  import { useStudioEditPipeline }       from './composables/useStudioEditPipeline.svelte';
  import { useStudioCrossRefs }          from './composables/useStudioCrossRefs.svelte';
  import { useStudioRenameBulkPipeline } from './composables/useStudioRenameBulkPipeline.svelte';
  import { useStudioSchema }             from './composables/useStudioSchema.svelte';
  import { useStudioQueryBar }           from './composables/useStudioQueryBar.svelte';
  import { useStudioTextDiff }           from './composables/useStudioTextDiff.svelte';
  import { useStudioSaveFlow }           from './composables/useStudioSaveFlow.svelte';
  import { useStudioUndoRedo }           from './composables/useStudioUndoRedo.svelte';
  import { useStudioGlobalKeys }         from './composables/useStudioGlobalKeys.svelte';
  import { useStudioOutsideEdit }        from './composables/useStudioOutsideEdit.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { studioStore } from '$lib/stores/studio/studio.svelte';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { StudioConfig, StudioCtx, StudioTreeNode } from './studio-config';

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'query' | 'bindings' | 'schema' | 'tools' | null;

  interface Props {
    config: StudioConfig<TKind, TNode>;
    /** Body-banner extras (JSONC / stream / parse-warning). */
    bannersExtras?:     Snippet<[]>;
    /** Tools sidecar extras (YAML↔properties converter). */
    toolsExtras?:       Snippet<[]>;
    /** Errors view body — per-format copy. Defaults to a generic Alert. */
    errorsBody?:        Snippet<[{ parseError: string }]>;
    /** Bindings sidecar empty-state copy. */
    bindingsEmpty?:     Snippet<[]>;
    /** Schema sidecar intro copy. */
    schemaIntro?:       Snippet<[]>;
    /** Header file icon. */
    headerIcon?:        Snippet<[]>;
    /** Extra auxiliary modals (JSONC save prompt). */
    auxiliaryExtras?:   Snippet<[]>;
    /** Override the row key cell entirely (.properties renders `$value`
     *  as a `(self)` marker). */
    rowKeyOverride?:    Snippet<[{ node: TNode }]>;
    /** Selected-path footer pill override (.properties strips `$value`). */
    footerPathSlot?:    Snippet<[{ path: string[] }]>;
  }

  let {
    config,
    bannersExtras,
    toolsExtras,
    errorsBody,
    bindingsEmpty,
    schemaIntro,
    headerIcon,
    auxiliaryExtras,
    rowKeyOverride,
    footerPathSlot,
  }: Props = $props();

  const BE = $derived(config.backend);

  // Object/array classification — formats with non-object/array kind
  // names (TOML table/array_of_tables, RON struct/list) override these.
  const isObjectLike = (k: TKind) => config.isObjectLike?.(k) ?? ((k as string) === 'object');
  const isArrayLike  = (k: TKind) => config.isArrayLike?.(k)  ?? ((k as string) === 'array');

  // ── View + right-pane state ─────────────────────────────────────────
  let viewMode = $state<ViewMode>('tree');

  function loadRightPane(): RightPane {
    if (typeof localStorage === 'undefined') return 'inspector';
    const v = localStorage.getItem(config.rightPaneKey) as RightPane;
    return v === 'inspector' || v === 'query' || v === 'bindings' || v === 'schema' || v === 'tools'
      ? v : 'inspector';
  }
  let rightPane = $state<RightPane>(loadRightPane());

  let studioModal: StudioModal<TKind> | undefined = $state();
  let treePane:    StudioTreePaneController<TKind, TNode> | undefined = $state();
  let diffPane:    StudioDiffPaneController | undefined = $state();
  let inspectorPanel: StudioInspectorPanelController | undefined = $state();

  function setRightPane(p: RightPane) { studioModal?.setRightPane(p); }

  // ── Tree state ──────────────────────────────────────────────────────
  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: StudioNodeViewLike): TNode {
    return { ...v, pid: pathId(v.path), children: null } as unknown as TNode;
  }
  type StudioNodeViewLike = { path: string[]; kind: TKind; key: string; preview: string };

  let roots         = $state<TNode[]>([]);
  let expanded      = $state<Set<string>>(new Set());
  let selectedNode  = $state<TNode | null>(null);
  let valueText     = $state<string | null>(null);
  let valueLoading  = $state(false);
  let expandAllBusy = $state(false);

  async function selectNode(node: TNode): Promise<void> { await treePane?.selectNode(node); }

  async function refreshAfterMutation(node: TNode, structural: boolean, removed = false): Promise<void> {
    await treePane?.refreshAfterMutation(node, structural, removed);
  }

  async function commitPendingEdit(): Promise<void> {
    if (editPipeline.editingPid && editPipeline.editingPid !== selectedNode?.pid) {
      try { await editPipeline.maybeCommitActiveEdit(selectedNode); }
      catch { editPipeline.cancelEdit(); }
    }
  }

  // ── Live ctx threaded into config lambdas that need composables ──────
  const ctx: StudioCtx<TKind, TNode> = {
    schema:          () => studioSchema.schema,
    primitiveHintAt: (p) => studioSchema.primitiveHintAt(p),
    enumDefAt:       (p) => studioSchema.enumDefAt(p),
    unquotedString:  (s) => crossRefs.unquotedString(s),
    isDefinitionFieldName: (k) => crossRefs.isDefinitionFieldName(k),
    refresh:         (n, structural, removed) => refreshAfterMutation(n, structural, removed),
    setEditError:    (m) => editPipeline.setEditError(m),
  };

  // ── rowEditMode / currentVariantTag (schema-aware) ──────────────────
  function rowEditMode(node: TNode): 'primitive' | 'variant' | null {
    if ((node.kind as string) === 'string') {
      const ed = studioSchema.enumDefAt(node.path);
      if (ed && ed.variants.length > 0 && ed.variants.every(v => v.shape === 'unit')) return 'variant';
    }
    if (config.isEditablePrimitive(node.kind)) return 'primitive';
    return null;
  }
  function currentVariantTag(node: TNode): string { return config.currentVariantTag(node, ctx); }

  // ── Edit pipeline ───────────────────────────────────────────────────
  const editPipeline = useStudioEditPipeline<TKind, TNode>({
    formatId: config.formatId,
    isEditablePrimitive: config.isEditablePrimitive,
    isPromotableNull: config.isPromotableNull,
    rowEditMode: (n) => rowEditMode(n),
    currentVariantTag,
    computeSeed: (n) => config.computeSeed(n, valueText),
    commit: (node, draft) => config.commit(node, draft, ctx),
    commitVariant: async (node, tag) => {
      try {
        await config.mutatePrimitive(node.path, { type: 'string', value: tag });
        await refreshAfterMutation(node, /* structural */ false);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    focusInspector: () => inspectorPanel?.focusEditInput(),
  });

  // ── Removability + remove ───────────────────────────────────────────
  function isRemovable(node: TNode | null): boolean {
    if (!node || node.path.length === 0) return false;
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent) return false;
    return config.isContainerKind(parent.kind);
  }
  async function removeSelected(): Promise<void> {
    if (!selectedNode || !isRemovable(selectedNode)) return;
    const node = selectedNode;
    try {
      await config.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) { console.warn(`${config.formatId}-studio: removeAt failed`, e); }
  }

  // ── Container mutations ─────────────────────────────────────────────
  async function addItemAction(parent: TNode): Promise<void> {
    if (!isArrayLike(parent.kind)) return;
    try {
      await config.insertItem(parent.path, config.newItemSnippet(parent.kind));
      await refreshAfterMutation(parent, true);
      editPipeline.maybeShowEditBanner();
    } catch (e) { console.warn(`${config.formatId}-studio: insertItem failed`, e); }
  }
  async function addFieldAction(parent: TNode, name?: string): Promise<void> {
    if (!isObjectLike(parent.kind)) return;
    let key = name ?? '';
    if (!key) {
      const proposed = window.prompt('Field name:', 'new_field');
      if (!proposed) return;
      key = proposed;
    }
    try {
      await config.insertField(parent.path, key, config.newFieldSnippet(parent.kind));
      await refreshAfterMutation(parent, true);
      editPipeline.maybeShowEditBanner();
    } catch (e) { console.warn(`${config.formatId}-studio: insertField failed`, e); }
  }
  async function duplicateAction(node: TNode): Promise<void> {
    if (!isRemovable(node)) return;
    try {
      await config.duplicateAt(node.path);
      const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
      if (parent) await refreshAfterMutation(parent, true);
      editPipeline.maybeShowEditBanner();
    } catch (e) { console.warn(`${config.formatId}-studio: duplicateAt failed`, e); }
  }
  async function moveAction(node: TNode, delta: number): Promise<void> {
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent || !isArrayLike(parent.kind)) return;
    try {
      await config.moveItem(node.path, delta);
      await refreshAfterMutation(parent, true);
      editPipeline.maybeShowEditBanner();
    } catch (e) { console.warn(`${config.formatId}-studio: moveItem failed`, e); }
  }
  async function pasteOverAction(node: TNode): Promise<void> {
    let text: string;
    try { text = await navigator.clipboard.readText(); }
    catch { uiStore.showToast('Clipboard read denied', 'error'); return; }
    const t = text.trim();
    if (!t) { uiStore.showToast('Clipboard is empty', 'error'); return; }
    try {
      await config.replaceAt(node.path, t);
      await refreshAfterMutation(node, true);
      editPipeline.maybeShowEditBanner();
    } catch (e: any) { uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error'); }
  }

  // ── Cross-refs + F12 rename + F13 bulk edit ─────────────────────────
  const crossRefs = useStudioCrossRefs<TKind, TNode>({
    formatId: config.formatId,
    getSourcePath: () => config.store.sourcePath,
    jumpToPath: async (path) => { await treePane?.jumpToPath(path); },
    openExternalDoc: async (absPath, path) => {
      await config.openDoc({ path: absPath });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(path);
    },
    unquotedString:        config.unquotedString,
    isDefinitionFieldName: config.isDefinitionFieldName,
    isReferenceFieldName:  config.isReferenceFieldName,
  });

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = config.store.sourcePath;
    if (!path) return;
    const title = config.store.title;
    await config.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  const renameBulk = useStudioRenameBulkPipeline<TNode>({
    formatId:        config.formatId,
    formatLabel:     config.formatLabel,
    getDocId:        () => config.store.docId,
    getSourcePath:   () => config.store.sourcePath,
    getDirty:        () => config.store.dirty,
    getActiveTabId:  () => tabsStore.activeTabId,
    extractRenameValue: (n) => config.extractRenameValue(n, ctx),
    reloadAfterDiskWrite: async () => { await reloadActiveDocFromDisk(); },
    applyExternalActiveDocState: async (state) => {
      await config.applyExternalMutate(state);
      await treePane?.reloadTree();
    },
  });

  // ── Schema sidecar ──────────────────────────────────────────────────
  const studioSchema = useStudioSchema<TKind, TNode>({
    backend: config.backend,
    getSchemaHint: config.getSchemaHint,
    walkType: config.walkType,
    flattenedFields: config.flattenedFields,
    cssPrefix: config.schemaCssPrefix,
    getSelectedChildKeys: (n) => (n.children ?? []).map((c) => c.key),
    currentVariantTag: (n) => currentVariantTag(n),
  });

  // Inspector → Tree adapters ──────────────────────────────────────────
  async function copyPathOf(node: TNode): Promise<void> {
    const text = config.copyPathText
      ? config.copyPathText(node.path)
      : (node.path.length === 0 ? '$' : '$.' + node.path.join('.'));
    await copyToClipboard(text, { successToast: 'Path copied', errorToast: true });
  }
  async function copyValue(): Promise<void> {
    if (valueText == null) return;
    await copyToClipboard(valueText);
  }
  async function inspectorAddField(parent: TNode, name: string): Promise<void> { await addFieldAction(parent, name); }
  function noopOption(): Promise<void> | void { /* simple formats have no Option */ }
  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || (selectedNode.kind as string) !== 'string') return;
    const current = currentVariantTag(selectedNode);
    if (!name || name === current) return;
    const node = selectedNode;
    try {
      await config.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, false);
    } catch (e: any) { editPipeline.setEditError(e?.message ?? String(e)); }
  }

  // ── Context menu ────────────────────────────────────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    const editable = config.editingEnabled?.() ?? true;
    items.push({ id: 'copy-path',  label: 'Copy path',           icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: config.copyValueLabel, icon: Copy,     iconColor: 'var(--text-muted)' });

    if (editable) {
      const editMode = rowEditMode(node);
      if (editMode === 'variant') {
        items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
        items.push({ id: 'edit-variant', label: 'Change variant…', icon: Replace, iconColor: '#ffc66d', shortcut: 'F2' });
      } else if (editMode === 'primitive') {
        items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
        items.push({ id: 'edit', label: 'Edit value', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
      } else if (config.isPromotableNull?.(node.kind)) {
        items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
        items.push({ id: 'edit', label: 'Edit value', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
      }
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

    if (editable) {
      items.push({ id: 'sep-mutate', label: '', separator: true } as MenuItem);
      items.push({ id: 'paste', label: config.pasteLabel, icon: ClipboardPaste, iconColor: 'var(--text-muted)' });

      if (isObjectLike(node.kind)) {
        items.push({ id: 'add-field', label: 'Add field…', icon: Plus, iconColor: 'var(--success)' });
      } else if (isArrayLike(node.kind)) {
        items.push({ id: 'add-item', label: 'Add item', icon: Plus, iconColor: 'var(--success)' });
      }

      const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
      if (parent && config.isContainerKind(parent.kind)) {
        items.push({ id: 'sep-reorder', label: '', separator: true } as MenuItem);
        items.push({ id: 'duplicate', label: 'Duplicate', icon: CopyPlus, iconColor: 'var(--text-muted)' });
        if (isArrayLike(parent.kind)) {
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
        label:     expanded.has(node.pid) ? 'Collapse'  : 'Expand',
        icon:      expanded.has(node.pid) ? ChevronUp   : ChevronDown,
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
      items.push({ id: 'rename-across-project', label: 'Rename across project…', icon: Replace, iconColor: '#ffc66d' });
    }
    return items;
  }

  async function onContextMenuSelect(id: string, node: TNode): Promise<void> {
    switch (id) {
      case 'copy-path':    await copyPathOf(node);                            break;
      case 'copy-value':   {
        try {
          const v = await BE.getValue(config.store.docId!, node.path);
          await copyToClipboard(v);
        } catch { /* ignore */ }
        break;
      }
      case 'edit':         editPipeline.startEdit(node, 'tree');             break;
      case 'edit-variant': editPipeline.startVariantEdit(node, 'tree');      break;
      case 'view-impl':    {
        const ty = studioSchema.typeAtPath(node.path);
        let p: string | null = null;
        if (ty?.kind === 'named') p = ty.path;
        else if (ty?.kind === 'option' && ty.inner.kind === 'named') p = ty.inner.path;
        if (p) void studioSchema.openViewSource(p);
        break;
      }
      case 'paste':        await pasteOverAction(node);                      break;
      case 'add-field':    await addFieldAction(node);                       break;
      case 'add-item':     await addItemAction(node);                        break;
      case 'duplicate':    await duplicateAction(node);                      break;
      case 'move-up':      await moveAction(node, -1);                       break;
      case 'move-down':    await moveAction(node, +1);                       break;
      case 'expand':       expandNode(node, true);                          break;
      case 'collapse':     expandNode(node, false);                         break;
      case 'expand-all':   await treePane?.expandSubtree(node);             break;
      case 'collapse-all': treePane?.collapseSubtree(node);                 break;
      case 'remove':       await removeSelected();                          break;
      case 'rename-across-project': renameBulk.openRenameModalForNode(node); break;
    }
  }

  function expandNode(node: TNode, want: boolean): void {
    const next = new Set(expanded);
    if (want) next.add(node.pid); else next.delete(node.pid);
    expanded = next;
  }

  // ── Query bar ───────────────────────────────────────────────────────
  const queryBarCtl = useStudioQueryBar<TKind>({
    getRightPane:    () => rightPane,
    setRightPane:    (p) => setRightPane(p),
    toggleQueryPane: () => studioModal?.toggleRightPane('query'),
  });
  function getChildKeysForPath(path: string[]): string[] | null { return treePane?.getChildKeysForPath(path) ?? null; }
  function ensureChildrenLoadedForPath(path: string[]): void { treePane?.ensureChildrenLoadedForPath(path); }
  async function jumpToQueryHit(path: string[]): Promise<void> { await treePane?.jumpToPath(path); }

  // ── Text + Diff views ───────────────────────────────────────────────
  const textDiff = useStudioTextDiff({
    getStoreCurrent: () => config.store.current,
    setText:         (text) => config.setText(text),
    reloadTree:      async () => { await treePane?.reloadTree(); },
  });
  function bumpDiffRefresh() { textDiff.bumpDiffRefresh(); }

  $effect(() => {
    const id = config.store.docId;
    if (!id) {
      queryBarCtl.resetForDocClose();
      editPipeline.cancelEdit();
      return;
    }
    viewMode = 'tree';
  });

  // Cross-ref index — load on modal open + every active-tab change.
  $effect(() => {
    if (!config.store.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, config.formatId); });
  });

  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: textDiff.diffTreeChangeCount > 0 ? textDiff.diffTreeChangeCount
           : textDiff.diffHunkCount > 0       ? textDiff.diffHunkCount
           : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse errors',
      disabled: !config.store.parseError,
      badge: config.store.parseError ? '!' : undefined,
      data: { errorBadge: !!config.store.parseError } },
  ]);

  // ── Indent + Format ─────────────────────────────────────────────────
  let indentUnit = $state<string>('  ');
  let actionBusy = $state(false);
  let actionError = $state<string | null>(null);
  $effect(() => {
    const id = config.store.docId;
    if (!id) return;
    void BE.getIndent(id).then(s => { if (s) indentUnit = s; }).catch(() => {});
  });

  const footerDoc: StudioFooterDoc = $derived({
    parseError: config.store.parseError ?? null,
    dirty:      config.store.dirty,
    sourcePath: config.store.sourcePath ?? null,
    encoding:   config.store.docId ? config.store.encoding : null,
    canUndo:    config.store.canUndo,
    canRedo:    config.store.canRedo,
    docId:      config.store.docId ?? null,
  });
  const selectedFooterPath = $derived<string[] | null>(
    selectedNode && viewMode === 'tree' ? selectedNode.path : null,
  );

  async function setIndentUnit(unit: string): Promise<void> {
    indentUnit = unit;
    const id = config.store.docId;
    if (!id) return;
    try { await BE.setIndent(id, unit); } catch (e) { console.warn(`${config.formatId}-studio: setIndent failed`, e); }
  }
  async function runFormat(): Promise<void> {
    const id = config.store.docId;
    if (!id || actionBusy || config.store.parseError) return;
    actionBusy = true; actionError = null;
    try {
      const formatted = await BE.format(id);
      await config.setText(formatted);
      await treePane?.reloadTree();
      bumpDiffRefresh();
    } catch (e: any) { actionError = `Format failed: ${e?.message ?? e}`; }
    finally { actionBusy = false; }
  }

  // ── Save / Save As ──────────────────────────────────────────────────
  const saveFlow = useStudioSaveFlow({
    getSourcePath: () => config.store.sourcePath,
    save:          (opts) => config.save(opts),
    onSaved:       bumpDiffRefresh,
  });
  function requestSave(): Promise<void> | void {
    return config.onSaveRequested ? config.onSaveRequested() : saveFlow.doSave();
  }

  // ── Misc ────────────────────────────────────────────────────────────
  async function close() {
    textDiff.cancelPendingTextPush();
    await config.closeDoc();
  }
  const fmtBytes   = fsFmtBytes;
  const jsBasename = fsBasename;

  const { doUndo, doRedo } = useStudioUndoRedo({
    undo: () => config.undo(),
    redo: () => config.redo(),
    reloadTree: async () => { await treePane?.reloadTree(); },
    bumpDiffRefresh,
  });

  // ── Keyboard shortcuts ──────────────────────────────────────────────
  const { onKey } = useStudioGlobalKeys<TKind, TNode>({
    isOpen:        () => config.store.open,
    doSave:        () => requestSave(),
    doUndo,
    doRedo,
    getViewMode:     () => viewMode,
    getSelectedNode: () => selectedNode,
    getEditingPid:   () => editPipeline.editingPid,
    startEdit:        (n, loc) => editPipeline.startEdit(n, loc),
    startVariantEdit: (n, loc) => editPipeline.startVariantEdit(n, loc),
    rowEditMode,
    isPromotableNull: config.isPromotableNull,
    isRemovable,
    removeSelected,
    getQueryBarController: () => queryBarCtl.queryBar,
    getDiffPaneController: () => diffPane,
  });

  useStudioOutsideEdit({ editPipeline, getSelectedNode: () => selectedNode });

  // Expose the save-as picker open setter so wrapper auxiliaryExtras can
  // route a custom save flow (JSON) through the shared picker.
  export function openSaveAs() { saveFlow.openSaveAs(); }
  export function doSaveShared() { return saveFlow.doSave(); }
  export function onSaveAsPicked(path: string) { return saveFlow.onSaveAsPicked(path); }
  /** Reload the tree + bump the diff view — wrappers call this after an
   *  out-of-band store mutation (e.g. YAML's .properties import). */
  export async function reloadAfterExternalSetText(): Promise<void> {
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  void tick;
</script>

<svelte:window onkeydown={onKey} />

<StudioModal
  bind:this={studioModal}
  formatId={config.formatId}
  backend={config.backend}
  open={config.store.open}
  loading={config.store.loading}
  loadingLabel={config.loadingLabel}
  errorState={config.store.error}
  parseError={config.store.parseError}
  hasDoc={!!config.store.docId}
  viewItems={viewItems}
  bind:viewMode
  bind:rightPane
  rightPaneStorageKey={config.rightPaneKey}
  ariaLabel={config.ariaLabel}
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
        : config.schemaRailTooltipEmpty}
      label={config.schemaRailLabel}
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
    <span class="st-header-icon-wrap" aria-hidden="true">
      {@render headerIcon?.()}
    </span>
    <StudioHeaderUndoRedo doc={footerDoc} onUndo={doUndo} onRedo={doRedo} />
    <span class="st-title" use:tooltip={config.store.sourcePath ?? ''}>
      {config.store.title ?? config.defaultTitle}
      {#if config.store.dirty}<span class="st-dirty" use:tooltip={'Unsaved changes'}>●</span>{/if}
    </span>
    {#if config.store.sizeBytes != null}
      <span class="st-meta">{fmtBytes(config.store.sizeBytes)}</span>
    {/if}
    <div class="st-spacer"></div>
  {/snippet}

  {#snippet footerStatusLeft()}
    <StudioFooterStatus
      doc={footerDoc}
      errorPillStrategy={footerPathSlot ? 'truncated' : 'short'}
      selectedPath={selectedFooterPath}
      selectedPathSlot={footerPathSlot}
    />
  {/snippet}

  {#snippet toolsSidecar()}
    <StudioToolsSidebar
      doc={footerDoc}
      {actionBusy}
      {indentUnit}
      indentOptions={config.indentOptions}
      indentTooltip={config.indentTooltip}
      formatTooltip={config.formatTooltip}
      onSetIndent={setIndentUnit}
      onFormat={runFormat}
      extras={toolsExtras}
    />
  {/snippet}

  {#snippet footerRight()}
    <StudioFooterRight
      doc={footerDoc}
      saving={saveFlow.saving}
      onSave={() => void requestSave()}
      onSaveAs={saveFlow.openSaveAs}
    />
  {/snippet}

  {#snippet bodyBanners()}
    <StudioBodyBanners saveError={saveFlow.saveError} {actionError} extras={bannersExtras} />
  {/snippet}

  {#snippet queryBarSlot()}
    <StudioQueryBar
      bind:this={queryBarCtl.queryBar}
      formatId={config.formatId}
      backend={config.backend}
      docId={config.store.docId}
      visible={viewMode === 'tree' && !config.store.parseError}
      placeholder={config.queryPlaceholder}
      historyStorageKey={config.queryHistoryKey}
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
        {#if config.kindBadgeStyle === 'tinted'}
          <StudioKindBadge label={config.kindBadge(kind)} tone={config.kindTone(kind)} tinted tooltip={kind} />
        {:else}
          <StudioKindBadge label={config.kindBadge(kind)} tone={config.kindTone(kind)} italic={(kind as string) === 'null'} tooltip={kind} />
        {/if}
      {/snippet}
      {#snippet toolbarRight()}
        <button type="button" class="st-query-tool-btn"
          onclick={() => void treePane?.expandAll()}
          disabled={expandAllBusy}
          use:tooltip={'Recursively load + expand every container'}
          aria-label="Expand all"
        >{#if expandAllBusy}<Loader2 size={12} class="st-query-spinner" />{:else}<ChevronsDown size={12} />{/if}<span>Expand</span></button>
        <button type="button" class="st-query-tool-btn"
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
        formatId={config.formatId}
        backend={config.backend}
        docId={config.store.docId}
        parseError={config.store.parseError}
        bind:roots
        bind:expanded
        bind:selectedNode
        bind:valueText
        bind:valueLoading
        bind:expandAllBusy
        toTree={toTree as any}
        sortChildren={config.sortChildren as any}
        isContainerKind={config.isContainerKind}
        getContextMenuItems={ctxItemsFor as any}
        onContextMenuSelect={(id: string, n: any) => onContextMenuSelect(id, n as TNode)}
        {commitPendingEdit}
        showRightBorder={false}
        ariaLabel={`${config.formatLabel} document tree`}
        errorMessage={config.treeErrorMessage}
      >
        {#snippet rowContent({ node }: RowSnippetCtx<any>)}
          {@const n = node as TNode}
          {@const ty = studioSchema.typeAtPath(n.path)}
          {@const namedType = studioSchema.namedTypeAt(n.path)}
          {#if config.kindBadgeStyle === 'tinted'}
            <StudioKindBadge label={config.kindBadge(n.kind)} tone={config.kindTone(n.kind)} tinted tooltip={n.kind} />
          {:else}
            <StudioKindBadge label={config.kindBadge(n.kind)} tone={config.kindTone(n.kind)} italic={(n.kind as string) === 'null'} tooltip={n.kind} />
          {/if}
          {#if rowKeyOverride}
            {@render rowKeyOverride({ node: n })}
          {:else}
            <span class="st-row-key" class:st-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          {/if}
          <span class="st-row-sep">{config.separator}</span>
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
            {:else if config.isBoolKind(n.kind)}
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
                placeholder={(n.kind as string) === 'null' ? 'Type a value…' : undefined}
                onkeydown={(e) => editPipeline.onEditKey(e, n)}
                errorMsg={editPipeline.editError}
              />
            {/if}
          {:else}
            {@const xrefs = crossRefs.crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            {@const editableRow = rowEditMode(n) !== null || (config.isPromotableNull?.(n.kind) ?? false)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="st-row-preview st-row-preview-{n.kind}"
                  class:st-row-preview-editable={editableRow}
                  class:st-row-preview-xref={hasX}
                  ondblclick={(e) => {
                    if (!editableRow) return;
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
                      : (config.isPromotableNull?.(n.kind) ?? false) ? 'Double-click to fill'
                      : '')}
            >{n.preview}{#if hasX}<span class="st-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="st-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}
          {#if n.loading}<Loader2 size={10} class="st-row-loader" />{/if}
          {#if namedType}
            <span class="st-row-type-slot">
              <TypePill label={namedType} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {:else if ty && ty.kind !== 'named'}
            <span class="st-row-type-slot">
              <TypePill label={studioSchema.fmtType(ty)} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textDiff.textBuf}
        language={config.formatId}
        oninput={textDiff.onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId={config.formatId}
        backend={config.backend}
        docId={config.store.docId}
        visible={viewMode === 'diff'}
        currentText={config.store.current}
        refreshTick={textDiff.diffRefreshTick}
        bind:treeChangeCount={textDiff.diffTreeChangeCount}
        bind:hunkCount={textDiff.diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- Simple formats have no variant tags. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if config.store.parseError}
        {#if errorsBody}
          {@render errorsBody({ parseError: config.store.parseError })}
        {:else}
          <div class="st-errors-wrap">
            <Alert variant="error" title={`${config.formatLabel} parse error`}>
              <pre class="st-errors-body">{config.store.parseError}</pre>
              <p class="st-errors-hint">
                Switch to the <strong>Text</strong> tab to fix it. The error will
                clear automatically once the document parses.
              </p>
            </Alert>
          </div>
        {/if}
      {:else}
        <StateBlock tone="success" label="No parse errors." />
      {/if}
    {/if}
  {/snippet}

  {#snippet inspectorSidecar()}
    <StudioInspectorPanel
      bind:this={inspectorPanel}
      formatId={config.formatId}
      backend={config.backend}
      selectedNode={selectedNode as any}
      {valueText}
      {valueLoading}
      editingPid={editPipeline.editingPid}
      editLocation={editPipeline.editLocation}
      bind:editBuf={editPipeline.editBuf}
      editError={editPipeline.editError}
      editBannerVisible={editPipeline.editBannerVisible}
      kindBadge={config.kindBadge as any}
      isRemovable={isRemovable as any}
      isEditablePrimitive={config.isEditablePrimitive as any}
      isBoolKind={config.isBoolKind as any}
      isContainerKind={config.isContainerKind as any}
      isDefinitionNode={((n: TNode) => config.isDefinitionNode(n, ctx)) as any}
      definitionValue={((n: TNode) => config.definitionValue(n, ctx)) as any}
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
    <PanelShell title="Query results" count={queryBarCtl.queryHits.length} class="st-query-shell">
      {#snippet icon()}<ListFilter size={13} />{/snippet}
    <div class="st-query-pane-body">
      {#if !queryBarCtl.query.trim()}
        <p class="st-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if queryBarCtl.querying && queryBarCtl.queryHits.length === 0}
        <div class="st-query-pane-status"><Spinner size="xs" /> <span>Running query…</span></div>
      {:else if queryBarCtl.queryError}
        <div class="st-query-pane-error"><AlertCircle size={11} /> {queryBarCtl.queryError}</div>
      {:else if queryBarCtl.queryHits.length === 0}
        <p class="st-query-pane-empty">No matches.</p>
      {:else}
        <div class="st-query-pane-list">
          {#each queryBarCtl.queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="st-query-pane-card"
              class:active={i === queryBarCtl.currentHitIdx}
              onclick={() => { queryBarCtl.currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="st-query-pane-card-head">
                {#if config.kindBadgeStyle === 'tinted'}
                  <StudioKindBadge label={config.kindBadge(hit.kind)} tone={config.kindTone(hit.kind)} tinted tooltip={hit.kind} />
                {:else}
                  <StudioKindBadge label={config.kindBadge(hit.kind)} tone={config.kindTone(hit.kind)} italic={(hit.kind as string) === 'null'} tooltip={hit.kind} />
                {/if}
                <span class="st-query-pane-card-idx">#{i + 1}</span>
              </div>
              <div class="st-query-pane-card-path">{hit.path.length === 0 ? '$' : '$.' + hit.path.join('.')}</div>
              {#if hit.preview}<div class="st-query-pane-card-preview">{hit.preview}</div>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    </PanelShell>
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId={config.formatId}
      backend={config.backend}
      sourcePath={config.store.sourcePath}
      onOpenDefinition={crossRefs.openDefinition}
    >
      {#snippet emptyState()}
        {#if bindingsEmpty}
          {@render bindingsEmpty()}
        {:else}
          <p class="st-bindings-empty">
            Project-wide cross-refs follow the <code>id</code> / <code>name</code>
            convention by default. Custom reference-field patterns live in
            the repo's <code>.arbor/studio.toml</code> bindings.
          </p>
        {/if}
      {/snippet}
    </StudioRefsPanel>
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId={config.formatId}
      backend={config.backend}
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
      pickerTitle={config.schemaPickerTitle}
      pickerExtensions={config.schemaPickerExts}
      pickerButtonLabel={config.schemaPickerButton}
      intro={schemaIntro}
    />
  {/snippet}

  {#snippet auxiliary()}
    {#if saveFlow.savePickerOpen}
      <FileExplorerModal
        mode="save"
        title={`Save ${config.formatLabel} document as`}
        extensions={config.saveExtensions}
        initialPath={config.store.sourcePath ?? undefined}
        initialFilename={jsBasename(config.store.sourcePath) || config.saveDefaultName}
        onConfirm={saveFlow.onSaveAsPicked}
        onCancel={() => saveFlow.savePickerOpen = false}
      />
    {/if}

    {@render auxiliaryExtras?.()}

    {#if renameBulk.renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={config.backend}
        tabId={tabsStore.activeTabId}
        formatLabel={config.formatLabel}
        oldValue={renameBulk.renameModalState.oldValue}
        openDocs={renameBulk.buildRenameOpenDocs()}
        onClose={renameBulk.closeRenameModal}
        onApplied={renameBulk.onRenameApplied}
      />
    {/if}

    {#if renameBulk.bulkEditModalState && tabsStore.activeTabId && config.store.docId}
      <StudioBulkEditModal
        backend={config.backend}
        tabId={tabsStore.activeTabId}
        docId={config.store.docId}
        formatLabel={config.formatLabel}
        query={renameBulk.bulkEditModalState.query}
        nullPolicy={config.nullPolicy}
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
  .st-header-icon-wrap { display: inline-flex; align-items: center; color: var(--accent); flex-shrink: 0; }
  .st-title {
    font-size: 13px; font-weight: 600; color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    min-width: 0; max-width: 50%;
  }
  .st-dirty { color: var(--accent); font-size: 14px; margin-left: 4px; line-height: 1; }
  .st-meta {
    font-family: var(--font-code); font-size: 10px; color: var(--text-muted);
    background: var(--bg-overlay); padding: 2px 6px; border-radius: 999px; flex-shrink: 0;
  }
  .st-spacer { flex: 1; }

  .st-row-key { color: var(--text-primary); font-family: var(--font-code); font-size: 11px; white-space: nowrap; }
  .st-row-key-index { color: var(--text-muted); font-style: italic; }
  .st-row-sep { color: var(--text-muted); font-family: var(--font-code); font-size: 11px; margin: 0 4px; }
  .st-row-preview {
    color: var(--text-secondary); font-family: var(--font-code); font-size: 11px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1;
  }
  .st-row-preview-string  { color: var(--syntax-string, #6a9956); }
  .st-row-preview-integer,
  .st-row-preview-float,
  .st-row-preview-number  { color: var(--syntax-number, #9876aa); }
  .st-row-preview-null    { color: var(--text-muted); font-style: italic; }
  .st-row-loader { color: var(--text-muted); flex-shrink: 0; }
  .st-row-preview-editable { cursor: text; }

  .st-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .st-errors-body {
    background: var(--bg-overlay); color: var(--text-primary);
    padding: 10px; border-radius: 4px; font-family: var(--font-code);
    font-size: 11px; margin: 6px 0 0; overflow: auto; white-space: pre-wrap;
  }
  .st-errors-hint { color: var(--text-muted); font-size: 11px; margin: 6px 0 0; }

  .st-query-pane-body { padding: 8px; overflow: auto; height: 100%; }
  .st-query-pane-empty,
  .st-query-pane-status,
  .st-query-pane-error {
    color: var(--text-muted); font-size: 11px; padding: 8px; margin: 0; line-height: 1.5;
  }
  .st-query-pane-error { color: var(--text-error, #ff6c5c); display: inline-flex; align-items: center; gap: 4px; }
  .st-query-pane-list { display: flex; flex-direction: column; gap: 4px; }
  .st-query-pane-card {
    display: flex; flex-direction: column; gap: 2px; padding: 6px 8px;
    border-radius: 4px; border: 1px solid var(--border-subtle); background: var(--bg-overlay);
    color: var(--text-primary); font-family: var(--font-code); font-size: 11px; cursor: pointer; text-align: left;
  }
  .st-query-pane-card:hover { background: var(--bg-hover); }
  .st-query-pane-card.active { border-color: var(--accent); background: var(--bg-hover); }
  .st-query-pane-card-head { display: flex; align-items: center; gap: 6px; }
  .st-query-pane-card-idx { color: var(--text-muted); }
  .st-query-pane-card-path { color: var(--text-primary); }
  .st-query-pane-card-preview { color: var(--text-secondary); }

  .st-query-tool-btn {
    display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; height: 22px;
    border: 1px solid var(--border-subtle); background: var(--bg-overlay);
    color: var(--text-secondary); border-radius: 4px; font-size: 10px; cursor: pointer;
  }
  .st-query-tool-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .st-query-tool-btn:disabled { color: var(--text-disabled); cursor: not-allowed; }
  .st-bindings-empty { color: var(--text-muted); font-size: 11px; padding: 12px; margin: 0; line-height: 1.5; }
  :global(.st-query-spinner) { animation: st-spin 1s linear infinite; }
  @keyframes st-spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }

  .st-row-type-slot { margin-left: auto; flex-shrink: 0; display: inline-flex; align-items: center; }

  .st-row-preview-xref { cursor: pointer; }
  .st-row-xref {
    display: inline-flex; align-items: center; gap: 2px; margin-left: 4px; padding: 1px 4px 1px 3px;
    color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-radius: 4px; line-height: 1; opacity: 0.85;
    transition: opacity var(--transition-fast), background var(--transition-fast); vertical-align: 2px;
  }
  .st-row-preview-xref:hover .st-row-xref { opacity: 1; background: color-mix(in srgb, var(--accent) 24%, transparent); }
  .st-row-xref-count { font-family: var(--font-code); font-size: 9.5px; font-weight: 700; color: var(--accent); }
</style>
