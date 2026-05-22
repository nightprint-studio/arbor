<!--
  RonStudioModal — IntelliJ-style RON viewer / editor for the
  ron-studio plugin.

  After Phase 2B-2.g this file is the RON wrapper around the generic
  `<StudioModal>` shell. It owns:
    · ron-studio + ron-studio-workspace store wiring (open / close /
      multi-tab / dirty / schema hint).
    · The schema state machine (probe / load / cache / autoload from
      directive or sidecar) plus the Rust source-viewer modal.
    · The cross-reference index (open-tab + on-disk merge,
      Ctrl+click navigation, multi-match popover) + Find Usages.
    · Inline edit + variant edit state + commit / cancel pipelines.
    · The format-specific row snippet (kind badges, named-type chip,
      cross-ref decoration, double-click-to-edit affordance).
    · Context-menu items + dispatch.
    · Format / Convert (RON ↔ JSON) actions and indent persistence.
    · The footer save split + save-as / disk-open file pickers.

  Everything UI-shape that's not RON-specific (modal chrome, view-mode
  tabs, right-rail container, bindings/query/schema sidecar layout,
  loading / error / empty StateBlocks) lives in `<StudioModal>` and
  is injected from here via snippet props.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import {
    FileCode, X, Copy, ListTree, FileText, AlertCircle, GitCompare,
    ChevronRight, ChevronUp, ChevronDown, Settings2,
    Link as LinkIcon, Repeat2, FileJson,
    PanelRightClose, PanelRightOpen,
    Pencil, Check, ToggleLeft, ToggleRight,
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
  import Prism from 'prismjs';
  import '$lib/utils/prism-shared';
  import '$lib/utils/prism-languages/ron';
  import Modal from './Modal.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import { type MenuItem } from './ContextMenu.svelte';
  import { type RowSnippetCtx } from './ui/Tree.svelte';
  import Dropdown, { type DropdownItem } from './ui/Dropdown.svelte';
  import Tabs, { type TabItem } from './ui/Tabs.svelte';
  import Alert from './ui/Alert.svelte';
  import StateBlock from './ui/StateBlock.svelte';
  import StudioModal from './studio/StudioModal.svelte';
  import StudioFooterStatus    from './studio/StudioFooterStatus.svelte';
  import StudioFooterRight     from './studio/StudioFooterRight.svelte';
  import StudioHeaderUndoRedo  from './studio/StudioHeaderUndoRedo.svelte';
  import StudioToolsSidebar    from './studio/StudioToolsSidebar.svelte';
  import StudioBodyBanners  from './studio/StudioBodyBanners.svelte';
  import {
    INDENT_OPTIONS_WITH_8,
    type StudioFooterDoc,
  } from './studio/studio-footer-types';
  import { basename as fsBasename, fmtBytes as fsFmtBytes } from './studio/helpers';
  import StudioQueryBar from './studio/StudioQueryBar.svelte';
  import StudioTextPane, { type StudioTextPaneController } from './studio/StudioTextPane.svelte';
  import StudioDiffPane, { type StudioDiffPaneController } from './studio/StudioDiffPane.svelte';
  import StudioSchemaPanel from './studio/StudioSchemaPanel.svelte';
  import StudioRefsPanel from './studio/StudioRefsPanel.svelte';
  import StudioViewSourceModal from './studio/StudioViewSourceModal.svelte';
  import StudioTreePane, { type StudioTreePaneController } from './studio/StudioTreePane.svelte';
  import StudioInspectorPanel, {
    type StudioInspectorPanelController,
    type InspectorSchemaTypeInfo,
    type InspectorVariantPickerInfo,
    type InspectorMissingField,
    type InspectorUsageEntry,
  } from './studio/StudioInspectorPanel.svelte';
  import StudioRenameModal from './studio/StudioRenameModal.svelte';
  import StudioBulkEditModal from './studio/StudioBulkEditModal.svelte';
  import type {
    RenameOpenDoc, RenameResult,
    BulkEditOpenDoc, BulkEditResult,
  } from '$lib/types/studio-format';
  import Icon from '@iconify/svelte';
  import ronIcon from '@iconify-icons/vscode-icons/file-type-ron';
  import { tooltip } from '$lib/actions/tooltip';
  import { ronStudioStore } from '$lib/stores/ron-studio.svelte';
  import { ronStudioWorkspaceStore } from '$lib/stores/ron-studio-workspace.svelte';
  import { studioStore } from '$lib/stores/studio.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import {
    studioBackend,
    type CrateProbe, type Schema, type TypeDef, type ResolvedType,
    type VariantDef, type TypeSource,
  } from '$lib/ipc/studio-format';
  import type {
    RonNodeView, RonNodeKind, RonPrimitiveValue, RonQueryHit,
  } from '$lib/types/ron-studio';
  import type { CrossRefDef } from '$lib/ipc/studio';

  /** Pre-bound backend for the RON format. All host IPC for RON
   *  Studio flows through here; per FROZEN F17 there are no
   *  `ronStudio*` commands anymore — every call hits the unified
   *  `studio_*` Tauri commands with `format_id="ron"` baked in. */
  const RON = studioBackend<RonNodeKind>('ron');

  type ViewMode = 'tree' | 'text' | 'diff' | 'errors';
  let viewMode = $state<ViewMode>('tree');

  // ── Tree state ──────────────────────────────────────────────────────────
  type TNode = RonNodeView & {
    pid:      string;
    children: TNode[] | null;
    loading?: boolean;
  };

  function pathId(p: string[]): string { return p.join('\x00'); }
  function toTree(v: RonNodeView): TNode { return { ...v, pid: pathId(v.path), children: null }; }

  /** Category bucket for the tree's cosmetic field-ordering pass.
   *  Lower bucket numbers render first. Stable inside the bucket so
   *  fields within the same category keep their source order. */
  function fieldOrderBucket(k: RonNodeKind): number {
    if (k === 'struct' || k === 'named_struct' || k === 'map')  return 0;
    if (k === 'list'   || k === 'tuple'        || k === 'named_tuple') return 1;
    if (k === 'option') return 2;
    return 3;
  }

  /** Reorder children for display under `parentKind`. Applied only to
   *  named-key parents (struct / named_struct / map): there the on-disk
   *  order has no semantic meaning, so grouping objects → arrays →
   *  optionals → rest improves readability. List / tuple parents are
   *  left untouched — their child paths are positional indices, and
   *  reshuffling the visible order would desync label and path. */
  function sortChildrenForDisplay(parentKind: RonNodeKind, kids: TNode[]): TNode[] {
    if (parentKind !== 'struct' && parentKind !== 'named_struct' && parentKind !== 'map') {
      return kids;
    }
    return kids
      .map((c, i) => ({ c, i, b: fieldOrderBucket(c.kind) }))
      .sort((a, b) => a.b - b.b || a.i - b.i)
      .map(x => x.c);
  }

  // Tree state — owned by `<StudioTreePane>`; bound here so the row
  // snippet, mutations, Inspector, and Query bar all read it from
  // this scope without prop-drilling through the panel.
  let roots         = $state<TNode[]>([]);
  let expanded      = $state<Set<string>>(new Set());
  let selectedNode  = $state<TNode | null>(null);
  let valueText     = $state<string | null>(null);
  let valueLoading  = $state(false);

  /** Helper that mirrors the panel's pre-select edit-commit hook.
   *  The panel calls `commitPendingEdit` before mutating selection so
   *  pending inline edits flush cleanly. */
  async function commitPendingEdit(): Promise<void> {
    if (editingPid && editingPid !== selectedNode?.pid) {
      try { await maybeCommitActiveEdit(); }
      catch { cancelEdit(); }
    }
  }

  /** Wrapper-side `selectNode` — forwards to the panel's controller.
   *  Used by jumpToCrossRef, jumpToUsage, mutation pipelines, and the
   *  Errors-view "Jump to error" callback. */
  async function selectNode(node: TNode): Promise<void> {
    await treePane?.selectNode(node);
  }

  function isContainerKindRon(k: RonNodeKind): boolean {
    return k === 'struct' || k === 'map' || k === 'list' || k === 'tuple';
  }

  async function copyValue() {
    if (valueText != null) {
      try { await navigator.clipboard.writeText(valueText); } catch {}
    }
  }
  async function copyPathOf(n: TNode) {
    const p = n.path.length === 0 ? '$' : '$.' + n.path.join('.');
    try { await navigator.clipboard.writeText(p); } catch {}
  }

  // ── Query bar ────────────────────────────────────────────────────────────
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

  let query         = $state('');
  let queryHits     = $state<RonQueryHit[]>([]);
  let queryError    = $state<string | null>(null);
  let querying      = $state(false);
  let currentHitIdx = $state(0);
  interface StudioQueryBarController {
    focus():            void;
    clear():            void;
    nav(delta: number): void;
    getHitCount():      number;
    getQuery():         string;
  }
  let queryBar = $state<StudioQueryBarController | undefined>();

  /** True once the user manually closed the query sidebar during
   *  the CURRENT query session — prevents the auto-open below from
   *  re-popping it open every time the user keeps typing. Reset
   *  to false when the query goes empty (a fresh session starts on
   *  the next non-empty input). */
  let queryAutoOpenDismissed = $state(false);

  function getChildKeysForPath(path: string[]): string[] | null {
    return treePane?.getChildKeysForPath(path) ?? null;
  }
  function ensureChildrenLoadedForPath(path: string[]) {
    treePane?.ensureChildrenLoadedForPath(path);
  }
  function onQueryActiveChange(active: boolean) {
    if (!active) {
      queryAutoOpenDismissed = false;
      return;
    }
    if (viewMode === 'tree' && rightPane === null && !queryAutoOpenDismissed) {
      setRightPane('query');
    }
  }
  function onQueryToggleRightPane() {
    if (rightPane === 'query') queryAutoOpenDismissed = true;
    studioModal?.toggleRightPane('query');
  }

  /** Controller for the extracted `<StudioTreePane>`. The panel owns
   *  the whole tree state machine (load / reload / select / expand /
   *  refresh-after-mutation / jump / context menu); the wrapper
   *  drives it via this surface. */
  let treePane: StudioTreePaneController<RonNodeKind, TNode> | undefined = $state();
  let inspectorPanel: StudioInspectorPanelController | undefined = $state();
  void untrack(() => inspectorPanel);

  async function jumpToQueryHit(path: string[]): Promise<void> {
    await treePane?.jumpToPath(path);
  }
  async function expandAll(): Promise<void> {
    await treePane?.expandAll();
  }
  function collapseAll(): void {
    treePane?.collapseAll();
  }
  let expandAllBusy = $state(false);

  // ── Right pane: at most one of {inspector, schema, bindings, query} is open ──
  // The shell owns the persistence (when we pass `rightPaneStorageKey`)
  // and the toggleRightPane() controller. We seed the initial value
  // here from localStorage because $bindable defaults can't read the
  // shell's `rightPaneStorageKey` prop.
  type RightPane = 'inspector' | 'schema' | 'bindings' | 'query' | 'tools' | null;
  const RIGHT_PANE_KEY = 'arbor:ron-studio:right-pane';
  function loadRightPane(): RightPane {
    if (typeof localStorage === 'undefined') return 'inspector';
    const v = localStorage.getItem(RIGHT_PANE_KEY);
    return (v === 'schema' || v === 'bindings' || v === 'query' || v === null)
      ? (v as RightPane)
      : 'inspector';
  }
  let rightPane = $state<RightPane>(loadRightPane());
  /** Local helper — just sets `rightPane` (the shell's $effect mirrors
   *  it back to localStorage). Kept so call-sites read the same as
   *  before the refactor. */
  function setRightPane(p: RightPane) { rightPane = p; }
  /** Shell controller — exposes toggleRightPane() / setRightPane() so
   *  rightRail buttons + footer schema chip can flip panes. */
  let studioModal: { toggleRightPane(p: 'inspector' | 'schema' | 'bindings' | 'query' | 'tools'): void; setRightPane(p: RightPane): void } | undefined = $state();

  // ── Workspace / tabs ───────────────────────────────────────────────────
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

  // ── Open .ron launcher (Bindings panel) ─────────────────────────────────
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

  /** Broken refs filtered to the currently open document. Used by
   *  the right-rail icon's warning dot + tooltip. */
  const docBrokenRefs = $derived.by(() => {
    const src = ronStudioStore.sourcePath;
    if (!src) return [] as typeof studioStore.brokenRefs;
    const norm = src.replace(/\\/g, '/');
    return studioStore.brokenRefs.filter(r =>
      r.absolute_path.replace(/\\/g, '/') === norm,
    );
  });

  async function openDefinition(d: CrossRefDef) {
    const docId = await ronStudioWorkspaceStore.openFile(d.absolute_path);
    if (docId !== ronStudioStore.docId) {
      await activateTab(docId);
    }
    if (d.def_path.length > 0) {
      await jumpToQueryHit(d.def_path);
    }
  }

  async function activateTab(docId: string) {
    if (docId === ronStudioStore.docId) return;
    if (editingPid) { try { await maybeCommitActiveEdit(); } catch { cancelEdit(); } }
    const ok = await ronStudioStore.switchTo(docId);
    if (ok) {
      ronStudioWorkspaceStore.setActive(docId);
      treePane?.resetState();
      textBuf = ronStudioStore.current;
      const hint = ronStudioWorkspaceStore.getSchemaHint(docId);
      if (hint) {
        if (!schema || schemaRsPath !== hint.rs_file || schema.root_type !== hint.root_type) {
          await autoLoadSchemaFromHint(hint.rs_file, hint.root_type);
        }
      } else {
        clearSchema();
      }
    }
  }

  async function closeTab(docId: string, e?: MouseEvent) {
    e?.stopPropagation();
    const wasActive = docId === ronStudioStore.docId;
    const { nextActive } = await ronStudioWorkspaceStore.closeTab(docId);
    if (wasActive) {
      if (nextActive) {
        await activateTab(nextActive);
      } else {
        await ronStudioStore.closeDoc();
      }
    }
  }

  async function openFileFromWorkspace(path: string) {
    if (editingPid) { try { await maybeCommitActiveEdit(); } catch { cancelEdit(); } }
    const id = await ronStudioWorkspaceStore.openFile(path);
    await activateTab(id);
  }

  // ── Cross-reference index ─────────────────────────────────────────────
  type CrossRefEntry = {
    sourcePath: string;
    fileName:   string;
    defPath:    string[];
    docId:      string | null;
    title:      string;
  };

  const BUILTIN_REF_NAMES = new Set([
    'target', 'source', 'parent', 'owner', 'prev', 'next',
  ]);
  function builtinIsReferenceField(key: string): boolean {
    if (BUILTIN_REF_NAMES.has(key)) return true;
    return key.endsWith('_id') || key.endsWith('_ref') || key.endsWith('Id') || key.endsWith('Ref');
  }

  function isReferenceFieldFor(sourcePath: string | null, key: string): boolean {
    if (!sourcePath) return builtinIsReferenceField(key);
    const repoRel = relPathInRepo(sourcePath);
    const patterns = repoRel ? studioStore.referenceFieldsFor(repoRel) : null;
    if (!patterns) return builtinIsReferenceField(key);
    return patterns.some(p => studioStore.matchesPattern(p, key));
  }
  function isReferenceFieldName(key: string): boolean {
    return isReferenceFieldFor(ronStudioStore.sourcePath, key);
  }

  function refFieldNameForNode(node: TNode): string | null {
    if (node.kind !== 'string') return null;
    const idx = parseInt(node.key, 10);
    if (Number.isInteger(idx) && String(idx) === node.key && node.path.length >= 2) {
      return node.path[node.path.length - 2];
    }
    return node.key;
  }

  function refContainerFieldNameForNode(node: TNode): string | null {
    if (node.kind !== 'list' && node.kind !== 'tuple') return null;
    if (node.path.length === 0) return null;
    return node.path[node.path.length - 1];
  }

  function refFieldScopeSuffix(): string {
    const sp = ronStudioStore.sourcePath;
    if (!sp) return '';
    const repoRel = relPathInRepo(sp);
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
    const repoRel = relPathInRepo(sp);
    if (!repoRel) return false;
    const patterns = studioStore.referenceFieldsFor(repoRel);
    if (!patterns) return false;
    return patterns.includes(fieldName);
  }

  async function toggleReferenceFieldForNode(fieldName: string) {
    const tabId = tabsStore.activeTabId;
    const sp    = ronStudioStore.sourcePath;
    if (!tabId || !sp) return;
    const repoRel = relPathInRepo(sp);
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
        now
          ? `\`${fieldName}\` is now a reference field for this binding.`
          : `\`${fieldName}\` is no longer a reference field.`,
        'success',
      );
    } catch (e) {
      notificationsStore.add('Reference fields', `Toggle failed: ${e}`, 'error');
    }
  }
  function isDefinitionFieldName(key: string): boolean {
    return key === 'id' || key === 'name';
  }

  function relPathInRepo(absPath: string): string | null {
    const norm = absPath.replace(/\\/g, '/');
    const hit = studioStore.files.find(f => f.absolute_path.replace(/\\/g, '/') === norm);
    return hit ? hit.relative_path : null;
  }

  function unquotedString(preview: string): string | null {
    if (preview.length < 2) return null;
    if (!preview.startsWith('"') || !preview.endsWith('"')) return null;
    const inner = preview.slice(1, -1);
    if (inner.endsWith('…')) return null;
    return inner;
  }

  type OpenTabDef = {
    docId:      string;
    sourcePath: string | null;
    title:      string;
    defPath:    string[];
  };
  let openTabDefs = $state<Map<string, OpenTabDef[]>>(new Map());

  async function rebuildOpenTabDefs() {
    const next = new Map<string, OpenTabDef[]>();
    for (const tab of ronStudioWorkspaceStore.tabs) {
      try {
        const root = await RON.getRoot(tab.docId);
        if (!root || root.child_count === 0) continue;
        const kids = await RON.getChildren(tab.docId, root.path);
        for (const c of kids) {
          if (!isDefinitionFieldName(c.key)) continue;
          if (c.kind !== 'string') continue;
          const val = await RON.getValue(tab.docId, c.path).catch(() => null);
          const raw = val ? unquotedString(val) : null;
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

  function crossRefsForValue(value: string): CrossRefEntry[] {
    const out:        CrossRefEntry[] = [];
    const claimed:    Set<string>     = new Set();

    for (const tabDef of openTabDefs.get(value) ?? []) {
      if (tabDef.sourcePath) claimed.add(tabDef.sourcePath);
      out.push({
        sourcePath: tabDef.sourcePath ?? '',
        fileName:   tabDef.title,
        defPath:    tabDef.defPath,
        docId:      tabDef.docId,
        title:      tabDef.title,
      });
    }
    for (const disk of studioStore.findCrossRefs(value)) {
      if (claimed.has(disk.absolute_path)) continue;
      const tab = ronStudioWorkspaceStore.tabs.find(t => t.sourcePath === disk.absolute_path);
      const defPath = (disk.def_path && disk.def_path.length > 0)
        ? disk.def_path
        : [disk.def_field];
      out.push({
        sourcePath: disk.absolute_path,
        fileName:   disk.file_name,
        defPath,
        docId:      tab?.docId ?? null,
        title:      tab?.title ?? disk.file_name,
      });
    }
    return out;
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

  /** Tiny Svelte action that re-parents the node to `document.body`
   *  on mount and removes it on destroy. Used for the cross-ref picker
   *  popover: rendering inside the Modal's transformed stack breaks
   *  `position: fixed` coordinates, so we escape to the body root
   *  where viewport-relative coords behave normally. */
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() { node.parentNode?.removeChild(node); },
    };
  }

  async function jumpToCrossRef(target: CrossRefEntry) {
    crossRefPicker = null;
    let docId = target.docId;
    if (!docId) {
      if (!target.sourcePath) return;
      docId = await ronStudioWorkspaceStore.openFile(target.sourcePath);
    }
    if (docId !== ronStudioStore.docId) {
      await activateTab(docId);
    }
    await treePane?.jumpToPath(target.defPath);
  }

  function onCrossRefClick(entries: CrossRefEntry[], e: MouseEvent) {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    e.stopPropagation();
    if (entries.length === 1) {
      void jumpToCrossRef(entries[0]);
    } else if (entries.length > 1) {
      crossRefPicker = { x: e.clientX, y: e.clientY, entries };
    }
  }

  // ── F12 — Cross-reference rename refactor ─────────────────────────────
  //
  // Gating: the menu item only appears when there's an active tab
  // (== a registered repo to refactor against). Opening RonStudioModal
  // via the command palette on a stray .ron — i.e. without a project
  // context — never proposes the refactor (per the user's directive).
  // The active tab's repo is the scan root; the doc itself doesn't
  // need to live inside that repo (external files registered via
  // .studio.toml work normally).

  let renameModalState = $state<{ oldValue: string } | null>(null);

  function isRenameableTreeNode(n: TNode): boolean {
    if (n.kind !== 'string') return false;
    const v = unquotedString(n.preview);
    if (!v) return false;
    if (isDefinitionFieldName(n.key)) return true;
    const ref = refFieldNameForNode(n);
    return !!ref && isReferenceFieldName(ref);
  }

  function openRenameModal(node: TNode): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Rename across project',
        'No active project — open this RON file from a project tab to rename across files.',
        'warning',
      );
      return;
    }
    const value = unquotedString(node.preview);
    if (!value) return;
    renameModalState = { oldValue: value };
  }

  /** Snapshot every open RON doc so the BE can dirty-check files
   *  affected by the refactor. The single-doc store knows whether
   *  the *active* doc is dirty; the workspace store mirrors that
   *  flag onto every tab so we can build the snapshot in one read. */
  function buildOpenDocsSnapshot(): RenameOpenDoc[] {
    return ronStudioWorkspaceStore.tabs.map(t => ({
      doc_id:      t.docId,
      source_path: t.sourcePath,
      dirty:       t.dirty,
    }));
  }

  function closeRenameModal(): void { renameModalState = null; }

  /** Re-parse a single tab's source from disk and swap the docId
   *  in-place. Used after a rename apply to refresh open tabs whose
   *  underlying file got rewritten on disk. */
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
    if (wasActive) {
      ronStudioWorkspaceStore.setActive(r.doc_id);
      // The single-doc store is bound to the active tab via the
      // existing $effect at the top of the file — switching the
      // active tab swings it onto the new doc id.
    }
  }

  async function onRenameApplied(result: RenameResult): Promise<void> {
    closeRenameModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    // Reload every open tab whose source file was rewritten so the
    // in-memory tree reflects the new value. Failures here are non-
    // fatal — the user can re-open the tab manually if needed.
    const writtenSet = new Set(written.map(p => p.replace(/\\/g, '/').toLowerCase()));
    const tabsToReload = ronStudioWorkspaceStore.tabs.filter(t => {
      if (!t.sourcePath) return false;
      return writtenSet.has(t.sourcePath.replace(/\\/g, '/').toLowerCase());
    });
    for (const t of tabsToReload) {
      try { await reloadTabFromDisk(t.docId); }
      catch (e) { console.warn('rename: tab reload failed for', t.sourcePath, e); }
    }

    // Cross-refs index is now stale — force a refresh so the next
    // Ctrl+click picker / Find Usages call sees the new namespace.
    const aTab = tabsStore.activeTabId;
    if (aTab) {
      try { await studioStore.loadCrossRefs(aTab, true); } catch { /* soft */ }
      try { await studioStore.refreshIndex(aTab); }       catch { /* soft */ }
    }
    await rebuildOpenTabDefs();

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

  // ── F13 — Bulk edit by query ──────────────────────────────────────
  //
  // Modal opens from the StudioQueryBar `[⚡ Edit]` button when the
  // descriptor reports `supports_bulk_edit = true` (RON does today).
  // The modal owns the action/value/preview lifecycle; the wrapper
  // only feeds it `query` + `docId` + open-docs snapshot and reacts
  // to `onApplied`.

  let bulkEditModalState = $state<{ query: string } | null>(null);

  function openBulkEditModal(q: string): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Bulk edit by query',
        'No active project — open this RON file from a project tab to run a bulk edit.',
        'warning',
      );
      return;
    }
    if (!ronStudioStore.docId) return;
    if (!q) return;
    bulkEditModalState = { query: q };
  }

  function closeBulkEditModal(): void { bulkEditModalState = null; }

  /** Reuse the rename pipeline's open-docs snapshot — same shape. */
  function buildBulkEditOpenDocs(): BulkEditOpenDoc[] {
    return buildOpenDocsSnapshot();
  }

  async function onBulkEditApplied(result: BulkEditResult): Promise<void> {
    closeBulkEditModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    if (result.active_doc_state) {
      // Active-doc scope — the BE already produced a single history
      // entry inside the registry; we just pipe its MutateResult
      // through the same FE sync path tree mutations use, so dirty
      // state / tree / diff / undo all snap to the new state without
      // a second IPC call (and without a duplicate history push).
      try {
        await ronStudioStore.applyExternalMutate(result.active_doc_state);
        await treePane?.reloadTree();
      } catch (e) {
        console.warn('bulk edit: active-doc sync failed', e);
      }
    } else {
      // Project-wide — reload every open tab whose source got
      // rewritten on disk + invalidate the cross-refs index.
      const writtenSet = new Set(written.map(p => p.replace(/\\/g, '/').toLowerCase()));
      const tabsToReload = ronStudioWorkspaceStore.tabs.filter(t => {
        if (!t.sourcePath) return false;
        return writtenSet.has(t.sourcePath.replace(/\\/g, '/').toLowerCase());
      });
      for (const t of tabsToReload) {
        try { await reloadTabFromDisk(t.docId); }
        catch (e) { console.warn('bulk edit: tab reload failed for', t.sourcePath, e); }
      }
      const aTab = tabsStore.activeTabId;
      if (aTab) {
        try { await studioStore.loadCrossRefs(aTab, true); } catch { /* soft */ }
        try { await studioStore.refreshIndex(aTab); }       catch { /* soft */ }
      }
      await rebuildOpenTabDefs();
    }

    // Toast — applied / skipped counts surfaced directly so the user
    // doesn't need to compare site list lengths.
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

  // ── Find Usages (reverse navigation: def → references) ────────────────
  function isDefinitionNode(n: TNode | null): boolean {
    if (!n) return false;
    if (n.kind !== 'string') return false;
    if (n.path.length !== 1) return false;
    return isDefinitionFieldName(n.key);
  }
  function definitionValue(n: TNode): string | null {
    return unquotedString(n.preview);
  }

  async function jumpToUsage(u: { absolute_path: string; field_path: string[] }) {
    let docId: string | null = null;
    const existing = ronStudioWorkspaceStore.tabs.find(t => t.sourcePath === u.absolute_path);
    if (existing) {
      docId = existing.docId;
    } else {
      docId = await ronStudioWorkspaceStore.openFile(u.absolute_path);
    }
    if (docId !== ronStudioStore.docId) {
      await activateTab(docId);
    }
    await treePane?.jumpToPath(u.field_path);
  }

  // ── Indent unit (2 spaces / 4 spaces / tab) ────────────────────────────
  const INDENT_KEY = 'arbor:ron-studio:indent';
  function loadIndent(): string {
    if (typeof localStorage === 'undefined') return '  ';
    return localStorage.getItem(INDENT_KEY) ?? '  ';
  }
  let indentUnit = $state(loadIndent());
  /* indentLabel moved to shared/studio/helpers.ts (used internally by
     <StudioToolsSidebar>). The label is now abbreviated ("2 sp" rather
     than "2 spaces") to match the other Studio modals. */
  async function setIndentUnit(s: string) {
    indentUnit = s;
    try { localStorage.setItem(INDENT_KEY, s); } catch { /* ignore */ }
    const id = ronStudioStore.docId;
    if (id) {
      try { await RON.setIndent(id, s); } catch (e) { console.warn('set indent failed', e); }
    }
  }

  // ── Footer snapshot (consumed by shared StudioFooter* components) ──────
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

  // ── View implementation modal ──────────────────────────────────────────
  let viewSource     = $state<TypeSource | null>(null);
  let viewSourceBusy = $state(false);
  let viewSourceErr  = $state<string | null>(null);

  async function openViewSource(canonicalPath: string) {
    if (!schemaRsPath) { actionError = 'No schema loaded — pick a .rs file first.'; return; }
    viewSourceBusy = true; viewSourceErr = null; viewSource = null;
    try {
      viewSource = await RON.schemaViewSource(schemaRsPath, canonicalPath);
    } catch (e: any) {
      viewSourceErr = e?.message ?? String(e);
    } finally {
      viewSourceBusy = false;
    }
  }
  function closeViewSource() { viewSource = null; viewSourceErr = null; }

  // ── Reference-field detection (schema + source viewer) ─────────────────
  const BUILTIN_REF_KEYS = new Set(['target', 'source', 'parent', 'owner', 'prev', 'next']);
  function builtinIsRef(name: string): boolean {
    return BUILTIN_REF_KEYS.has(name)
        || name.endsWith('_id')  || name.endsWith('_ref')
        || name.endsWith('Id')   || name.endsWith('Ref');
  }
  function matchesGlob(pattern: string, key: string): boolean {
    if (pattern === '*')        return true;
    if (pattern.startsWith('*')) return key.endsWith(pattern.slice(1));
    if (pattern.endsWith('*'))   return key.startsWith(pattern.slice(0, -1));
    return pattern === key;
  }

  const activeRefPatterns = $derived.by<string[] | null>(() => {
    void studioStore.config;
    const src = ronStudioStore.sourcePath;
    if (!src) return null;
    const norm = src.replace(/\\/g, '/');
    const hit  = studioStore.files.find(f => f.absolute_path.replace(/\\/g, '/') === norm);
    if (!hit) return null;
    return studioStore.referenceFieldsFor(hit.relative_path);
  });

  function isRefFieldName(name: string): boolean {
    const patterns = activeRefPatterns;
    if (patterns && patterns.length > 0) {
      return patterns.some(p => matchesGlob(p, name));
    }
    return builtinIsRef(name);
  }

  function refCountForType(def: TypeDef): number {
    if (def.kind === 'struct') {
      return def.fields.filter(f => isRefFieldName(f.name)).length;
    }
    if (def.kind === 'enum') {
      let n = 0;
      for (const v of def.variants) for (const f of v.fields) {
        if (isRefFieldName(f.name)) n++;
      }
      return n;
    }
    return 0;
  }

  function canonicalNamedTypeAt(path: string[]): string | null {
    if (!schema) return null;
    let ty = typeAtPath(path);
    if (!ty) return null;
    if (ty.kind === 'option') ty = ty.inner;
    if (ty.kind !== 'named') return null;
    return ty.path;
  }

  // ── Tree-edit state ─────────────────────────────────────────────────────
  let editingPid    = $state<string | null>(null);
  let editLocation  = $state<'tree' | 'detail'>('detail');
  let editBuf       = $state('');
  let editError     = $state<string | null>(null);
  let editInlineEl:   HTMLInputElement   | undefined = $state();
  let editInlineSelectEl: HTMLSelectElement | undefined = $state();

  const EDIT_BANNER_KEY = 'arbor:ron-studio:edit-warning-dismissed';
  let editBannerVisible = $state(false);
  function maybeShowEditBanner() {
    if (typeof localStorage === 'undefined') return;
    if (localStorage.getItem(EDIT_BANNER_KEY) !== '1') editBannerVisible = true;
  }
  function dismissEditBanner() {
    editBannerVisible = false;
    try { localStorage.setItem(EDIT_BANNER_KEY, '1'); } catch { /* ignore */ }
  }

  function isEditablePrimitive(k: RonNodeKind): boolean {
    return k === 'string' || k === 'number' || k === 'bool' || k === 'char';
  }

  function isContainerKind(k: RonNodeKind): boolean {
    return k === 'struct' || k === 'named_struct'
        || k === 'tuple'  || k === 'named_tuple'
        || k === 'map'    || k === 'list';
  }

  async function refreshAfterMutation(
    node:       TNode,
    structural: boolean,
    removed:    boolean = false,
  ): Promise<void> {
    await treePane?.refreshAfterMutation(node, structural, removed);
  }

  function startEdit(location: 'tree' | 'detail' = 'detail') {
    if (!selectedNode || !isEditablePrimitive(selectedNode.kind)) return;
    let seed = valueText ?? selectedNode.preview;
    const k = selectedNode.kind;
    if (k === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
      seed = seed.slice(1, -1);
    } else if (k === 'char' && seed.startsWith("'") && seed.endsWith("'")) {
      seed = seed.slice(1, -1);
    }
    editBuf      = seed;
    editError    = null;
    editingPid   = selectedNode.pid;
    editLocation = location;
    maybeShowEditBanner();
    if (location === 'tree') {
      queueMicrotask(() => queueMicrotask(() => {
        const el = editInlineEl ?? editInlineSelectEl;
        el?.focus();
        if (el instanceof HTMLInputElement) el.select();
      }));
    }
  }

  function rowEditMode(node: TNode): 'primitive' | 'variant' | null {
    if (isEditablePrimitive(node.kind)) return 'primitive';
    const ed = enumDefAt(node.path);
    if (ed && ed.variants.length > 0 &&
        (node.kind === 'unit_variant' || node.kind === 'named_struct' || node.kind === 'named_tuple')) {
      return 'variant';
    }
    return null;
  }

  async function startInlineEditAt(node: TNode) {
    await selectNode(node);
    const mode = rowEditMode(node);
    if (mode === 'primitive')    startEdit('tree');
    else if (mode === 'variant') startVariantEdit('tree');
  }

  function startVariantEdit(location: 'tree' | 'detail' = 'detail') {
    if (!selectedNode) return;
    const ed = enumDefAt(selectedNode.path);
    if (!ed) return;
    editBuf      = selectedNode.variant_tag ?? '';
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
    const name = editBuf;
    editingPid = null;
    editError  = null;
    if (name && name !== selectedNode.variant_tag) {
      await pickVariant(name);
    }
  }

  function cancelEdit() {
    editingPid = null;
    editError  = null;
  }

  async function commitEdit() {
    if (!selectedNode || !editingPid) return;
    const node = selectedNode;
    let value: RonPrimitiveValue;
    try {
      switch (node.kind) {
        case 'string':
          value = { type: 'string', value: editBuf };
          break;
        case 'char': {
          const ch = [...editBuf][0];
          if (!ch) throw new Error('char cannot be empty');
          value = { type: 'char', value: ch };
          break;
        }
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
          let forceFloat = false;
          if (schema) {
            const ty = typeAtPath(node.path);
            if (ty && ty.kind === 'primitive' && (ty.name === 'f32' || ty.name === 'f64')) {
              forceFloat = true;
            }
          }
          const looksInt = /^-?\d+$/.test(s);
          value = (forceFloat || !looksInt)
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
      await ronStudioStore.mutatePrimitive(node.path, value);
      syncTextFromStore();
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

  function onEditKey(e: KeyboardEvent) {
    if (e.key === 'Enter')      { e.preventDefault(); e.stopPropagation(); void commitEdit(); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); cancelEdit(); }
  }

  // ── Schema-driven variant picker + remove ───────────────────────────────
  function enumDefAt(path: string[]): (TypeDef & { kind: 'enum' }) | null {
    if (!schema) return null;
    const ty = typeAtPath(path);
    if (!ty) return null;
    const inner = ty.kind === 'option' ? ty.inner : ty;
    if (inner.kind !== 'named') return null;
    const def = schema.types[inner.path];
    if (!def || def.kind !== 'enum') return null;
    return def;
  }

  function defaultRonText(ty: ResolvedType, depth = 0): string {
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

  function defaultPrimText(n: string): string {
    if (n === 'bool')                                       return 'false';
    if (n === 'String' || n === '&str' || n === 'str')      return '""';
    if (n === 'char')                                       return "' '";
    if (n === '()')                                         return '()';
    if (n.startsWith('f'))                                  return '0.0';
    return '0';
  }

  async function pickVariant(name: string) {
    if (!selectedNode || !schema) return;
    if (name === selectedNode.variant_tag) return;
    const def = enumDefAt(selectedNode.path);
    if (!def) return;
    const v = def.variants.find(x => x.name === name);
    if (!v) return;
    const node = selectedNode;
    const ronText = defaultRonTextForVariant(v);
    try {
      await ronStudioStore.replaceAt(node.path, ronText);
      syncTextFromStore();
      await refreshAfterMutation(node, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: pickVariant failed', e);
    }
  }

  function parentNodeOf(node: TNode): TNode | null {
    if (node.path.length === 0) return null;
    return treePane?.getNode(pathId(node.path.slice(0, -1))) ?? null;
  }

  function isInOrderedContainer(node: TNode): boolean {
    const p = parentNodeOf(node);
    return !!p && (p.kind === 'list' || p.kind === 'tuple' || p.kind === 'named_tuple');
  }

  function addableFieldsAt(node: TNode): { name: string; ty: ResolvedType }[] {
    return missingFieldsFor(node).map(f => ({ name: f.name, ty: f.ty }));
  }

  function itemTypeOf(node: TNode): ResolvedType | null {
    if (!schema) return null;
    const ty = typeAtPath(node.path);
    if (!ty) return null;
    if (ty.kind === 'vec') return ty.inner;
    if (ty.kind === 'map') return ty.value;
    return null;
  }

  function mapKeyTypeOf(node: TNode): ResolvedType | null {
    if (!schema) return null;
    const ty = typeAtPath(node.path);
    if (!ty || ty.kind !== 'map') return null;
    return ty.key;
  }

  async function addFieldAction(parent: TNode, name: string, defaultText: string) {
    try {
      await ronStudioStore.insertField(parent.path, name, defaultText);
      syncTextFromStore();
      await refreshAfterMutation(parent, /* structural */ true);
      if (!expanded.has(parent.pid)) {
        const next = new Set(expanded);
        next.add(parent.pid);
        expanded = next;
      }
      const childPid = pathId([...parent.path, name]);
      const child = treePane?.getNode(childPid);
      if (child && rowEditMode(child)) await startInlineEditAt(child);
      maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }

  async function addItemAction(parent: TNode) {
    const itemTy = itemTypeOf(parent);
    const defaultText = itemTy ? defaultRonText(itemTy) : '()';
    try {
      await ronStudioStore.insertItem(parent.path, defaultText);
      syncTextFromStore();
      await refreshAfterMutation(parent, /* structural */ true);
      if (!expanded.has(parent.pid)) {
        const next = new Set(expanded);
        next.add(parent.pid);
        expanded = next;
      }
      const newIdx = parent.child_count;
      const newPath = [...parent.path, String(newIdx)];
      const child = treePane?.getNode(pathId(newPath));
      if (child && rowEditMode(child)) await startInlineEditAt(child);
      maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }

  async function addMapEntryAction(parent: TNode) {
    const keyRaw = window.prompt('Map key (RON syntax, e.g. "foo" or 42):');
    if (!keyRaw) return;
    const valTy = itemTypeOf(parent);
    const valText = valTy ? defaultRonText(valTy) : '()';
    try {
      await ronStudioStore.insertMapEntry(parent.path, keyRaw, valText);
      syncTextFromStore();
      await refreshAfterMutation(parent, /* structural */ true);
      if (!expanded.has(parent.pid)) {
        const next = new Set(expanded);
        next.add(parent.pid);
        expanded = next;
      }
      maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }

  async function duplicateAction(node: TNode) {
    try {
      await ronStudioStore.duplicateAt(node.path);
      syncTextFromStore();
      const parent = parentNodeOf(node);
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }

  async function moveAction(node: TNode, delta: number) {
    try {
      await ronStudioStore.moveItem(node.path, delta);
      syncTextFromStore();
      const parent = parentNodeOf(node);
      if (parent) await refreshAfterMutation(parent, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      actionError = (e as any)?.message ?? String(e);
    }
  }

  function isRemovable(node: TNode | null): boolean {
    if (!node || node.path.length === 0) return false;
    const parent = treePane?.getNode(pathId(node.path.slice(0, -1)));
    if (!parent) return false;
    return parent.kind === 'struct' || parent.kind === 'named_struct'
        || parent.kind === 'tuple'  || parent.kind === 'named_tuple'
        || parent.kind === 'list'   || parent.kind === 'map';
  }

  async function removeSelected() {
    if (!selectedNode || !isRemovable(selectedNode)) return;
    const node = selectedNode;
    try {
      await ronStudioStore.removeAt(node.path);
      syncTextFromStore();
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: removeAt failed', e);
    }
  }

  // ── Value coloring + dblclick inline edit ───────────────────────────────
  function numberIsFloat(node: TNode): boolean {
    if (schema) {
      const ty = typeAtPath(node.path);
      if (ty && ty.kind === 'primitive') {
        if (ty.name === 'f32' || ty.name === 'f64') return true;
        if (ty.name.startsWith('i') || ty.name.startsWith('u') || ty.name === 'usize' || ty.name === 'isize') return false;
      }
    }
    return /[.eE]/.test(node.preview);
  }

  function previewClassFromText(text: string): string {
    if (!text) return '';
    if (text.startsWith('"') && text.endsWith('"'))        return 'rs-val-string';
    if (text === 'true' || text === 'false')                return 'rs-val-bool';
    if (text.startsWith("'") && text.endsWith("'"))         return 'rs-val-char';
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

  function optionInnerType(node: TNode): ResolvedType | null {
    if (node.kind !== 'option' || !schema) return null;
    const ty = typeAtPath(node.path);
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

  async function onOptionDblClick(node: TNode) {
    const innerTy = optionInnerEditableType(node);
    if (!innerTy) {
      await selectNode(node);
      return;
    }
    const isNone = node.preview === 'None';
    let needsReseed = isNone;
    if (!isNone) {
      if (node.children === null) await treePane?.loadChildren(node);
      const inner = treePane?.getNode(pathId([...node.path, 'Some']));
      if (!inner || inner.kind === 'unit') needsReseed = true;
    }
    if (needsReseed) {
      try {
        const defaultText = defaultRonText(innerTy);
        await ronStudioStore.replaceAt(node.path, `Some(${defaultText})`);
        syncTextFromStore();
        await refreshAfterMutation(node, /* structural */ true);
      } catch (e) {
        console.warn('ron-studio: enable option failed', e);
        return;
      }
    }
    if (node.children === null) await treePane?.loadChildren(node);
    const innerPath = [...node.path, 'Some'];
    const inner = treePane?.getNode(pathId(innerPath));
    if (inner) {
      if (!expanded.has(node.pid)) {
        const next = new Set(expanded);
        next.add(node.pid);
        expanded = next;
      }
      await startInlineEditAt(inner);
    }
  }

  async function onValueDblClick(node: TNode, e: MouseEvent) {
    e.stopPropagation();
    if (node.kind === 'option') { await onOptionDblClick(node); return; }
    if (rowEditMode(node) !== null) { await startInlineEditAt(node); }
  }

  // ── Context menu items (right-click on tree row) ─────────────────────
  function ctxItemsFor(node: TNode): MenuItem[] {
    const items: MenuItem[] = [];
    items.push({ id: 'copy-path',  label: 'Copy path',        icon: LinkIcon, iconColor: 'var(--text-muted)' });
    items.push({ id: 'copy-value', label: 'Copy value (RON)', icon: Copy,     iconColor: 'var(--text-muted)' });

    if (schema) {
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
      const refTarget = refFieldNameForNode(node) ?? refContainerFieldNameForNode(node);
      if (refTarget) {
        const marked = isFieldExplicitlyMarked(refTarget);
        const scope  = refFieldScopeSuffix();
        items.push({ id: 'sep-ref-field', label: '', separator: true } as MenuItem);
        items.push({
          id:    `toggle-ref-field:${refTarget}`,
          label: (marked
            ? `Unmark \`${refTarget}\` as reference field`
            : `Mark \`${refTarget}\` as reference field`) + scope,
          icon:      marked ? Link2Off : LinkIcon,
          iconColor: 'var(--accent)',
        });
      }
    }

    // F12 — Rename across project. Gated on (a) an active project tab
    // (without one there's no project root to refactor against), and
    // (b) the node being a string definition or reference site.
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
    if (schema && typeAtPath(node.path)) {
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
            label:     `${f.name} : ${fmtType(f.ty)}`,
            icon:      Plus,
            iconColor: 'var(--success)',
          });
        }
        if (missing.length > 8) {
          items.push({ id: 'add-field-prompt', label: `${missing.length - 8} more… (type name)`, icon: Plus, iconColor: 'var(--success)' });
        }
      } else if (!schema) {
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
      items.push({
        id:        'move-up',
        label:     'Move up',
        icon:      ArrowUp,
        iconColor: 'var(--text-muted)',
        disabled:  !Number.isFinite(idx) || idx <= 0,
      });
      items.push({
        id:        'move-down',
        label:     'Move down',
        icon:      ArrowDown,
        iconColor: 'var(--text-muted)',
        disabled:  !Number.isFinite(idx) || idx >= total - 1,
      });
    } else if (node.path.length > 0 && (parentNodeOf(node)?.kind === 'struct' || parentNodeOf(node)?.kind === 'named_struct' || parentNodeOf(node)?.kind === 'map')) {
      items.push({ id: 'sep-reorder', label: '', separator: true } as MenuItem);
      items.push({ id: 'duplicate', label: 'Duplicate', icon: CopyPlus, iconColor: 'var(--text-muted)' });
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

  async function onContextMenuSelect(id: string, node: TNode) {
    switch (id) {
      case 'copy-path':      await copyPathOf(node);                                       break;
      case 'copy-value':     await copyValueRonOf(node);                                   break;
      case 'edit':           startEdit('tree');                                            break;
      case 'edit-variant':   startVariantEdit('tree');                                     break;
      case 'toggle-option':  await toggleSelectedOption();                                 break;
      case 'reset':          await resetToDefault(node);                                   break;
      case 'paste':          await pasteOverNode(node);                                    break;
      case 'expand':         expandNode(node, true);                                       break;
      case 'collapse':       expandNode(node, false);                                      break;
      case 'expand-all':     await expandSubtree(node);                                    break;
      case 'collapse-all':   collapseSubtree(node);                                        break;
      case 'remove':         await removeSelected();                                       break;
      case 'view-source': {
        const named = canonicalNamedTypeAt(node.path);
        if (named) await openViewSource(named);
        break;
      }
      case 'rename-across-project': openRenameModal(node);                              break;
      case 'duplicate':      await duplicateAction(node);                                  break;
      case 'move-up':        await moveAction(node, -1);                                   break;
      case 'move-down':      await moveAction(node, +1);                                   break;
      case 'add-item':       await addItemAction(node);                                    break;
      case 'add-map-entry':  await addMapEntryAction(node);                                break;
      case 'add-field-prompt': {
        const name = window.prompt('New field name:');
        if (name) {
          let defaultText = '()';
          if (schema) {
            const ty = typeAtPath(node.path);
            if (ty && ty.kind === 'named') {
              const def = schema.types[ty.path];
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
          if (missing) {
            await addFieldAction(node, fname, defaultRonText(missing.ty));
          }
        } else if (id.startsWith('toggle-ref-field:')) {
          const fname = id.slice('toggle-ref-field:'.length);
          await toggleReferenceFieldForNode(fname);
        }
      }
    }
  }

  async function copyValueRonOf(node: TNode) {
    const id = ronStudioStore.docId;
    if (!id) return;
    try {
      const text = await RON.getValue(id, node.path);
      await navigator.clipboard.writeText(text);
    } catch (e) {
      console.warn('ron-studio: copy value failed', e);
    }
  }

  async function resetToDefault(node: TNode) {
    if (!schema) return;
    const ty = typeAtPath(node.path);
    if (!ty) return;
    try {
      const text = defaultRonText(ty);
      await ronStudioStore.replaceAt(node.path, text);
      syncTextFromStore();
      await refreshAfterMutation(node, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: reset failed', e);
    }
  }

  async function pasteOverNode(node: TNode) {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) return;
      await ronStudioStore.replaceAt(node.path, text);
      syncTextFromStore();
      await refreshAfterMutation(node, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: paste failed', e);
      actionError = (e as any)?.message ?? String(e);
    }
  }

  async function expandSubtree(node: TNode): Promise<void> {
    await treePane?.expandSubtree(node);
  }
  function collapseSubtree(node: TNode): void {
    treePane?.collapseSubtree(node);
  }

  function expandNode(node: TNode, next: boolean): void {
    const set = new Set(expanded);
    if (next) set.add(node.pid); else set.delete(node.pid);
    expanded = set;
    if (next && node.children === null && node.child_count > 0) {
      void treePane?.loadChildren(node);
    }
  }

  async function toggleSelectedOption() {
    if (!selectedNode || selectedNode.kind !== 'option') return;
    const node   = selectedNode;
    const isNone = node.preview === 'None';
    if (isNone) {
      const innerTy = optionInnerType(node);
      if (innerTy) {
        try {
          const defaultText = defaultRonText(innerTy);
          await ronStudioStore.replaceAt(node.path, `Some(${defaultText})`);
          syncTextFromStore();
          await refreshAfterMutation(node, /* structural */ true);
          maybeShowEditBanner();
          return;
        } catch (e) {
          console.warn('ron-studio: schema-default Some failed, falling back to toggle', e);
        }
      }
    }
    try {
      await ronStudioStore.toggleOption(node.path);
      syncTextFromStore();
      await refreshAfterMutation(node, /* structural */ true);
      maybeShowEditBanner();
    } catch (e) {
      console.warn('ron-studio: toggleOption failed', e);
    }
  }


  // ── Text view: CodeMirror 6 host via StudioTextPane ────────────────────
  let textBuf = $state('');
  let textPane: StudioTextPaneController | undefined = $state();

  let pushTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleTextPush() {
    if (pushTimer) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
      void ronStudioStore.setText(textBuf).then(() => {
        void treePane?.reloadTree();
      });
    }, 180);
  }

  function onTextInput(next: string) {
    textBuf = next;
    scheduleTextPush();
  }

  function syncTextFromStore() {
    textBuf = ronStudioStore.current;
  }

  // ── Diff view ──────────────────────────────────────────────────────────
  let diffPane: StudioDiffPaneController | undefined = $state();
  let diffRefreshTick = $state(0);
  let diffHunkCount = $state(0);
  let diffTreeChangeCount = $state(0);
  function bumpDiffRefresh() { diffRefreshTick++; }

  /** Items for the shell's view-mode switcher. Diff badge mirrors the
   *  live structural-change count (or hunk count fallback); Errors
   *  carries a sticky '!' chip that the shell flips to red via the
   *  `errorBadge` data marker. */
  const viewItems = $derived<TabItem[]>([
    { id: 'tree',   label: 'Tree',   icon: ListTree,    title: 'Tree view' },
    { id: 'text',   label: 'Text',   icon: FileText,    title: 'Edit text' },
    { id: 'diff',   label: 'Diff',   icon: GitCompare,  title: 'Diff against original',
      badge: diffTreeChangeCount > 0
        ? diffTreeChangeCount
        : diffHunkCount > 0
          ? diffHunkCount
          : undefined },
    { id: 'errors', label: 'Errors', icon: AlertCircle, title: 'Parse errors',
      disabled: !ronStudioStore.parseError,
      badge: ronStudioStore.parseError ? '!' : undefined,
      data: { errorBadge: !!ronStudioStore.parseError } },
  ]);

  // ── Save / Save As ─────────────────────────────────────────────────────
  let saving       = $state(false);
  let saveError    = $state<string | null>(null);
  let savePickerOpen = $state(false);

  async function doSave() {
    if (!ronStudioStore.sourcePath) { savePickerOpen = true; return; }
    saving = true; saveError = null;
    try {
      await ronStudioStore.save({ path: null, bindToDoc: false });
      bumpDiffRefresh();
      void refreshProjectCrossRefs();
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
      await ronStudioStore.save({ path: p, bindToDoc: true });
      bumpDiffRefresh();
      void refreshProjectCrossRefs();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }

  async function refreshProjectCrossRefs() {
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    await studioStore.loadCrossRefs(tabId, true);
    if (studioStore.settings.use_index && !studioStore.indexJobRunning) {
      void studioStore.refreshIndex(tabId);
    }
  }
  // ── Format + Convert actions ───────────────────────────────────────────
  let actionBusy = $state(false);
  let actionError = $state<string | null>(null);
  let pendingAction = $state<'format' | 'tojson' | null>(null);
  function runFormat() {
    if (!ronStudioStore.docId) return;
    pendingAction = 'format';
  }
  function runToJson() {
    if (!ronStudioStore.docId) return;
    pendingAction = 'tojson';
  }
  async function performPendingAction() {
    const which = pendingAction;
    pendingAction = null;
    const id = ronStudioStore.docId;
    if (!which || !id) return;
    actionBusy = true; actionError = null;
    try {
      const out = which === 'format' ? await RON.format(id) : await RON.toJson(id);
      await ronStudioStore.setText(out);
      syncTextFromStore();
      void treePane?.reloadTree();
      bumpDiffRefresh();
    } catch (e) {
      actionError = String(e);
    } finally {
      actionBusy = false;
    }
  }
  async function runFromJson() {
    const id = ronStudioStore.docId;
    if (!id) return;
    actionBusy = true; actionError = null;
    try {
      const out = await RON.fromJson(id, ronStudioStore.current);
      await ronStudioStore.setText(out);
      syncTextFromStore();
      void treePane?.reloadTree();
      bumpDiffRefresh();
    } catch (e) {
      actionError = String(e);
    } finally {
      actionBusy = false;
    }
  }

  // ── Schema panel ───────────────────────────────────────────────────────
  // After 2B-2.g.fix-1 the Inspector lives in its own sidecar (same
  // pattern as bindings/schema/query) — no more split-aware
  // `detailCollapsed` flag, no more right-border toggle on the tree.
  let schemaProbe    = $state<CrateProbe | null>(null);
  let schemaRsPath   = $state<string | null>(null);
  let schemaLoading  = $state(false);
  let schemaError    = $state<string | null>(null);
  let schemaRootSel  = $state<string | null>(null);
  let schema         = $state<Schema | null>(null);

  async function probeSchemaSource(path: string) {
    schemaRsPath  = path;
    schema        = null;
    schemaRootSel = null;
    schemaError   = null;
    schemaLoading = true;
    try {
      schemaProbe = await RON.schemaProbe(path);
      if (schemaProbe.root_candidates.length === 0) {
        schemaError = `No structs or enums found in ${path}.`;
      } else {
        schemaRootSel = schemaProbe.root_candidates[0].canonical_path;
      }
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }
  function setSchemaRoot(canonical: string) { schemaRootSel = canonical; }
  async function loadSchemaForRoot() {
    if (!schemaRsPath || !schemaRootSel) return;
    schemaLoading = true; schemaError = null;
    try {
      schema = await RON.schemaLoad(schemaRsPath, schemaRootSel);
      stashActiveSchemaHint();
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }

  function stashActiveSchemaHint() {
    const id = ronStudioStore.docId;
    if (!id || !schemaRsPath || !schemaRootSel) return;
    ronStudioWorkspaceStore.setSchemaHint(id, {
      rs_file:   schemaRsPath,
      root_type: schemaRootSel,
      origin:    'directive',
    });
  }
  function clearSchema() {
    schema        = null;
    schemaProbe   = null;
    schemaRsPath  = null;
    schemaRootSel = null;
    schemaError   = null;
  }

  $effect(() => {
    const hint = ronStudioStore.schemaHint;
    if (!hint) return;
    if (schema && schema.crate_manifest && schemaRsPath === hint.rs_file && schema.root_type === hint.root_type) return;
    void autoLoadSchemaFromHint(hint.rs_file, hint.root_type);
  });
  async function autoLoadSchemaFromHint(rsFile: string, rootCanonical: string) {
    schemaRsPath  = rsFile;
    schema        = null;
    schemaError   = null;
    schemaLoading = true;
    try {
      schemaProbe   = await RON.schemaProbe(rsFile);
      schemaRootSel = rootCanonical;
      schema        = await RON.schemaLoad(rsFile, rootCanonical);
      stashActiveSchemaHint();
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }

  function typeAtPath(path: string[]): ResolvedType | null {
    if (!schema) return null;
    let curTy: ResolvedType = { kind: 'named', path: schema.root_type };
    const pathSoFar: string[] = [];
    for (const seg of path) {
      curTy = stepTypeBySegment(curTy, seg, pathSoFar);
      pathSoFar.push(seg);
      if (curTy.kind === 'unknown' || curTy.kind === 'external') return curTy;
    }
    return curTy;
  }

  function fieldNamesAt(path: string[]): Set<string> | null {
    const node = treePane?.getNode(pathId(path));
    if (!node || !node.children) return null;
    return new Set(node.children.map(c => c.key));
  }

  function variantTagAt(path: string[]): string | null {
    const node = treePane?.getNode(pathId(path));
    return node?.variant_tag ?? null;
  }

  function variantInnerStructFields(v: VariantDef): string[] | null {
    if (v.fields.length === 0) return null;
    if (v.shape === 'struct') return v.fields.map(f => f.name);
    if (v.shape === 'tuple' && v.fields.length === 1) {
      const inner = v.fields[0].ty;
      if (inner.kind === 'named' && schema) {
        const innerDef = schema.types[inner.path];
        if (innerDef && innerDef.kind === 'struct') {
          return innerDef.fields.map(f => f.name);
        }
      }
    }
    return null;
  }

  function discriminateVariant(def: TypeDef & { kind: 'enum' }, observed: Set<string>): VariantDef | null {
    type Score = { v: VariantDef; missing: number; extras: number };
    const scored: Score[] = [];
    for (const v of def.variants) {
      const fields = variantInnerStructFields(v);
      if (!fields) continue;
      const fieldSet = new Set(fields);
      let missing = 0;
      let extras  = 0;
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

  /** Resolve `seg` inside a flatten-field's type. Mirrors the helper
   *  in `studio-schema.ts` but inlined here so RON's enum-aware walker
   *  stays self-contained. */
  function stepIntoRonFlatten(ty: ResolvedType, seg: string): ResolvedType | null {
    if (!schema) return null;
    switch (ty.kind) {
      case 'option': return stepIntoRonFlatten(ty.inner, seg);
      case 'map':    return ty.value;  // catch-all-keys → V
      case 'named': {
        const def: TypeDef | undefined = schema.types[ty.path];
        if (!def) return null;
        if (def.kind === 'alias') return stepIntoRonFlatten(def.target, seg);
        if (def.kind !== 'struct') return null;
        const direct = def.fields.find(f =>
          f.name === seg || (f.aliases ?? []).includes(seg));
        if (direct) return direct.ty;
        for (const ff of def.fields) {
          if (!ff.flatten) continue;
          const hit = stepIntoRonFlatten(ff.ty, seg);
          if (hit) return hit;
        }
        return null;
      }
      default: return null;
    }
  }

  function stepTypeBySegment(ty: ResolvedType, seg: string, pathSoFar: string[]): ResolvedType {
    if (!schema) return { kind: 'unknown', hint: 'no schema' };
    switch (ty.kind) {
      case 'option': {
        if (seg === 'Some') return ty.inner;
        return stepTypeBySegment(ty.inner, seg, pathSoFar);
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
        if (def.kind === 'alias') return stepTypeBySegment(def.target, seg, pathSoFar);
        if (def.kind === 'struct') {
          // Direct match by serialised name or alias (handles
          // `#[serde(rename)]` + `#[serde(alias)]` + the original Rust
          // ident promoted to aliases by the host).
          const direct = def.fields.find(f =>
            f.name === seg || (f.aliases ?? []).includes(seg));
          if (direct) return direct.ty;
          // Flatten fallback — same semantics as the shared walker
          // (`studio-schema.ts`): step into each `#[serde(flatten)]`
          // field's type and try to resolve seg there. Map<String,V>
          // catches arbitrary keys; nested structs recurse into their
          // own fields.
          for (const ff of def.fields) {
            if (!ff.flatten) continue;
            const hit = stepIntoRonFlatten(ff.ty, seg);
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
              if (Number.isFinite(idx) && idx >= 0 && idx < v.fields.length) {
                return v.fields[idx].ty;
              }
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
            const v = discriminateVariant(def, observed);
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

  function fmtType(ty: ResolvedType | null): string {
    if (!ty) return '';
    switch (ty.kind) {
      case 'primitive': return ty.name;
      case 'option':    return `Option<${fmtType(ty.inner)}>`;
      case 'vec':       return `Vec<${fmtType(ty.inner)}>`;
      case 'map':       return `Map<${fmtType(ty.key)}, ${fmtType(ty.value)}>`;
      case 'tuple':     return `(${ty.items.map(fmtType).join(', ')})`;
      case 'named':     return ty.path.replace(/^crate::/, '');
      case 'external':  return ty.path + ' (external)';
      case 'unknown':   return `? ${ty.hint}`;
    }
  }

  function typeChipClass(ty: ResolvedType | null): string {
    if (!ty) return '';
    switch (ty.kind) {
      case 'primitive': return 'rs-type-prim';
      case 'option':    return 'rs-type-option';
      case 'vec':       return 'rs-type-vec';
      case 'map':       return 'rs-type-map';
      case 'tuple':     return 'rs-type-tupletype';
      case 'external':  return 'rs-type-external';
      case 'unknown':   return 'rs-type-unknown';
      default:          return '';
    }
  }

  function namedTypeAt(path: string[]): string | null {
    const ty = typeAtPath(path);
    if (!ty || ty.kind !== 'named') return null;
    return ty.path.replace(/^crate::/, '').split('::').pop() ?? null;
  }

  function isTupleStructAt(path: string[]): boolean {
    if (!schema) return false;
    const ty = typeAtPath(path);
    if (!ty || ty.kind !== 'named') return false;
    const def = schema.types[ty.path];
    return !!def && def.kind === 'struct' && def.tuple_like;
  }

  function splitOptionPreview(preview: string): { isNone: boolean; inner: string } {
    if (preview === 'None') return { isNone: true, inner: '' };
    if (preview.startsWith('Some(') && preview.endsWith(')')) {
      return { isNone: false, inner: preview.slice(5, -1) };
    }
    return { isNone: false, inner: preview };
  }

  function missingFieldsFor(node: TNode | null): { name: string; ty: ResolvedType; has_default: boolean }[] {
    if (!schema || !node) return [];
    const ty = typeAtPath(node.path);
    if (!ty || ty.kind !== 'named') return [];
    const def = schema.types[ty.path];
    if (!def || def.kind !== 'struct') return [];
    // Match against serialised name OR alias so a doc that hand-typed
    // the Rust ident doesn't surface as a "missing" field. Flatten
    // sub-struct fields are not walked here (RON rarely uses flatten
    // in practice; the YAML/JSON/TOML/.properties modals delegate to
    // `flattenedStructFields` in `studio-schema.ts`).
    const seenSegs = new Set((node.children ?? []).map(c => c.key));
    return def.fields
      .filter(f => !seenSegs.has(f.name) && !(f.aliases ?? []).some(a => seenSegs.has(a)))
      .map(f => ({ name: f.name, ty: f.ty, has_default: f.has_default }));
  }

  // ── Inspector-panel adapters ────────────────────────────────────────
  function inspectorSchemaTypeInfo(node: TNode): InspectorSchemaTypeInfo | null {
    if (!schema) return null;
    const ty = typeAtPath(node.path);
    if (!ty) return null;
    return {
      label:      fmtType(ty),
      isUnknown:  ty.kind === 'unknown',
      isExternal: ty.kind === 'external',
    };
  }
  function inspectorVariantPickerInfo(node: TNode): InspectorVariantPickerInfo | null {
    if (!schema) return null;
    const def = enumDefAt(node.path);
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
  function inspectorMissingFields(node: TNode): InspectorMissingField[] {
    return missingFieldsFor(node).map(f => ({
      name:       f.name,
      typeLabel:  fmtType(f.ty),
      hasDefault: f.has_default,
    }));
  }
  async function inspectorAddField(parent: TNode, name: string): Promise<void> {
    const missing = missingFieldsFor(parent).find(f => f.name === name);
    if (!missing) return;
    await addFieldAction(parent, name, defaultRonText(missing.ty));
  }

  // ── Open + sync lifecycle ──────────────────────────────────────────────
  // Track ONLY docId here — without `untrack` the body would also depend
  // on `ronStudioStore.current` (via `syncTextFromStore`) and re-fire on
  // every mutation, blowing away the user's tree expansion state. Tree
  // reload after edits is the responsibility of `refreshAfterMutation`
  // (panel-side). The shell's view-mode auto-reset on doc close is wired
  // there — we only handle our own non-tree state here.
  $effect(() => {
    const id = ronStudioStore.docId;
    untrack(() => {
      if (!id) {
        viewMode = 'tree';
        textBuf = '';
        clearSchema();
        saveError = null; actionError = null;
        return;
      }
      syncTextFromStore();
      void RON.setIndent(id, indentUnit).catch(() => { /* best-effort */ });
    });
  });

  // ── Undo / redo wiring ─────────────────────────────────────────────────
  async function doUndo() {
    if (editingPid) { try { await maybeCommitActiveEdit(); } catch { cancelEdit(); } }
    const ok = await ronStudioStore.undo();
    if (ok) await postHistoryStep();
  }
  async function doRedo() {
    if (editingPid) cancelEdit();
    const ok = await ronStudioStore.redo();
    if (ok) await postHistoryStep();
  }
  async function postHistoryStep() {
    syncTextFromStore();
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  // ── Pointer outside active inline edit = implicit commit ────────────────
  // Mirrors the IntelliJ "click elsewhere accepts the value" convention.
  // Failure is silent — `commitEdit` keeps the editor open on validation
  // errors so the user can fix them; we just don't force a cancel here.
  //
  // Note: detail-location commits are owned by <StudioInspectorPanel>;
  // its own pointerdown handler walks the panel's input refs. The
  // tree-location refs (editInlineEl / editInlineSelectEl) live in the
  // row snippet below, so we still bind the listener here for those.
  $effect(() => {
    function onDown(e: PointerEvent) {
      if (!editingPid) return;
      if (editError) return;
      if (editLocation !== 'tree') return;
      const el = editInlineEl ?? editInlineSelectEl;
      if (!el) return;
      const target = e.target as Node | null;
      if (target && (el === target || el.contains(target))) return;
      void maybeCommitActiveEdit();
    }
    window.addEventListener('pointerdown', onDown, true);
    return () => window.removeEventListener('pointerdown', onDown, true);
  });

  // ── Keyboard: tree-view shortcuts (F2 / Delete) + undo / redo ──────────
  $effect(() => {
    function onKey(e: KeyboardEvent) {
      if (!ronStudioStore.open) return;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && !e.altKey && (e.key === 'z' || e.key === 'Z')) {
        e.preventDefault();
        if (e.shiftKey) void doRedo(); else void doUndo();
        return;
      }
      if (mod && !e.altKey && (e.key === 'y' || e.key === 'Y')) {
        e.preventDefault();
        void doRedo();
        return;
      }
      if (mod && !e.altKey && !e.shiftKey && (e.key === 'f' || e.key === 'F') && viewMode === 'tree') {
        if (queryBar) {
          e.preventDefault();
          queryBar.focus();
          return;
        }
      }
      if (viewMode !== 'tree') return;
      if (e.key === 'F3' && queryHits.length > 0) {
        e.preventDefault();
        queryBar?.nav(e.shiftKey ? -1 : 1);
        return;
      }
      if (!selectedNode) return;
      if (editingPid) return;
      if (e.key === 'F2') {
        const mode = rowEditMode(selectedNode);
        if (mode === 'primitive') { e.preventDefault(); startEdit('tree'); }
        else if (mode === 'variant') { e.preventDefault(); startVariantEdit('tree'); }
      } else if (e.key === 'Delete' && isRemovable(selectedNode)) {
        e.preventDefault();
        void removeSelected();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ── Keyboard: F3 / Shift+F3 within diff view ────────────────────────────
  $effect(() => {
    function onKey(e: KeyboardEvent) {
      if (!ronStudioStore.open) return;
      if (viewMode !== 'diff') return;
      if (e.key === 'F3') {
        e.preventDefault();
        diffPane?.nav(e.shiftKey ? -1 : 1);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ── Header + actions ───────────────────────────────────────────────────
  /** Close the modal entirely — releases every open tab's host-side
   *  state and resets the workspace store, so reopening starts fresh
   *  (no stale tabs pointing at closed docs). */
  async function close() {
    const tabs = [...ronStudioWorkspaceStore.tabs];
    for (const t of tabs) {
      try { await ronStudioWorkspaceStore.closeTab(t.docId); } catch { /* ignore */ }
    }
    ronStudioWorkspaceStore.closeFolder();
    await ronStudioStore.closeDoc();
  }

  function fmtBytes(n: number | null): string {
    if (n == null) return '';
    return fsFmtBytes(n);
  }

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

  const rsBasename = fsBasename;
</script>

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
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'bindings'}
      onclick={() => studioModal?.toggleRightPane('bindings')}
      use:tooltip={(() => {
        const n        = (studioStore.config.overrides?.length ?? 0) + (studioStore.config.default ? 1 : 0);
        const inFile   = docBrokenRefs.length;
        const inRepo   = studioStore.brokenRefs.length;
        if (inFile > 0) {
          return `Schema bindings — ${n} configured · ${inFile} broken reference${inFile === 1 ? '' : 's'} in this file`;
        }
        if (inRepo > 0) {
          return `Schema bindings — ${n} configured · ${inRepo} broken elsewhere in this repo (see Studio sidebar)`;
        }
        return n > 0
          ? `Schema bindings — ${n} configured`
          : 'Schema bindings — none configured yet';
      })()}
      aria-label="Bindings"
      aria-pressed={rightPane === 'bindings'}
    >
      <Layers size={20} />
      {#if docBrokenRefs.length > 0}
        <span class="rs-rail-dot rs-rail-dot-warn"></span>
      {:else if (studioStore.config.overrides?.length ?? 0) > 0 || studioStore.config.default}
        <span class="rs-rail-dot"></span>
      {/if}
    </button>
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
      class:ab-active={rightPane === 'schema'}
      onclick={() => studioModal?.toggleRightPane('schema')}
      use:tooltip={schema ? `Schema: ${schema.root_name} (${schema.stats.resolved} types)` : 'Load Rust schema'}
      aria-label="Schema panel"
      aria-pressed={rightPane === 'schema'}
    >
      <BookOpen size={20} />
      {#if schema}<span class="rs-rail-dot"></span>{/if}
    </button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'query'}
      onclick={() => {
        if (rightPane === 'query') {
          queryAutoOpenDismissed = true;
        }
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
        <span class="rs-rail-count" aria-hidden="true">{queryHits.length >= 100 ? '99+' : queryHits.length}</span>
      {/if}
    </button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'tools'}
      onclick={() => studioModal?.toggleRightPane('tools')}
      use:tooltip={'Tools — Format / Indent / Convert'}
      aria-label="Tools"
      aria-pressed={rightPane === 'tools'}
    >
      <Wrench size={20} />
    </button>
  {/snippet}

  {#snippet headerLeft()}
    <span class="rs-header-icon-wrap" aria-hidden="true">
      <Icon icon={ronIcon} width={18} height={18} />
    </span>
    <!-- Undo / redo sit to the LEFT of the workspace tab strip (multi-
         doc case) or the title cluster (single-doc case). Renders on
         the modal-header chrome bg so the buttons read as part of the
         titlebar. -->
    <StudioHeaderUndoRedo doc={footerDoc} onUndo={doUndo} onRedo={doRedo} />
    <!-- Title + size only when the workspace tab strip is hidden
         (single-doc case). With multiple tabs the strip already
         carries the file name + dirty dot, so we drop the redundant
         label and let the strip eat the entire middle of the header. -->
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
        <button
          type="button"
          class="rs-tab-plus"
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
      {#snippet footer({ close })}
        <button
          type="button"
          class="rs-bindings-disk"
          onclick={() => { close(); openWorkspaceFilePicker(); }}
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
        {#if schema}
          <button class="rs-footer-pill rs-footer-pill-schema"
                  onclick={() => setRightPane(rightPane === 'schema' ? null : 'schema')}
                  use:tooltip={schema.root_type}
                  aria-label="Open schema panel">
            <BookOpen size={11} />
            <span>{schema.root_name}</span>
          </button>
        {:else if schemaLoading}
          <span class="rs-footer-pill rs-footer-pill-schema rs-footer-pill-schema-loading"
                use:tooltip={schemaRsPath ?? 'Loading schema…'}>
            <BookOpen size={11} /> loading…
          </span>
        {/if}
        <button class="rs-footer-pill rs-footer-pill-refs"
                onclick={() => {
                  const tid = tabsStore.activeTabId;
                  if (tid && !studioStore.indexJobRunning) {
                    void studioStore.refreshIndex(tid);
                  }
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
      {saving}
      onSave={() => void doSave()}
      onSaveAs={() => void openSaveAs()}
    />
  {/snippet}

  {#snippet bodyBanners()}
    <StudioBodyBanners {saveError} {actionError} />
  {/snippet}

  {#snippet queryBarSlot()}
    <!-- ── Query bar (Tree view only) ────────────────────────────
         Full-RFC-9535 JSONPath subset over the parsed RON AST.
         Owned by `<StudioQueryBar>`: ghost autocomplete (Tab),
         ↑/↓ inside the input, Enter to run, Esc to clear, F3 /
         Shift+F3 from outside the input via `queryBar.nav(±1)`.
         Wrapper injects RON-specific kind badges + Expand /
         Collapse toolbar buttons via snippets and provides the
         jump-to-tree navigation through `onJumpToHit`. -->
    <StudioQueryBar
      bind:this={queryBar}
      formatId="ron"
      backend={RON}
      docId={ronStudioStore.docId}
      visible={viewMode === 'tree' && !ronStudioStore.parseError}
      placeholder='Query — name (recursive), $.foo.bar, $.arr[0:5], $..[?@.$type == "Goblin"]…'
      historyStorageKey="arbor:ron-studio:query-history"
      knownKeys={knownKeys}
      getChildKeysForPath={getChildKeysForPath}
      ensureChildrenLoaded={ensureChildrenLoadedForPath}
      onJumpToHit={(path) => void jumpToQueryHit(path)}
      rightPaneOpen={rightPane === 'query'}
      onToggleRightPane={onQueryToggleRightPane}
      onActiveChange={onQueryActiveChange}
      onHits={(hits) => noteKeys(hits)}
      bulkEditEnabled={true}
      onBulkEditRequest={(q) => openBulkEditModal(q)}
      bind:query
      bind:queryHits
      bind:querying
      bind:queryError
      bind:currentHitIdx
    >
      {#snippet kindChip(kind)}
        <span class="rs-row-badge rs-row-badge-{kind}" use:tooltip={kind}>{kindBadge(kind)}</span>
      {/snippet}
      {#snippet toolbarRight()}
        <button
          type="button"
          class="rs-query-tool-btn"
          onclick={() => void expandAll()}
          disabled={expandAllBusy}
          use:tooltip={'Recursively load + expand every container (capped at 5000 nodes for large docs)'}
          aria-label="Expand all"
        >{#if expandAllBusy}<Loader2 size={12} class="rs-query-spinner" />{:else}<ChevronsDown size={12} />{/if}<span>Expand</span></button>
        <button
          type="button"
          class="rs-query-tool-btn"
          onclick={collapseAll}
          use:tooltip={'Collapse all (root stays open)'}
          aria-label="Collapse all"
        ><ChevronsUp size={12} /><span>Collapse</span></button>
      {/snippet}
    </StudioQueryBar>
  {/snippet}

  {#snippet bodyMain()}
    {#if viewMode === 'tree'}
      <!-- Tree pane fills the main card on its own — the Inspector
           moved out to a sidecar (see `inspectorSidecar` snippet
           below) so it isn't clipped under the query bar that lives
           above this view. The right border on the tree is gone:
           the sidecar's own card chrome already provides the visual
           seam. -->
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
            {@const ty = typeAtPath(n.path)}
            {@const tupleStruct = isTupleStructAt(n.path)}
            {@const namedType = namedTypeAt(n.path)}
            {@const opt = n.kind === 'option' ? splitOptionPreview(n.preview) : null}

            <span class="rs-row-badge rs-row-badge-{n.kind}"
              class:rs-row-badge-tuple-struct={tupleStruct}
              class:rs-row-badge-none={opt?.isNone}
              use:tooltip={tupleStruct ? 'tuple struct' : (opt?.isNone ? 'None (Option)' : n.kind)}
            >{tupleStruct ? '()' : (opt?.isNone ? '∘' : kindBadge(n.kind))}</span>

            <span class="rs-row-key" class:rs-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
            <span class="rs-row-sep">:</span>

            <!-- Variant / struct-type tag preserved from source.
                 Rendered as the FIRST piece of the value column so
                 `element: Dark` reads as "Dark" rather than "()",
                 and `variant: Action(…)` reads as "Action(…)". -->
            {#if n.variant_tag && !(editingPid === n.pid && editLocation === 'tree')}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="rs-row-tag"
                    ondblclick={(e) => void onValueDblClick(n, e)}
                    use:tooltip={rowEditMode(n) === 'variant' ? 'Double-click to change variant' : 'Variant / struct tag from source'}
              >{n.variant_tag}</span>
            {/if}

            {#if editingPid === n.pid && editLocation === 'tree'}
              {#if rowEditMode(n) === 'variant'}
                {@const ed = enumDefAt(n.path)}
                {#if ed}
                  <select class="rs-inline-edit rs-inline-edit-variant"
                          bind:this={editInlineSelectEl}
                          bind:value={editBuf}
                          onchange={() => void commitVariantEdit()}
                          onkeydown={(e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); cancelEdit(); } }}
                          onclick={(e) => e.stopPropagation()}
                          onmousedown={(e) => e.stopPropagation()}>
                    {#each ed.variants as v (v.name)}
                      <option value={v.name}>
                        {v.name}{v.shape === 'unit' ? '' : v.shape === 'tuple' ? '(…)' : ' { … }'}
                      </option>
                    {/each}
                  </select>
                {/if}
              {:else if n.kind === 'bool'}
                <select class="rs-inline-edit"
                        bind:this={editInlineSelectEl}
                        bind:value={editBuf}
                        onkeydown={onEditKey}
                        onclick={(e) => e.stopPropagation()}
                        onmousedown={(e) => e.stopPropagation()}>
                  <option value="true">true</option>
                  <option value="false">false</option>
                </select>
              {:else}
                <input class="rs-inline-edit"
                       bind:this={editInlineEl}
                       bind:value={editBuf}
                       onkeydown={onEditKey}
                       onclick={(e) => e.stopPropagation()}
                       onmousedown={(e) => e.stopPropagation()}
                       placeholder={n.kind === 'char' ? 'single char' : ''}
                       spellcheck="false" />
              {/if}
              {#if editError}
                <span class="rs-inline-edit-err" use:tooltip={editError}>!</span>
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
              <!-- The tag IS the value for unit variants; skip the
                   redundant preview. -->
            {:else}
              {@const xrefs = crossRefsForNode(n)}
              {@const hasX = xrefs.length > 0}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <span class="rs-row-preview {previewClass(n)}"
                    class:rs-row-preview-editable={rowEditMode(n) !== null}
                    class:rs-row-preview-xref={hasX}
                    ondblclick={(e) => void onValueDblClick(n, e)}
                    onclick={hasX ? ((e) => onCrossRefClick(xrefs, e)) : undefined}
                    use:tooltip={hasX
                      ? (xrefs.length === 1
                          ? `Ctrl+click → ${xrefs[0].title} (${xrefs[0].defPath.join('.')})`
                          : `Ctrl+click → choose between ${xrefs.length} matches`)
                      : (rowEditMode(n) === 'primitive' ? 'Double-click to edit'
                        : rowEditMode(n) === 'variant'  ? 'Double-click to change variant'
                        : '')}
              >{n.preview}{#if hasX}<span class="rs-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="rs-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
            {/if}

            {#if namedType}
              <span class="rs-row-named" use:tooltip={fmtType(ty)}>{namedType}</span>
            {:else if ty}
              <span class="rs-row-type {typeChipClass(ty)}"
                use:tooltip={fmtType(ty)}
              >{fmtType(ty)}</span>
            {/if}
          {/snippet}
      </StudioTreePane>

    {:else if viewMode === 'text'}
      <StudioTextPane
        bind:this={textPane}
        language="ron"
        value={textBuf}
        oninput={onTextInput}
      >
        {#snippet footer()}
          <span>{textBuf.length} chars · {textBuf.split('\n').length} lines</span>
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
        refreshTick={diffRefreshTick}
        storageKey="arbor:ron-studio:diff-sub"
        bind:treeChangeCount={diffTreeChangeCount}
        bind:hunkCount={diffHunkCount}
      >
        {#snippet tagChip(tag, position)}
          <span class="rs-row-tag" class:rs-row-tag-before={position === 'before'}>{tag}</span>
        {/snippet}
      </StudioDiffPane>

    {:else if viewMode === 'errors'}
      <div class="rs-errors-pane">
        {#if ronStudioStore.parseError}
          <div class="rs-error-box">
            <AlertCircle size={14} />
            <span class="rs-error-text">{ronStudioStore.parseError}</span>
          </div>
          <p class="rs-error-hint">
            Switch to <strong>Text</strong> to fix the document. The tree and diff views will update live as soon as it parses again.
          </p>
        {:else}
          <StateBlock tone="success" label="No parse errors." />
        {/if}
      </div>
    {/if}
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="ron"
      backend={RON}
      sourcePath={ronStudioStore.sourcePath}
      onOpenDefinition={openDefinition}
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
    <!-- Lives in its own sidecar (next to the main card) so it
         spans the entire body height and isn't clipped under the
         query bar that sits inside the main card. The panel renders
         its own empty state when no node is selected (notably in
         Text/Diff/Errors view, where there's no selectedNode). -->
    <StudioInspectorPanel
      bind:this={inspectorPanel}
      formatId="ron"
      backend={RON}
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
      isBoolKind={(k: RonNodeKind) => k === 'bool'}
      isCharKind={(k: RonNodeKind) => k === 'char'}
      isOptionKind={(k: RonNodeKind) => k === 'option'}
      isContainerKind={isContainerKindRon as any}
      isDefinitionNode={isDefinitionNode as any}
      definitionValue={definitionValue as any}
      schemaTypeInfo={inspectorSchemaTypeInfo as any}
      variantPickerInfo={inspectorVariantPickerInfo as any}
      missingFields={inspectorMissingFields as any}
      onCopyPath={copyPathOf as any}
      onCopyValue={copyValue}
      onRemove={removeSelected}
      onStartEdit={startEdit}
      onCommitEdit={commitEdit}
      onCancelEdit={cancelEdit}
      onPickVariant={pickVariant}
      onAddField={inspectorAddField as any}
      onToggleOption={toggleSelectedOption}
      onDismissEditBanner={dismissEditBanner}
      onJumpToUsage={jumpToUsage}
      onSelectChild={(c) => void selectNode(c as TNode)}
    />
  {/snippet}

  {#snippet querySidecar()}
    <div class="rs-panel-head">
      <ListFilter size={13} />
      <span class="rs-panel-title">Query results</span>
      {#if queryHits.length > 0}
        <span class="rs-panel-count">{queryHits.length}{queryHits.length >= 500 ? '+' : ''}</span>
      {/if}
      <span class="rs-spacer"></span>
    </div>
    <div class="rs-query-pane-body">
      {#if !query.trim()}
        <p class="rs-query-pane-empty">
          Type in the search bar at the top of the tree view
          to populate this list. Supports the same JSONPath
          subset shown in the input's placeholder.
        </p>
      {:else if querying && queryHits.length === 0}
        <div class="rs-query-pane-status">
          <Spinner size="xs" /> <span>Running query…</span>
        </div>
      {:else if queryError}
        <div class="rs-query-pane-error">
          <AlertCircle size={11} /> {queryError}
        </div>
      {:else if queryHits.length === 0}
        <p class="rs-query-pane-empty">No matches.</p>
      {:else}
        <div class="rs-query-pane-list">
          {#each queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="rs-query-pane-card"
              class:active={i === currentHitIdx}
              onclick={() => { currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="rs-query-pane-card-head">
                <span class="rs-row-badge rs-row-badge-{hit.kind}" use:tooltip={hit.kind}>{kindBadge(hit.kind)}</span>
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
  {/snippet}

  {#snippet schemaSidecar()}
    <StudioSchemaPanel
      formatId="ron"
      backend={RON}
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
    {#if savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save RON document as"
        extensions={['ron']}
        initialPath={ronStudioStore.sourcePath ?? undefined}
        initialFilename={rsBasename(ronStudioStore.sourcePath) || 'document.ron'}
        onConfirm={onSaveAsPicked}
        onCancel={() => savePickerOpen = false}
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

    {#if viewSource || viewSourceBusy || viewSourceErr}
      <StudioViewSourceModal
        viewSource={viewSource}
        busy={viewSourceBusy}
        err={viewSourceErr}
        language="rust"
        loadingLabel="Re-parsing crate…"
        decorateLine={(line) => {
          // Mirror of the old `decorateRustSource` regex: match the
          // field-name token at the head of a `pub? field: …` line, then
          // gate it through `isRefFieldName` so the ref-field convention
          // (or the user's custom patterns) drives the gutter highlight.
          const m = line.match(/^\s*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_]\w*)\s*:/);
          const name = m?.[1];
          return name && isRefFieldName(name) ? name : null;
        }}
        onClose={closeViewSource}
      />
    {/if}

    {#if renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={RON}
        tabId={tabsStore.activeTabId}
        formatLabel="RON"
        oldValue={renameModalState.oldValue}
        openDocs={buildOpenDocsSnapshot()}
        onClose={closeRenameModal}
        onApplied={onRenameApplied}
      />
    {/if}

    {#if bulkEditModalState && tabsStore.activeTabId && ronStudioStore.docId}
      <StudioBulkEditModal
        backend={RON}
        tabId={tabsStore.activeTabId}
        docId={ronStudioStore.docId}
        formatLabel="RON"
        query={bulkEditModalState.query}
        nullPolicy="not_supported"
        openDocs={buildBulkEditOpenDocs()}
        onClose={closeBulkEditModal}
        onApplied={onBulkEditApplied}
      />
    {/if}

    {#if crossRefPicker}
      <!-- Portalled to document.body — Modal.svelte's `transition:fly`
           applies a transform to `.modal`, which creates a containing
           block for `position: fixed` descendants. Inside that context,
           `clientX/clientY` coords (viewport-relative) end up offset by
           the modal's transformed origin and the picker either lands
           off-screen or behind clipping. Re-parenting to body sidesteps
           the stacking + coordinate-space tangle entirely. -->
      <div use:portal>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="rs-xref-overlay" onclick={() => crossRefPicker = null}></div>
        <div class="rs-xref-popover"
             style="left: {crossRefPicker.x}px; top: {crossRefPicker.y}px;"
             role="menu"
             aria-label="Cross-reference matches"
        >
          <div class="rs-xref-header">{crossRefPicker.entries.length} matches</div>
          {#each crossRefPicker.entries as entry, i (i)}
            <button class="rs-xref-item"
                    role="menuitem"
                    onclick={() => void jumpToCrossRef(entry)}
                    use:tooltip={entry.sourcePath || entry.title}>
              <span class="rs-xref-item-icon" aria-hidden="true">
                <Icon icon={ronIcon} width={13} height={13} />
              </span>
              <span class="rs-xref-item-name">{entry.fileName}</span>
              <span class="rs-xref-item-path">{entry.defPath.join('.')}</span>
              {#if entry.docId}
                <span class="rs-xref-item-open" use:tooltip={'Already open'}>•</span>
              {/if}
            </button>
          {/each}
        </div>
      </div>
    {/if}
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
  /* ── Header bits (rendered straight inside Modal.svelte's
     .modal-header via the shell's `headerLeft` snippet) ──
     Modal already applies its chrome background + padding, we just
     sit our header bits inline next to the mac-close-btn. */
  :global(.rs-header-icon) { color: var(--accent); flex-shrink: 0; }
  .rs-header-icon-wrap {
    display: inline-flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }

  /* Right-anchored close button for the View-source aux modal — small
     left margin so it doesn't crowd the adjacent control. */
  .rs-close-right { margin-left: 10px; }
  .rs-title  { font-size: 13px; font-weight: 600; max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: inline-flex; align-items: center; gap: 6px; }
  .rs-dirty  { color: var(--accent); font-size: 14px; line-height: 1; }
  .rs-meta   { color: var(--text-muted); font-size: 11px; display: inline-flex; align-items: center; gap: 3px; }
  .rs-spacer { flex: 1; }

  /* ── Footer ─────────────────────────────────────────────────────────── */
  /* Most of the footer styling moved to the shared <StudioFooter*>
     components. RON keeps the schema/refs pill rules (used inside
     <StudioFooterStatus> as `extras` snippet). The Convert dropdown
     trigger now lives in the left tools rail as an `.ab-btn`, so no
     local button style needed. */
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

  /* Rust source preview inside the "View implementation" modal —
     reuses prism-rust tokens via the .language-rust class. */
  .rs-source-pre {
    margin: 0;
    padding: 14px 18px;
    background: var(--bg-base);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.55;
    height: 100%;
    overflow: auto;
    white-space: pre;
  }

  .rs-source-wrap {
    display: flex; flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .rs-source-banner {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 14px;
    background: color-mix(in srgb, var(--accent) 10%, var(--bg-base));
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--accent);
    flex-shrink: 0;
  }
  .rs-source-pre-decorated {
    padding: 8px 0;
    height: auto;
    flex: 1;
    overflow: auto;
  }
  .rs-source-line {
    display: flex;
    align-items: flex-start;
    min-height: 1.55em;
  }
  .rs-source-line-ref {
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }
  .rs-source-gutter {
    display: inline-flex;
    align-items: center; justify-content: center;
    width: 28px;
    flex-shrink: 0;
    color: var(--accent);
    opacity: 0.85;
  }
  .rs-source-code {
    flex: 1;
    min-width: 0;
    padding-right: 18px;
    white-space: pre;
  }

  /* Decorative "schema loaded" indicator dot — sits inside the schema
     rail button (an .ab-btn from the shared ActivityBar). Positioned
     relative to that button via the parent's relative positioning. */
  .rs-rail-dot {
    position: absolute;
    top: 4px; right: 4px;
    width: 6px; height: 6px;
    border-radius: 50%;
    background: var(--success, #98c379);
  }
  .rs-rail-dot-warn { background: var(--warning, #e5c07b); }

  /* ── Query results sidebar ──────────────────────────────────────── */
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
  .rs-query-pane-list {
    display: flex; flex-direction: column;
    gap: 4px;
  }
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

  /* Rail count badge on the Query Results icon — sits on the
     bottom-right of the 38px button, mirrors how the schema/
     bindings dot indicators are positioned. */
  .rs-rail-count {
    position: absolute;
    bottom: 1px;
    right: 1px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    background: var(--accent);
    color: var(--bg-base);
    border-radius: 8px;
    font-family: var(--font-code);
    font-size: 9px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
  }

  /* Panel count chip — small badge next to the title in the
     Query Results header. */
  .rs-panel-count {
    display: inline-flex; align-items: center;
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 600;
    padding: 0 6px;
    height: 16px;
    border-radius: 8px;
    color: var(--accent);
    background: var(--accent-subtle);
  }

  /* "+" tab-strip launcher — appended after the last tab. Same
     visual rhythm as a ghost rail button so it doesn't compete with
     the file tabs for attention, but the accent border on hover/open
     makes the affordance discoverable. */
  .rs-tab-plus {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px;
    height: 24px;
    margin-left: 4px;
    padding: 0;
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

  /* "Browse disk…" footer inside the Open dropdown — sits below the
     project file list as the explicit "I want the file picker"
     escape hatch. Borderless, transparent, with subtle hover. */
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

  /* ── Tab strip (workspace multi-file) ───────────────────────────────── */
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

  /* Thin wrapper around the shared <Alert compact> banners — preserves
     the vertical breathing room the legacy `.rs-banner` had so the
     alert doesn't crowd the toolbar above it. */
  .rs-banner-wrap { margin: 6px 8px 0; }

  /* (`.rs-split` + `.rs-detail-pane` removed — the Inspector lives in
     its own sidecar (`inspectorSidecar` snippet) so the main card now
     hosts the tree alone, full width.) */

  /* ── Standard panel header (used by Query Results sidecar) ────────────
     Aligns 1:1 with the main app sidebar's `PanelShell` ps-header
     pattern: 34px tall, transparent so the panel's own --bg-base
     shows through, icon + small-caps title + actions slot. */
  .rs-panel-head {
    display: flex; align-items: center; gap: 6px;
    padding: 0 8px 0 12px;
    height: 34px;
    min-height: 34px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .rs-panel-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rs-panel-head > :global(svg:first-child) { color: var(--accent); flex-shrink: 0; }

  /* Row chips, badges, etc. — match JsonStudio's pill rhythm */
  .rs-row-badge {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 22px; height: 16px; padding: 0 4px;
    border-radius: 3px;
    font-family: var(--font-code); font-size: 9px; font-weight: 600; line-height: 1;
    flex-shrink: 0;
  }
  .rs-row-badge-struct, .rs-row-badge-map, .rs-row-badge-tuple, .rs-row-badge-list {
    background: color-mix(in srgb, var(--syntax-keyword) 18%, transparent);
    color: var(--syntax-keyword);
  }
  .rs-row-badge-string, .rs-row-badge-char {
    background: color-mix(in srgb, var(--syntax-string) 18%, transparent);
    color: var(--syntax-string);
  }
  .rs-row-badge-number {
    background: color-mix(in srgb, var(--syntax-number) 18%, transparent);
    color: var(--syntax-number);
  }
  .rs-row-badge-bool, .rs-row-badge-option {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .rs-row-badge-unit { background: var(--bg-overlay); color: var(--text-muted); }
  .rs-row-badge-named_struct,
  .rs-row-badge-named_tuple,
  .rs-row-badge-unit_variant {
    background: color-mix(in srgb, var(--syntax-type, var(--accent)) 18%, transparent);
    color: var(--syntax-type, var(--accent));
  }
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
  }
  .rs-row-badge-tuple-struct {
    background: color-mix(in srgb, var(--syntax-number) 18%, transparent);
    color: var(--syntax-number);
  }
  .rs-row-badge-none {
    background: var(--bg-overlay);
    color: var(--text-disabled);
  }

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

  .rs-inline-edit {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: 11px;
    height: 18px; line-height: 18px;
    padding: 0 6px;
    background: var(--bg-base);
    color: var(--text-primary);
    border: 1px solid var(--accent);
    border-radius: 3px;
    outline: none;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .rs-inline-edit-variant {
    min-width: 140px;
    cursor: pointer;
  }
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

  /* ── Cross-ref multi-match picker ─────────────────────────────────────
     Floating popover anchored to the click position when one id resolves
     to defs in multiple files. Rendered at the modal root so it can
     escape the body's overflow:hidden. */
  .rs-xref-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: transparent;
    cursor: default;
  }
  .rs-xref-popover {
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
  .rs-xref-header {
    padding: 4px 8px 6px;
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 2px;
  }
  .rs-xref-item {
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
  .rs-xref-item:hover { background: var(--bg-hover); }
  .rs-xref-item-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .rs-xref-item-name {
    flex: 1;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-ui-sans);
    font-weight: 500;
  }
  .rs-xref-item-path {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .rs-xref-item-open {
    color: var(--accent);
    font-size: 14px;
    line-height: 1;
    margin-left: 2px;
  }

  .rs-row-tag { cursor: pointer; }
  .rs-row-tag:hover { filter: brightness(1.15); }

  .rs-inline-edit-err {
    margin-left: 4px;
    width: 14px; height: 14px;
    display: inline-flex; align-items: center; justify-content: center;
    font-size: 10px; font-weight: 700;
    color: #fff;
    background: var(--error, #e06c75);
    border-radius: 50%;
    cursor: help;
  }
  .rs-row-type {
    margin-left: auto;
    color: var(--text-secondary);
    font-family: var(--font-code); font-size: 10px;
    padding: 1px 6px;
    background: var(--bg-overlay); border-radius: 8px;
    flex-shrink: 0;
  }
  .rs-row-type.rs-type-prim {
    color:      var(--syntax-type, #61afef);
    background: color-mix(in srgb, var(--syntax-type, #61afef) 14%, var(--bg-overlay));
  }
  .rs-row-type.rs-type-option {
    color:      var(--syntax-keyword, #d19a66);
    background: color-mix(in srgb, var(--syntax-keyword, #d19a66) 14%, var(--bg-overlay));
  }
  .rs-row-type.rs-type-vec {
    color:      var(--syntax-function, #c678dd);
    background: color-mix(in srgb, var(--syntax-function, #c678dd) 14%, var(--bg-overlay));
  }
  .rs-row-type.rs-type-map {
    color:      var(--syntax-char, #56b6c2);
    background: color-mix(in srgb, var(--syntax-char, #56b6c2) 14%, var(--bg-overlay));
  }
  .rs-row-type.rs-type-tupletype {
    color:      var(--syntax-decimal, #e5c07b);
    background: color-mix(in srgb, var(--syntax-decimal, #e5c07b) 14%, var(--bg-overlay));
  }
  .rs-row-type.rs-type-unknown  {
    color:      var(--warning, #d19a66);
    background: color-mix(in srgb, var(--warning, #d19a66) 18%, transparent);
  }
  .rs-row-type.rs-type-external {
    color:      var(--text-disabled);
    background: var(--bg-overlay);
    font-style: italic;
  }
  .rs-row-named {
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

  /* ── Text view footer pills (rendered via StudioTextPane's `footer`
     snippet — its scope is the parent's, so the classes belong here). */
  .rs-text-err { color: var(--danger, #e06c75); }
  .rs-text-ok  { color: var(--success, #98c379); }

  /* ── Diff view ────────────────────────────────────────────────────────
     Body + toolbar moved into <StudioDiffPane>. The "before" modifier on
     the variant-tag chip stays here because it lives on the parent's
     `rs-row-tag` element, which is rendered by the `tagChip` snippet we
     pass to the pane. */
  .rs-row-tag-before { opacity: 0.7; text-decoration: line-through; }

  /* ── Errors view ────────────────────────────────────────────────────── */
  .rs-errors-pane { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .rs-error-box {
    display: flex; gap: 10px; align-items: flex-start;
    padding: 12px;
    background: color-mix(in srgb, var(--danger, #e06c75) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger, #e06c75) 30%, transparent);
    border-radius: var(--radius-sm);
    color: var(--danger, #e06c75);
  }
  .rs-error-text { font-family: var(--font-code); font-size: 12px; }
  .rs-error-hint { color: var(--text-muted); font-size: 12px; line-height: 1.6; }

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
