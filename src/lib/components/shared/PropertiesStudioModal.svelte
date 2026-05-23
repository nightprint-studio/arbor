<!--
  PropertiesStudioModal — `.properties` wrapper around the generic
  `<StudioModal>` shell.

  Architecture mirrors YamlStudioModal: all reusable Studio logic lives
  in `./studio/composables/*`; this file only owns the Properties-
  specific bits — flat tree decoration, `$value` self-marker for keys
  that are both leaf and prefix, every-string-is-a-ref cross-refs, no
  native typing on the wire.

  Capabilities exposed to the user:
    · Lossless line-based parse + edit on the host (comments, blank
      lines, separator whitespace, continuation backslashes, Unicode
      escapes all survive a round-trip).
    · Tree view with mutations (edit primitives, remove, add field/item,
      duplicate, move). F2 inline edit; schema-aware variant picker on
      enum-typed string leaves.
    · Text view via `<StudioTextPane>` (CodeMirror 6 + hand-rolled
      `.properties` StreamLanguage in studio-codemirror.ts).
    · Diff + Errors + Inspector + Query + Bindings + Schema sidecars.
    · F12 cross-ref rename (every key is a target, every value a ref).
    · F13 bulk edit (`null_handling = ask_user` — empty value vs delete
      key is a runtime choice).

  FROZEN F4 / F5:
    · `.properties` has no native typing — every leaf is a string. The
      kind enum carries `null` for parity with the bulk-edit modal hint,
      but actual leaves emit `string` from the BE.
    · Every flat dotted key is a cross-ref target; every value is a
      potential reference. No `id`/`name` heuristic.
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
  import { propertiesStudioStore, type PropertiesNodeKind } from '$lib/stores/properties-studio.svelte';
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

  /** Pre-bound `.properties` backend. */
  const PROPS_BE = studioBackend<PropertiesNodeKind>('properties');

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'query' | 'bindings' | 'schema' | 'tools' | null;

  let viewMode = $state<ViewMode>('tree');

  const RIGHT_PANE_KEY = 'arbor:properties-studio:right-pane';
  function loadRightPane(): RightPane {
    if (typeof localStorage === 'undefined') return 'inspector';
    const v = localStorage.getItem(RIGHT_PANE_KEY) as RightPane;
    return v === 'inspector' || v === 'query' || v === 'bindings' || v === 'schema'
      ? v : 'inspector';
  }
  let rightPane = $state<RightPane>(loadRightPane());

  let studioModal: StudioModal<PropertiesNodeKind> | undefined = $state();
  let treePane:    StudioTreePaneController<PropertiesNodeKind, TNode> | undefined = $state();
  let diffPane:    StudioDiffPaneController | undefined = $state();
  let inspectorPanel: StudioInspectorPanelController | undefined = $state();

  function setRightPane(p: RightPane) { studioModal?.setRightPane(p); }

  // ── Tree state ─────────────────────────────────────────────────────────
  type PropsNodeView = StudioNodeView<PropertiesNodeKind>;
  type TNode = PropsNodeView & {
    pid:      string;
    children: TNode[] | null;
    loading?: boolean;
  };

  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: PropsNodeView): TNode { return { ...v, pid: pathId(v.path), children: null }; }
  /** Source order — `.properties` lines are inherently ordered. */
  function sortChildren(_parentKind: PropertiesNodeKind, kids: TNode[]): TNode[] { return kids; }
  function isContainerKind(k: PropertiesNodeKind): boolean {
    return k === 'object' || k === 'array';
  }
  /** `.properties` leaves are always projected as `string`. The
   *  schema-aware narrowing in `commit` lets the user type
   *  number/bool when the schema says so — we coerce back to string on
   *  the wire because the format itself has no typing. */
  function isEditablePrimitive(k: PropertiesNodeKind): boolean {
    return k === 'string';
  }
  function isPromotableNull(k: PropertiesNodeKind): boolean { return k === 'null'; }

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
  const editPipeline = useStudioEditPipeline<PropertiesNodeKind, TNode>({
    formatId: 'properties',
    isEditablePrimitive,
    isPromotableNull,
    rowEditMode:       (n) => rowEditMode(n),
    currentVariantTag,
    computeSeed: (n) => {
      let seed = valueText ?? n.preview;
      // `.properties` previews don't quote — strip just in case.
      if (n.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
        try { seed = JSON.parse(seed) as string; }
        catch { seed = seed.slice(1, -1); }
      }
      if (n.kind === 'null') seed = '';
      return seed;
    },
    commit: async (node, draft) => {
      // Schema-aware narrowing — `.properties` has no native typing on
      // the wire, but the schema can hint that this leaf SHOULD be an
      // int/float/bool. We pass the typed primitive to the BE so the
      // tree projection updates correctly; the actual on-disk value is
      // still the string representation.
      const hint = studioSchema.schema ? studioSchema.primitiveHintAt(node.path) : null;
      const wantFloat = hint === 'f32' || hint === 'f64' || hint === 'number';
      const wantInt   = hint === 'integer'
        || (hint != null && (hint.startsWith('i') || hint.startsWith('u'))
            && hint !== 'isize' && hint !== 'usize')
        || hint === 'isize' || hint === 'usize';
      const wantBool  = hint === 'bool' || hint === 'boolean';

      let value: StudioPrimitiveValue;
      try {
        if (wantBool) {
          const t = draft.trim().toLowerCase();
          if (t !== 'true' && t !== 'false') throw new Error('schema: expected boolean');
          value = { type: 'bool', value: t === 'true' };
        } else if (wantInt) {
          const n = Number(draft.trim());
          if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
          value = { type: 'int', value: Math.trunc(n) };
        } else if (wantFloat) {
          const n = Number(draft.trim());
          if (!Number.isFinite(n)) throw new Error('schema: expected number');
          value = { type: 'float', value: n };
        } else {
          value = { type: 'string', value: draft };
        }
      } catch (e: any) {
        return { error: e?.message ?? String(e) };
      }
      try {
        await propertiesStudioStore.mutatePrimitive(node.path, value);
        await refreshAfterMutation(node, /* structural */ false);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    commitVariant: async (node, tag) => {
      try {
        await propertiesStudioStore.mutatePrimitive(node.path, { type: 'string', value: tag });
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
      await propertiesStudioStore.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('properties-studio: removeAt failed', e);
    }
  }

  // ── Container mutations ────────────────────────────────────────────────
  async function addItemAction(parent: TNode): Promise<void> {
    if (parent.kind !== 'array') return;
    try {
      // `.properties` arrays use Spring `[N]` brackets — the BE assigns
      // the next index automatically. The snippet is a placeholder
      // string value; the user typically edits it immediately after.
      await propertiesStudioStore.insertItem(parent.path, '');
      await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('properties-studio: insertItem failed', e);
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
      await propertiesStudioStore.insertField(parent.path, key, '');
      await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('properties-studio: insertField failed', e);
    }
  }

  async function duplicateAction(node: TNode): Promise<void> {
    if (!isRemovable(node)) return;
    try {
      await propertiesStudioStore.duplicateAt(node.path);
      const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('properties-studio: duplicateAt failed', e);
    }
  }

  async function moveAction(node: TNode, delta: number): Promise<void> {
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent || parent.kind !== 'array') return;
    try {
      await propertiesStudioStore.moveItem(node.path, delta);
      await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('properties-studio: moveItem failed', e);
    }
  }

  async function pasteOverAction(node: TNode): Promise<void> {
    let text: string;
    try { text = await navigator.clipboard.readText(); }
    catch { uiStore.showToast('Clipboard read denied', 'error'); return; }
    const t = text.trim();
    if (!t) { uiStore.showToast('Clipboard is empty', 'error'); return; }
    try {
      // For `.properties`, replace_at on a leaf is identical to
      // set_primitive: the BE writes the raw text as the new value.
      await propertiesStudioStore.replaceAt(node.path, t);
      await refreshAfterMutation(node, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e: any) {
      uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error');
    }
  }

  // ── Cross-refs + F12 rename + F13 bulk edit ────────────────────────────
  //
  // FROZEN F5: every key is a cross-ref target, every value is a
  // potential reference. The composable's defaults are overridden so
  // `isDefinitionFieldName` and `isReferenceFieldName` accept every key.
  // `unquotedString` is identity — `.properties` has no quotes.

  const crossRefs = useStudioCrossRefs<PropertiesNodeKind, TNode>({
    formatId: 'properties',
    getSourcePath: () => propertiesStudioStore.sourcePath,
    jumpToPath: async (path) => { await treePane?.jumpToPath(path); },
    openExternalDoc: async (absPath, path) => {
      await propertiesStudioStore.openDoc({ path: absPath });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(path);
    },
    unquotedString:        (preview) => preview || null,
    isDefinitionFieldName: () => true,
    isReferenceFieldName:  () => true,
  });

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = propertiesStudioStore.sourcePath;
    if (!path) return;
    const title = propertiesStudioStore.title;
    await propertiesStudioStore.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  const renameBulk = useStudioRenameBulkPipeline<TNode>({
    formatId:        'properties',
    formatLabel:     '.properties',
    getDocId:        () => propertiesStudioStore.docId,
    getSourcePath:   () => propertiesStudioStore.sourcePath,
    getDirty:        () => propertiesStudioStore.dirty,
    getActiveTabId:  () => tabsStore.activeTabId,
    extractRenameValue: (n) => n.preview || null,
    reloadAfterDiskWrite: async () => { await reloadActiveDocFromDisk(); },
    applyExternalActiveDocState: async (state) => {
      await propertiesStudioStore.applyExternalMutate(state);
      await treePane?.reloadTree();
    },
  });

  // ── Schema sidecar + walker + chips + Inspector adapters ──────────────
  const studioSchema = useStudioSchema<PropertiesNodeKind, TNode>({
    backend: PROPS_BE,
    getSchemaHint: () => propertiesStudioStore.schemaHint,
    walkType: walkTypeAtPath,
    flattenedFields: flattenedStructFields,
    cssPrefix: 'ps',
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
    return node.preview ?? '';
  }

  // Inspector → Tree adapters ──────────────────────────────────────────
  async function copyPathOf(node: TNode): Promise<void> {
    // Strip the `$value` sentinel — it's a UI artefact, not part of the
    // actual flat .properties key the user wants on the clipboard.
    const segs = node.path.filter(s => s !== '$value');
    const text = segs.length === 0 ? '$' : '$.' + segs.join('.');
    await copyToClipboard(text, { successToast: 'Path copied', errorToast: true });
  }
  async function copyValue(): Promise<void> {
    if (valueText == null) return;
    await copyToClipboard(valueText);
  }
  async function inspectorAddField(parent: TNode, name: string): Promise<void> {
    await addFieldAction(parent, name);
  }
  function noopOption(): Promise<void> | void { /* .properties has no Option */ }

  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || selectedNode.kind !== 'string') return;
    const current = currentVariantTag(selectedNode);
    if (!name || name === current) return;
    const node = selectedNode;
    try {
      await propertiesStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editPipeline.setEditError(e?.message ?? String(e));
    }
  }

  // ── Context menu ───────────────────────────────────────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({ id: 'copy-path',  label: 'Copy path',  icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: 'Copy value', icon: Copy,     iconColor: 'var(--text-muted)' });

    const editMode = rowEditMode(node);
    if (editMode === 'variant') {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
      items.push({ id: 'edit-variant', label: 'Change variant…', icon: Replace, iconColor: '#ffc66d', shortcut: 'F2' });
    } else if (editMode === 'primitive') {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
      items.push({ id: 'edit', label: 'Edit value', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
    } else if (isPromotableNull(node.kind)) {
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
    items.push({ id: 'paste', label: 'Paste over value…', icon: ClipboardPaste, iconColor: 'var(--text-muted)' });

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
      case 'copy-path':    await copyPathOf(node);                          break;
      case 'copy-value':   {
        try {
          const v = await PROPS_BE.getValue(propertiesStudioStore.docId!, node.path);
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
      case 'expand':       expandNode(node, true);                           break;
      case 'collapse':     expandNode(node, false);                          break;
      case 'expand-all':   await treePane?.expandSubtree(node);              break;
      case 'collapse-all': treePane?.collapseSubtree(node);                  break;
      case 'remove':       await removeSelected();                           break;
      case 'rename-across-project': renameBulk.openRenameModalForNode(node); break;
    }
  }

  function expandNode(node: TNode, want: boolean): void {
    const next = new Set(expanded);
    if (want) next.add(node.pid); else next.delete(node.pid);
    expanded = next;
  }

  // ── Query bar ──────────────────────────────────────────────────────────
  const queryBarCtl = useStudioQueryBar<PropertiesNodeKind>({
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
    getStoreCurrent: () => propertiesStudioStore.current,
    setText:         (text) => propertiesStudioStore.setText(text),
    reloadTree:      async () => { await treePane?.reloadTree(); },
  });
  function bumpDiffRefresh() { textDiff.bumpDiffRefresh(); }

  $effect(() => {
    const id = propertiesStudioStore.docId;
    if (!id) {
      queryBarCtl.resetForDocClose();
      editPipeline.cancelEdit();
      return;
    }
    viewMode = 'tree';
  });

  // Cross-ref index — load on modal open + every active-tab change.
  $effect(() => {
    if (!propertiesStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, 'properties'); });
  });

  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: textDiff.diffTreeChangeCount > 0 ? textDiff.diffTreeChangeCount
           : textDiff.diffHunkCount > 0       ? textDiff.diffHunkCount
           : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse warnings',
      disabled: !propertiesStudioStore.parseError,
      badge: propertiesStudioStore.parseError ? '!' : undefined,
      data: { errorBadge: !!propertiesStudioStore.parseError } },
  ]);

  // ── Indent + Format ────────────────────────────────────────────────────
  let indentUnit = $state<string>('  ');
  let actionBusy = $state(false);
  let actionError = $state<string | null>(null);
  $effect(() => {
    const id = propertiesStudioStore.docId;
    if (!id) return;
    void PROPS_BE.getIndent(id).then(s => { if (s) indentUnit = s; }).catch(() => {});
  });

  // ── Footer snapshot (consumed by shared StudioFooter* components) ──────
  const footerDoc: StudioFooterDoc = $derived({
    parseError: propertiesStudioStore.parseError ?? null,
    dirty:      propertiesStudioStore.dirty,
    sourcePath: propertiesStudioStore.sourcePath ?? null,
    encoding:   propertiesStudioStore.docId ? propertiesStudioStore.encoding : null,
    canUndo:    propertiesStudioStore.canUndo,
    canRedo:    propertiesStudioStore.canRedo,
    docId:      propertiesStudioStore.docId ?? null,
  });
  const selectedFooterPath = $derived<string[] | null>(
    selectedNode && viewMode === 'tree' ? selectedNode.path : null,
  );

  async function setIndentUnit(unit: string): Promise<void> {
    indentUnit = unit;
    const id = propertiesStudioStore.docId;
    if (!id) return;
    try { await PROPS_BE.setIndent(id, unit); } catch (e) {
      console.warn('properties-studio: setIndent failed', e);
    }
  }
  async function runFormat(): Promise<void> {
    // `.properties` has no canonical pretty form — `format` is a no-op
    // on the host (returns the current buffer unchanged). Keep the
    // button enabled so the user discovers it; future enhancement
    // could re-align the `=` columns.
    const id = propertiesStudioStore.docId;
    if (!id || actionBusy || propertiesStudioStore.parseError) return;
    actionBusy = true; actionError = null;
    try {
      const formatted = await PROPS_BE.format(id);
      await propertiesStudioStore.setText(formatted);
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
    getSourcePath: () => propertiesStudioStore.sourcePath,
    save:          (opts) => propertiesStudioStore.save(opts),
    onSaved:       bumpDiffRefresh,
  });

  // ── Misc ───────────────────────────────────────────────────────────────
  async function close() {
    textDiff.cancelPendingTextPush();
    await propertiesStudioStore.closeDoc();
  }

  const fmtBytes   = fsFmtBytes;
  const jsBasename = fsBasename;

  function kindBadge(k: PropertiesNodeKind): string {
    switch (k) {
      case 'object':  return '{}';
      case 'array':   return '[]';
      case 'string':  return '“';
      case 'null':    return '∅';
    }
  }
  function kindTone(k: PropertiesNodeKind): StudioKindTone {
    switch (k) {
      case 'object':
      case 'array':   return 'type';
      case 'string':  return 'string';
      case 'null':    return 'muted';
    }
  }
  function isBoolKind(_k: PropertiesNodeKind): boolean { return false; }

  const { doUndo, doRedo } = useStudioUndoRedo({
    undo: () => propertiesStudioStore.undo(),
    redo: () => propertiesStudioStore.redo(),
    reloadTree: async () => { await treePane?.reloadTree(); },
    bumpDiffRefresh,
  });

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  const { onKey } = useStudioGlobalKeys<PropertiesNodeKind, TNode>({
    isOpen:        () => propertiesStudioStore.open,
    doSave:        () => saveFlow.doSave(),
    doUndo,
    doRedo,
    getViewMode:     () => viewMode,
    getSelectedNode: () => selectedNode,
    getEditingPid:   () => editPipeline.editingPid,
    startEdit:        (n, loc) => editPipeline.startEdit(n, loc),
    startVariantEdit: (n, loc) => editPipeline.startVariantEdit(n, loc),
    rowEditMode,
    isPromotableNull,
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
  formatId="properties"
  backend={PROPS_BE}
  open={propertiesStudioStore.open}
  loading={propertiesStudioStore.loading}
  loadingLabel="Opening .properties document…"
  errorState={propertiesStudioStore.error}
  parseError={propertiesStudioStore.parseError}
  hasDoc={!!propertiesStudioStore.docId}
  viewItems={viewItems}
  bind:viewMode
  bind:rightPane
  rightPaneStorageKey={RIGHT_PANE_KEY}
  ariaLabel=".properties Studio"
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
    <span class="ps-header-icon-wrap" aria-hidden="true">
      <svg viewBox="0 0 24 24" width="18" height="18" xmlns="http://www.w3.org/2000/svg">
        <rect x="3" y="3" width="18" height="18" rx="2" fill="currentColor" opacity="0.18" />
        <path d="M6 9h4M11 9h7M6 13h3M10 13h8M6 17h5M12 17h6"
              stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </span>
    <StudioHeaderUndoRedo doc={footerDoc} onUndo={doUndo} onRedo={doRedo} />
    <span class="ps-title" use:tooltip={propertiesStudioStore.sourcePath ?? ''}>
      {propertiesStudioStore.title ?? 'Properties Studio'}
      {#if propertiesStudioStore.dirty}<span class="ps-dirty" use:tooltip={'Unsaved changes'}>●</span>{/if}
    </span>
    {#if propertiesStudioStore.sizeBytes != null}
      <span class="ps-meta">{fmtBytes(propertiesStudioStore.sizeBytes)}</span>
    {/if}
    <div class="ps-spacer"></div>
  {/snippet}

  {#snippet footerStatusLeft()}
    <StudioFooterStatus
      doc={footerDoc}
      errorPillStrategy="truncated"
      selectedPath={selectedFooterPath}
    >
      {#snippet selectedPathSlot({ path }: { path: string[] })}
        {@const segs = path.filter(s => s !== '$value')}
        {@const isSelf = path.includes('$value')}
        <span class="ps-footer-path-pill" use:tooltip={isSelf
          ? `Selected node path — points at the value carried by the prefix \`${segs.join('.')}\` itself (next to its sub-keys).`
          : 'Selected node path'}>
          {segs.length === 0 ? '$' : '$.' + segs.join('.')}{#if isSelf}<span class="ps-footer-path-self"> · self</span>{/if}
        </span>
      {/snippet}
    </StudioFooterStatus>
  {/snippet}

  {#snippet toolsSidecar()}
    <StudioToolsSidebar
      doc={footerDoc}
      {actionBusy}
      {indentUnit}
      indentTooltip="Indent — informational; .properties has no nested indentation"
      formatTooltip="Format — no-op for .properties (every byte already preserved)"
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
    <StudioBodyBanners saveError={saveFlow.saveError} {actionError}>
      {#snippet extras()}
        {#if propertiesStudioStore.parseError}
          <div class="ps-banner-wrap"><Alert variant="warning" compact text={propertiesStudioStore.parseError} /></div>
        {/if}
      {/snippet}
    </StudioBodyBanners>
  {/snippet}

  {#snippet queryBarSlot()}
    <StudioQueryBar
      bind:this={queryBarCtl.queryBar}
      formatId="properties"
      backend={PROPS_BE}
      docId={propertiesStudioStore.docId}
      visible={viewMode === 'tree' && !propertiesStudioStore.parseError}
      placeholder='Query — server.port, $..host, $.servers[0], …'
      historyStorageKey="arbor:properties-studio:query-history"
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
        <StudioKindBadge label={kindBadge(kind)} tone={kindTone(kind)} italic={kind === 'null'} tooltip={kind} />
      {/snippet}
      {#snippet toolbarRight()}
        <button type="button" class="ps-query-tool-btn"
          onclick={() => void treePane?.expandAll()}
          disabled={expandAllBusy}
          use:tooltip={'Recursively load + expand every container'}
          aria-label="Expand all"
        >{#if expandAllBusy}<Loader2 size={12} class="ps-query-spinner" />{:else}<ChevronsDown size={12} />{/if}<span>Expand</span></button>
        <button type="button" class="ps-query-tool-btn"
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
        formatId="properties"
        backend={PROPS_BE}
        docId={propertiesStudioStore.docId}
        parseError={propertiesStudioStore.parseError}
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
        ariaLabel=".properties tree"
        errorMessage="Dotted-key conflicts — switch to Errors or fix the text."
      >
        {#snippet rowContent({ node }: RowSnippetCtx<any>)}
          {@const n = node as TNode}
          {@const ty = studioSchema.typeAtPath(n.path)}
          {@const namedType = studioSchema.namedTypeAt(n.path)}
          <StudioKindBadge label={kindBadge(n.kind)} tone={kindTone(n.kind)} italic={n.kind === 'null'} tooltip={n.kind} />
          {#if n.key === '$value'}
            <span class="ps-row-key ps-row-key-self"
                  use:tooltip={'Value at the parent prefix — `.properties` allows a key to be both a leaf and a sub-key prefix.'}>(self)</span>
          {:else}
            <span class="ps-row-key" class:ps-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          {/if}
          <span class="ps-row-sep">=</span>
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
            {:else}
              <StudioInlineEdit
                mode="input"
                bind:value={editPipeline.editBuf}
                bind:inputEl={editPipeline.editInlineEl}
                placeholder={n.kind === 'null' ? 'Type a value…' : undefined}
                onkeydown={(e) => editPipeline.onEditKey(e, n)}
                errorMsg={editPipeline.editError}
              />
            {/if}
          {:else}
            {@const xrefs = crossRefs.crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="ps-row-preview ps-row-preview-{n.kind}"
                  class:ps-row-preview-editable={rowEditMode(n) !== null || isPromotableNull(n.kind)}
                  class:ps-row-preview-xref={hasX}
                  ondblclick={(e) => {
                    if (!rowEditMode(n) && !isPromotableNull(n.kind)) return;
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
                      : isPromotableNull(n.kind)       ? 'Double-click to fill'
                      : '')}
            >{n.preview}{#if hasX}<span class="ps-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="ps-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}
          {#if n.loading}<Loader2 size={10} class="ps-row-loader" />{/if}
          {#if namedType}
            <span class="ps-row-type-slot">
              <TypePill label={namedType} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {:else if ty && ty.kind !== 'named'}
            <span class="ps-row-type-slot">
              <TypePill label={studioSchema.fmtType(ty)} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textDiff.textBuf}
        language="properties"
        oninput={textDiff.onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="properties"
        backend={PROPS_BE}
        docId={propertiesStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={propertiesStudioStore.current}
        refreshTick={textDiff.diffRefreshTick}
        bind:treeChangeCount={textDiff.diffTreeChangeCount}
        bind:hunkCount={textDiff.diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- .properties has no variant tags. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if propertiesStudioStore.parseError}
        <div class="ps-errors-wrap">
          <Alert variant="warning" title="Parse warning">
            <pre class="ps-errors-body">{propertiesStudioStore.parseError}</pre>
            <p class="ps-errors-hint">
              Dotted-key conflicts happen when the same prefix is used as
              both a leaf and a container (e.g. <code>foo=string</code> and
              <code>foo.sub=value</code>). The tree falls back to a flat
              view so every key stays editable. Resolve by renaming one of
              the colliding keys.
            </p>
          </Alert>
        </div>
      {:else}
        <StateBlock tone="success" label="No parse warnings." />
      {/if}
    {/if}
  {/snippet}

  {#snippet inspectorSidecar()}
    <StudioInspectorPanel
      bind:this={inspectorPanel}
      formatId="properties"
      backend={PROPS_BE}
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
      isDefinitionNode={((n: TNode) => n.kind === 'string' && n.preview.length > 0) as any}
      definitionValue={((n: TNode) => n.kind === 'string' && n.preview ? n.preview : null) as any}
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
    <PanelShell title="Query results" count={queryBarCtl.queryHits.length} class="ps-query-shell">
      {#snippet icon()}<ListFilter size={13} />{/snippet}
    <div class="ps-query-pane-body">
      {#if !queryBarCtl.query.trim()}
        <p class="ps-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if queryBarCtl.querying && queryBarCtl.queryHits.length === 0}
        <div class="ps-query-pane-status"><Spinner size="xs" /> <span>Running query…</span></div>
      {:else if queryBarCtl.queryError}
        <div class="ps-query-pane-error"><AlertCircle size={11} /> {queryBarCtl.queryError}</div>
      {:else if queryBarCtl.queryHits.length === 0}
        <p class="ps-query-pane-empty">No matches.</p>
      {:else}
        <div class="ps-query-pane-list">
          {#each queryBarCtl.queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="ps-query-pane-card"
              class:active={i === queryBarCtl.currentHitIdx}
              onclick={() => { queryBarCtl.currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="ps-query-pane-card-head">
                <StudioKindBadge label={kindBadge(hit.kind)} tone={kindTone(hit.kind)} italic={hit.kind === 'null'} tooltip={hit.kind} />
                <span class="ps-query-pane-card-idx">#{i + 1}</span>
              </div>
              <div class="ps-query-pane-card-path">{hit.path.length === 0 ? '$' : '$.' + hit.path.join('.')}</div>
              {#if hit.preview}<div class="ps-query-pane-card-preview">{hit.preview}</div>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    </PanelShell>
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="properties"
      backend={PROPS_BE}
      sourcePath={propertiesStudioStore.sourcePath}
      onOpenDefinition={crossRefs.openDefinition}
    >
      {#snippet emptyState()}
        <p class="ps-bindings-empty">
          Every flat dotted key is a cross-ref target; every value is a
          potential reference. The sidecar lists every key whose value
          matches another file's key.
        </p>
      {/snippet}
    </StudioRefsPanel>
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId="properties"
      backend={PROPS_BE}
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
      pickerButtonLabel="Pick schema file"
    >
      {#snippet intro()}
        <p class="ps-schema-hint">
          Pick a JSON Schema file (<code>*.schema.json</code> or
          <code>*.json</code> with a <code>$schema</code> keyword) to
          decorate this <code>.properties</code> document. Properties
          Studio surfaces every <code>$defs</code> entry as a root
          candidate.
        </p>
      {/snippet}
    </StudioSchemaPanel>
  {/snippet}

  {#snippet auxiliary()}
    {#if saveFlow.savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save .properties document as"
        extensions={['properties']}
        initialPath={propertiesStudioStore.sourcePath ?? undefined}
        initialFilename={jsBasename(propertiesStudioStore.sourcePath) || 'application.properties'}
        onConfirm={saveFlow.onSaveAsPicked}
        onCancel={() => saveFlow.savePickerOpen = false}
      />
    {/if}

    {#if renameBulk.renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={PROPS_BE}
        tabId={tabsStore.activeTabId}
        formatLabel=".properties"
        oldValue={renameBulk.renameModalState.oldValue}
        openDocs={renameBulk.buildRenameOpenDocs()}
        onClose={renameBulk.closeRenameModal}
        onApplied={renameBulk.onRenameApplied}
      />
    {/if}

    {#if renameBulk.bulkEditModalState && tabsStore.activeTabId && propertiesStudioStore.docId}
      <StudioBulkEditModal
        backend={PROPS_BE}
        tabId={tabsStore.activeTabId}
        docId={propertiesStudioStore.docId}
        formatLabel=".properties"
        query={renameBulk.bulkEditModalState.query}
        nullPolicy="ask_user"
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
  .ps-header-icon-wrap { display: inline-flex; align-items: center; color: var(--accent); flex-shrink: 0; }
  .ps-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    max-width: 50%;
  }
  .ps-dirty { color: var(--accent); font-size: 14px; margin-left: 4px; line-height: 1; }
  .ps-meta {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 2px 6px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .ps-spacer { flex: 1; }

  .ps-footer-path-pill {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 2px 6px;
    border-radius: 999px;
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ps-banner-wrap { padding: 6px 12px 0 12px; }

  .ps-row-key {
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11px;
    white-space: nowrap;
  }
  .ps-row-key-index { color: var(--text-muted); font-style: italic; }
  .ps-row-key-self {
    color: var(--accent);
    font-style: italic;
    font-size: 10.5px;
    opacity: 0.85;
  }
  .ps-footer-path-self {
    color: var(--accent);
    font-style: italic;
    margin-left: 1px;
  }
  .ps-row-sep { color: var(--text-muted); font-family: var(--font-code); font-size: 11px; margin: 0 4px; }
  .ps-row-preview {
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }
  .ps-row-preview-string { color: var(--syntax-string, #6a9956); }
  .ps-row-preview-null   { color: var(--text-muted); font-style: italic; }
  .ps-row-loader { color: var(--text-muted); flex-shrink: 0; }

  .ps-row-preview-editable { cursor: text; }

  /* Errors view — Alert wrapper + pre/hint styling. */
  .ps-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .ps-errors-body {
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
  .ps-errors-hint { color: var(--text-muted); font-size: 11px; margin: 6px 0 0; line-height: 1.5; }
  .ps-errors-hint code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }

  .ps-query-pane-body { padding: 8px; overflow: auto; height: 100%; }
  .ps-query-pane-empty,
  .ps-query-pane-status,
  .ps-query-pane-error {
    color: var(--text-muted);
    font-size: 11px;
    padding: 8px;
    margin: 0;
    line-height: 1.5;
  }
  .ps-query-pane-error { color: var(--text-error, #ff6c5c); display: inline-flex; align-items: center; gap: 4px; }
  .ps-query-pane-list { display: flex; flex-direction: column; gap: 4px; }
  .ps-query-pane-card {
    display: flex; flex-direction: column; gap: 2px;
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
  .ps-query-pane-card:hover { background: var(--bg-hover); }
  .ps-query-pane-card.active { border-color: var(--accent); background: var(--bg-hover); }
  .ps-query-pane-card-head { display: flex; align-items: center; gap: 6px; }
  .ps-query-pane-card-idx { color: var(--text-muted); }
  .ps-query-pane-card-path { color: var(--text-primary); }
  .ps-query-pane-card-preview { color: var(--text-secondary); }

  .ps-query-tool-btn {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 8px; height: 22px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border-radius: 4px;
    font-size: 10px;
    cursor: pointer;
  }
  .ps-query-tool-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .ps-query-tool-btn:disabled { color: var(--text-disabled); cursor: not-allowed; }
  .ps-bindings-empty {
    color: var(--text-muted);
    font-size: 11px;
    padding: 12px;
    margin: 0;
    line-height: 1.5;
  }
  :global(.ps-query-spinner) { animation: ps-spin 1s linear infinite; }
  @keyframes ps-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  .ps-schema-hint { color: var(--text-secondary); font-size: 11px; line-height: 1.5; margin: 0; }
  .ps-schema-hint code {
    font-family: var(--font-code);
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 4px;
    border-radius: 3px;
  }

  .ps-row-type-slot {
    margin-left: auto;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
  }

  .ps-row-preview-xref { cursor: pointer; }
  .ps-row-xref {
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
  .ps-row-preview-xref:hover .ps-row-xref {
    opacity: 1;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .ps-row-xref-count {
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 700;
    color: var(--accent);
  }

</style>
