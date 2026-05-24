<!--
  RonStudioModal — RON wrapper around the generic `<StudioModal>` shell.

  This file owns the format-specific bits that don't fit any shared
  composable:
    · ron-studio + ron-studio-workspace store wiring (multi-tab open /
      close / activate / dirty / schema hint).
    · The open-tab def index that's merged on top of the on-disk
      cross-ref index for live, docId-aware Ctrl+click navigation.
    · Reference-field toggle + scope suffix (per-binding override).
    · Container mutations parameterised by RON kinds (struct vs map vs
      list vs tuple vs named_tuple).
    · Default-RON-text builders used by `Reset to default`, `Set to
      Some(default)`, `Add item`, variant picker.
    · Format / Convert (RON ↔ JSON) confirmation flow + indent
      persistence.
    · The row snippet (kind badges, named-type chip, cross-ref
      decoration, Option splitting, variant tag chip).

  Everything else — edit pipeline, schema sidecar, query bar, text/diff,
  rename + bulk-edit modals, save flow, undo/redo, global keys —
  delegates to the composables in `./studio/composables/*`.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import {
    FileCode, Copy, ListTree, FileText, AlertCircle, GitCompare,
    ChevronUp, ChevronDown,
    Link as LinkIcon, Repeat2, FileJson,
    Pencil, ToggleLeft, ToggleRight,
    Trash2, Replace, RotateCcw, ClipboardPaste,
    Maximize2, Minimize2, Plus, CopyPlus, ArrowUp, ArrowDown, ArrowUpRight,
    Link2Off,
    ScanSearch, BookOpen,
    Loader2, ChevronsDown, ChevronsUp,
    Network,
    Layers, HardDrive, ListFilter,
    Wrench,
  } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import TypePill from '$lib/components/shared/internal/TypePill.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import { type MenuItem } from './ContextMenu.svelte';
  import { type RowSnippetCtx } from './ui/Tree.svelte';
  import Dropdown, { type DropdownItem } from './ui/Dropdown.svelte';
  import Tabs, { type TabItem } from './ui/Tabs.svelte';
  import StudioModal from './studio/StudioModal.svelte';
  import StudioRightRailButton from './studio/StudioRightRailButton.svelte';
  import StudioFooterStatus    from './studio/StudioFooterStatus.svelte';
  import StudioFooterRight     from './studio/StudioFooterRight.svelte';
  import StudioHeaderUndoRedo  from './studio/StudioHeaderUndoRedo.svelte';
  import StudioToolsSidebar    from './studio/StudioToolsSidebar.svelte';
  import StudioBodyBanners     from './studio/StudioBodyBanners.svelte';
  import {
    INDENT_OPTIONS_WITH_8,
    type StudioFooterDoc,
  } from './studio/studio-footer-types';
  import { basename as fsBasename, fmtBytes as fsFmtBytes, typePillKind } from './studio/helpers';
  import StudioQueryBar from './studio/StudioQueryBar.svelte';
  import StudioTextPane from './studio/StudioTextPane.svelte';
  import StudioDiffPane, { type StudioDiffPaneController } from './studio/StudioDiffPane.svelte';
  import StudioSchemaPanel from './studio/StudioSchemaPanel.svelte';
  import StudioRefsPanel from './studio/StudioRefsPanel.svelte';
  import StudioViewSourceModal from './studio/StudioViewSourceModal.svelte';
  import StudioTreePane, { type StudioTreePaneController } from './studio/StudioTreePane.svelte';
  import StudioInspectorPanel, {
    type StudioInspectorPanelController,
  } from './studio/StudioInspectorPanel.svelte';
  import StudioRenameModal from './studio/StudioRenameModal.svelte';
  import StudioBulkEditModal from './studio/StudioBulkEditModal.svelte';
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
  import Icon from '@iconify/svelte';
  import ronIcon from '@iconify-icons/vscode-icons/file-type-ron';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { ronStudioStore } from '$lib/stores/ron-studio.svelte';
  import { ronStudioWorkspaceStore } from '$lib/stores/ron-studio-workspace.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import {
    studioBackend,
    type Schema, type TypeDef, type ResolvedType, type VariantDef,
  } from '$lib/ipc/studio-format';
  import type {
    RonNodeView, RonNodeKind, RonPrimitiveValue,
  } from '$lib/types/ron-studio';

  /** Pre-bound backend for the RON format. */
  const RON = studioBackend<RonNodeKind>('ron');

  type ViewMode  = 'tree' | 'text' | 'diff' | 'errors';
  type RightPane = 'inspector' | 'schema' | 'bindings' | 'query' | 'tools' | null;

  let viewMode = $state<ViewMode>('tree');

  const RIGHT_PANE_KEY = 'arbor:ron-studio:right-pane';
  function loadRightPane(): RightPane {
    if (typeof localStorage === 'undefined') return 'inspector';
    const v = localStorage.getItem(RIGHT_PANE_KEY) as RightPane;
    return v === 'inspector' || v === 'schema' || v === 'bindings' || v === 'query' || v === 'tools'
      ? v : 'inspector';
  }
  let rightPane = $state<RightPane>(loadRightPane());

  let studioModal: StudioModal<RonNodeKind> | undefined = $state();
  let treePane:    StudioTreePaneController<RonNodeKind, TNode> | undefined = $state();
  let diffPane:    StudioDiffPaneController | undefined = $state();
  let inspectorPanel: StudioInspectorPanelController | undefined = $state();
  void untrack(() => inspectorPanel);

  // ── Tree state ─────────────────────────────────────────────────────────
  type TNode = RonNodeView & {
    pid:      string;
    children: TNode[] | null;
    loading?: boolean;
  };

  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: RonNodeView): TNode { return { ...v, pid: pathId(v.path), children: null }; }

  /** Cosmetic field-ordering: objects → arrays → optionals → rest.
   *  Only applied to named-key parents (struct / named_struct / map). */
  function fieldOrderBucket(k: RonNodeKind): number {
    if (k === 'struct' || k === 'named_struct' || k === 'map')  return 0;
    if (k === 'list'   || k === 'tuple'        || k === 'named_tuple') return 1;
    if (k === 'option') return 2;
    return 3;
  }
  function sortChildrenForDisplay(parentKind: RonNodeKind, kids: TNode[]): TNode[] {
    if (parentKind !== 'struct' && parentKind !== 'named_struct' && parentKind !== 'map') {
      return kids;
    }
    return kids
      .map((c, i) => ({ c, i, b: fieldOrderBucket(c.kind) }))
      .sort((a, b) => a.b - b.b || a.i - b.i)
      .map(x => x.c);
  }

  function isContainerKind(k: RonNodeKind): boolean {
    return k === 'struct' || k === 'named_struct'
        || k === 'tuple'  || k === 'named_tuple'
        || k === 'map'    || k === 'list';
  }
  function isContainerKindRon(k: RonNodeKind): boolean {
    return k === 'struct' || k === 'map' || k === 'list' || k === 'tuple';
  }
  function isEditablePrimitive(k: RonNodeKind): boolean {
    return k === 'string' || k === 'number' || k === 'bool' || k === 'char';
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

  function setRightPane(p: RightPane) { studioModal?.setRightPane(p); }

  // ── Workspace tabs ─────────────────────────────────────────────────────
  $effect(() => {
    const id = ronStudioStore.docId;
    if (!id) return;
    untrack(() => {
      ronStudioWorkspaceStore.addTab({
        docId:      id,
        sourcePath: ronStudioStore.sourcePath,
        title:      ronStudioStore.title ?? 'Untitled',
        dirty:      ronStudioStore.dirty,
      });
      if (ronStudioStore.schemaHint) {
        ronStudioWorkspaceStore.setSchemaHint(id, ronStudioStore.schemaHint);
      }
    });
  });
  $effect(() => {
    const id    = ronStudioStore.docId;
    const dirty = ronStudioStore.dirty;
    if (id) ronStudioWorkspaceStore.setDirty(id, dirty);
  });

  async function activateTab(docId: string): Promise<void> {
    if (docId === ronStudioStore.docId) return;
    if (editPipeline.editingPid) {
      try { await editPipeline.maybeCommitActiveEdit(selectedNode); }
      catch { editPipeline.cancelEdit(); }
    }
    const ok = await ronStudioStore.switchTo(docId);
    if (ok) {
      ronStudioWorkspaceStore.setActive(docId);
      treePane?.resetState();
    }
  }

  async function closeTab(docId: string, e?: MouseEvent): Promise<void> {
    e?.stopPropagation();
    const wasActive = docId === ronStudioStore.docId;
    const { nextActive } = await ronStudioWorkspaceStore.closeTab(docId);
    if (wasActive) {
      if (nextActive) await activateTab(nextActive);
      else            await ronStudioStore.closeDoc();
    }
  }

  async function openFileFromWorkspace(path: string): Promise<void> {
    if (editPipeline.editingPid) {
      try { await editPipeline.maybeCommitActiveEdit(selectedNode); }
      catch { editPipeline.cancelEdit(); }
    }
    const id = await ronStudioWorkspaceStore.openFile(path);
    await activateTab(id);
  }

  /** Re-parse a single tab's source from disk and swap the docId
   *  in-place. Used after rename / bulk-edit + workspace sync. */
  async function reloadTabFromDisk(docId: string): Promise<void> {
    const tab = ronStudioWorkspaceStore.tabs.find(t => t.docId === docId);
    if (!tab || !tab.sourcePath) return;
    const path  = tab.sourcePath;
    const aTab  = tabsStore.activeTabId ?? undefined;
    const relPath = aTab && studioStore.loadedTabId === aTab
      ? studioStore.files.find(e => e.absolute_path === path)?.relative_path
      : undefined;
    try { await RON.close(docId); } catch { /* best-effort */ }
    const r = await RON.parse({ path, tabId: aTab, relativePath: relPath });
    const wasActive = ronStudioStore.docId === docId;
    ronStudioWorkspaceStore.replaceDocId(docId, r.doc_id, /* dirty */ false);
    if (wasActive) ronStudioWorkspaceStore.setActive(r.doc_id);
  }

  async function reloadTouchedTabs(written: string[]): Promise<void> {
    const writtenSet = new Set(written.map(p => p.replace(/\\/g, '/').toLowerCase()));
    const tabsToReload = ronStudioWorkspaceStore.tabs.filter(t =>
      !!t.sourcePath && writtenSet.has(t.sourcePath.replace(/\\/g, '/').toLowerCase()));
    for (const t of tabsToReload) {
      try { await reloadTabFromDisk(t.docId); }
      catch (e) { console.warn('reload tab failed for', t.sourcePath, e); }
    }
    await rebuildOpenTabDefs();
  }

  // ── Open .ron launcher (header dropdown + Browse disk picker) ──────────
  let diskFilePicking = $state(false);
  function openWorkspaceFilePicker() { diskFilePicking = true; }
  async function onDiskFilePicked(p: string) {
    diskFilePicking = false;
    await openFileFromWorkspace(p);
  }

  $effect(() => {
    if (!ronStudioStore.open) return;
    const tid = tabsStore.activeTabId;
    if (!tid) return;
    untrack(() => { void studioStore.ensureLoadedFor(tid); });
  });

  const openFileItems = $derived.by<DropdownItem[]>(() => {
    return studioStore.files
      .filter(f => f.kind === 'ron' && !f.excluded)
      .map(f => {
        const dir = f.relative_path.includes('/')
          ? f.relative_path.slice(0, f.relative_path.lastIndexOf('/'))
          : '';
        return {
          kind:     'item',
          id:       f.absolute_path,
          label:    f.name,
          subtitle: dir || undefined,
          active:   ronStudioStore.sourcePath === f.absolute_path,
          onclick:  () => { void openFileFromWorkspace(f.absolute_path); },
        } satisfies DropdownItem;
      });
  });

  /** Broken refs filtered to the currently open document. */
  const docBrokenRefs = $derived.by(() => {
    const src = ronStudioStore.sourcePath;
    if (!src) return [] as typeof studioStore.brokenRefs;
    const norm = src.replace(/\\/g, '/');
    return studioStore.brokenRefs.filter(r =>
      r.absolute_path.replace(/\\/g, '/') === norm,
    );
  });

  // ── Cross-references: open-tab index + composable + docId-aware jump ───
  //
  // RON unique-vs-shared considerations:
  //   · Live cross-refs include UNSAVED defs in open tabs ("Untitled"
  //     or dirty tabs not yet reflected in the on-disk index).
  //   · Jumps are docId-aware: if the target file is already open, we
  //     activate that tab instead of re-parsing the file.
  //
  // We use the shared `useStudioCrossRefs` for conventions (unquoting,
  // id/name def fields, *_id/*_ref refs + per-binding patterns,
  // isRenameableTreeNode, portal, picker state). The docId-aware look-
  // up + claim/dedupe live inline because they cross the
  // workspace-store boundary.

  type OpenTabDef = {
    docId:      string;
    sourcePath: string | null;
    title:      string;
    defPath:    string[];
  };
  let openTabDefs = $state<Map<string, OpenTabDef[]>>(new Map());

  const crossRefs = useStudioCrossRefs<RonNodeKind, TNode>({
    formatId: 'ron',
    getSourcePath: () => ronStudioStore.sourcePath,
    jumpToPath: async (path) => { await treePane?.jumpToPath(path); },
    openExternalDoc: async (absPath, path) => {
      const docId = await ronStudioWorkspaceStore.openFile(absPath);
      if (docId !== ronStudioStore.docId) await activateTab(docId);
      await treePane?.jumpToPath(path);
    },
    /** Fold open-tab defs in front of on-disk results so the picker
     *  shows the live tab title and can short-circuit nav via docId. */
    extraEntries: (value) => {
      const tabDefs = openTabDefs.get(value);
      if (!tabDefs || tabDefs.length === 0) return [];
      return tabDefs.map((tabDef) => ({
        sourcePath: tabDef.sourcePath ?? '',
        fileName:   tabDef.title,
        defPath:    tabDef.defPath,
        docId:      tabDef.docId,
        title:      tabDef.title,
      }));
    },
    /** When an on-disk match happens to be in the workspace's tab list,
     *  surface its docId so the picker badges it as "already open" and
     *  the jump can activate the tab. */
    enrichOnDiskEntry: (entry) => {
      const tab = ronStudioWorkspaceStore.tabs.find(t => t.sourcePath === entry.sourcePath);
      if (!tab) return entry;
      return { ...entry, docId: tab.docId, title: tab.title };
    },
    /** docId-aware fast path — used when an entry resolves to an open
     *  tab. Skips the file re-open round-trip. */
    jumpToOpenTab: async (docId, defPath) => {
      if (docId !== ronStudioStore.docId) await activateTab(docId);
      await treePane?.jumpToPath(defPath);
    },
    /** RON references can target other formats (e.g. a RON config
     *  pointing at a YAML or JSON entity). Use the project-wide
     *  cross-ref index instead of the per-kind one. */
    onDiskLookup: (value) => studioStore.findCrossRefs(value),
  });

  async function rebuildOpenTabDefs(): Promise<void> {
    const next = new Map<string, OpenTabDef[]>();
    for (const tab of ronStudioWorkspaceStore.tabs) {
      try {
        const root = await RON.getRoot(tab.docId);
        if (!root || root.child_count === 0) continue;
        const kids = await RON.getChildren(tab.docId, root.path);
        for (const c of kids) {
          if (!crossRefs.isDefinitionFieldName(c.key)) continue;
          if (c.kind !== 'string') continue;
          const val = await RON.getValue(tab.docId, c.path).catch(() => null);
          const raw = val ? crossRefs.unquotedString(val) : null;
          if (!raw) continue;
          const entry: OpenTabDef = {
            docId:      tab.docId,
            sourcePath: tab.sourcePath,
            title:      tab.title,
            defPath:    c.path,
          };
          const arr = next.get(raw);
          if (arr) arr.push(entry); else next.set(raw, [entry]);
        }
      } catch (e) {
        console.warn('cross-ref open-tab scan failed for', tab.docId, e);
      }
    }
    openTabDefs = next;
  }

  $effect(() => {
    void ronStudioWorkspaceStore.tabs.length;
    void ronStudioStore.current;
    untrack(() => { void rebuildOpenTabDefs(); });
  });
  $effect(() => {
    if (!ronStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefs(tabId); });
  });

  function isRenameableTreeNode(n: TNode): boolean {
    return crossRefs.isRenameableTreeNode(n);
  }

  // ── Reference-field toggle (per-binding override) ──────────────────────
  function refContainerFieldNameForNode(node: TNode): string | null {
    if (node.kind !== 'list' && node.kind !== 'tuple') return null;
    if (node.path.length === 0) return null;
    return node.path[node.path.length - 1];
  }
  function refFieldScopeSuffix(): string {
    const sp = ronStudioStore.sourcePath;
    if (!sp) return '';
    const repoRel = crossRefs.relPathInRepo(sp);
    if (!repoRel) return '';
    const scope = studioStore.resolveBindingScope(repoRel);
    switch (scope.kind) {
      case 'file':    return ' (this file)';
      case 'folder':  return ` (folder ${scope.folder}/)`;
      case 'glob':    return ` (binding ${scope.glob})`;
      case 'default': return ' (default — applies repo-wide)';
      case 'new':     return ' (new file binding)';
    }
  }
  function isFieldExplicitlyMarked(fieldName: string): boolean {
    const sp = ronStudioStore.sourcePath;
    if (!sp) return false;
    const repoRel = crossRefs.relPathInRepo(sp);
    if (!repoRel) return false;
    const patterns = studioStore.referenceFieldsFor(repoRel);
    return !!patterns && patterns.includes(fieldName);
  }
  async function toggleReferenceFieldForNode(fieldName: string): Promise<void> {
    const tabId = tabsStore.activeTabId;
    const sp    = ronStudioStore.sourcePath;
    if (!tabId || !sp) return;
    const repoRel = crossRefs.relPathInRepo(sp);
    if (!repoRel) {
      notificationsStore.add(
        'Reference fields',
        'This document is not inside a registered repo — open it from the Studio sidebar to manage reference fields.',
        'warning',
      );
      return;
    }
    try {
      const now = await studioStore.toggleReferenceFieldFor(tabId, repoRel, fieldName);
      await studioStore.loadCrossRefs(tabId, true);
      notificationsStore.add(
        'Reference fields',
        now ? `\`${fieldName}\` is now a reference field for this binding.`
            : `\`${fieldName}\` is no longer a reference field.`,
        'success',
      );
    } catch (e) {
      notificationsStore.add('Reference fields', `Toggle failed: ${e}`, 'error');
    }
  }

  // ── Schema sidecar (composable + RON enum-aware walker) ───────────────
  function ronWalkType(schema: Schema | null, path: string[]): ResolvedType | null {
    if (!schema) return null;
    let curTy: ResolvedType = { kind: 'named', path: schema.root_type };
    const pathSoFar: string[] = [];
    for (const seg of path) {
      curTy = stepTypeBySegment(schema, curTy, seg, pathSoFar);
      pathSoFar.push(seg);
      if (curTy.kind === 'unknown' || curTy.kind === 'external') return curTy;
    }
    return curTy;
  }
  function variantTagAt(path: string[]): string | null {
    const node = treePane?.getNode(pathId(path));
    return node?.variant_tag ?? null;
  }
  function fieldNamesAt(path: string[]): Set<string> | null {
    const node = treePane?.getNode(pathId(path));
    if (!node || !node.children) return null;
    return new Set(node.children.map(c => c.key));
  }
  function variantInnerStructFields(schema: Schema, v: VariantDef): string[] | null {
    if (v.fields.length === 0) return null;
    if (v.shape === 'struct') return v.fields.map(f => f.name);
    if (v.shape === 'tuple' && v.fields.length === 1) {
      const inner = v.fields[0].ty;
      if (inner.kind === 'named') {
        const innerDef = schema.types[inner.path];
        if (innerDef && innerDef.kind === 'struct') return innerDef.fields.map(f => f.name);
      }
    }
    return null;
  }
  function discriminateVariant(schema: Schema, def: TypeDef & { kind: 'enum' }, observed: Set<string>): VariantDef | null {
    type Score = { v: VariantDef; missing: number; extras: number };
    const scored: Score[] = [];
    for (const v of def.variants) {
      const fields = variantInnerStructFields(schema, v);
      if (!fields) continue;
      const fieldSet = new Set(fields);
      let missing = 0, extras = 0;
      for (const f of fieldSet) if (!observed.has(f)) missing++;
      for (const f of observed) if (!fieldSet.has(f)) extras++;
      scored.push({ v, missing, extras });
    }
    if (scored.length === 0) return null;
    scored.sort((a, b) => a.extras - b.extras || a.missing - b.missing);
    const best = scored[0];
    if (best.extras > 0) return null;
    return best.v;
  }
  function variantPayloadType(v: VariantDef): ResolvedType {
    if (v.fields.length === 0) return { kind: 'primitive', name: '()' };
    if (v.shape === 'tuple' && v.fields.length === 1) return v.fields[0].ty;
    return { kind: 'tuple', items: v.fields.map(f => f.ty) };
  }
  function stepIntoRonFlatten(schema: Schema, ty: ResolvedType, seg: string): ResolvedType | null {
    switch (ty.kind) {
      case 'option': return stepIntoRonFlatten(schema, ty.inner, seg);
      case 'map':    return ty.value;
      case 'named': {
        const def: TypeDef | undefined = schema.types[ty.path];
        if (!def) return null;
        if (def.kind === 'alias') return stepIntoRonFlatten(schema, def.target, seg);
        if (def.kind !== 'struct') return null;
        const direct = def.fields.find(f => f.name === seg || (f.aliases ?? []).includes(seg));
        if (direct) return direct.ty;
        for (const ff of def.fields) {
          if (!ff.flatten) continue;
          const hit = stepIntoRonFlatten(schema, ff.ty, seg);
          if (hit) return hit;
        }
        return null;
      }
      default: return null;
    }
  }
  function stepTypeBySegment(schema: Schema, ty: ResolvedType, seg: string, pathSoFar: string[]): ResolvedType {
    switch (ty.kind) {
      case 'option': {
        if (seg === 'Some') return ty.inner;
        return stepTypeBySegment(schema, ty.inner, seg, pathSoFar);
      }
      case 'vec':    return ty.inner;
      case 'map':    return ty.value;
      case 'tuple': {
        const idx = parseInt(seg, 10);
        if (!Number.isFinite(idx) || idx < 0 || idx >= ty.items.length) {
          return { kind: 'unknown', hint: `tuple index ${seg} out of range` };
        }
        return ty.items[idx];
      }
      case 'named': {
        const def: TypeDef | undefined = schema.types[ty.path];
        if (!def) return { kind: 'unknown', hint: `unresolved ${ty.path}` };
        if (def.kind === 'alias') return stepTypeBySegment(schema, def.target, seg, pathSoFar);
        if (def.kind === 'struct') {
          const direct = def.fields.find(f => f.name === seg || (f.aliases ?? []).includes(seg));
          if (direct) return direct.ty;
          for (const ff of def.fields) {
            if (!ff.flatten) continue;
            const hit = stepIntoRonFlatten(schema, ff.ty, seg);
            if (hit) return hit;
          }
          return { kind: 'unknown', hint: `unknown field "${seg}" on ${def.name}` };
        }
        const tag = variantTagAt(pathSoFar);
        if (tag) {
          const v = def.variants.find(v => v.name === tag);
          if (v) {
            if (v.shape === 'tuple') {
              const idx = parseInt(seg, 10);
              if (Number.isFinite(idx) && idx >= 0 && idx < v.fields.length) return v.fields[idx].ty;
            }
            if (v.shape === 'struct') {
              const f = v.fields.find(f => f.name === seg);
              if (f) return f.ty;
            }
          }
        }
        if (/^\d+$/.test(seg)) {
          const observed = fieldNamesAt([...pathSoFar, seg]);
          if (observed) {
            const v = discriminateVariant(schema, def, observed);
            if (v) return variantPayloadType(v);
          }
          const names = def.variants.map(v => v.name).join(' / ');
          return {
            kind: 'unknown',
            hint: `index "${seg}" on enum ${def.name} — variant tag not preserved on this node; could be ${names || '(no variants?)'}.`,
          };
        }
        const v = def.variants.find(v => v.name === seg);
        if (!v) return { kind: 'unknown', hint: `unknown variant "${seg}" on ${def.name}` };
        return variantPayloadType(v);
      }
      default:
        return { kind: 'unknown', hint: `cannot step into ${ty.kind}` };
    }
  }

  const studioSchema = useStudioSchema<RonNodeKind, TNode>({
    backend: RON,
    getSchemaHint: () => ronStudioStore.schemaHint,
    walkType: ronWalkType,
    // RON does not flatten fields when listing "missing" fields for the
    // inspector — direct struct fields only (matches the original RON
    // behaviour; flatten is rare in RON crates).
    flattenedFields: (_s, def) => def.fields.map(f => ({
      name: f.name, ty: f.ty, has_default: f.has_default, aliases: f.aliases,
    })),
    cssPrefix: 'rs',
    getSelectedChildKeys: (n) => (n.children ?? []).map((c: TNode) => c.key),
    currentVariantTag: (n) => n.variant_tag ?? '',
  });

  // Mirror schema selection onto the workspace store so re-activating
  // the tab restores the hint. The composable owns the load lifecycle;
  // the wrapper stashes after a successful load.
  $effect(() => {
    const id     = ronStudioStore.docId;
    const path   = studioSchema.schemaRsPath;
    const root   = studioSchema.schemaRootSel;
    const schema = studioSchema.schema;
    if (!id || !path || !root || !schema) return;
    untrack(() => {
      ronStudioWorkspaceStore.setSchemaHint(id, {
        rs_file:   path,
        root_type: root,
        origin:    'directive',
      });
    });
  });
  // Clear the schema when switching to a tab whose hint is null —
  // otherwise the previous tab's schema would bleed across.
  $effect(() => {
    const id   = ronStudioStore.docId;
    const hint = ronStudioStore.schemaHint;
    if (!id || hint) return;
    untrack(() => { studioSchema.clearSchema(); });
  });

  // ── Schema helpers used by the row snippet + context menu ──────────────
  function canonicalNamedTypeAt(path: string[]): string | null {
    if (!studioSchema.schema) return null;
    let ty = studioSchema.typeAtPath(path);
    if (!ty) return null;
    if (ty.kind === 'option') ty = ty.inner;
    if (ty.kind !== 'named') return null;
    return ty.path;
  }
  function namedTypeAt(path: string[]): string | null {
    const ty = studioSchema.typeAtPath(path);
    if (!ty || ty.kind !== 'named') return null;
    return ty.path.replace(/^crate::/, '').split('::').pop() ?? null;
  }
  function isTupleStructAt(path: string[]): boolean {
    if (!studioSchema.schema) return false;
    const ty = studioSchema.typeAtPath(path);
    if (!ty || ty.kind !== 'named') return false;
    const def = studioSchema.schema.types[ty.path];
    return !!def && def.kind === 'struct' && def.tuple_like;
  }
  function refCountForType(def: TypeDef): number {
    if (def.kind === 'struct') return def.fields.filter(f => isRefFieldName(f.name)).length;
    if (def.kind === 'enum') {
      let n = 0;
      for (const v of def.variants) for (const f of v.fields) {
        if (isRefFieldName(f.name)) n++;
      }
      return n;
    }
    return 0;
  }
  /** Used by the `View implementation` modal's gutter highlight. */
  function isRefFieldName(name: string): boolean {
    return crossRefs.isReferenceFieldName(name);
  }

  // ── RON-text default builders (Reset / Add item / Set Some(default)) ──
  function defaultPrimText(n: string): string {
    if (n === 'bool')                                       return 'false';
    if (n === 'String' || n === '&str' || n === 'str')      return '""';
    if (n === 'char')                                       return "' '";
    if (n === '()')                                         return '()';
    if (n.startsWith('f'))                                  return '0.0';
    return '0';
  }
  function defaultRonText(ty: ResolvedType, depth = 0): string {
    const schema = studioSchema.schema;
    if (depth > 4) return '()';
    switch (ty.kind) {
      case 'primitive': return defaultPrimText(ty.name);
      case 'option':    return 'None';
      case 'vec':       return '[]';
      case 'map':       return '{}';
      case 'tuple':     return '(' + ty.items.map(t => defaultRonText(t, depth + 1)).join(', ') + ')';
      case 'external':
      case 'unknown':   return '()';
      case 'named': {
        if (!schema) return '()';
        const def = schema.types[ty.path];
        if (!def) return '()';
        if (def.kind === 'alias')  return defaultRonText(def.target, depth + 1);
        if (def.kind === 'struct') {
          if (def.tuple_like) {
            return def.name + '(' + def.fields.map(f => defaultRonText(f.ty, depth + 1)).join(', ') + ')';
          }
          const inner = def.fields
            .filter(f => !f.has_default && !f.skip_if_default)
            .map(f => `${f.name}: ${defaultRonText(f.ty, depth + 1)}`)
            .join(', ');
          return def.name + '(' + inner + ')';
        }
        const v0 = def.variants[0];
        return v0 ? defaultRonTextForVariant(v0, depth) : '()';
      }
    }
  }
  function defaultRonTextForVariant(v: VariantDef, depth = 0): string {
    if (v.fields.length === 0) return v.name;
    if (v.shape === 'tuple') {
      const inner = v.fields.map(f => defaultRonText(f.ty, depth + 1)).join(', ');
      return v.name + '(' + inner + ')';
    }
    const inner = v.fields
      .filter(f => !f.has_default && !f.skip_if_default)
      .map(f => `${f.name}: ${defaultRonText(f.ty, depth + 1)}`)
      .join(', ');
    return v.name + '(' + inner + ')';
  }

  // ── Editing: variant detection + edit pipeline + commit dispatch ──────
  function rowEditMode(node: TNode): 'primitive' | 'variant' | null {
    if (isEditablePrimitive(node.kind)) return 'primitive';
    const ed = studioSchema.enumDefAt(node.path);
    if (ed && ed.variants.length > 0 &&
        (node.kind === 'unit_variant' || node.kind === 'named_struct' || node.kind === 'named_tuple')) {
      return 'variant';
    }
    return null;
  }

  async function refreshAfterMutation(node: TNode, structural: boolean, removed = false): Promise<void> {
    await treePane?.refreshAfterMutation(node, structural, removed);
  }

  const editPipeline = useStudioEditPipeline<RonNodeKind, TNode>({
    formatId: 'ron',
    isEditablePrimitive,
    rowEditMode,
    currentVariantTag: (n) => n.variant_tag ?? '',
    computeSeed: (n) => {
      let seed = valueText ?? n.preview;
      const k = n.kind;
      if (k === 'string' && seed.startsWith('"') && seed.endsWith('"')) seed = seed.slice(1, -1);
      else if (k === 'char' && seed.startsWith("'") && seed.endsWith("'")) seed = seed.slice(1, -1);
      return seed;
    },
    commit: async (node, draft) => {
      let value: RonPrimitiveValue;
      try {
        switch (node.kind) {
          case 'string':
            value = { type: 'string', value: draft };
            break;
          case 'char': {
            const ch = [...draft][0];
            if (!ch) throw new Error('char cannot be empty');
            value = { type: 'char', value: ch };
            break;
          }
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
            let forceFloat = false;
            const hint = studioSchema.primitiveHintAt(node.path);
            if (hint === 'f32' || hint === 'f64') forceFloat = true;
            const looksInt = /^-?\d+$/.test(s);
            value = (forceFloat || !looksInt)
              ? { type: 'float', value: n }
              : { type: 'int',   value: Math.trunc(n) };
            break;
          }
          default: return {};
        }
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
      try {
        await ronStudioStore.mutatePrimitive(node.path, value);
        textDiff.bumpDiffRefresh();
        await refreshAfterMutation(node, /* structural */ false);
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    commitVariant: async (node, name) => {
      const def = studioSchema.enumDefAt(node.path);
      if (!def) return {};
      const v = def.variants.find(x => x.name === name);
      if (!v) return {};
      try {
        const ronText = defaultRonTextForVariant(v);
        await ronStudioStore.replaceAt(node.path, ronText);
        textDiff.bumpDiffRefresh();
        await refreshAfterMutation(node, /* structural */ true);
        editPipeline.maybeShowEditBanner();
        return {};
      } catch (e: any) { return { error: e?.message ?? String(e) }; }
    },
    focusInspector: () => inspectorPanel?.focusEditInput(),
  });

  async function startInlineEditAt(node: TNode): Promise<void> {
    await selectNode(node);
    const mode = rowEditMode(node);
    if (mode === 'primitive')    editPipeline.startEdit(node, 'tree');
    else if (mode === 'variant') editPipeline.startVariantEdit(node, 'tree');
  }

  // ── Container helpers ─────────────────────────────────────────────────
  function parentNodeOf(node: TNode): TNode | null {
    if (node.path.length === 0) return null;
    return treePane?.getNode(pathId(node.path.slice(0, -1))) ?? null;
  }
  function isInOrderedContainer(node: TNode): boolean {
    const p = parentNodeOf(node);
    return !!p && (p.kind === 'list' || p.kind === 'tuple' || p.kind === 'named_tuple');
  }
  function isRemovable(node: TNode | null): boolean {
    if (!node || node.path.length === 0) return false;
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent) return false;
    return isContainerKind(parent.kind);
  }
  function itemTypeOf(node: TNode): ResolvedType | null {
    if (!studioSchema.schema) return null;
    const ty = studioSchema.typeAtPath(node.path);
    if (!ty) return null;
    if (ty.kind === 'vec') return ty.inner;
    if (ty.kind === 'map') return ty.value;
    return null;
  }
  function addableFieldsAt(node: TNode): { name: string; ty: ResolvedType }[] {
    const schema = studioSchema.schema;
    if (!schema) return [];
    const ty = studioSchema.typeAtPath(node.path);
    if (!ty || ty.kind !== 'named') return [];
    const def = schema.types[ty.path];
    if (!def || def.kind !== 'struct') return [];
    const seen = new Set((node.children ?? []).map(c => c.key));
    return def.fields
      .filter(f => !seen.has(f.name) && !(f.aliases ?? []).some(a => seen.has(a)))
      .map(f => ({ name: f.name, ty: f.ty }));
  }

  // ── Container mutations ───────────────────────────────────────────────
  let actionError = $state<string | null>(null);

  async function addFieldAction(parent: TNode, name: string, defaultText: string): Promise<void> {
    try {
      await ronStudioStore.insertField(parent.path, name, defaultText);
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(parent, /* structural */ true);
      if (!expanded.has(parent.pid)) {
        const next = new Set(expanded); next.add(parent.pid); expanded = next;
      }
      const childPid = pathId([...parent.path, name]);
      const child = treePane?.getNode(childPid);
      if (child && rowEditMode(child)) await startInlineEditAt(child);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }
  async function addItemAction(parent: TNode): Promise<void> {
    const itemTy = itemTypeOf(parent);
    const defaultText = itemTy ? defaultRonText(itemTy) : '()';
    try {
      await ronStudioStore.insertItem(parent.path, defaultText);
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(parent, /* structural */ true);
      if (!expanded.has(parent.pid)) {
        const next = new Set(expanded); next.add(parent.pid); expanded = next;
      }
      const newIdx = parent.child_count;
      const newPath = [...parent.path, String(newIdx)];
      const child = treePane?.getNode(pathId(newPath));
      if (child && rowEditMode(child)) await startInlineEditAt(child);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }
  async function addMapEntryAction(parent: TNode): Promise<void> {
    const keyRaw = window.prompt('Map key (RON syntax, e.g. "foo" or 42):');
    if (!keyRaw) return;
    const valTy = itemTypeOf(parent);
    const valText = valTy ? defaultRonText(valTy) : '()';
    try {
      await ronStudioStore.insertMapEntry(parent.path, keyRaw, valText);
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(parent, /* structural */ true);
      if (!expanded.has(parent.pid)) {
        const next = new Set(expanded); next.add(parent.pid); expanded = next;
      }
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }
  async function duplicateAction(node: TNode): Promise<void> {
    try {
      await ronStudioStore.duplicateAt(node.path);
      textDiff.bumpDiffRefresh();
      const parent = parentNodeOf(node);
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }
  async function moveAction(node: TNode, delta: number): Promise<void> {
    try {
      await ronStudioStore.moveItem(node.path, delta);
      textDiff.bumpDiffRefresh();
      const parent = parentNodeOf(node);
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }
  async function removeSelected(): Promise<void> {
    if (!selectedNode || !isRemovable(selectedNode)) return;
    const node = selectedNode;
    try {
      await ronStudioStore.removeAt(node.path);
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: removeAt failed', e);
    }
  }
  async function resetToDefault(node: TNode): Promise<void> {
    if (!studioSchema.schema) return;
    const ty = studioSchema.typeAtPath(node.path);
    if (!ty) return;
    try {
      const text = defaultRonText(ty);
      await ronStudioStore.replaceAt(node.path, text);
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(node, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: reset failed', e);
    }
  }
  async function pasteOverNode(node: TNode): Promise<void> {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) return;
      await ronStudioStore.replaceAt(node.path, text);
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(node, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }
  function optionInnerType(node: TNode): ResolvedType | null {
    if (node.kind !== 'option' || !studioSchema.schema) return null;
    const ty = studioSchema.typeAtPath(node.path);
    if (!ty || ty.kind !== 'option') return null;
    return ty.inner;
  }
  function optionInnerEditableType(node: TNode): ResolvedType | null {
    const inner = optionInnerType(node);
    if (!inner || inner.kind !== 'primitive') return null;
    const n = inner.name;
    const editable = n === 'bool' || n === 'String' || n === 'char'
      || n.startsWith('i') || n.startsWith('u') || n.startsWith('f')
      || n === 'usize' || n === 'isize';
    return editable ? inner : null;
  }
  async function toggleSelectedOption(): Promise<void> {
    if (!selectedNode || selectedNode.kind !== 'option') return;
    const node   = selectedNode;
    const isNone = node.preview === 'None';
    if (isNone) {
      const innerTy = optionInnerType(node);
      if (innerTy) {
        try {
          await ronStudioStore.replaceAt(node.path, `Some(${defaultRonText(innerTy)})`);
          textDiff.bumpDiffRefresh();
          await refreshAfterMutation(node, /* structural */ true);
          editPipeline.maybeShowEditBanner();
          return;
        } catch (e) {
          console.warn('ron-studio: schema-default Some failed, falling back to toggle', e);
        }
      }
    }
    try {
      await ronStudioStore.toggleOption(node.path);
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(node, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: toggleOption failed', e);
    }
  }
  async function onOptionDblClick(node: TNode): Promise<void> {
    const innerTy = optionInnerEditableType(node);
    if (!innerTy) { await selectNode(node); return; }
    const isNone = node.preview === 'None';
    let needsReseed = isNone;
    if (!isNone) {
      if (node.children === null) await treePane?.loadChildren(node);
      const inner = treePane?.getNode(pathId([...node.path, 'Some']));
      if (!inner || inner.kind === 'unit') needsReseed = true;
    }
    if (needsReseed) {
      try {
        await ronStudioStore.replaceAt(node.path, `Some(${defaultRonText(innerTy)})`);
        textDiff.bumpDiffRefresh();
        await refreshAfterMutation(node, /* structural */ true);
      } catch (e) {
        console.warn('ron-studio: enable option failed', e);
        return;
      }
    }
    if (node.children === null) await treePane?.loadChildren(node);
    const inner = treePane?.getNode(pathId([...node.path, 'Some']));
    if (inner) {
      if (!expanded.has(node.pid)) {
        const next = new Set(expanded); next.add(node.pid); expanded = next;
      }
      await startInlineEditAt(inner);
    }
  }
  async function onValueDblClick(node: TNode, e: MouseEvent): Promise<void> {
    e.stopPropagation();
    if (node.kind === 'option') { await onOptionDblClick(node); return; }
    if (rowEditMode(node) !== null) await startInlineEditAt(node);
  }

  // ── Misc helpers ───────────────────────────────────────────────────────
  async function copyPathOf(node: TNode): Promise<void> {
    const text = node.path.length === 0 ? '$' : '$.' + node.path.join('.');
    await copyToClipboard(text);
  }
  async function copyValue(): Promise<void> {
    if (valueText == null) return;
    await copyToClipboard(valueText);
  }
  async function copyValueRonOf(node: TNode): Promise<void> {
    const id = ronStudioStore.docId;
    if (!id) return;
    try {
      const text = await RON.getValue(id, node.path);
      await copyToClipboard(text);
    } catch (e) { console.warn('ron-studio: copy value failed', e); }
  }
  async function inspectorAddField(parent: TNode, name: string): Promise<void> {
    const missing = addableFieldsAt(parent).find(f => f.name === name);
    if (!missing) return;
    await addFieldAction(parent, name, defaultRonText(missing.ty));
  }
  /** Inspector adapter for the variant picker — overrides the composable
   *  default because RON's variant nodes are `unit_variant` / `named_struct`
   *  / `named_tuple`, not `'string'` (YAML/JSON's case). */
  function ronInspectorVariantPickerInfo(node: TNode) {
    if (!studioSchema.schema) return null;
    const def = studioSchema.enumDefAt(node.path);
    if (!def || def.variants.length === 0) return null;
    if (!(node.kind === 'unit_variant' || node.kind === 'named_struct' || node.kind === 'named_tuple')) return null;
    return {
      enumName:   def.name,
      currentTag: node.variant_tag ?? '',
      variants:   def.variants.map(v => ({
        name:   v.name,
        suffix: v.shape === 'unit' ? '' : v.shape === 'tuple' ? '(…)' : ' { … }',
      })),
    };
  }

  async function inspectorPickVariant(name: string): Promise<void> {
    if (!selectedNode || !studioSchema.schema) return;
    if (name === selectedNode.variant_tag) return;
    const def = studioSchema.enumDefAt(selectedNode.path);
    if (!def) return;
    const v = def.variants.find(x => x.name === name);
    if (!v) return;
    const node = selectedNode;
    try {
      await ronStudioStore.replaceAt(node.path, defaultRonTextForVariant(v));
      textDiff.bumpDiffRefresh();
      await refreshAfterMutation(node, /* structural */ true);
      editPipeline.maybeShowEditBanner();
    } catch (e: any) {
      editPipeline.setEditError(e?.message ?? String(e));
    }
  }
  function isDefinitionNode(n: TNode | null): boolean {
    if (!n || n.kind !== 'string' || n.path.length !== 1) return false;
    return crossRefs.isDefinitionFieldName(n.key);
  }
  function definitionValue(n: TNode): string | null {
    return crossRefs.unquotedString(n.preview);
  }

  function expandNode(node: TNode, next: boolean): void {
    const set = new Set(expanded);
    if (next) set.add(node.pid); else set.delete(node.pid);
    expanded = set;
    if (next && node.children === null && node.child_count > 0) {
      void treePane?.loadChildren(node);
    }
  }

  // ── Context menu ───────────────────────────────────────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({ id: 'copy-path',  label: 'Copy path',        icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: 'Copy value (RON)', icon: Copy,     iconColor: 'var(--text-muted)' });

    if (studioSchema.schema) {
      const named = canonicalNamedTypeAt(node.path);
      if (named) {
        items.push({
          id:        'view-source',
          label:     `View ${named.split('::').pop()} implementation…`,
          icon:      BookOpen,
          iconColor: '#20b2aa',
        });
      }
    }

    {
      const refTarget = crossRefs.refFieldNameForNode(node) ?? refContainerFieldNameForNode(node);
      if (refTarget) {
        const marked = isFieldExplicitlyMarked(refTarget);
        const scope  = refFieldScopeSuffix();
        items.push({ id: 'sep-ref-field', label: '', separator: true } as MenuItem);
        items.push({
          id:    `toggle-ref-field:${refTarget}`,
          label: (marked ? `Unmark \`${refTarget}\` as reference field`
                         : `Mark \`${refTarget}\` as reference field`) + scope,
          icon:      marked ? Link2Off : LinkIcon,
          iconColor: 'var(--accent)',
        });
      }
    }

    if (tabsStore.activeTabId && isRenameableTreeNode(node)) {
      items.push({ id: 'sep-rename-xref', label: '', separator: true } as MenuItem);
      items.push({
        id:        'rename-across-project',
        label:     'Rename across project…',
        icon:      Replace,
        iconColor: '#ffc66d',
      });
    }

    const mode = rowEditMode(node);
    if (mode || node.kind === 'option') {
      items.push({ id: 'sep-edit', label: '', separator: true } as MenuItem);
    }
    if (mode === 'primitive') {
      items.push({ id: 'edit', label: 'Edit value', icon: Pencil, iconColor: '#ffc66d', shortcut: 'F2' });
    } else if (mode === 'variant') {
      items.push({ id: 'edit-variant', label: 'Change variant…', icon: Replace, iconColor: '#ffc66d' });
    }
    if (node.kind === 'option') {
      const isNone = node.preview === 'None';
      items.push({
        id:        'toggle-option',
        label:     isNone ? 'Set to Some(default)' : 'Set to None',
        icon:      isNone ? ToggleLeft : ToggleRight,
        iconColor: 'var(--text-muted)',
      });
    }

    items.push({ id: 'sep-mutate', label: '', separator: true } as MenuItem);
    if (studioSchema.schema && studioSchema.typeAtPath(node.path)) {
      items.push({ id: 'reset', label: 'Reset to default', icon: RotateCcw, iconColor: 'var(--warning)' });
    }
    items.push({ id: 'paste', label: 'Paste RON over value…', icon: ClipboardPaste, iconColor: 'var(--text-muted)' });

    if (node.kind === 'struct' || node.kind === 'named_struct') {
      const missing = addableFieldsAt(node);
      if (missing.length > 0) {
        items.push({ id: 'sep-add-field', label: '', separator: true } as MenuItem);
        items.push({ id: 'sep-add-field-header', label: 'Add field', header: true } as MenuItem);
        for (const f of missing.slice(0, 8)) {
          items.push({
            id:        `add-field:${f.name}`,
            label:     `${f.name} : ${studioSchema.fmtType(f.ty)}`,
            icon:      Plus,
            iconColor: 'var(--success)',
          });
        }
        if (missing.length > 8) {
          items.push({ id: 'add-field-prompt', label: `${missing.length - 8} more… (type name)`, icon: Plus, iconColor: 'var(--success)' });
        }
      } else if (!studioSchema.schema) {
        items.push({ id: 'sep-add-field', label: '', separator: true } as MenuItem);
        items.push({ id: 'add-field-prompt', label: 'Add field…', icon: Plus, iconColor: 'var(--success)' });
      }
    } else if (node.kind === 'list') {
      items.push({ id: 'sep-add-item', label: '', separator: true } as MenuItem);
      items.push({ id: 'add-item', label: 'Add item', icon: Plus, iconColor: 'var(--success)' });
    } else if (node.kind === 'map') {
      items.push({ id: 'sep-add-entry', label: '', separator: true } as MenuItem);
      items.push({ id: 'add-map-entry', label: 'Add entry…', icon: Plus, iconColor: 'var(--success)' });
    }

    if (isInOrderedContainer(node)) {
      items.push({ id: 'sep-reorder', label: '', separator: true } as MenuItem);
      items.push({ id: 'duplicate',  label: 'Duplicate',  icon: CopyPlus, iconColor: 'var(--text-muted)' });
      const idx = parseInt(node.key, 10);
      const parent = parentNodeOf(node);
      const total  = parent?.child_count ?? 0;
      items.push({ id: 'move-up',   label: 'Move up',   icon: ArrowUp,
        iconColor: 'var(--text-muted)', disabled: !Number.isFinite(idx) || idx <= 0 });
      items.push({ id: 'move-down', label: 'Move down', icon: ArrowDown,
        iconColor: 'var(--text-muted)', disabled: !Number.isFinite(idx) || idx >= total - 1 });
    } else if (node.path.length > 0) {
      const pk = parentNodeOf(node)?.kind;
      if (pk === 'struct' || pk === 'named_struct' || pk === 'map') {
        items.push({ id: 'sep-reorder', label: '', separator: true } as MenuItem);
        items.push({ id: 'duplicate', label: 'Duplicate', icon: CopyPlus, iconColor: 'var(--text-muted)' });
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
    return items;
  }

  async function onContextMenuSelect(id: string, node: TNode): Promise<void> {
    switch (id) {
      case 'copy-path':      await copyPathOf(node);                    break;
      case 'copy-value':     await copyValueRonOf(node);                break;
      case 'edit':           editPipeline.startEdit(node, 'tree');      break;
      case 'edit-variant':   editPipeline.startVariantEdit(node, 'tree'); break;
      case 'toggle-option':  await toggleSelectedOption();              break;
      case 'reset':          await resetToDefault(node);                break;
      case 'paste':          await pasteOverNode(node);                 break;
      case 'expand':         expandNode(node, true);                    break;
      case 'collapse':       expandNode(node, false);                   break;
      case 'expand-all':     await treePane?.expandSubtree(node);       break;
      case 'collapse-all':   treePane?.collapseSubtree(node);           break;
      case 'remove':         await removeSelected();                    break;
      case 'view-source': {
        const named = canonicalNamedTypeAt(node.path);
        if (named) await studioSchema.openViewSource(named);
        break;
      }
      case 'rename-across-project': renameBulk.openRenameModalForNode(node); break;
      case 'duplicate':      await duplicateAction(node);               break;
      case 'move-up':        await moveAction(node, -1);                break;
      case 'move-down':      await moveAction(node, +1);                break;
      case 'add-item':       await addItemAction(node);                 break;
      case 'add-map-entry':  await addMapEntryAction(node);             break;
      case 'add-field-prompt': {
        const name = window.prompt('New field name:');
        if (name) {
          let defaultText = '()';
          if (studioSchema.schema) {
            const ty = studioSchema.typeAtPath(node.path);
            if (ty && ty.kind === 'named') {
              const def = studioSchema.schema.types[ty.path];
              if (def && def.kind === 'struct') {
                const f = def.fields.find(f => f.name === name);
                if (f) defaultText = defaultRonText(f.ty);
              }
            }
          }
          await addFieldAction(node, name, defaultText);
        }
        break;
      }
      default: {
        if (id.startsWith('add-field:')) {
          const fname = id.slice('add-field:'.length);
          const missing = addableFieldsAt(node).find(f => f.name === fname);
          if (missing) await addFieldAction(node, fname, defaultRonText(missing.ty));
        } else if (id.startsWith('toggle-ref-field:')) {
          await toggleReferenceFieldForNode(id.slice('toggle-ref-field:'.length));
        }
      }
    }
  }

  // ── Rename + Bulk-edit pipelines ──────────────────────────────────────
  const renameBulk = useStudioRenameBulkPipeline<TNode>({
    formatId:       'ron',
    formatLabel:    'RON',
    getDocId:       () => ronStudioStore.docId,
    getSourcePath:  () => ronStudioStore.sourcePath,
    getDirty:       () => ronStudioStore.dirty,
    getActiveTabId: () => tabsStore.activeTabId,
    extractRenameValue: (n) => crossRefs.unquotedString(n.preview),
    buildOpenDocs: () => ronStudioWorkspaceStore.tabs.map(t => ({
      doc_id:      t.docId,
      source_path: t.sourcePath,
      dirty:       t.dirty,
    })),
    reloadAfterDiskWrite: async (written) => { await reloadTouchedTabs(written); },
    applyExternalActiveDocState: async (state) => {
      await ronStudioStore.applyExternalMutate(state);
      await treePane?.reloadTree();
      textDiff.bumpDiffRefresh();
    },
  });

  // ── Query bar ─────────────────────────────────────────────────────────
  const queryBarCtl = useStudioQueryBar<RonNodeKind>({
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

  // ── Text + Diff views ─────────────────────────────────────────────────
  const textDiff = useStudioTextDiff({
    getStoreCurrent: () => ronStudioStore.current,
    setText:         (text) => ronStudioStore.setText(text),
    reloadTree:      async () => { await treePane?.reloadTree(); },
  });

  // ── Indent (2 / 4 / 8 / tab) — persisted to localStorage ──────────────
  const INDENT_KEY = 'arbor:ron-studio:indent';
  function loadIndent(): string {
    if (typeof localStorage === 'undefined') return '  ';
    return localStorage.getItem(INDENT_KEY) ?? '  ';
  }
  let indentUnit = $state(loadIndent());
  async function setIndentUnit(s: string): Promise<void> {
    indentUnit = s;
    try { localStorage.setItem(INDENT_KEY, s); } catch { /* ignore */ }
    const id = ronStudioStore.docId;
    if (id) {
      try { await RON.setIndent(id, s); } catch (e) { console.warn('set indent failed', e); }
    }
  }

  // ── Format + Convert actions ──────────────────────────────────────────
  let actionBusy    = $state(false);
  let pendingAction = $state<'format' | 'tojson' | null>(null);
  function runFormat() { if (ronStudioStore.docId) pendingAction = 'format'; }
  function runToJson() { if (ronStudioStore.docId) pendingAction = 'tojson'; }
  async function performPendingAction(): Promise<void> {
    const which = pendingAction;
    pendingAction = null;
    const id = ronStudioStore.docId;
    if (!which || !id) return;
    actionBusy = true; actionError = null;
    try {
      const out = which === 'format' ? await RON.format(id) : await RON.toJson(id);
      await ronStudioStore.setText(out);
      void treePane?.reloadTree();
      textDiff.bumpDiffRefresh();
    } catch (e) {
      actionError = String(e);
    } finally {
      actionBusy = false;
    }
  }
  async function runFromJson(): Promise<void> {
    const id = ronStudioStore.docId;
    if (!id) return;
    actionBusy = true; actionError = null;
    try {
      const out = await RON.fromJson(id, ronStudioStore.current);
      await ronStudioStore.setText(out);
      void treePane?.reloadTree();
      textDiff.bumpDiffRefresh();
    } catch (e) {
      actionError = String(e);
    } finally {
      actionBusy = false;
    }
  }

  // ── Save / Save As ────────────────────────────────────────────────────
  async function refreshProjectCrossRefs(): Promise<void> {
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    await studioStore.loadCrossRefs(tabId, true);
    if (studioStore.settings.use_index && !studioStore.indexJobRunning) {
      void studioStore.refreshIndex(tabId);
    }
  }
  const saveFlow = useStudioSaveFlow({
    getSourcePath: () => ronStudioStore.sourcePath,
    save:          (opts) => ronStudioStore.save(opts),
    onSaved:       () => { textDiff.bumpDiffRefresh(); void refreshProjectCrossRefs(); },
  });

  // ── Footer snapshot ───────────────────────────────────────────────────
  const footerDoc: StudioFooterDoc = $derived({
    parseError: ronStudioStore.parseError ?? null,
    dirty:      ronStudioStore.dirty,
    sourcePath: ronStudioStore.sourcePath ?? null,
    encoding:   ronStudioStore.docId ? ronStudioStore.encoding : null,
    canUndo:    ronStudioStore.canUndo,
    canRedo:    ronStudioStore.canRedo,
    docId:      ronStudioStore.docId ?? null,
  });
  const selectedFooterPath = $derived<string[] | null>(
    selectedNode && viewMode === 'tree' ? selectedNode.path : null,
  );

  // ── View-mode tab descriptors ─────────────────────────────────────────
  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: textDiff.diffTreeChangeCount > 0 ? textDiff.diffTreeChangeCount
           : textDiff.diffHunkCount > 0       ? textDiff.diffHunkCount
           : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse errors',
      disabled: !ronStudioStore.parseError,
      badge: ronStudioStore.parseError ? '!' : undefined,
      data: { errorBadge: !!ronStudioStore.parseError } },
  ]);

  // ── Open + sync lifecycle ─────────────────────────────────────────────
  $effect(() => {
    const id = ronStudioStore.docId;
    untrack(() => {
      if (!id) {
        viewMode = 'tree';
        actionError = null;
        editPipeline.cancelEdit();
        queryBarCtl.resetForDocClose();
        studioSchema.clearSchema();
        return;
      }
      void RON.setIndent(id, indentUnit).catch(() => { /* best-effort */ });
    });
  });

  // ── Undo / Redo + Global keys ─────────────────────────────────────────
  const { doUndo, doRedo } = useStudioUndoRedo({
    undo: async () => {
      if (editPipeline.editingPid) {
        try { await editPipeline.maybeCommitActiveEdit(selectedNode); }
        catch { editPipeline.cancelEdit(); }
      }
      return ronStudioStore.undo();
    },
    redo: async () => {
      if (editPipeline.editingPid) editPipeline.cancelEdit();
      return ronStudioStore.redo();
    },
    reloadTree:      async () => { await treePane?.reloadTree(); },
    bumpDiffRefresh: () => textDiff.bumpDiffRefresh(),
  });

  const { onKey } = useStudioGlobalKeys<RonNodeKind, TNode>({
    isOpen:          () => ronStudioStore.open,
    doSave:          () => saveFlow.doSave(),
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

  // ── Misc ──────────────────────────────────────────────────────────────
  async function close(): Promise<void> {
    textDiff.cancelPendingTextPush();
    const tabs = [...ronStudioWorkspaceStore.tabs];
    for (const t of tabs) {
      try { await ronStudioWorkspaceStore.closeTab(t.docId); } catch { /* ignore */ }
    }
    ronStudioWorkspaceStore.closeFolder();
    await ronStudioStore.closeDoc();
  }

  const fmtBytes = fsFmtBytes;
  const rsBasename = fsBasename;

  function kindBadge(k: RonNodeKind): string {
    switch (k) {
      case 'struct':       return '{}';
      case 'named_struct': return '◆';
      case 'map':          return '⊞';
      case 'tuple':        return '()';
      case 'named_tuple':  return '◆';
      case 'unit_variant': return '◆';
      case 'list':         return '[]';
      case 'string':       return '"';
      case 'char':         return "'";
      case 'number':       return '#';
      case 'bool':         return '?';
      case 'option':       return '∘';
      case 'unit':         return '·';
    }
  }
  function kindTone(k: RonNodeKind): StudioKindTone {
    switch (k) {
      case 'struct':
      case 'map':
      case 'tuple':
      case 'list':         return 'keyword';
      case 'string':
      case 'char':         return 'string';
      case 'number':       return 'number';
      case 'bool':
      case 'option':       return 'accent';
      case 'unit':         return 'muted';
      case 'named_struct':
      case 'named_tuple':
      case 'unit_variant': return 'type';
    }
  }
  function numberIsFloat(node: TNode): boolean {
    const hint = studioSchema.primitiveHintAt(node.path);
    if (hint) {
      if (hint === 'f32' || hint === 'f64') return true;
      if (hint.startsWith('i') || hint.startsWith('u') || hint === 'usize' || hint === 'isize') return false;
    }
    return /[.eE]/.test(node.preview);
  }
  function previewClassFromText(text: string): string {
    if (!text) return '';
    if (text.startsWith('"') && text.endsWith('"'))   return 'rs-val-string';
    if (text === 'true' || text === 'false')          return 'rs-val-bool';
    if (text.startsWith("'") && text.endsWith("'"))   return 'rs-val-char';
    if (/^-?\d/.test(text)) {
      return /[.eE]/.test(text) ? 'rs-val-float' : 'rs-val-int';
    }
    return '';
  }
  function previewClass(node: TNode): string {
    switch (node.kind) {
      case 'string': return 'rs-val-string';
      case 'bool':   return 'rs-val-bool';
      case 'char':   return 'rs-val-char';
      case 'number': return numberIsFloat(node) ? 'rs-val-float' : 'rs-val-int';
      default:       return '';
    }
  }
  function splitOptionPreview(preview: string): { isNone: boolean; inner: string } {
    if (preview === 'None') return { isNone: true, inner: '' };
    if (preview.startsWith('Some(') && preview.endsWith(')')) {
      return { isNone: false, inner: preview.slice(5, -1) };
    }
    return { isNone: false, inner: preview };
  }
</script>

<svelte:window onkeydown={onKey} />

<StudioModal
  bind:this={studioModal}
  formatId="ron"
  backend={RON}
  open={ronStudioStore.open}
  loading={ronStudioStore.loading}
  loadingLabel="Parsing RON…"
  errorState={ronStudioStore.error}
  parseError={ronStudioStore.parseError}
  hasDoc={!!ronStudioStore.docId}
  viewItems={viewItems}
  bind:viewMode
  bind:rightPane
  rightPaneStorageKey={RIGHT_PANE_KEY}
  ariaLabel="RON Studio"
  onClose={close}
>
  {#snippet rightRailButtons()}
    {@const bindingsConfigured = (studioStore.config.overrides?.length ?? 0) > 0 || !!studioStore.config.default}
    {@const bindingsTip = (() => {
      const n      = (studioStore.config.overrides?.length ?? 0) + (studioStore.config.default ? 1 : 0);
      const inFile = docBrokenRefs.length;
      const inRepo = studioStore.brokenRefs.length;
      if (inFile > 0) return `Schema bindings — ${n} configured · ${inFile} broken reference${inFile === 1 ? '' : 's'} in this file`;
      if (inRepo > 0) return `Schema bindings — ${n} configured · ${inRepo} broken elsewhere in this repo (see Studio sidebar)`;
      return n > 0 ? `Schema bindings — ${n} configured` : 'Schema bindings — none configured yet';
    })()}
    <StudioRightRailButton
      icon={Layers}
      active={rightPane === 'bindings'}
      tooltip={bindingsTip}
      label="Bindings"
      onClick={() => studioModal?.toggleRightPane('bindings')}
      dot={docBrokenRefs.length > 0 || bindingsConfigured}
      dotTone={docBrokenRefs.length > 0 ? 'warning' : 'success'}
    />
    <StudioRightRailButton
      icon={ScanSearch}
      active={rightPane === 'inspector'}
      tooltip="Inspector — selected node detail (Tree view)"
      label="Inspector"
      onClick={() => studioModal?.toggleRightPane('inspector')}
    />
    <StudioRightRailButton
      icon={BookOpen}
      active={rightPane === 'schema'}
      tooltip={studioSchema.schema
        ? `Schema: ${studioSchema.schema.root_name} (${studioSchema.schema.stats.resolved} types)`
        : 'Load Rust schema'}
      label="Schema panel"
      onClick={() => studioModal?.toggleRightPane('schema')}
      dot={!!studioSchema.schema}
      dotTone="success"
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
      icon={Wrench}
      active={rightPane === 'tools'}
      tooltip="Tools — Format / Indent / Convert"
      label="Tools"
      onClick={() => studioModal?.toggleRightPane('tools')}
    />
  {/snippet}

  {#snippet headerLeft()}
    <span class="rs-header-icon-wrap" aria-hidden="true">
      <Icon icon={ronIcon} width={18} height={18} />
    </span>
    <StudioHeaderUndoRedo doc={footerDoc} onUndo={doUndo} onRedo={doRedo} />

    {#if ronStudioWorkspaceStore.tabs.length <= 1}
      <span class="rs-title" use:tooltip={ronStudioStore.sourcePath ?? ''}>
        {ronStudioStore.title ?? 'RON Studio'}
        {#if ronStudioStore.dirty}<span class="rs-dirty" use:tooltip={'Unsaved changes'}>●</span>{/if}
      </span>
      {#if ronStudioStore.sizeBytes != null}
        <span class="rs-meta">{fmtBytes(ronStudioStore.sizeBytes)}</span>
      {/if}
    {/if}

    {#if ronStudioWorkspaceStore.tabs.length > 1}
      <div class="rs-tabs-host rs-tabs-host-inline">
        <Tabs
          items={ronStudioWorkspaceStore.tabs.map(t => ({
            id:       t.docId,
            label:    (t.dirty ? '● ' : '') + t.title,
            title:    t.sourcePath ?? t.title,
            closable: true,
            data:     t,
          } satisfies TabItem))}
          value={ronStudioStore.docId}
          variant="panel"
          size="sm"
          overflow={true}
          onSelect={(id) => void activateTab(id)}
          onClose={(id) => void closeTab(id)}
          ariaLabel="Open RON files"
        >
          {#snippet itemContent({ item, active })}
            <span class="rs-tab-icon" aria-hidden="true">
              <Icon icon={ronIcon} width={13} height={13} />
            </span>
            <span class="rs-tab-title" class:rs-tab-title-dim={!active}>
              {(item.data as any).title}
            </span>
            {#if (item.data as any).dirty}
              <span class="rs-tab-dirty" use:tooltip={'Unsaved changes'}>●</span>
            {/if}
          {/snippet}
        </Tabs>
      </div>
    {/if}

    <Dropdown
      items={openFileItems}
      searchable={openFileItems.length > 0}
      searchPlaceholder="Filter .ron in project…"
      emptyMessage={tabsStore.activeTabId
        ? 'No .ron files indexed in this project yet.'
        : 'Open a project tab first.'}
      width="320px"
      maxHeight={360}
      position="fixed"
    >
      {#snippet trigger({ toggle, open })}
        <button type="button" class="rs-tab-plus"
          class:rs-tab-plus-open={open}
          onclick={toggle}
          aria-haspopup="listbox"
          aria-expanded={open}
          aria-label="Open .ron"
          use:tooltip={'Open a .ron — from this project or anywhere on disk'}
        >
          <Plus size={14} />
        </button>
      {/snippet}
      {#snippet footer({ close: closeMenu })}
        <button type="button" class="rs-bindings-disk"
          onclick={() => { closeMenu(); openWorkspaceFilePicker(); }}
        >
          <HardDrive size={12} /> Browse disk…
        </button>
      {/snippet}
    </Dropdown>

    {#if ronStudioWorkspaceStore.tabs.length <= 1}
      <div class="rs-spacer"></div>
    {/if}
  {/snippet}

  {#snippet footerStatusLeft()}
    <StudioFooterStatus doc={footerDoc} selectedPath={selectedFooterPath}>
      {#snippet extras()}
        {#if studioSchema.schema}
          <button class="rs-footer-pill rs-footer-pill-schema"
                  onclick={() => setRightPane(rightPane === 'schema' ? null : 'schema')}
                  use:tooltip={studioSchema.schema.root_type}
                  aria-label="Open schema panel">
            <BookOpen size={11} />
            <span>{studioSchema.schema.root_name}</span>
          </button>
        {:else if studioSchema.schemaLoading}
          <span class="rs-footer-pill rs-footer-pill-schema rs-footer-pill-schema-loading"
                use:tooltip={studioSchema.schemaRsPath ?? 'Loading schema…'}>
            <BookOpen size={11} /> loading…
          </span>
        {/if}
        <button class="rs-footer-pill rs-footer-pill-refs"
                onclick={() => {
                  const tid = tabsStore.activeTabId;
                  if (tid && !studioStore.indexJobRunning) void studioStore.refreshIndex(tid);
                }}
                disabled={!tabsStore.activeTabId || studioStore.indexJobRunning}
                use:tooltip={studioStore.indexJobRunning && studioStore.indexProgress
                  ? `Indexing ${studioStore.indexProgress.processed}/${studioStore.indexProgress.total}…`
                  : 'Rebuild cross-reference index'}
                aria-label="Rebuild cross-reference index">
          <Network size={11} class={studioStore.indexJobRunning ? 'spin' : ''} />
          {#if studioStore.indexJobRunning && studioStore.indexProgress && studioStore.indexProgress.total > 0}
            <span>{Math.round((studioStore.indexProgress.processed / studioStore.indexProgress.total) * 100)}%</span>
          {:else}
            <span>refs</span>
          {/if}
        </button>
      {/snippet}
    </StudioFooterStatus>
  {/snippet}

  {#snippet toolsSidecar()}
    <StudioToolsSidebar
      doc={footerDoc}
      {actionBusy}
      {indentUnit}
      indentOptions={INDENT_OPTIONS_WITH_8}
      indentTooltip="Indent — applied to Format and tree edits"
      formatTooltip="Format — re-emit canonical RON (loses comments)"
      onSetIndent={setIndentUnit}
      onFormat={runFormat}
    >
      {#snippet extras()}
        <div class="sts-row">
          <div class="sts-row-label">Convert</div>
          <Dropdown items={[
            { kind: 'item', id: 'to-json',   label: 'RON → JSON',  icon: FileJson, onclick: runToJson },
            { kind: 'item', id: 'from-json', label: 'JSON → RON',  icon: FileCode, onclick: runFromJson },
          ]} position="fixed" direction="down">
            {#snippet trigger({ toggle })}
              <button type="button" class="sts-btn" onclick={toggle}
                use:tooltip={'Convert RON ↔ JSON'}>
                <Repeat2 size={13} />
                <span>RON ↔ JSON</span>
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
      formatId="ron"
      backend={RON}
      docId={ronStudioStore.docId}
      visible={viewMode === 'tree' && !ronStudioStore.parseError}
      placeholder='Query — name (recursive), $.foo.bar, $.arr[0:5], $..[?@.$type == "Goblin"]…'
      historyStorageKey="arbor:ron-studio:query-history"
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
        <button type="button" class="rs-query-tool-btn"
          onclick={() => void treePane?.expandAll()}
          disabled={expandAllBusy}
          use:tooltip={'Recursively load + expand every container (capped at 5000 nodes for large docs)'}
          aria-label="Expand all"
        >{#if expandAllBusy}<Loader2 size={12} class="rs-query-spinner" />{:else}<ChevronsDown size={12} />{/if}<span>Expand</span></button>
        <button type="button" class="rs-query-tool-btn"
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
        formatId="ron"
        backend={RON}
        docId={ronStudioStore.docId}
        parseError={ronStudioStore.parseError}
        bind:roots
        bind:expanded
        bind:selectedNode
        bind:valueText
        bind:valueLoading
        bind:expandAllBusy
        toTree={toTree as any}
        sortChildren={sortChildrenForDisplay as any}
        isContainerKind={isContainerKindRon}
        getContextMenuItems={ctxItemsFor as any}
        onContextMenuSelect={(id: string, n: any) => onContextMenuSelect(id, n as TNode)}
        {commitPendingEdit}
        showRightBorder={false}
        ariaLabel="RON document tree"
      >
        {#snippet rowContent({ node }: RowSnippetCtx<any>)}
          {@const n = node as TNode}
          {@const ty = studioSchema.typeAtPath(n.path)}
          {@const tupleStruct = isTupleStructAt(n.path)}
          {@const named = namedTypeAt(n.path)}
          {@const opt = n.kind === 'option' ? splitOptionPreview(n.preview) : null}

          {@const ronTone = tupleStruct ? 'number' : (opt?.isNone ? 'muted' : kindTone(n.kind))}
          {@const ronLabel = tupleStruct ? '()' : (opt?.isNone ? '∘' : kindBadge(n.kind))}
          {@const ronTooltip = tupleStruct ? 'tuple struct' : (opt?.isNone ? 'None (Option)' : n.kind)}
          <StudioKindBadge label={ronLabel} tone={ronTone} tinted tooltip={ronTooltip} />

          <span class="rs-row-key" class:rs-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          <span class="rs-row-sep">:</span>

          {#if n.variant_tag && !(editPipeline.editingPid === n.pid && editPipeline.editLocation === 'tree')}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="rs-row-tag"
                  ondblclick={(e) => void onValueDblClick(n, e)}
                  use:tooltip={rowEditMode(n) === 'variant' ? 'Double-click to change variant' : 'Variant / struct tag from source'}
            >{n.variant_tag}</span>
          {/if}

          {#if editPipeline.editingPid === n.pid && editPipeline.editLocation === 'tree'}
            {#if rowEditMode(n) === 'variant'}
              {@const ed = studioSchema.enumDefAt(n.path)}
              {#if ed}
                <StudioInlineEdit
                  mode="select"
                  variant
                  minWidth={140}
                  bind:value={editPipeline.editBuf}
                  options={ed.variants.map(v => ({
                    value: v.name,
                    label: v.name + (v.shape === 'unit' ? '' : v.shape === 'tuple' ? '(…)' : ' { … }'),
                  }))}
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
                placeholder={n.kind === 'char' ? 'single char' : undefined}
                onkeydown={(e) => editPipeline.onEditKey(e, n)}
                errorMsg={editPipeline.editError}
              />
            {/if}
          {:else if opt}
            {#if opt.isNone}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="rs-row-preview rs-preview-none"
                    ondblclick={(e) => void onValueDblClick(n, e)}
                    use:tooltip={optionInnerEditableType(n) ? 'Double-click to set a value' : 'Option is None'}
              >None</span>
            {:else}
              <span class="rs-row-option-tag" use:tooltip={'Some(_) — value is set'}>Some</span>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="rs-row-preview {previewClassFromText(opt.inner)}"
                    ondblclick={(e) => void onValueDblClick(n, e)}
                    use:tooltip={optionInnerEditableType(n) ? 'Double-click to edit' : ''}
              >{opt.inner}</span>
            {/if}
          {:else if n.kind === 'unit_variant'}
            <!-- variant tag IS the value — skip the redundant preview. -->
          {:else}
            {@const xrefs = crossRefs.crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="rs-row-preview {previewClass(n)}"
                  class:rs-row-preview-editable={rowEditMode(n) !== null}
                  class:rs-row-preview-xref={hasX}
                  ondblclick={(e) => void onValueDblClick(n, e)}
                  onclick={hasX ? ((e) => crossRefs.onCrossRefClick(xrefs, e)) : undefined}
                  use:tooltip={hasX
                    ? (xrefs.length === 1
                        ? `Ctrl+click → ${xrefs[0].title} (${xrefs[0].defPath.join('.')})`
                        : `Ctrl+click → choose between ${xrefs.length} matches`)
                    : (rowEditMode(n) === 'primitive' ? 'Double-click to edit'
                      : rowEditMode(n) === 'variant'  ? 'Double-click to change variant'
                      : '')}
            >{n.preview}{#if hasX}<span class="rs-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="rs-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}

          {#if named}
            <span class="rs-row-type-slot">
              <TypePill label={named} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {:else if ty}
            <span class="rs-row-type-slot">
              <TypePill label={studioSchema.fmtType(ty)} kind={typePillKind(ty, studioSchema.schema)} tooltip={studioSchema.fmtType(ty)} />
            </span>
          {/if}
        {/snippet}
      </StudioTreePane>

    {:else if viewMode === 'text'}
      <StudioTextPane
        language="ron"
        value={textDiff.textBuf}
        oninput={textDiff.onTextInput}
      >
        {#snippet footer()}
          <span>{textDiff.textBuf.length} chars · {textDiff.textBuf.split('\n').length} lines</span>
          {#if ronStudioStore.parseError}
            <span class="rs-text-err">{ronStudioStore.parseError}</span>
          {:else}
            <span class="rs-text-ok">✓ parses</span>
          {/if}
        {/snippet}
      </StudioTextPane>

    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="ron"
        backend={RON}
        docId={ronStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={ronStudioStore.current}
        refreshTick={textDiff.diffRefreshTick}
        storageKey="arbor:ron-studio:diff-sub"
        bind:treeChangeCount={textDiff.diffTreeChangeCount}
        bind:hunkCount={textDiff.diffHunkCount}
      >
        {#snippet tagChip(tag, position)}
          <span class="rs-row-tag" class:rs-row-tag-before={position === 'before'}>{tag}</span>
        {/snippet}
      </StudioDiffPane>

    {:else if viewMode === 'errors'}
      {#if ronStudioStore.parseError}
        <div class="rs-errors-wrap">
          <Alert variant="error" title="RON parse error">
            <pre class="rs-errors-body">{ronStudioStore.parseError}</pre>
            <p class="rs-errors-hint">
              Switch to <strong>Text</strong> to fix the document. The tree and diff views will update live as soon as it parses again.
            </p>
          </Alert>
        </div>
      {:else}
        <StateBlock tone="success" label="No parse errors." />
      {/if}
    {/if}
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="ron"
      backend={RON}
      sourcePath={ronStudioStore.sourcePath}
      onOpenDefinition={crossRefs.openDefinition}
    >
      {#snippet emptyState()}
        <p class="rs-bindings-empty">
          No schema bindings configured for this project. Use
          the <strong>Studio</strong> sidebar's right-click
          menu on a <code>.ron</code> file or folder to bind
          it to a Rust source.
        </p>
      {/snippet}
    </StudioRefsPanel>
  {/snippet}

  {#snippet inspectorSidecar()}
    <StudioInspectorPanel
      bind:this={inspectorPanel}
      formatId="ron"
      backend={RON}
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
      isBoolKind={(k: RonNodeKind) => k === 'bool'}
      isCharKind={(k: RonNodeKind) => k === 'char'}
      isOptionKind={(k: RonNodeKind) => k === 'option'}
      isContainerKind={isContainerKindRon as any}
      isDefinitionNode={isDefinitionNode as any}
      definitionValue={definitionValue as any}
      schemaTypeInfo={studioSchema.inspectorSchemaTypeInfo as any}
      variantPickerInfo={ronInspectorVariantPickerInfo as any}
      missingFields={studioSchema.inspectorMissingFields as any}
      onCopyPath={copyPathOf as any}
      onCopyValue={copyValue}
      onRemove={removeSelected}
      onStartEdit={(loc?: 'tree' | 'detail') => editPipeline.startEdit(selectedNode, loc)}
      onCommitEdit={() => selectedNode ? editPipeline.runCommit(selectedNode) : Promise.resolve()}
      onCancelEdit={editPipeline.cancelEdit}
      onPickVariant={(name: string) => void inspectorPickVariant(name)}
      onAddField={inspectorAddField as any}
      onToggleOption={toggleSelectedOption}
      onDismissEditBanner={editPipeline.dismissEditBanner}
      onJumpToUsage={crossRefs.jumpToUsage}
      onSelectChild={(c) => void selectNode(c as TNode)}
    />
  {/snippet}

  {#snippet querySidecar()}
    <PanelShell title="Query results" count={queryBarCtl.queryHits.length} class="rs-query-shell">
      {#snippet icon()}<ListFilter size={13} />{/snippet}
    <div class="rs-query-pane-body">
      {#if !queryBarCtl.query.trim()}
        <p class="rs-query-pane-empty">
          Type in the search bar at the top of the tree view
          to populate this list. Supports the same JSONPath
          subset shown in the input's placeholder.
        </p>
      {:else if queryBarCtl.querying && queryBarCtl.queryHits.length === 0}
        <div class="rs-query-pane-status">
          <Spinner size="xs" /> <span>Running query…</span>
        </div>
      {:else if queryBarCtl.queryError}
        <div class="rs-query-pane-error">
          <AlertCircle size={11} /> {queryBarCtl.queryError}
        </div>
      {:else if queryBarCtl.queryHits.length === 0}
        <p class="rs-query-pane-empty">No matches.</p>
      {:else}
        <div class="rs-query-pane-list">
          {#each queryBarCtl.queryHits as hit, i (hit.path.join('\x00'))}
            <button type="button" class="rs-query-pane-card"
              class:active={i === queryBarCtl.currentHitIdx}
              onclick={() => { queryBarCtl.currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="rs-query-pane-card-head">
                <StudioKindBadge label={kindBadge(hit.kind)} tone={kindTone(hit.kind)} tinted tooltip={hit.kind} />
                {#if hit.variant_tag}
                  <span class="rs-row-tag rs-query-pane-card-tag">{hit.variant_tag}</span>
                {/if}
                <span class="rs-query-pane-card-idx">#{i + 1}</span>
              </div>
              <div class="rs-query-pane-card-path">{hit.path.length === 0 ? '$' : '$.' + hit.path.join('.')}</div>
              {#if hit.preview}
                <div class="rs-query-pane-card-preview {previewClassFromText(hit.preview)}">{hit.preview}</div>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    </PanelShell>
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId="ron"
      backend={RON}
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
      pickerTitle="Pick Rust source for schema"
      pickerExtensions={['rs']}
      pickerButtonLabel="Pick .rs file"
      {refCountForType}
    >
      {#snippet intro()}
        <p class="rs-schema-hint">
          Load a Rust source file from your crate. RON Studio walks every <code>mod</code> declaration from the enclosing <code>Cargo.toml</code> and indexes every <code>struct</code>, <code>enum</code> and <code>type</code> alias.
        </p>
      {/snippet}
    </StudioSchemaPanel>
  {/snippet}

  {#snippet auxiliary()}
    {#if saveFlow.savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save RON document as"
        extensions={['ron']}
        initialPath={ronStudioStore.sourcePath ?? undefined}
        initialFilename={rsBasename(ronStudioStore.sourcePath) || 'document.ron'}
        onConfirm={saveFlow.onSaveAsPicked}
        onCancel={() => saveFlow.savePickerOpen = false}
      />
    {/if}

    {#if diskFilePicking}
      <FilePickerModal
        mode="file"
        title="Open .ron from disk"
        extensions={['ron']}
        initialPath={ronStudioStore.sourcePath ?? undefined}
        onConfirm={onDiskFilePicked}
        onCancel={() => diskFilePicking = false}
      />
    {/if}

    {#if studioSchema.viewSource || studioSchema.viewSourceBusy || studioSchema.viewSourceErr}
      <StudioViewSourceModal
        viewSource={studioSchema.viewSource}
        busy={studioSchema.viewSourceBusy}
        err={studioSchema.viewSourceErr}
        language="rust"
        loadingLabel="Re-parsing crate…"
        decorateLine={(line) => {
          const m = line.match(/^\s*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_]\w*)\s*:/);
          const name = m?.[1];
          return name && isRefFieldName(name) ? name : null;
        }}
        onClose={studioSchema.closeViewSource}
      />
    {/if}

    {#if renameBulk.renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={RON}
        tabId={tabsStore.activeTabId}
        formatLabel="RON"
        oldValue={renameBulk.renameModalState.oldValue}
        openDocs={renameBulk.buildRenameOpenDocs()}
        onClose={renameBulk.closeRenameModal}
        onApplied={renameBulk.onRenameApplied}
      />
    {/if}

    {#if renameBulk.bulkEditModalState && tabsStore.activeTabId && ronStudioStore.docId}
      <StudioBulkEditModal
        backend={RON}
        tabId={tabsStore.activeTabId}
        docId={ronStudioStore.docId}
        formatLabel="RON"
        query={renameBulk.bulkEditModalState.query}
        nullPolicy="not_supported"
        openDocs={renameBulk.buildBulkEditOpenDocs()}
        onClose={renameBulk.closeBulkEditModal}
        onApplied={renameBulk.onBulkEditApplied}
      />
    {/if}

    <!-- Portalled to document.body via crossRefs.portal — Modal.svelte's
         `transition:fly` applies a transform to `.modal`, which creates
         a containing block for `position: fixed` descendants. Re-parenting
         to body sidesteps the stacking + coordinate-space tangle entirely. -->
    <StudioXrefPicker
      picker={crossRefs.crossRefPicker}
      portal={crossRefs.portal}
      onPick={(entry) => void crossRefs.jumpToCrossRef(entry)}
      onDismiss={crossRefs.dismissPicker}
    >
      {#snippet icon(_entry)}
        <Icon icon={ronIcon} width={13} height={13} />
      {/snippet}
    </StudioXrefPicker>
  {/snippet}
</StudioModal>

{#if pendingAction === 'format'}
  <ConfirmModal
    title="Format document"
    message="Format will re-emit the document through the RON serialiser."
    detail="This drops comments and any custom whitespace. Continue?"
    variant="warning"
    confirmLabel="Format"
    onCancel={() => pendingAction = null}
    onConfirm={performPendingAction}
  />
{:else if pendingAction === 'tojson'}
  <ConfirmModal
    title="Convert to JSON"
    message="Convert the document to JSON?"
    detail="This replaces the current text with the JSON equivalent (comments are lost; struct names become string keys)."
    variant="warning"
    confirmLabel="Convert"
    onCancel={() => pendingAction = null}
    onConfirm={performPendingAction}
  />
{/if}

<style>
  /* ── Header bits (rendered inside Modal.svelte's .modal-header via
     the shell's `headerLeft` snippet). */
  :global(.rs-header-icon) { color: var(--accent); flex-shrink: 0; }
  .rs-header-icon-wrap {
    display: inline-flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }
  .rs-title  { font-size: 13px; font-weight: 600; max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: inline-flex; align-items: center; gap: 6px; }
  .rs-dirty  { color: var(--accent); font-size: 14px; line-height: 1; }
  .rs-meta   { color: var(--text-muted); font-size: 11px; display: inline-flex; align-items: center; gap: 3px; }
  .rs-spacer { flex: 1; }

  /* ── Footer pills (Schema chip + Refs index chip) ─────────────────── */
  .rs-footer-pill {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--bg-overlay);
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .rs-footer-pill-schema {
    color: var(--syntax-type, #61afef);
    background: color-mix(in srgb, var(--syntax-type, #61afef) 14%, var(--bg-overlay));
    border: none;
    cursor: pointer;
    font: inherit;
    font-family: var(--font-code);
    font-size: 10.5px;
    transition: background var(--transition-fast);
  }
  .rs-footer-pill-schema:hover {
    background: color-mix(in srgb, var(--syntax-type, #61afef) 28%, var(--bg-overlay));
  }
  .rs-footer-pill-schema-loading {
    color: var(--text-muted);
    background: var(--bg-overlay);
    font-style: italic;
  }
  .rs-footer-pill-refs {
    color: var(--text-secondary);
    background: var(--bg-overlay);
    border: none;
    cursor: pointer;
    font: inherit;
    font-family: var(--font-code);
    font-size: 10.5px;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .rs-footer-pill-refs:hover:not(:disabled) {
    color: var(--text-primary);
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-overlay));
  }
  .rs-footer-pill-refs:disabled { opacity: 0.7; cursor: default; }

  /* ── Query results sidebar ─────────────────────────────────────── */
  .rs-query-pane-body {
    padding: 10px 8px 8px;
    overflow: auto;
    display: flex; flex-direction: column;
    gap: 6px;
    flex: 1;
  }
  .rs-query-pane-empty,
  .rs-query-pane-status {
    font-size: 11px;
    color: var(--text-muted);
    padding: 4px 6px;
    line-height: 1.45;
  }
  .rs-query-pane-status {
    display: flex; align-items: center; gap: 6px;
    font-style: italic;
  }
  .rs-query-pane-error {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 8px;
    font-size: 11px;
    color: var(--error, #e07a5f);
    background: color-mix(in srgb, var(--error, #e07a5f) 8%, transparent);
    border-radius: var(--radius-sm);
  }
  .rs-query-pane-list { display: flex; flex-direction: column; gap: 4px; }
  .rs-query-pane-card {
    display: flex; flex-direction: column;
    gap: 4px;
    width: 100%;
    padding: 6px 8px 6px 10px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-left: 2px solid transparent;
    border-radius: var(--radius-sm);
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    min-width: 0;
  }
  .rs-query-pane-card:hover {
    background: var(--bg-hover);
    border-color: color-mix(in srgb, var(--accent) 28%, var(--border-subtle));
  }
  .rs-query-pane-card.active {
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    border-color: color-mix(in srgb, var(--accent) 32%, var(--border-subtle));
    border-left-color: var(--accent);
  }
  .rs-query-pane-card-head {
    display: flex; align-items: center; gap: 5px;
    font-size: 10px;
    color: var(--text-muted);
  }
  .rs-query-pane-card-tag { margin: 0; }
  .rs-query-pane-card-idx {
    margin-left: auto;
    font-family: var(--font-code);
    color: var(--text-disabled);
  }
  .rs-query-pane-card-path {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--syntax-type, var(--accent));
    overflow-wrap: anywhere;
    line-height: 1.35;
  }
  .rs-query-pane-card-preview {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* "+" tab-strip launcher. */
  .rs-tab-plus {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    margin-left: 4px; padding: 0;
    background: transparent;
    color: var(--text-muted);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .rs-tab-plus:hover,
  .rs-tab-plus-open {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
    border-style: solid;
  }

  /* "Browse disk…" footer inside the Open dropdown. */
  .rs-bindings-disk {
    display: flex; align-items: center; gap: 6px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 11.5px;
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .rs-bindings-disk:hover { background: var(--bg-hover); color: var(--text-primary); }

  .rs-bindings-empty {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.5;
    padding: 4px 4px 0;
  }
  .rs-bindings-empty strong { color: var(--text-primary); font-weight: 600; }
  .rs-bindings-empty code {
    font-family: var(--font-code);
    font-size: 10.5px;
    background: var(--bg-overlay);
    padding: 0 4px;
    border-radius: 3px;
  }

  /* ── Tab strip (multi-file workspace). */
  .rs-tabs-host-inline {
    flex: 1 1 auto;
    min-width: 0;
    align-self: stretch;
    display: flex;
    align-items: stretch;
    margin-left: 6px;
    overflow: hidden;
  }
  :global(.rs-tabs-host-inline .tabs-panel) { padding: 0; flex: 1; min-width: 0; }
  :global(.rs-tabs-host-inline .tabs-panel .tabs-tab) {
    font-size: var(--font-size-sm);
    padding: 0 7px 0 9px;
    max-width: 320px;
  }
  :global(.rs-tabs-host-inline .tabs-panel .tabs-tab.tab-active) {
    background: var(--bg-base);
  }
  .rs-tab-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .rs-tab-title {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 300px;
  }
  .rs-tab-title-dim { color: var(--text-secondary); }
  .rs-tab-dirty { color: var(--accent); font-size: 12px; line-height: 1; margin-left: 2px; }

  /* Row chips, badges, etc. */
  .rs-row-tag {
    display: inline-flex; align-items: center;
    font-family: var(--font-code);
    font-size: 10px; font-weight: 600;
    color: var(--syntax-keyword, var(--accent));
    background: color-mix(in srgb, var(--syntax-keyword, var(--accent)) 14%, transparent);
    padding: 1px 6px;
    border-radius: 6px;
    margin-right: 4px;
    flex-shrink: 0;
    letter-spacing: 0.01em;
    cursor: pointer;
  }
  .rs-row-tag:hover { filter: brightness(1.15); }

  .rs-row-key { font-family: var(--font-code); font-size: 12px; font-weight: 500; color: var(--text-primary); flex-shrink: 0; }
  .rs-row-key-index { color: var(--text-muted); font-weight: 400; }
  .rs-row-sep { color: var(--text-disabled); font-family: var(--font-code); }
  .rs-preview-none { color: var(--text-disabled); font-style: italic; }
  .rs-row-option-tag {
    display: inline-flex; align-items: center;
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--accent);
    padding: 0 4px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    margin-right: 4px;
    flex-shrink: 0;
  }
  .rs-row-preview {
    font-family: var(--font-code); font-size: 11px;
    color: var(--text-secondary);
    flex: 1; min-width: 0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .rs-row-preview.rs-val-string { color: var(--syntax-string,   #98c379); }
  .rs-row-preview.rs-val-int    { color: var(--syntax-number,   #61afef); }
  .rs-row-preview.rs-val-float  { color: var(--syntax-decimal,  #c678dd); }
  .rs-row-preview.rs-val-bool   { color: var(--syntax-keyword,  #d19a66); font-weight: 500; }
  .rs-row-preview.rs-val-char   { color: var(--syntax-char,     #56b6c2); }
  .rs-row-preview { cursor: text; }
  .rs-row-preview:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); border-radius: 3px; }

  .rs-row-preview-editable { cursor: pointer; }
  .rs-row-preview-xref {
    text-decoration: underline dotted;
    text-decoration-color: color-mix(in srgb, var(--accent) 50%, transparent);
    text-underline-offset: 3px;
  }
  .rs-row-xref {
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
  .rs-row-preview-xref:hover .rs-row-xref {
    opacity: 1;
    background: color-mix(in srgb, var(--accent) 24%, transparent);
  }
  .rs-row-xref-count {
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 700;
    color: var(--accent);
  }

  .rs-row-type-slot {
    margin-left: auto;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
  }

  /* ── Text view footer pills (rendered via StudioTextPane's `footer`
     snippet — its scope is the parent's, so the classes belong here). */
  .rs-text-err { color: var(--danger, #e06c75); }
  .rs-text-ok  { color: var(--success, #98c379); }

  /* ── Diff view — variant-tag chip modifier ────────────────────────── */
  .rs-row-tag-before { opacity: 0.7; text-decoration: line-through; }

  /* ── Errors view — Alert wrapper + pre/hint styling. */
  .rs-errors-wrap { padding: 16px; height: 100%; overflow: auto; }
  .rs-errors-body {
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
  .rs-errors-hint { color: var(--text-muted); font-size: 11px; margin: 6px 0 0; }

  /* ── Schema panel intro hint (rendered via the `intro` snippet
     passed to <StudioSchemaPanel>). */
  .rs-schema-hint { color: var(--text-muted); font-size: 11px; line-height: 1.6; }

  /* ── Query bar leftovers (rendered via `toolbarRight` snippet). */
  :global(.rs-query-spinner) {
    color: var(--accent);
    animation: rs-spin 0.9s linear infinite;
  }
  @keyframes rs-spin { to { transform: rotate(360deg); } }
  .rs-query-tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 3px;
    color: var(--text-secondary);
    font-size: 10.5px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .rs-query-tool-btn:hover {
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .rs-query-tool-btn:disabled { opacity: 0.45; cursor: not-allowed; }
</style>
