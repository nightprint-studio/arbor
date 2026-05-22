<!--
  PropertiesStudioModal — `.properties` wrapper around the generic
  `<StudioModal>` shell (Phase 6).

  Mirrors YamlStudioModal closely:
    · Lossless line-based parse + edit on the host (every byte of the
      original survives a round-trip — comments, blank lines, separator
      whitespace, continuation backslashes, Unicode escapes).
    · Tree view with mutations (edit primitives, remove, add field/item,
      duplicate, move). F2 inline edit; schema-aware variant picker on
      enum-typed string leaves.
    · Text view via `<StudioTextPane>` (CodeMirror 6 + hand-rolled
      `.properties` StreamLanguage in studio-codemirror.ts).
    · Diff view via `<StudioDiffPane>`.
    · Errors banner — shows the dotted-key-conflict warning when the BE
      surfaces one.
    · Inspector + Query + Bindings + Schema right-rail panes.
    · Query bar with F13 `[⚡ Edit]` entry.
    · F12 cross-ref rename (`null_handling = AskUser`,
      `cross_ref_scopes = [Key, Value]` — every key is a target, every
      value is a reference).
    · Schema-aware tree decoration: type chips + ↗ xref jumps.

  FROZEN F4 / F5:
    · `.properties` has no native typing — every leaf is a string. The
      kind enum carries `null` for parity with the bulk-edit modal
      hint, but actual leaves emit `string` from the BE.
    · Every flat dotted key is a cross-ref target; every value is a
      potential reference. No `id`/`name` heuristic.
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
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { propertiesStudioStore, type PropertiesNodeKind } from '$lib/stores/properties-studio.svelte';
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
  // Shared schema-aware walker — handles serde rename / alias /
  // rename_all / flatten (incl. HashMap<String,V> catch-all flatten
  // common in Spring Boot configs).
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
  type PropsQueryHit = StudioQueryHit<PropertiesNodeKind>;
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
   *  schema-aware narrowing in `commitEdit` lets the user type
   *  number/bool when the schema says so — we coerce back to string
   *  on the wire because the format itself has no typing. */
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

  const EDIT_BANNER_KEY = 'arbor:properties-studio:edit-warning-dismissed';
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
    if (selectedNode.kind === 'string' && enumDefAt(selectedNode.path)) {
      startVariantEdit(location);
      return;
    }
    if (!isEditablePrimitive(selectedNode.kind) && !isPromotableNull(selectedNode.kind)) return;
    let seed = valueText ?? selectedNode.preview;
    // `.properties` previews don't quote — strip just in case.
    if (selectedNode.kind === 'string' && seed.startsWith('"') && seed.endsWith('"')) {
      try { seed = JSON.parse(seed) as string; }
      catch { seed = seed.slice(1, -1); }
    }
    if (selectedNode.kind === 'null') seed = '';
    editBuf      = seed;
    editError    = null;
    editingPid   = selectedNode.pid;
    editLocation = location;
    maybeShowEditBanner();
    if (location === 'detail') {
      inspectorPanel?.focusEditInput();
    } else {
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

    // Schema-aware narrowing — `.properties` has no native typing on
    // the wire, but the schema can hint that this leaf SHOULD be an
    // int/float/bool. We pass the typed primitive to the BE so the
    // tree projection updates correctly; the actual on-disk value is
    // still the string representation.
    const hint = schema ? primitiveHintAt(node.path) : null;
    const wantFloat  = hint === 'f32' || hint === 'f64' || hint === 'number';
    const wantInt    = hint === 'integer'
      || (hint != null && (hint.startsWith('i') || hint.startsWith('u'))
          && hint !== 'isize' && hint !== 'usize')
      || hint === 'isize' || hint === 'usize';
    const wantBool   = hint === 'bool' || hint === 'boolean';
    const _wantString = hint === 'string' || hint === 'String' || hint === '&str' || hint === 'str';

    let value: StudioPrimitiveValue;
    try {
      if (wantBool) {
        const t = editBuf.trim().toLowerCase();
        if (t !== 'true' && t !== 'false') throw new Error('schema: expected boolean');
        value = { type: 'bool', value: t === 'true' };
      } else if (wantInt) {
        const s = editBuf.trim();
        const n = Number(s);
        if (!Number.isFinite(n) || !Number.isInteger(n)) throw new Error('schema: expected integer');
        value = { type: 'int', value: Math.trunc(n) };
      } else if (wantFloat) {
        const s = editBuf.trim();
        const n = Number(s);
        if (!Number.isFinite(n)) throw new Error('schema: expected number');
        value = { type: 'float', value: n };
      } else {
        value = { type: 'string', value: editBuf };
      }
    } catch (e: any) {
      editError = e?.message ?? String(e);
      return;
    }
    try {
      await propertiesStudioStore.mutatePrimitive(node.path, value);
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
      await propertiesStudioStore.removeAt(node.path);
      await refreshAfterMutation(node, /* structural */ true, /* removed */ true);
      maybeShowEditBanner();
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
      maybeShowEditBanner();
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
      maybeShowEditBanner();
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
      maybeShowEditBanner();
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
      maybeShowEditBanner();
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
      maybeShowEditBanner();
    } catch (e: any) {
      uiStore.showToast(`Paste failed: ${e?.message ?? e}`, 'error');
    }
  }

  // ── F12 — Cross-refs / Rename across project ──────────────────────────
  //
  // FROZEN F5: every key is a cross-ref target, every value is a
  // potential reference. We surface "Rename across project…" on every
  // string-valued tree leaf whose preview is non-empty.

  function isRenameableTreeNode(n: TNode): boolean {
    if (n.kind !== 'string') return false;
    return n.preview.length > 0;
  }

  let renameModalState = $state<{ oldValue: string } | null>(null);

  function openRenameModal(node: TNode): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Rename across project',
        'No active project — open this .properties file from a project tab to rename across files.',
        'warning',
      );
      return;
    }
    const value = node.preview;
    if (!value) return;
    renameModalState = { oldValue: value };
  }
  function closeRenameModal(): void { renameModalState = null; }

  function buildOpenDocsSnapshot(): RenameOpenDoc[] {
    if (!propertiesStudioStore.docId) return [];
    return [{
      doc_id:      propertiesStudioStore.docId,
      source_path: propertiesStudioStore.sourcePath,
      dirty:       propertiesStudioStore.dirty,
    }];
  }

  async function reloadActiveDocFromDisk(): Promise<void> {
    const path = propertiesStudioStore.sourcePath;
    if (!path) return;
    const title = propertiesStudioStore.title;
    await propertiesStudioStore.openDoc({ path, title });
    await treePane?.reloadTree();
    bumpDiffRefresh();
  }

  async function onRenameApplied(result: RenameResult): Promise<void> {
    closeRenameModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    const sp = propertiesStudioStore.sourcePath;
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
      try { await studioStore.loadCrossRefsForKind(aTab, 'properties', true); } catch { /* soft */ }
      try { await studioStore.refreshIndex(aTab); }                            catch { /* soft */ }
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
    const sp = propertiesStudioStore.sourcePath;
    const sameFile = sp && hit.absolute_path.replace(/\\/g, '/').toLowerCase()
                       === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(hit.field_path);
      return;
    }
    try {
      await propertiesStudioStore.openDoc({ path: hit.absolute_path });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(hit.field_path);
    } catch (e) {
      console.warn('jumpToUsage: open target failed', e);
    }
  }

  async function openDefinition(d: CrossRefDef): Promise<void> {
    const sp = propertiesStudioStore.sourcePath;
    const sameFile = sp && d.absolute_path.replace(/\\/g, '/').toLowerCase()
                       === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(d.def_path);
      return;
    }
    try {
      await propertiesStudioStore.openDoc({ path: d.absolute_path });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(d.def_path);
    } catch (e) {
      console.warn('openDefinition: open target failed', e);
    }
  }

  // ── F13 — Bulk edit by query ─────────────────────────────────────────
  //
  // `null_handling = AskUser`: the FE surfaces the policy hint, and the
  // BE collapses literal-null into an empty-value write. "Remove key
  // entirely" is reachable via the `Delete` action.

  let bulkEditModalState = $state<{ query: string } | null>(null);

  function openBulkEditModal(q: string): void {
    if (!tabsStore.activeTabId) {
      notificationsStore.add(
        'Bulk edit by query',
        'No active project — open this .properties file from a project tab to run a bulk edit.',
        'warning',
      );
      return;
    }
    if (!propertiesStudioStore.docId) return;
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
        await propertiesStudioStore.applyExternalMutate(result.active_doc_state);
        await treePane?.reloadTree();
      } catch (e) {
        console.warn('bulk edit: active-doc sync failed', e);
      }
    } else {
      const sp = propertiesStudioStore.sourcePath;
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
        try { await studioStore.loadCrossRefsForKind(aTab, 'properties', true); } catch { /* soft */ }
        try { await studioStore.refreshIndex(aTab); }                            catch { /* soft */ }
      }
    }

    const appliedTxt = `${result.applied_sites} ${result.applied_sites === 1 ? 'site' : 'sites'}`;
    const skippedTxt = result.skipped_sites > 0 ? ` (${result.skipped_sites} skipped)` : '';
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

  // ── Schema sidecar (JSON Schema only) ─────────────────────────────────

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
      const probe = await PROPS_BE.schemaProbe(rsPath);
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
      schema = await PROPS_BE.schemaLoad(schemaRsPath, schemaRootSel);
    } catch (e) {
      schemaError = String(e);
    } finally {
      schemaLoading = false;
    }
  }
  function clearSchema(): void {
    schema = null; schemaProbe = null; schemaRsPath = null;
    schemaRootSel = null; schemaError = null;
  }

  // ── View source (JSON Schema fragment) ──────────────────────────────
  let viewSource:    TypeSource | null = $state(null);
  let viewSourceBusy                  = $state(false);
  let viewSourceErr: string | null    = $state(null);

  async function openViewSource(canonical: string): Promise<void> {
    if (!schemaRsPath) return;
    viewSourceBusy = true; viewSourceErr  = null; viewSource     = null;
    try {
      viewSource = await PROPS_BE.schemaViewSource(schemaRsPath, canonical);
    } catch (e) {
      viewSourceErr = String(e);
    } finally {
      viewSourceBusy = false;
    }
  }
  function closeViewSource(): void { viewSource = null; viewSourceErr = null; }

  // ── Schema-aware type walker — delegates to the shared helper.
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
    return node.preview ?? '';
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
      await propertiesStudioStore.mutatePrimitive(node.path, { type: 'string', value: name });
      await refreshAfterMutation(node, /* structural */ false);
    } catch (e: any) {
      editError = e?.message ?? String(e);
    }
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
      editError = e?.message ?? String(e);
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
    // Use the shared helper so flattened sub-struct fields surface
    // alongside direct fields. Match by serialised name OR alias so a
    // doc that hand-typed the Rust ident doesn't double-list.
    const seenSegs = new Set((node.children ?? []).map((c: TNode) => c.key));
    return flattenedStructFields(schema, def)
      .filter(f => !seenSegs.has(f.name) && !(f.aliases ?? []).some(a => seenSegs.has(a)))
      .map(f => ({
        name:       f.name,
        typeLabel:  fmtType(f.ty),
        hasDefault: f.has_default,
      }));
  }

  function typeChipClass(ty: ResolvedType | null): string {
    if (!ty) return '';
    switch (ty.kind) {
      case 'primitive': return 'ps-type-prim';
      case 'option':    return 'ps-type-option';
      case 'vec':       return 'ps-type-vec';
      case 'map':       return 'ps-type-map';
      case 'tuple':     return 'ps-type-tupletype';
      case 'external':  return 'ps-type-external';
      case 'unknown':   return 'ps-type-unknown';
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

  // ── Cross-reference click affordance ─────────────────────────────────
  type CrossRefEntry = {
    sourcePath: string;
    fileName:   string;
    defPath:    string[];
    title:      string;
  };

  function crossRefsForValue(value: string): CrossRefEntry[] {
    return studioStore.findCrossRefsForKind(value, 'properties').map(d => ({
      sourcePath: d.absolute_path,
      fileName:   d.file_name,
      defPath:    (d.def_path && d.def_path.length > 0) ? d.def_path : [d.def_field],
      title:      d.file_name,
    }));
  }

  /** FROZEN F5: every string leaf is a potential reference. We only
   *  render the ↗ chip when the value actually matches a known key
   *  in the index — that's the signal that this string is "pointing
   *  somewhere" rather than an arbitrary literal. */
  function crossRefsForNode(node: TNode): CrossRefEntry[] {
    if (node.kind !== 'string') return [];
    const value = node.preview;
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
    const sp = propertiesStudioStore.sourcePath;
    const sameFile = sp && target.sourcePath.replace(/\\/g, '/').toLowerCase()
                      === sp.replace(/\\/g, '/').toLowerCase();
    if (sameFile) {
      await treePane?.jumpToPath(target.defPath);
      return;
    }
    try {
      await propertiesStudioStore.openDoc({ path: target.sourcePath });
      await treePane?.reloadTree();
      await treePane?.jumpToPath(target.defPath);
    } catch (e) {
      console.warn('jumpToCrossRef: open target failed', e);
    }
  }

  function onCrossRefClick(entries: CrossRefEntry[], e: MouseEvent): void {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault(); e.stopPropagation();
    if (entries.length === 1) {
      void jumpToCrossRef(entries[0]);
    } else if (entries.length > 1) {
      crossRefPicker = { x: e.clientX, y: e.clientY, entries };
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
      case 'copy-path':    await copyPathOf(node);                          break;
      case 'copy-value':   {
        try {
          const v = await PROPS_BE.getValue(propertiesStudioStore.docId!, node.path);
          await copyToClipboard(v);
        } catch { /* ignore */ }
        break;
      }
      case 'edit':         startEdit('tree');                                break;
      case 'edit-variant': startVariantEdit('tree');                         break;
      case 'view-impl':    {
        const ty = typeAtPath(node.path);
        let p: string | null = null;
        if (ty?.kind === 'named') p = ty.path;
        else if (ty?.kind === 'option' && ty.inner.kind === 'named') p = ty.inner.path;
        if (p) void openViewSource(p);
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
      case 'rename-across-project': openRenameModal(node);                    break;
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
  let queryHits     = $state<PropsQueryHit[]>([]);
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
      void propertiesStudioStore.setText(textBuf).then(() => {
        void treePane?.reloadTree();
        bumpDiffRefresh();
      });
    }, 180);
  }

  function onTextInput(next: string) {
    textBuf = next;
    scheduleTextPush();
  }

  $effect(() => {
    const c = propertiesStudioStore.current;
    untrack(() => { textBuf = c; });
  });

  $effect(() => {
    const id = propertiesStudioStore.docId;
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
  $effect(() => {
    if (!propertiesStudioStore.open) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    untrack(() => { void studioStore.loadCrossRefsForKind(tabId, 'properties'); });
  });

  // Auto-load schema from the .arbor/studio.toml binding.
  $effect(() => {
    const hint = propertiesStudioStore.schemaHint;
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
      schemaProbe   = await PROPS_BE.schemaProbe(rsFile);
      schemaRootSel = rootCanonical;
      schema        = await PROPS_BE.schemaLoad(rsFile, rootCanonical);
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
  let saving         = $state(false);
  let saveError      = $state<string | null>(null);
  let savePickerOpen = $state(false);

  async function doSave(): Promise<void> {
    if (!propertiesStudioStore.sourcePath) { savePickerOpen = true; return; }
    await runSave();
  }
  async function runSave(): Promise<void> {
    saving = true; saveError = null;
    try {
      await propertiesStudioStore.save({ path: null, bindToDoc: false });
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
      await propertiesStudioStore.save({ path: p, bindToDoc: true });
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
    await propertiesStudioStore.closeDoc();
  }

  // fmtBytes / jsBasename moved to shared/studio/helpers.ts; aliased
  // locally to keep existing call sites readable.
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
  function isBoolKind(_k: PropertiesNodeKind): boolean { return false; }

  // ── Keyboard shortcuts ─────────────────────────────────────────────────
  function onKey(e: KeyboardEvent) {
    if (!propertiesStudioStore.open) return;
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
      if (!inEditableField) { e.preventDefault(); void doUndo(); }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'z' || e.key === 'Z')) {
      if (!inEditableField) { e.preventDefault(); void doRedo(); }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === 'y' || e.key === 'Y')) {
      if (!inEditableField) { e.preventDefault(); void doRedo(); }
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
        if (mode === 'variant') { e.preventDefault(); startVariantEdit('tree'); }
        else if (mode === 'primitive') { e.preventDefault(); startEdit('tree'); }
        else if (isPromotableNull(selectedNode.kind)) { e.preventDefault(); startEdit('tree'); }
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
    const ok = await propertiesStudioStore.undo();
    if (ok) {
      await treePane?.reloadTree();
      bumpDiffRefresh();
    }
  }
  async function doRedo() {
    const ok = await propertiesStudioStore.redo();
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
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'inspector'}
      onclick={() => studioModal?.toggleRightPane('inspector')}
      use:tooltip={'Inspector — selected node detail (Tree view)'}
      aria-label="Inspector"
      aria-pressed={rightPane === 'inspector'}
    ><ScanSearch size={20} /></button>
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
        <span class="ps-rail-count" aria-hidden="true">{queryHits.length >= 100 ? '99+' : queryHits.length}</span>
      {/if}
    </button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'bindings'}
      onclick={() => studioModal?.toggleRightPane('bindings')}
      use:tooltip={'Bindings & broken refs — project-wide cross-references'}
      aria-label="Bindings & broken refs"
      aria-pressed={rightPane === 'bindings'}
    ><Layers size={20} /></button>
    <button type="button" class="ab-btn"
      class:ab-active={rightPane === 'schema'}
      onclick={() => studioModal?.toggleRightPane('schema')}
      use:tooltip={schema
        ? `Schema — ${schema.root_name} (${Object.keys(schema.types).length} types)`
        : 'Schema — bind a JSON Schema file'}
      aria-label="Schema"
      aria-pressed={rightPane === 'schema'}
    ><BookOpen size={20} /></button>
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
      {saving}
      onSave={() => void doSave()}
      onSaveAs={openSaveAs}
    />
  {/snippet}

  {#snippet bodyBanners()}
    <StudioBodyBanners {saveError} {actionError}>
      {#snippet extras()}
        {#if propertiesStudioStore.parseError}
          <div class="ps-banner-wrap"><Alert variant="warning" compact text={propertiesStudioStore.parseError} /></div>
        {/if}
      {/snippet}
    </StudioBodyBanners>
  {/snippet}

  {#snippet queryBarSlot()}
    <StudioQueryBar
      bind:this={queryBar}
      formatId="properties"
      backend={PROPS_BE}
      docId={propertiesStudioStore.docId}
      visible={viewMode === 'tree' && !propertiesStudioStore.parseError}
      placeholder='Query — server.port, $..host, $.servers[0], …'
      historyStorageKey="arbor:properties-studio:query-history"
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
        <span class="ps-row-badge ps-row-badge-{kind}" use:tooltip={kind}>{kindBadge(kind)}</span>
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
          {@const ty = typeAtPath(n.path)}
          {@const namedType = namedTypeAt(n.path)}
          <span class="ps-row-badge ps-row-badge-{n.kind}" use:tooltip={n.kind}>{kindBadge(n.kind)}</span>
          {#if n.key === '$value'}
            <span class="ps-row-key ps-row-key-self"
                  use:tooltip={'Value at the parent prefix — `.properties` allows a key to be both a leaf and a sub-key prefix.'}>(self)</span>
          {:else}
            <span class="ps-row-key" class:ps-row-key-index={/^\d+$/.test(n.key)}>{n.key}</span>
          {/if}
          <span class="ps-row-sep">=</span>
          {#if editingPid === n.pid && editLocation === 'tree'}
            {#if rowEditMode(n) === 'variant'}
              {@const ed = enumDefAt(n.path)}
              {#if ed}
                <select class="ps-inline-edit ps-inline-edit-variant"
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
            {:else}
              <input class="ps-inline-edit"
                     bind:this={editInlineEl}
                     bind:value={editBuf}
                     onkeydown={onEditKey}
                     onclick={(e) => e.stopPropagation()}
                     onmousedown={(e) => e.stopPropagation()}
                     placeholder={n.kind === 'null' ? 'Type a value…' : ''}
                     spellcheck="false" />
            {/if}
            {#if editError}
              <span class="ps-inline-edit-err" use:tooltip={editError}>!</span>
            {/if}
          {:else}
            {@const xrefs = crossRefsForNode(n)}
            {@const hasX = xrefs.length > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <span class="ps-row-preview ps-row-preview-{n.kind}"
                  class:ps-row-preview-editable={rowEditMode(n) !== null || isPromotableNull(n.kind)}
                  class:ps-row-preview-xref={hasX}
                  ondblclick={(e) => {
                    if (!rowEditMode(n) && !isPromotableNull(n.kind)) return;
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
                      : isPromotableNull(n.kind)       ? 'Double-click to fill'
                      : '')}
            >{n.preview}{#if hasX}<span class="ps-row-xref" aria-hidden="true"><ArrowUpRight size={11} strokeWidth={2.4} />{#if xrefs.length > 1}<span class="ps-row-xref-count">{xrefs.length}</span>{/if}</span>{/if}</span>
          {/if}
          {#if n.loading}<Loader2 size={10} class="ps-row-loader" />{/if}
          {#if namedType}
            <span class="ps-row-named" use:tooltip={fmtType(ty)}>{namedType}</span>
          {:else if ty && ty.kind !== 'named'}
            <span class="ps-row-type {typeChipClass(ty)}"
              use:tooltip={fmtType(ty)}
            >{fmtType(ty)}</span>
          {/if}
        {/snippet}
      </StudioTreePane>
    {:else if viewMode === 'text'}
      <StudioTextPane
        value={textBuf}
        language="properties"
        oninput={onTextInput}
      />
    {:else if viewMode === 'diff'}
      <StudioDiffPane
        bind:this={diffPane}
        formatId="properties"
        backend={PROPS_BE}
        docId={propertiesStudioStore.docId}
        visible={viewMode === 'diff'}
        currentText={propertiesStudioStore.current}
        refreshTick={diffRefreshTick}
        bind:treeChangeCount={diffTreeChangeCount}
        bind:hunkCount={diffHunkCount}
      >
        {#snippet tagChip(_tag, _position)}
          <!-- .properties has no variant tags. -->
        {/snippet}
      </StudioDiffPane>
    {:else if viewMode === 'errors'}
      {#if propertiesStudioStore.parseError}
        <div class="ps-errors">
          <div class="ps-errors-head">
            <AlertCircle size={14} />
            <span>Parse warning</span>
          </div>
          <pre class="ps-errors-body">{propertiesStudioStore.parseError}</pre>
          <p class="ps-errors-hint">
            Dotted-key conflicts happen when the same prefix is used as
            both a leaf and a container (e.g. <code>foo=string</code> and
            <code>foo.sub=value</code>). The tree falls back to a flat
            view so every key stays editable. Resolve by renaming one of
            the colliding keys.
          </p>
        </div>
      {:else}
        <div class="ps-errors-empty">
          <Check size={16} />
          <span>No parse warnings.</span>
        </div>
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
      isDefinitionNode={((n: TNode) => n.kind === 'string' && n.preview.length > 0) as any}
      definitionValue={((n: TNode) => n.kind === 'string' && n.preview ? n.preview : null) as any}
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
    <div class="ps-panel-head">
      <ListFilter size={13} />
      <span class="ps-panel-title">Query results</span>
      {#if queryHits.length > 0}
        <span class="ps-panel-count">{queryHits.length}{queryHits.length >= 500 ? '+' : ''}</span>
      {/if}
      <span class="ps-spacer"></span>
    </div>
    <div class="ps-query-pane-body">
      {#if !query.trim()}
        <p class="ps-query-pane-empty">
          Type in the search bar at the top of the tree view to populate
          this list. Supports the JSONPath subset shown in the input's
          placeholder.
        </p>
      {:else if querying && queryHits.length === 0}
        <div class="ps-query-pane-status"><Spinner size="xs" /> <span>Running query…</span></div>
      {:else if queryError}
        <div class="ps-query-pane-error"><AlertCircle size={11} /> {queryError}</div>
      {:else if queryHits.length === 0}
        <p class="ps-query-pane-empty">No matches.</p>
      {:else}
        <div class="ps-query-pane-list">
          {#each queryHits as hit, i (hit.path.join('\x00'))}
            <button
              type="button"
              class="ps-query-pane-card"
              class:active={i === currentHitIdx}
              onclick={() => { currentHitIdx = i; void jumpToQueryHit(hit.path); }}
            >
              <div class="ps-query-pane-card-head">
                <span class="ps-row-badge ps-row-badge-{hit.kind}" use:tooltip={hit.kind}>{kindBadge(hit.kind)}</span>
                <span class="ps-query-pane-card-idx">#{i + 1}</span>
              </div>
              <div class="ps-query-pane-card-path">{hit.path.length === 0 ? '$' : '$.' + hit.path.join('.')}</div>
              {#if hit.preview}<div class="ps-query-pane-card-preview">{hit.preview}</div>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/snippet}

  {#snippet bindingsSidecar()}
    <StudioRefsPanel
      formatId="properties"
      backend={PROPS_BE}
      sourcePath={propertiesStudioStore.sourcePath}
      onOpenDefinition={openDefinition}
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
    {#if savePickerOpen}
      <FilePickerModal
        mode="save"
        title="Save .properties document as"
        extensions={['properties']}
        initialPath={propertiesStudioStore.sourcePath ?? undefined}
        initialFilename={jsBasename(propertiesStudioStore.sourcePath) || 'application.properties'}
        onConfirm={onSaveAsPicked}
        onCancel={() => savePickerOpen = false}
      />
    {/if}

    {#if renameModalState && tabsStore.activeTabId}
      <StudioRenameModal
        backend={PROPS_BE}
        tabId={tabsStore.activeTabId}
        formatLabel=".properties"
        oldValue={renameModalState.oldValue}
        openDocs={buildOpenDocsSnapshot()}
        onClose={closeRenameModal}
        onApplied={onRenameApplied}
      />
    {/if}

    {#if bulkEditModalState && tabsStore.activeTabId && propertiesStudioStore.docId}
      <StudioBulkEditModal
        backend={PROPS_BE}
        tabId={tabsStore.activeTabId}
        docId={propertiesStudioStore.docId}
        formatLabel=".properties"
        query={bulkEditModalState.query}
        nullPolicy="ask_user"
        openDocs={buildBulkEditOpenDocs()}
        onClose={closeBulkEditModal}
        onApplied={onBulkEditApplied}
      />
    {/if}

    {#if crossRefPicker}
      <div class="ps-xref-overlay"
           use:portal
           role="presentation"
           onclick={() => crossRefPicker = null}
           oncontextmenu={(e) => { e.preventDefault(); crossRefPicker = null; }}
      >
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="ps-xref-popover"
             style:left="{crossRefPicker.x}px"
             style:top="{crossRefPicker.y}px"
             role="menu"
             onclick={(e) => e.stopPropagation()}
        >
          <div class="ps-xref-header">{crossRefPicker.entries.length} matches</div>
          {#each crossRefPicker.entries as entry (entry.sourcePath + entry.defPath.join('\x00'))}
            <button type="button" class="ps-xref-item"
              onclick={() => void jumpToCrossRef(entry)}
            >
              <span class="ps-xref-item-icon"><FileTextIcon size={13} /></span>
              <span class="ps-xref-item-name">{entry.title}</span>
              <span class="ps-xref-item-path">{entry.defPath.join('.')}</span>
              <span class="ps-xref-item-open">›</span>
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

  .ps-rail-count {
    position: absolute;
    bottom: 2px; right: 2px;
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

  /* The shared <StudioFooter*> components own the pill / button / sep
     CSS now. The only Properties-specific footer rule that remains is
     the custom selected-path pill (for the `$value` self-marker
     override) and the inline body-banner wrapper. */
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

  .ps-row-badge {
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 18px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10px; font-weight: 700;
    flex-shrink: 0;
  }
  .ps-row-badge-object,
  .ps-row-badge-array  { color: var(--syntax-type, #4d78cc); }
  .ps-row-badge-string { color: var(--syntax-string, #6a9956); }
  .ps-row-badge-null   { color: var(--text-muted); font-style: italic; }
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

  .ps-inline-edit {
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
  .ps-inline-edit:focus {
    border-color: var(--accent-strong, var(--accent));
    box-shadow: 0 0 0 2px var(--accent-subtle);
  }
  .ps-inline-edit-err {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    border-radius: 50%;
    background: var(--bg-error, rgba(255, 90, 80, 0.18));
    color: var(--text-error, #ff6c5c);
    font-size: 11px; font-weight: 700;
    margin-left: 4px;
    cursor: help;
  }
  .ps-inline-edit-variant { background: var(--bg-base); color: var(--syntax-keyword, #cc7832); padding-right: 18px; }
  .ps-row-preview-editable { cursor: text; }

  .ps-errors {
    padding: 16px;
    display: flex; flex-direction: column; gap: 8px;
    height: 100%;
    overflow: auto;
  }
  .ps-errors-head {
    display: flex; align-items: center; gap: 6px;
    color: var(--warning, #d19a66);
    font-size: 12px;
    font-weight: 600;
  }
  .ps-errors-body {
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
  .ps-errors-hint { color: var(--text-muted); font-size: 11px; margin: 0; line-height: 1.5; }
  .ps-errors-hint code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }
  .ps-errors-empty {
    display: flex; align-items: center; gap: 6px;
    padding: 16px;
    color: var(--text-secondary);
    font-size: 12px;
  }

  .ps-panel-head {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-secondary);
  }
  .ps-panel-title { font-weight: 600; color: var(--text-primary); }
  .ps-panel-count {
    background: var(--bg-overlay);
    color: var(--text-muted);
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
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
  .ps-query-spinner { animation: ps-spin 1s linear infinite; }
  @keyframes ps-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  .ps-schema-hint { color: var(--text-secondary); font-size: 11px; line-height: 1.5; margin: 0; }
  .ps-schema-hint code {
    font-family: var(--font-code);
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-overlay);
    color: var(--text-primary);
  }

  .ps-row-type {
    margin-left: auto;
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 10px;
    padding: 1px 6px;
    background: var(--bg-overlay);
    border-radius: 8px;
    flex-shrink: 0;
  }
  .ps-row-type.ps-type-prim {
    color:      var(--syntax-type, #61afef);
    background: color-mix(in srgb, var(--syntax-type, #61afef) 14%, var(--bg-overlay));
  }
  .ps-row-type.ps-type-option {
    color:      var(--syntax-keyword, #d19a66);
    background: color-mix(in srgb, var(--syntax-keyword, #d19a66) 14%, var(--bg-overlay));
  }
  .ps-row-type.ps-type-vec {
    color:      var(--syntax-function, #c678dd);
    background: color-mix(in srgb, var(--syntax-function, #c678dd) 14%, var(--bg-overlay));
  }
  .ps-row-type.ps-type-map {
    color:      var(--syntax-char, #56b6c2);
    background: color-mix(in srgb, var(--syntax-char, #56b6c2) 14%, var(--bg-overlay));
  }
  .ps-row-type.ps-type-tupletype {
    color:      var(--syntax-decimal, #e5c07b);
    background: color-mix(in srgb, var(--syntax-decimal, #e5c07b) 14%, var(--bg-overlay));
  }
  .ps-row-type.ps-type-unknown {
    color:      var(--warning, #d19a66);
    background: color-mix(in srgb, var(--warning, #d19a66) 18%, transparent);
  }
  .ps-row-type.ps-type-external {
    color:      var(--text-disabled);
    background: var(--bg-overlay);
    font-style: italic;
  }
  .ps-row-named {
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

  .ps-row-preview-xref { cursor: pointer; }
  .ps-row-xref {
    display: inline-flex; align-items: center; gap: 2px;
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

  .ps-xref-overlay {
    position: fixed; inset: 0;
    z-index: 60;
    background: transparent;
    cursor: default;
  }
  .ps-xref-popover {
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
  .ps-xref-header {
    padding: 4px 8px 6px;
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 2px;
  }
  .ps-xref-item {
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
  .ps-xref-item:hover { background: var(--bg-hover); }
  .ps-xref-item-icon { display: inline-flex; align-items: center; flex-shrink: 0; }
  .ps-xref-item-name {
    flex: 1;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-ui-sans);
    font-weight: 500;
  }
  .ps-xref-item-path {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .ps-xref-item-open {
    color: var(--accent);
    font-size: 14px;
    line-height: 1;
    margin-left: 2px;
  }
</style>
