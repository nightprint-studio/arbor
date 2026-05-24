<!--
  YamlStudioModal — YAML wrapper around the generic `<StudioModal>` shell.

  Phase 5.a scaffold + 5.b lossless edit + Phase 5.c cross-refs, F12
  rename, F13 query-driven bulk edit and JSON Schema sidecar panel.

  Capabilities exposed to the user:
    · Tree view with mutations (edit primitives, remove, add field/item,
      duplicate, move). F2 inline edit for primitive scalars; null leaves
      route through `replace_at` so the user can promote them by typing.
    · Text view via `<StudioTextPane>` (CodeMirror 6 + YAML stream
      parser) — typing pushes through the host's lossless `yaml_edit`
      editor.
    · Diff view via `<StudioDiffPane>` (text + tree sub-views).
    · Errors view — inline banner with the host's parse error.
    · Inspector + Query + Bindings + Schema right-rail panes.
    · Query bar with F13 `[⚡ Edit]` entry point (descriptor flag).
    · F12 cross-ref rename modal — context-menu "Rename across project…".
    · Schema-aware tree decoration: type chips, named-type chip, ↗
      cross-ref chip with Ctrl+click jump.
    · YAML ↔ .properties converter (5.b extension) in the footer.
    · Save + Save As via the standard `<FilePickerModal>` flow.

  YAML supports first-class null (`null_handling = Native`), so the
  bulk-edit modal uses `nullPolicy = "as_null"`. Schema sources are
  JSON Schema only — the BE delegates to `crate::json_studio::schema`.

  FROZEN F9 update for 5.b: YAML edits are lossless. Comments + anchors
  + quote style survive round-trip via `yaml_edit`.
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
    ArrowLeftRight,
    Wrench,
  } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import yamlIcon from '@iconify-icons/vscode-icons/file-type-yaml';
  import Spinner from './ui/Spinner.svelte';
  import PanelShell from './ui/PanelShell.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import { type MenuItem } from './ContextMenu.svelte';
  import { type RowSnippetCtx } from './ui/Tree.svelte';
  import Dropdown from './ui/Dropdown.svelte';
  import { type TabItem } from './ui/Tabs.svelte';
  import Alert from './ui/Alert.svelte';
  import StateBlock from './ui/StateBlock.svelte';
  import TypePill from './internal/TypePill.svelte';
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
  import StudioConvertPreviewModal from './studio/StudioConvertPreviewModal.svelte';
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
  import { invoke } from '@tauri-apps/api/core';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { yamlStudioStore, type YamlNodeKind } from '$lib/stores/yaml-studio.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import {
    studioBackend,
    type StudioNodeView, type StudioPrimitiveValue,
  } from '$lib/ipc/studio-format';
  // Shared schema-aware walker — handles serde rename / alias /
  // rename_all / flatten (incl. HashMap<String,V> catch-all flatten
  // common in Spring Boot configs).
  import {
    typeAtPath as walkTypeAtPath,
    flattenedStructFields,
  } from '$lib/utils/studio-schema';

  /** Pre-bound YAML backend. */
  const YAML_BE = studioBackend<YamlNodeKind>('yaml');

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'query' | 'bindings' | 'schema' | 'tools' | null;

  let viewMode = $state<ViewMode>('tree');

  const RIGHT_PANE_KEY = 'arbor:yaml-studio:right-pane';
  function loadRightPane(): RightPane {
    if (typeof localStorage === 'undefined') return 'inspector';
    const v = localStorage.getItem(RIGHT_PANE_KEY) as RightPane;
    return v === 'inspector' || v === 'query' || v === 'bindings' || v === 'schema'
      ? v : 'inspector';
  }
  let rightPane = $state<RightPane>(loadRightPane());

  let studioModal: StudioModal<YamlNodeKind> | undefined = $state();
  let treePane:    StudioTreePaneController<YamlNodeKind, TNode> | undefined = $state();
  let diffPane:    StudioDiffPaneController | undefined = $state();
  let inspectorPanel: StudioInspectorPanelController | undefined = $state();

  function setRightPane(p: RightPane) { studioModal?.setRightPane(p); }

  // ── Tree state ─────────────────────────────────────────────────────────
  type YamlNodeView = StudioNodeView<YamlNodeKind>;
  type TNode = YamlNodeView & {
    pid:      string;
    children: TNode[] | null;
    loading?: boolean;
  };

  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: YamlNodeView): TNode { return { ...v, pid: pathId(v.path), children: null }; }
  /** YAML preserves source order — hand rows back as-is. */
  function sortChildren(_parentKind: YamlNodeKind, kids: TNode[]): TNode[] { return kids; }
  function isContainerKind(k: YamlNodeKind): boolean {
    return k === 'object' || k === 'array';
  }
  function isEditablePrimitive(k: YamlNodeKind): boolean {
    return k === 'string' || k === 'integer' || k === 'float' || k === 'bool';
  }
  function isPromotableNull(k: YamlNodeKind): boolean { return k === 'null'; }

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

  const editPipeline = useStudioEditPipeline<YamlNodeKind, TNode>({
    formatId: 'yaml',
    isEditablePrimitive,
    isPromotableNull,
    rowEditMode:   (n) => rowEditMode(n),
    currentVariantTag,
    computeSeed: (n) => {
      let seed = valueText ?? n.preview;
      if (n.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
        try { seed = JSON.parse(seed) as string; }
        catch { seed = seed.slice(1, -1); }
      }
      if (n.kind === 'null') seed = '';
      return seed;
    },
    commit: async (node, draft) => {
      // YAML's `null` leaf — route through `replace_at` parsing the buf
      // as a YAML snippet. Empty draft = keep as null.
      if (node.kind === 'null') {
        const snippet = draft.trim().length === 0 ? 'null' : draft;
        try {
          await yamlStudioStore.replaceAt(node.path, snippet);
          await refreshAfterMutation(node, /* structural */ true);
          return {};
        } catch (e: any) { return { error: e?.message ?? String(e) }; }
      }

      // Schema-aware numeric narrowing (parity with TOML 4.c.b.2).
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
              const s = draft.trim();
              const n = Number(s);
              if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
              value = { type: 'int', value: Math.trunc(n) };
              break;
            }
            if (wantFloat) {
              const s = draft.trim();
              const n = Number(s);
              if (!Number.isFinite(n)) throw new Error('schema: expected number');
              value = { type: 'float', value: n };
              break;
            }
            value = { type: 'string', value: draft };
            break;
          case 'bool': {
            const t = draft.trim().toLowerCase();
            if (t === 'true' || t === 'yes' || t === 'on')  { value = { type: 'bool', value: true };  break; }
            if (t === 'false' || t === 'no' || t === 'off') { value = { type: 'bool', value: false }; break; }
            throw new Error('expected "true" or "false"');
          }
          case 'integer': {
            const s = draft.trim();
            const n = Number(s);
            if (!Number.isFinite(n)) throw new Error('not an integer');
            if (wantFloat)  { value = { type: 'float',  value: n      }; break; }
            if (wantString) { value = { type: 'string', value: draft  }; break; }
            if (!Number.isInteger(n)) throw new Error('not an integer');
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
        await yamlStudioStore.mutatePrimitive(node.path, value);
        await refreshAfterMutation(node, /* structural */ false);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    commitVariant: async (node, tag) => {
      try {
        await yamlStudioStore.mutatePrimitive(node.path, { type: 'string', value: tag });
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
      await yamlStudioStore.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('yaml-studio: removeAt failed', e);
    }
  }

  // ── Container mutations ────────────────────────────────────────────────
  async function addItemAction(parent: TNode): Promise<void> {
    if (parent.kind !== 'array') return;
    try {
      await yamlStudioStore.insertItem(parent.path, 'null');
      await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('yaml-studio: insertItem failed', e);
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
      await yamlStudioStore.insertField(parent.path, key, 'null');
      await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('yaml-studio: insertField failed', e);
    }
  }

  async function duplicateAction(node: TNode): Promise<void> {
    if (!isRemovable(node)) return;
    try {
      await yamlStudioStore.duplicateAt(node.path);
      const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('yaml-studio: duplicateAt failed', e);
    }
  }

  async function moveAction(node: TNode, delta: number): Promise<void> {
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent || parent.kind !== 'array') return;
    try {
      await yamlStudioStore.moveItem(node.path, delta);
      await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('yaml-studio: moveItem failed', e);
    }
  }

  async function pasteOverAction(node: TNode): Promise<void> {
    let text: string;
    try { text = await navigator.clipboard.readText(); }
    catch { uiStore.showToast('Clipboard read denied', 'error'); return; }
    const t = text.trim();
    if (!t) { uiStore.showToast('Clipboard is empty', 'error'); return; }
    try {
      await yamlStudioStore.replaceAt(node.path, t);
      await refreshAfterMutation(node, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e: any) {
      uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error');
    }
  }

  // ── Cross-refs + F12 rename + F13 bulk edit ───────────────────────────
  //
  // YAML follows the id/name + *_id/*_ref convention; reference-field
  // patterns can be overridden per-binding via the shared `studioStore`
  // (driven by `.arbor/studio.toml`).
  //
  // YAML has first-class null (`null_handling = Native`), so the
  // bulk-edit modal uses `nullPolicy = "as_null"`.

  const crossRefs = useStudioCrossRefs<YamlNodeKind, TNode>({
    formatId: 'yaml',
    getSourcePath: () => yamlStudioStore.sourcePath,
    jumpToPath: async (path) => { await treePane?.jumpToPath(path); },
    openExternalDoc: async (absPath, path) => {
      await yamlStudioStore.openDoc({ path: absPath });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(path);
    },
  });

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = yamlStudioStore.sourcePath;
    if (!path) return;
    const title = yamlStudioStore.title;
    await yamlStudioStore.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  const renameBulk = useStudioRenameBulkPipeline<TNode>({
    formatId:        'yaml',
    formatLabel:     'YAML',
    getDocId:        () => yamlStudioStore.docId,
    getSourcePath:   () => yamlStudioStore.sourcePath,
    getDirty:        () => yamlStudioStore.dirty,
    getActiveTabId:  () => tabsStore.activeTabId,
    extractRenameValue: (n) => crossRefs.unquotedString(n.preview),
    reloadAfterDiskWrite: async () => { await reloadActiveDocFromDisk(); },
    applyExternalActiveDocState: async (state) => {
      await yamlStudioStore.applyExternalMutate(state);
      await treePane?.reloadTree();
    },
  });

  // ── Schema sidecar + walker + chips + Inspector adapters ──────────────
  // Owned by `useStudioSchema`. JSON Schema only — the BE delegates to
  // `crate::json_studio::schema`. The shared walker handles serde
  // rename / alias / rename_all / flatten (incl. HashMap<String,V>
  // catch-all flatten common in Spring Boot configs).

  const studioSchema = useStudioSchema<YamlNodeKind, TNode>({
    backend: YAML_BE,
    getSchemaHint: () => yamlStudioStore.schemaHint,
    walkType: walkTypeAtPath,
    flattenedFields: flattenedStructFields,
    cssPrefix: 'ys',
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
  function noopOption(): Promise<void> | void { /* YAML has no Option */ }

  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || selectedNode.kind !== 'string') return;
    const current = currentVariantTag(selectedNode);
    if (!name || name === current) return;
    const node = selectedNode;
    try {
      await yamlStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editPipeline.setEditError(e?.message ?? String(e));
    }
  }

  // ── Context menu ───────────────────────────────────────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({ id: 'copy-path',  label: 'Copy path',         icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: 'Copy value (YAML)', icon: Copy,     iconColor: 'var(--text-muted)' });

    const editMode = rowEditMode(node);
    if (editMode === 'variant') {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
      items.push({ id: 'edit-variant', label: 'Change variant…', icon: Replace, iconColor: '#ffc66d', shortcut: 'F2' });
    } else if (editMode === 'primitive') {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
      items.push({ id: 'edit', label: 'Edit value', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
    } else if (isPromotableNull(node.kind)) {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
      items.push({ id: 'edit', label: 'Promote null…', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
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
    items.push({ id: 'paste', label: 'Paste YAML over value…', icon: ClipboardPaste, iconColor: 'var(--text-muted)' });

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
          const v = await YAML_BE.getValue(yamlStudioStore.docId!, node.path);
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
  const queryBarCtl = useStudioQueryBar<YamlNodeKind>({
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
    getStoreCurrent: () => yamlStudioStore.current,
    setText:         (text) => yamlStudioStore.setText(text),
    reloadTree:      async () => { await treePane?.reloadTree(); },
  });
  /** Local alias so the body still reads `bumpDiffRefresh()` everywhere. */
  function bumpDiffRefresh() { textDiff.bumpDiffRefresh(); }

  $effect(() => {
    const id = yamlStudioStore.docId;
    if (!id) {
      queryBarCtl.resetForDocClose();
      editPipeline.cancelEdit();
      return;
    }
    viewMode = 'tree';
  });

  // Cross-ref index — load on modal open + every active-tab change.
  $effect(() => {
    if (!yamlStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, 'yaml'); });
  });

  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: textDiff.diffTreeChangeCount > 0 ? textDiff.diffTreeChangeCount
           : textDiff.diffHunkCount > 0       ? textDiff.diffHunkCount
           : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse errors',
      disabled: !yamlStudioStore.parseError,
      badge: yamlStudioStore.parseError ? '!' : undefined,
      data: { errorBadge: !!yamlStudioStore.parseError } },
  ]);

  // ── Indent + Format ────────────────────────────────────────────────────
  let indentUnit = $state<string>('  ');
  let actionBusy = $state(false);
  let actionError = $state<string | null>(null);
  $effect(() => {
    const id = yamlStudioStore.docId;
    if (!id) return;
    void YAML_BE.getIndent(id).then(s => { if (s) indentUnit = s; }).catch(() => {});
  });

  // ── Footer snapshot (consumed by shared StudioFooter* components) ──────
  const footerDoc: StudioFooterDoc = $derived({
    parseError: yamlStudioStore.parseError ?? null,
    dirty:      yamlStudioStore.dirty,
    sourcePath: yamlStudioStore.sourcePath ?? null,
    encoding:   yamlStudioStore.docId ? yamlStudioStore.encoding : null,
    canUndo:    yamlStudioStore.canUndo,
    canRedo:    yamlStudioStore.canRedo,
    docId:      yamlStudioStore.docId ?? null,
  });
  const selectedFooterPath = $derived<string[] | null>(
    selectedNode && viewMode === 'tree' ? selectedNode.path : null,
  );

  async function setIndentUnit(unit: string): Promise<void> {
    indentUnit = unit;
    const id = yamlStudioStore.docId;
    if (!id) return;
    try { await YAML_BE.setIndent(id, unit); } catch (e) {
      console.warn('yaml-studio: setIndent failed', e);
    }
  }
  async function runFormat(): Promise<void> {
    const id = yamlStudioStore.docId;
    if (!id || actionBusy || yamlStudioStore.parseError) return;
    actionBusy = true; actionError = null;
    try {
      const formatted = await YAML_BE.format(id);
      await yamlStudioStore.setText(formatted);
      await treePane?.reloadTree();
      bumpDiffRefresh();
    } catch (e: any) {
      actionError = `Format failed: ${e?.message ?? e}`;
    } finally {
      actionBusy = false;
    }
  }

  // ── YAML ↔ .properties converter (Phase 5.b extension) ────────────────
  type ConvertMode = 'yaml-to-properties' | 'properties-to-yaml';
  let convertOpen   = $state(false);
  let convertMode   = $state<ConvertMode>('yaml-to-properties');
  let convertSource = $state<string>('');
  let importPickerOpen = $state(false);

  function ysBasenameNoExt(p: string | null | undefined): string {
    const base = jsBasename(p);
    const dot = base.lastIndexOf('.');
    return dot > 0 ? base.slice(0, dot) : base;
  }

  function openConvertToProperties() {
    convertMode   = 'yaml-to-properties';
    convertSource = yamlStudioStore.current;
    convertOpen   = true;
  }
  function openImportProperties() { importPickerOpen = true; }
  async function onImportPicked(p: string) {
    importPickerOpen = false;
    try {
      const text = await invoke<string>('fs_read_text_file', { path: p });
      convertMode   = 'properties-to-yaml';
      convertSource = text;
      convertOpen   = true;
    } catch (e: any) {
      actionError = `Read .properties failed: ${e?.message ?? e}`;
    }
  }
  function closeConvert() { convertOpen = false; }
  function convertReplaceHandler() {
    if (convertMode === 'properties-to-yaml') {
      return async (text: string) => {
        await yamlStudioStore.setText(text);
        await treePane?.reloadTree();
        bumpDiffRefresh();
      };
    }
    return null;
  }

  // ── Save / Save As ─────────────────────────────────────────────────────
  const saveFlow = useStudioSaveFlow({
    getSourcePath: () => yamlStudioStore.sourcePath,
    save:          (opts) => yamlStudioStore.save(opts),
    onSaved:       bumpDiffRefresh,
  });

  // ── Misc ───────────────────────────────────────────────────────────────
  async function close() {
    textDiff.cancelPendingTextPush();
    await yamlStudioStore.closeDoc();
  }

  // fmtBytes / jsBasename moved to shared/studio/helpers.ts.
  const fmtBytes   = fsFmtBytes;
  const jsBasename = fsBasename;

  function kindBadge(k: YamlNodeKind): string {
    switch (k) {
      case 'object':  return '{}';
      case 'array':   return '[]';
      case 'string':  return '“';
      case 'integer': return '#';
      case 'float':   return '⊘';
      case 'bool':    return '✓';
      case 'null':    return '∅';
    }
  }
  function kindTone(k: YamlNodeKind): StudioKindTone {
    switch (k) {
      case 'object':
      case 'array':   return 'type';
      case 'string':  return 'string';
      case 'integer':
      case 'float':   return 'number';
      case 'bool':    return 'keyword';
      case 'null':    return 'muted';
    }
  }
  function isBoolKind(k: YamlNodeKind): boolean { return k === 'bool'; }

  const { doUndo, doRedo } = useStudioUndoRedo({
    undo: () => yamlStudioStore.undo(),
    redo: () => yamlStudioStore.redo(),
    reloadTree: async () => { await treePane?.reloadTree(); },
    bumpDiffRefresh,
  });

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  const { onKey } = useStudioGlobalKeys<YamlNodeKind, TNode>({
    isOpen:        () => yamlStudioStore.open,
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
  formatId="yaml"
  backend={YAML_BE}
  open={yamlStudioStore.open}
  loading={yamlStudioStore.loading}
  loadingLabel="Opening YAML document…"
  errorState={yamlStudioStore.error}
  parseError={yamlStudioStore.parseError}
  hasDoc={!!yamlStudioStore.docId}
  viewItems={viewItems}
  bind:viewMode
  bind:rightPane
  rightPaneStorageKey={RIGHT_PANE_KEY}
  ariaLabel="YAML Studio"
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
      tooltip="Tools — Format / Indent / Convert"
      label="Tools"
      onClick={() => studioModal?.toggleRightPane('tools')}
    />
  {/snippet}

  {#snippet headerLeft()}
    <span class="ys-header-icon-wrap" aria-hidden="true">
      <Icon icon={yamlIcon} width={18} height={18} />
    </span>
    <StudioHeaderUndoRedo doc={footerDoc} onUndo={doUndo} onRedo={doRedo} />
    <span class="ys-title" use:tooltip={yamlStudioStore.sourcePath ?? ''}>
      {yamlStudioStore.title ?? 'YAML Studio'}
      {#if yamlStudioStore.dirty}<span class="ys-dirty" use:tooltip={'Unsaved changes'}>●</span>{/if}
    </span>
    {#if yamlStudioStore.sizeBytes != null}
      <span class="ys-meta">{fmtBytes(yamlStudioStore.sizeBytes)}</span>
    {/if}
    <div class="ys-spacer"></div>
  {/snippet}

  {#snippet footerStatusLeft()}
    <StudioFooterStatus doc={footerDoc} selectedPath={selectedFooterPath} />
  {/snippet}

  {#snippet toolsSidecar()}
    <StudioToolsSidebar
      doc={footerDoc}
      {actionBusy}
      {indentUnit}
      indentTooltip="Indent — informational; yaml_edit preserves the per-doc style on edit"
      formatTooltip="Format — re-emit the YAML through yaml_edit (preserves comments)"
      onSetIndent={setIndentUnit}
      onFormat={runFormat}
    >
      {#snippet extras()}
        <div class="sts-row">
          <div class="sts-row-label">Convert</div>
          <Dropdown items={[
            { kind: 'item', id: 'yaml-to-properties',
              label: 'Convert → .properties…',
              onclick: openConvertToProperties,
              disabled: !!yamlStudioStore.parseError || !yamlStudioStore.docId },
            { kind: 'item', id: 'properties-to-yaml',
              label: 'Import .properties → YAML…',
              onclick: openImportProperties },
          ]} position="fixed" direction="down">
            {#snippet trigger({ toggle })}
              <button type="button" class="sts-btn" onclick={toggle}
                use:tooltip={'YAML ↔ .properties bridge'}>
                <ArrowLeftRight size={13} />
                <span>YAML ↔ .properties</span>
              </button>
            {/snippet}
          </Dropdown>
        </div>
      {/snippet}
    </StudioToolsSidebar>
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
      formatId="yaml"
      backend={YAML_BE}
      docId={yamlStudioStore.docId}
      visible={viewMode === 'tree' && !yamlStudioStore.parseError}
      placeholder='Query — name (recursive), $.servers[0], $..port, …'
      historyStorageKey="arbor:yaml-studio:query-history"
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
        <button type="button" class="ys-query-tool-btn"
          onclick={() => void treePane?.expandAll()}
          disabled={expandAllBusy}
          use:tooltip={'Recursively load + expand every container'}
          aria-label="Expand all"
        >{#if expandAllBusy}<Loader2 size={12} class="ys-query-spinner" />{:else}<ChevronsDown size={12} />{/if}<span>Expand</span></button>
        <button type="button" class="ys-query-tool-btn"
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
        formatId="yaml"
        backend={YAML_BE}
        docId={yamlStudioStore.docId}
        parseError={yamlStudioStore.parseError}
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
        ariaLabel="YAML document tree"
        errorMessage="YAML document doesn't parse — switch to Errors or fix the text."
      >
        {#snippet rowContent({ node }: RowSnippetCtx<any>)}
          {@const n = node as TNode}
          {@const ty = studioSchema.typeAtPath(n.path)}
          {@const namedType = studioSchema.namedTypeAt(n.path)}
          <StudioKindBadge label={kindBadge(n.kind)} tone={kindTone(n.kind)} italic={n.kind === 'null'} tooltip={n.kind} />
          <span class="ys-row-key" class:ys-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          <span class="ys-row-sep">:</span>
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
                placeholder={n.kind === 'null' ? 'Type a YAML value…' : undefined}
                onkeydown={(e) => editPipeline.onEditKey(e, n)}
                errorMsg={editPipeline.editError}
              />
            {/if}
          {:else}
            {@const xrefs = crossRefs.crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="ys-row-preview ys-row-preview-{n.kind}"
                  class:ys-row-preview-editable={rowEditMode(n) !== null || isPromotableNull(n.kind)}
                  class:ys-row-preview-xref={hasX}
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
                      : isPromotableNull(n.kind)       ? 'Double-click to promote null'
                      : '')}
            >{n.preview}{#if hasX}<span class="ys-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="ys-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}
          {#if n.loading}<Loader2 size={10} class="ys-row-loader" />{/if}
          {#if namedType}
            <span class="ys-row-type-slot">
              <TypePill label={namedType} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {:else if ty && ty.kind !== 'named'}
            <span class="ys-row-type-slot">
              <TypePill label={studioSchema.fmtType(ty)} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textDiff.textBuf}
        language="yaml"
        oninput={textDiff.onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="yaml"
        backend={YAML_BE}
        docId={yamlStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={yamlStudioStore.current}
        refreshTick={textDiff.diffRefreshTick}
        bind:treeChangeCount={textDiff.diffTreeChangeCount}
        bind:hunkCount={textDiff.diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- YAML has no variant tags. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if yamlStudioStore.parseError}
        <div class="ys-errors-wrap">
          <Alert variant="error" title="YAML parse error">
            <pre class="ys-errors-body">{yamlStudioStore.parseError}</pre>
            <p class="ys-errors-hint">
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
      formatId="yaml"
      backend={YAML_BE}
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
    <PanelShell title="Query results" count={queryBarCtl.queryHits.length} class="ys-query-shell">
      {#snippet icon()}<ListFilter size={13} />{/snippet}
    <div class="ys-query-pane-body">
      {#if !queryBarCtl.query.trim()}
        <p class="ys-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if queryBarCtl.querying && queryBarCtl.queryHits.length === 0}
        <div class="ys-query-pane-status">
          <Spinner size="xs" /> <span>Running query…</span>
        </div>
      {:else if queryBarCtl.queryError}
        <div class="ys-query-pane-error">
          <AlertCircle size={11} /> {queryBarCtl.queryError}
        </div>
      {:else if queryBarCtl.queryHits.length === 0}
        <p class="ys-query-pane-empty">No matches.</p>
      {:else}
        <div class="ys-query-pane-list">
          {#each queryBarCtl.queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="ys-query-pane-card"
              class:active={i === queryBarCtl.currentHitIdx}
              onclick={() => { queryBarCtl.currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="ys-query-pane-card-head">
                <StudioKindBadge label={kindBadge(hit.kind)} tone={kindTone(hit.kind)} italic={hit.kind === 'null'} tooltip={hit.kind} />
                <span class="ys-query-pane-card-idx">#{i + 1}</span>
              </div>
              <div class="ys-query-pane-card-path">{hit.path.length === 0 ? '$' : '$.' + hit.path.join('.')}</div>
              {#if hit.preview}
                <div class="ys-query-pane-card-preview">{hit.preview}</div>
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
      formatId="yaml"
      backend={YAML_BE}
      sourcePath={yamlStudioStore.sourcePath}
      onOpenDefinition={crossRefs.openDefinition}
    >
      {#snippet emptyState()}
        <p class="ys-bindings-empty">
          Project-wide cross-refs follow the <code>id</code> / <code>name</code>
          convention by default. Custom reference-field patterns live in
          the repo's <code>.arbor/studio.toml</code> bindings.
        </p>
      {/snippet}
    </StudioRefsPanel>
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId="yaml"
      backend={YAML_BE}
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
        <p class="ys-schema-hint">
          Pick a JSON Schema file (<code>*.schema.json</code> or
          <code>*.json</code> with a <code>$schema</code> keyword) to
          decorate this YAML document. YAML Studio surfaces every
          <code>$defs</code> entry as a root candidate.
        </p>
      {/snippet}
    </StudioSchemaPanel>
  {/snippet}

  {#snippet auxiliary()}
    {#if saveFlow.savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save YAML document as"
        extensions={['yaml', 'yml']}
        initialPath={yamlStudioStore.sourcePath ?? undefined}
        initialFilename={jsBasename(yamlStudioStore.sourcePath) || 'document.yaml'}
        onConfirm={saveFlow.onSaveAsPicked}
        onCancel={() => saveFlow.savePickerOpen = false}
      />
    {/if}

    {#if importPickerOpen}
      <FilePickerModal
        mode="file"
        title="Pick a .properties file to convert"
        extensions={['properties']}
        onConfirm={onImportPicked}
        onCancel={() => importPickerOpen = false}
      />
    {/if}

    {#if convertOpen}
      <StudioConvertPreviewModal
        mode={convertMode}
        sourceText={convertSource}
        defaultFilename={
          convertMode === 'yaml-to-properties'
            ? `${ysBasenameNoExt(yamlStudioStore.sourcePath) || 'document'}.properties`
            : `${ysBasenameNoExt(yamlStudioStore.sourcePath) || 'document'}.yaml`
        }
        onReplace={convertReplaceHandler()}
        onClose={closeConvert}
      />
    {/if}

    {#if renameBulk.renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={YAML_BE}
        tabId={tabsStore.activeTabId}
        formatLabel="YAML"
        oldValue={renameBulk.renameModalState.oldValue}
        openDocs={renameBulk.buildRenameOpenDocs()}
        onClose={renameBulk.closeRenameModal}
        onApplied={renameBulk.onRenameApplied}
      />
    {/if}

    {#if renameBulk.bulkEditModalState && tabsStore.activeTabId && yamlStudioStore.docId}
      <StudioBulkEditModal
        backend={YAML_BE}
        tabId={tabsStore.activeTabId}
        docId={yamlStudioStore.docId}
        formatLabel="YAML"
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
  /* Header. */
  .ys-header-icon-wrap { display: inline-flex; align-items: center; color: var(--accent); flex-shrink: 0; }
  .ys-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    max-width: 50%;
  }
  .ys-dirty {
    color: var(--accent);
    font-size: 14px;
    margin-left: 4px;
    line-height: 1;
  }
  .ys-meta {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 2px 6px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .ys-spacer { flex: 1; }

  /* The Convert button moved to the left tools rail as an `.ab-btn`,
     so the local footer-btn style is gone. Shared <StudioFooter*>
     components own the rest of the footer pill / sep / path CSS. */

  /* Row content. */
  .ys-row-key {
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 11px;
    white-space: nowrap;
  }
  .ys-row-key-index {
    color: var(--text-muted);
    font-style: italic;
  }
  .ys-row-sep {
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 11px;
    margin: 0 4px;
  }
  .ys-row-preview {
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }
  .ys-row-preview-string { color: var(--syntax-string, #6a9956); }
  .ys-row-preview-integer,
  .ys-row-preview-float  { color: var(--syntax-number, #9876aa); }
  .ys-row-preview-null   { color: var(--text-muted); font-style: italic; }
  .ys-row-loader { color: var(--text-muted); flex-shrink: 0; }

  .ys-row-preview-editable { cursor: text; }

  /* Errors view — Alert wrapper + pre/hint styling. */
  .ys-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .ys-errors-body {
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
  .ys-errors-hint {
    color: var(--text-muted);
    font-size: 11px;
    margin: 6px 0 0;
  }

  .ys-query-pane-body {
    padding: 8px;
    overflow: auto;
    height: 100%;
  }
  .ys-query-pane-empty,
  .ys-query-pane-status,
  .ys-query-pane-error {
    color: var(--text-muted);
    font-size: 11px;
    padding: 8px;
    margin: 0;
    line-height: 1.5;
  }
  .ys-query-pane-error { color: var(--text-error, #ff6c5c); display: inline-flex; align-items: center; gap: 4px; }
  .ys-query-pane-list { display: flex; flex-direction: column; gap: 4px; }
  .ys-query-pane-card {
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
  .ys-query-pane-card:hover { background: var(--bg-hover); }
  .ys-query-pane-card.active {
    border-color: var(--accent);
    background: var(--bg-hover);
  }
  .ys-query-pane-card-head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ys-query-pane-card-idx { color: var(--text-muted); }
  .ys-query-pane-card-path { color: var(--text-primary); }
  .ys-query-pane-card-preview { color: var(--text-secondary); }

  .ys-query-tool-btn {
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
  .ys-query-tool-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .ys-query-tool-btn:disabled {
    color: var(--text-disabled);
    cursor: not-allowed;
  }
  .ys-bindings-empty {
    color: var(--text-muted);
    font-size: 11px;
    padding: 12px;
    margin: 0;
    line-height: 1.5;
  }
  .ys-bindings-empty code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .ys-query-spinner { animation: ys-spin 1s linear infinite; }
  @keyframes ys-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  /* Schema sidecar. */
  .ys-schema-hint {
    color: var(--text-secondary);
    font-size: 11px;
    line-height: 1.5;
    margin: 0;
  }
  .ys-schema-hint code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }

  /* Type chip — pinned right via the slot wrapper (TypePill itself is
     just `display: inline-flex`). */
  .ys-row-type-slot {
    margin-left: auto;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
  }

  /* Cross-ref ↗ arrow. */
  .ys-row-preview-xref { cursor: pointer; }
  .ys-row-xref {
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
  .ys-row-preview-xref:hover .ys-row-xref {
    opacity: 1;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .ys-row-xref-count {
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 700;
    color: var(--accent);
  }

</style>
