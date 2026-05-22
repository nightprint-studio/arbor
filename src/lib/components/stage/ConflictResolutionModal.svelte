<script lang="ts">
  import { tick } from 'svelte';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalSidebarToggle from '$lib/components/shared/ui/ModalSidebarToggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import { encodingOverrides } from '$lib/stores/encodingOverrides.svelte';
  import {
    AlertTriangle, CheckCircle2, GitMerge, Archive, XCircle,
    ChevronLeft, ChevronRight, ChevronUp, ChevronDown, X, TriangleAlert, PackageCheck, Eye,
    Equal, List, FolderTree, Folder, GitBranch, FileText,
  } from 'lucide-svelte';
  import { tabsStore }  from '$lib/stores/tabs.svelte';
  import { repoStore }  from '$lib/stores/repo.svelte';
  import { uiStore }    from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { diffStore }  from '$lib/stores/diff.svelte';
  import {
    getConflictContent, resolveConflict, resolveStashConflict,
    completeMerge, abortMerge, getMergeMessage, getConflictPresence,
    removeConflictFile,
  } from '$lib/ipc/merge';
  import { abortStashApply, forceStashApply, getStashFileContent, writeWorkdirFile, checkoutBranch, stashApply, stashDrop } from '$lib/ipc/branch';
  import { getStatus } from '$lib/ipc/stage';
  import { applyPostCheckout } from '$lib/utils/applyPostCheckout';
  import { highlight }  from '$lib/utils/diff-formatter';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { matchesBinding } from '$lib/utils/keybindings';
  import type { ConflictContent, StashBlockingContent } from '$lib/types/git';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // ---------------------------------------------------------------------------
  // Props
  // ---------------------------------------------------------------------------

  let { mode }: { mode: 'merge' | 'stash' } = $props();
  const isMerge = $derived(mode === 'merge');
  // True only when there's a genuine merge in progress (MERGE_HEAD present).
  // False when the modal opens on an "orphan" conflict state — unmerged
  // index entries left over from an aborted op. In that case the UI drops
  // all merge-specific wording/controls and uses a generic "resolve" vocabulary.
  const isRealMerge = $derived(isMerge && (repoStore.status?.is_merging ?? false));

  // ---------------------------------------------------------------------------
  // Region types
  // ---------------------------------------------------------------------------

  type ContextRegion  = { kind: 'context'; lines: string[] };
  type ConflictRegion = {
    kind: 'conflict';
    id: number;
    oursLines:   string[];
    theirsLines: string[];
    oursLabel:   string;
    theirsLabel: string;
  };
  type Region = ContextRegion | ConflictRegion;

  // ---------------------------------------------------------------------------
  // Blocking mode types
  // ---------------------------------------------------------------------------

  type BlockingData = {
    content:        StashBlockingContent | null;
    regions:        Region[];
    /** Per-conflict region: which ours lines are selected */
    oursSelected:   Record<number, boolean[]>;
    /** Per-conflict region: which theirs lines are selected */
    theirsSelected: Record<number, boolean[]>;
    /** Non-null = user manually typed result, overrides computed */
    manualResult:   string | null;
  };

  type ContextDisplayItem = {
    kind: 'context';
    lines: string[];
    oursStart: number;
    theirsStart: number;
  };
  type ConflictDisplayItem = {
    kind: 'conflict';
    regionId: number;
    oursLines: string[];
    theirsLines: string[];
    oursStart: number;
    theirsStart: number;
    oursSelected: boolean[];
    theirsSelected: boolean[];
  };
  // Placeholder row for context blocks too big to render fully — keeps the
  // DOM small (huge files used to freeze the app while Svelte rendered
  // thousands of <div>s plus per-line Prism highlights). Click to expand.
  type CollapsedContextDisplayItem = {
    kind: 'collapsed';
    contextKey: string;       // stable id to track expansion state
    hiddenLines: number;
    oursStart: number;
    theirsStart: number;
  };
  type DisplayItem = ContextDisplayItem | ConflictDisplayItem | CollapsedContextDisplayItem;

  // Clipping thresholds — context blocks larger than CONTEXT_MAX get
  // truncated to CONTEXT_HEAD_TAIL lines at top + bottom with a clickable
  // placeholder in the middle.
  const CONTEXT_MAX       = 30;
  const CONTEXT_HEAD_TAIL = 12;

  // User-expanded context blocks (key = `${file}|${contextIdx}`).
  let expandedContextKeys = $state(new Set<string>());

  function emitContext(
    lines:       string[],
    oursStart:   number,
    theirsStart: number,
    contextKey:  string,
    into:        DisplayItem[],
  ) {
    // Honor the global "Show full file" preference: when on, always emit the
    // full context regardless of the size threshold or per-block expansion.
    if (diffStore.fullFile || lines.length <= CONTEXT_MAX || expandedContextKeys.has(contextKey)) {
      into.push({ kind: 'context', lines, oursStart, theirsStart });
      return;
    }
    into.push({
      kind: 'context',
      lines: lines.slice(0, CONTEXT_HEAD_TAIL),
      oursStart, theirsStart,
    });
    into.push({
      kind: 'collapsed',
      contextKey,
      hiddenLines: lines.length - CONTEXT_HEAD_TAIL * 2,
      oursStart:   oursStart   + CONTEXT_HEAD_TAIL,
      theirsStart: theirsStart + CONTEXT_HEAD_TAIL,
    });
    const tailIdx = lines.length - CONTEXT_HEAD_TAIL;
    into.push({
      kind: 'context',
      lines: lines.slice(tailIdx),
      oursStart:   oursStart   + tailIdx,
      theirsStart: theirsStart + tailIdx,
    });
  }

  function expandCollapsed(key: string) {
    expandedContextKeys = new Set([...expandedContextKeys, key]);
  }

  // ---------------------------------------------------------------------------
  // Base state
  // ---------------------------------------------------------------------------

  const tab    = $derived(tabsStore.activeTab);
  const status = $derived(repoStore.status);

  // Stash-specific store values
  const initialPaths  = $derived(uiStore.stashConflictFiles);
  const stash         = $derived(uiStore.stashConflictEntry);
  const blockingFiles = $derived(uiStore.stashBlockingFiles);
  const blockingPop   = $derived(uiStore.stashBlockingPop);
  const isBlockingMode = $derived(!isMerge && blockingFiles.length > 0);

  // Persistent snapshot of every path that was conflicted at any point
  // during this modal session. Without this, `conflictedFiles` would shrink
  // as files get resolved (because git no longer flags them as conflicted)
  // — which would in turn break the "X/Y resolved" counter and disable the
  // "Mergia" button after the last resolution.
  let seenConflictedPaths = $state(new Set<string>());

  $effect(() => {
    const next = new Set(seenConflictedPaths);
    let changed = false;
    if (isMerge) {
      for (const f of status?.conflicted ?? []) {
        if (!next.has(f.path)) { next.add(f.path); changed = true; }
      }
    } else {
      for (const p of initialPaths) {
        if (!next.has(p)) { next.add(p); changed = true; }
      }
      for (const f of status?.conflicted ?? []) {
        if (!next.has(f.path)) { next.add(f.path); changed = true; }
      }
    }
    if (changed) seenConflictedPaths = next;
  });

  let conflictedFiles = $derived.by(() => {
    // Map: path → original ConflictedFile entry from git (when available)
    const live = new Map((status?.conflicted ?? []).map(f => [f.path, f]));
    return [...seenConflictedPaths].map(p => live.get(p) ?? {
      path: p,
      old_path: null,
      index_status: 'conflicted' as const,
      workdir_status: 'conflicted' as const,
    });
  });

  // ---------------------------------------------------------------------------
  // Conflict mode state
  // ---------------------------------------------------------------------------

  let selectedPath  = $state<string | null>(null);
  let isLoading     = $state(false);
  let resolvedPaths = $state(new Set<string>());
  let mergeMessage  = $state('');
  let isMerging     = $state(false);
  let isCompleting  = $state(false);
  let isAborting    = $state(false);
  let isStagingFile = $state(false);
  let confirmAbort  = $state(false);

  // Result panel collapsed state — hides the preview editor below the
  // conflict columns, giving more vertical room to the actual diff while
  // the final rendered content is built up. Persisted so the preference
  // sticks across modal re-opens.
  let resultCollapsed = $state<boolean>(
    localStorage.getItem('arbor:conflict-result-collapsed') === '1',
  );
  $effect(() => {
    localStorage.setItem('arbor:conflict-result-collapsed', resultCollapsed ? '1' : '0');
  });

  // File-list view mode (flat list vs nested tree). Persisted so the user's
  // preference sticks across modal re-opens.
  let filesViewMode = $state<'list' | 'tree'>(
    (localStorage.getItem('arbor:conflict-files-view-mode') as 'list' | 'tree') ?? 'list',
  );
  $effect(() => {
    localStorage.setItem('arbor:conflict-files-view-mode', filesViewMode);
  });
  let filesTreeExpanded = $state<Set<string>>(new Set());

  type ConflictTreeNode = {
    name: string;
    fullPath: string;
    children: Map<string, ConflictTreeNode>;
    sortedChildren: ConflictTreeNode[];
    /** Set when this leaf represents an actual conflicted file. */
    filePath?: string;
  };

  function buildConflictTree(files: { path: string }[]): ConflictTreeNode {
    const root: ConflictTreeNode = { name: '', fullPath: '', children: new Map(), sortedChildren: [] };
    for (const f of files) {
      const parts = f.path.split('/');
      let node = root;
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        if (!node.children.has(part)) {
          node.children.set(part, {
            name: part,
            fullPath: parts.slice(0, i + 1).join('/'),
            children: new Map(),
            sortedChildren: [],
          });
        }
        node = node.children.get(part)!;
      }
      node.filePath = f.path;
    }
    // Collapse single-child directory chains ("a/b/c/Main.java" → "a/b/c" one row)
    // for compactness. Leaves (files) are never collapsed.
    const collapse = (n: ConflictTreeNode): ConflictTreeNode => {
      if (n.filePath) return n;
      if (n.children.size === 1 && !n.filePath) {
        const only = [...n.children.values()][0];
        if (!only.filePath) {
          const collapsed = collapse(only);
          return {
            ...collapsed,
            name: (n.name ? n.name + '/' : '') + collapsed.name,
          };
        }
      }
      return n;
    };
    const bakeSort = (n: ConflictTreeNode) => {
      const kids = [...n.children.values()].map(collapse);
      kids.sort((a, b) => {
        const aIsDir = !a.filePath;
        const bIsDir = !b.filePath;
        if (aIsDir !== bIsDir) return aIsDir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      n.sortedChildren = kids;
      for (const c of kids) bakeSort(c);
    };
    bakeSort(root);
    return root;
  }

  function toggleTreeDir(path: string) {
    const s = new Set(filesTreeExpanded);
    if (s.has(path)) s.delete(path); else s.add(path);
    filesTreeExpanded = s;
  }

  /** Flatten the tree to a render-friendly list of rows.
   *  Directory rows collapse their subtree when not in filesTreeExpanded.
   *  depth drives the left indent in the template. */
  type ConflictTreeRow =
    | { kind: 'dir';  depth: number; name: string; fullPath: string; hasChildren: boolean }
    | { kind: 'file'; depth: number; name: string; path: string };

  function flattenConflictTree(root: ConflictTreeNode, expanded: Set<string>): ConflictTreeRow[] {
    const rows: ConflictTreeRow[] = [];
    const visit = (nodes: ConflictTreeNode[], depth: number) => {
      for (const n of nodes) {
        if (n.filePath) {
          rows.push({ kind: 'file', depth, name: n.name, path: n.filePath });
        } else {
          rows.push({
            kind: 'dir', depth,
            name: n.name, fullPath: n.fullPath,
            hasChildren: n.sortedChildren.length > 0,
          });
          if (expanded.has(n.fullPath)) visit(n.sortedChildren, depth + 1);
        }
      }
    };
    visit(root.sortedChildren, 0);
    return rows;
  }

  // active conflict block for keyboard / button navigation
  let activeConflictId = $state<number | null>(null);

  // Collapse the file-list sidebar — IntelliJ-style toggle in the header.
  let sidebarCollapsed = $state(false);

  // ── File-list context menu (right-click on a file item) ────────────────────
  type FileCtxMenu = { x: number; y: number; path: string };
  let fileCtxMenu = $state<FileCtxMenu | null>(null);
  /** Separate state for the blocking-mode list so its menu items can be
   *  shaped differently (no labels yet — paths haven't been loaded), and
   *  so the two menus don't fight each other for the same anchor. */
  let blockingFileCtxMenu = $state<FileCtxMenu | null>(null);

  function openBlockingFileCtxMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    blockingFileCtxMenu = { x: e.clientX, y: e.clientY, path };
  }

  /** Mark a blocking file's decision without opening it.  Used by the
   *  right-click "Tieni il mio" / "Usa stash" shortcuts — the user has
   *  already decided, no need to load the diff view. */
  function quickResolveBlockingFile(path: string, decision: 'keep_mine' | 'use_stash') {
    markBlockingDecision(path, decision);
  }

  function openFileCtxMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    fileCtxMenu = { x: e.clientX, y: e.clientY, path };
  }

  let mergeFileData     = $state<Record<string, BlockingData>>({});
  let mergeFileLabels   = $state<Record<string, { ours: string; theirs: string }>>({});
  // Encoding tracked per-file so resolve_conflict can re-encode the resolved
  // text back to its original byte representation. Legacy windows-1252
  // sources would otherwise be silently rewritten as UTF-8 on save.
  let mergeFileEncoding = $state<Record<string, string>>({});
  // Tracks which index stages exist for each conflicted file.  Drives the
  // "(added on incoming)" / "(deleted on incoming)" hint in the file list
  // and disables side-specific accept actions that would be no-ops.
  let mergeFilePresence = $state<Record<string, { ours: boolean; theirs: boolean }>>({});

  /** For modify/delete + add/modify conflicts (one stage missing), the
   *  user's chosen action: keep the existing content or remove the file.
   *  Defaults to 'keep' on first selection since that preserves work.  */
  let presenceDecisions = $state<Record<string, 'keep' | 'remove'>>({});

  function setPresenceDecision(path: string, decision: 'keep' | 'remove') {
    presenceDecisions = { ...presenceDecisions, [path]: decision };
  }

  /** True for the active file when one side of the conflict has no index
   *  entry — i.e. it's a modify/delete or add/modify case.  The regular
   *  2-column diff editor is misleading for these (it shows duplicated
   *  context lines on both sides) so we render a dedicated panel. */
  const isPresenceConflict = $derived.by(() => {
    if (!selectedPath) return false;
    const p = mergeFilePresence[selectedPath];
    return !!p && (!p.ours || !p.theirs);
  });

  // ---------------------------------------------------------------------------
  // Blocking mode state — declared before the derived values further down so
  // TS lexical-scope checks (TDZ) don't complain when `activeEncodingPath` /
  // `activeConflictIds` reference them. The actual blocking-derived values
  // (`activeBlocking`, `activeBlockingDisplay`, …) still live below the
  // shared `computeSideState` helper.
  // ---------------------------------------------------------------------------

  let blockingSelectedPath = $state<string | null>(null);
  let isLoadingBlocking    = $state(false);
  let isForcing            = $state(false);
  let blockingFileData     = $state<Record<string, BlockingData>>({});

  const activeMerge = $derived(
    selectedPath ? (mergeFileData[selectedPath] ?? null) : null
  );
  const activeMergeLabels = $derived(
    selectedPath
      ? (mergeFileLabels[selectedPath] ?? { ours: 'HEAD', theirs: 'THEIRS' })
      : { ours: 'HEAD', theirs: 'THEIRS' }
  );

  // Encoding currently in effect for the selected file — drives the pill
  // shown in the modal header. Reads from the per-file map populated when
  // the file is loaded (`mergeFileEncoding` for merge, `bc.encoding` for
  // blocking). Empty string means "no file selected yet" — pill hides.
  const activeEncoding = $derived.by(() => {
    if (isBlockingMode) {
      const p = blockingSelectedPath;
      const bc = p ? blockingFileData[p]?.content : null;
      return bc?.encoding ?? '';
    }
    return selectedPath ? (mergeFileEncoding[selectedPath] ?? '') : '';
  });
  const activeEncodingPath = $derived(
    isBlockingMode ? blockingSelectedPath : selectedPath
  );
  const activeEncodingOverridden = $derived.by(() => {
    if (!tab || !activeEncodingPath) return false;
    return encodingOverrides.get(tab.path, activeEncodingPath) !== undefined;
  });
  const activeMergeDisplay: DisplayItem[] = $derived.by(() => {
    if (!activeMerge) return [];
    const out: DisplayItem[] = [];
    let oursN = 1, theirsN = 1, ctxIdx = 0;
    const fileKey = selectedPath ?? '';
    for (const r of activeMerge.regions) {
      if (r.kind === 'context') {
        emitContext(r.lines, oursN, theirsN, `${fileKey}|merge|${ctxIdx++}`, out);
        oursN   += r.lines.length;
        theirsN += r.lines.length;
      } else {
        out.push({
          kind: 'conflict', regionId: r.id,
          oursLines: r.oursLines, theirsLines: r.theirsLines,
          oursStart: oursN, theirsStart: theirsN,
          oursSelected:   activeMerge.oursSelected[r.id]   ?? r.oursLines.map(() => false),
          theirsSelected: activeMerge.theirsSelected[r.id] ?? r.theirsLines.map(() => true),
        });
        oursN   += r.oursLines.length;
        theirsN += r.theirsLines.length;
      }
    }
    return out;
  });
  const activeMergeResult = $derived(
    activeMerge ? (activeMerge.manualResult ?? computeBlockingResult(activeMerge)) : ''
  );

  // Aggregate "side state" across all conflict regions of the active file:
  //   'all'      → every checkbox on this side is ticked
  //   'none'     → no checkbox on this side is ticked (or no lines at all)
  //   'partial'  → mixed (drives the checkbox indeterminate state)
  type SideState = 'all' | 'none' | 'partial';
  function computeSideState(
    data: BlockingData | null,
    side: 'ours' | 'theirs',
  ): SideState {
    if (!data) return 'none';
    let total = 0, sel = 0;
    for (const r of data.regions) {
      if (r.kind !== 'conflict') continue;
      const lines = side === 'ours' ? r.oursLines : r.theirsLines;
      const arr   = (side === 'ours' ? data.oursSelected : data.theirsSelected)[r.id] ?? [];
      for (let i = 0; i < lines.length; i++) {
        total++;
        if (arr[i]) sel++;
      }
    }
    if (total === 0 || sel === 0) return 'none';
    if (sel === total) return 'all';
    return 'partial';
  }

  const mergeOursState     = $derived(computeSideState(activeMerge, 'ours'));
  const mergeTheirsState   = $derived(computeSideState(activeMerge, 'theirs'));

  // ---------------------------------------------------------------------------
  // Blocking mode derivations — moved above the conflict-navigation block
  // so `activeConflictIds` can read `activeBlockingDisplay` without hitting
  // TDZ.
  // ---------------------------------------------------------------------------

  const activeBlocking = $derived(
    blockingSelectedPath ? (blockingFileData[blockingSelectedPath] ?? null) : null
  );

  const blockingOursState   = $derived(computeSideState(activeBlocking, 'ours'));
  const blockingTheirsState = $derived(computeSideState(activeBlocking, 'theirs'));

  const activeBlockingDisplay: DisplayItem[] = $derived.by(() => {
    if (!activeBlocking) return [];
    const out: DisplayItem[] = [];
    let oursN = 1, theirsN = 1, ctxIdx = 0;
    const fileKey = blockingSelectedPath ?? '';
    for (const r of activeBlocking.regions) {
      if (r.kind === 'context') {
        emitContext(r.lines, oursN, theirsN, `${fileKey}|stash|${ctxIdx++}`, out);
        oursN   += r.lines.length;
        theirsN += r.lines.length;
      } else {
        out.push({
          kind: 'conflict',
          regionId: r.id,
          oursLines: r.oursLines,
          theirsLines: r.theirsLines,
          oursStart: oursN,
          theirsStart: theirsN,
          oursSelected:   activeBlocking.oursSelected[r.id]   ?? r.oursLines.map(() => false),
          theirsSelected: activeBlocking.theirsSelected[r.id] ?? r.theirsLines.map(() => true),
        });
        oursN   += r.oursLines.length;
        theirsN += r.theirsLines.length;
      }
    }
    return out;
  });

  const allFilesResolved = $derived(
    conflictedFiles.length > 0 && conflictedFiles.every(f => resolvedPaths.has(f.path))
  );

  // ---------------------------------------------------------------------------
  // Conflict block navigation
  // ---------------------------------------------------------------------------

  // Pick the display list that is actually on-screen.  `isMerge` was the
  // wrong gate: stash-apply conflicts run the merge-style editor (the `:else`
  // branch of the template), so we must key off `isBlockingMode` instead.
  // Otherwise stash conflicts render `activeBlockingDisplay` here (empty) and
  // the whole prev/next + Stage toolbar vanishes.
  const activeConflictIds = $derived(
    (isBlockingMode ? activeBlockingDisplay : activeMergeDisplay)
      .filter((d): d is ConflictDisplayItem => d.kind === 'conflict')
      .map(d => d.regionId)
  );

  const activeConflictIdx = $derived(
    activeConflictId !== null ? activeConflictIds.indexOf(activeConflictId) : -1
  );

  // Auto-select first conflict when file changes or data loads.
  $effect(() => {
    const ids = activeConflictIds;
    if (ids.length > 0 && (activeConflictId === null || !ids.includes(activeConflictId))) {
      activeConflictId = ids[0];
    } else if (ids.length === 0) {
      activeConflictId = null;
    }
  });

  function goToConflict(id: number) {
    activeConflictId = id;
    tick().then(() => {
      document.querySelector(`[data-conflict-id="${id}"]`)
        ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    });
  }

  function prevConflict() {
    const ids = activeConflictIds;
    if (!ids.length) return;
    const idx = activeConflictIdx;
    goToConflict(ids[idx <= 0 ? ids.length - 1 : idx - 1]);
  }

  function nextConflict() {
    const ids = activeConflictIds;
    if (!ids.length) return;
    const idx = activeConflictIdx;
    goToConflict(ids[idx >= ids.length - 1 ? 0 : idx + 1]);
  }

  const activeBlockingResult = $derived(
    activeBlocking
      ? (activeBlocking.manualResult ?? computeBlockingResult(activeBlocking))
      : ''
  );

  function computeBlockingResult(data: BlockingData): string {
    const lines: string[] = [];
    for (const r of data.regions) {
      if (r.kind === 'context') {
        lines.push(...r.lines);
      } else {
        const os = data.oursSelected[r.id]   ?? r.oursLines.map(() => false);
        const ts = data.theirsSelected[r.id] ?? r.theirsLines.map(() => true);
        r.oursLines.forEach((l, i)   => { if (os[i]) lines.push(l); });
        r.theirsLines.forEach((l, i) => { if (ts[i]) lines.push(l); });
      }
    }
    return lines.join('\n');
  }

  function initBlockingSelections(regions: Region[]): { os: Record<number, boolean[]>, ts: Record<number, boolean[]> } {
    const os: Record<number, boolean[]> = {};
    const ts: Record<number, boolean[]> = {};
    for (const r of regions) {
      if (r.kind === 'conflict') {
        os[r.id] = r.oursLines.map(() => false);
        ts[r.id] = r.theirsLines.map(() => true); // default: use stash
      }
    }
    return { os, ts };
  }

  function isBlockingFileViewed(path: string): boolean {
    return path in blockingFileData;
  }

  /** Files the user has explicitly resolved via the per-file "Tieni il mio"
   *  / "Usa stash" buttons (or by typing a custom merged result).  Drives
   *  the green check in the sidebar and lets `handleForceApply` route
   *  decided files to the right bucket without re-deriving from line
   *  checkboxes. */
  let blockingDecided = $state<Record<string, 'keep_mine' | 'use_stash' | 'custom'>>({});

  function isBlockingFileDecided(path: string): boolean {
    return path in blockingDecided;
  }

  /** Resolution decision for the currently active blocking file.
   *  Declared as a derived (rather than a `{@const}` inside the editor
   *  template) because Svelte 5 only allows `{@const}` as the immediate
   *  child of control-flow blocks, not inside arbitrary <div>s. */
  const activeBlockingDecision = $derived(
    blockingSelectedPath ? blockingDecided[blockingSelectedPath] : undefined
  );

  function markBlockingDecision(path: string, decision: 'keep_mine' | 'use_stash' | 'custom') {
    blockingDecided = { ...blockingDecided, [path]: decision };
  }

  /** Confirm the resolution for the active blocking file based on the
   *  current line-checkbox state. Mirrors the merge mode's "Stage file"
   *  affordance — one button per file, the user composes via per-region /
   *  per-line buttons (or types a custom result) and then commits the
   *  decision. The inferred decision flavour (keep_mine / use_stash /
   *  custom) drives the badge + bucket routing in `handleForceApply`. */
  function confirmBlockingFile() {
    const p = blockingSelectedPath; if (!p) return;
    const d = blockingFileData[p]; if (!d) return;

    // Custom merged content typed by the user wins over the inferred state.
    if (d.manualResult !== null) {
      markBlockingDecision(p, 'custom');
      return;
    }

    let allOursOnly   = true;
    let allTheirsOnly = true;
    let anyRegion     = false;
    for (const r of d.regions) {
      if (r.kind !== 'conflict') continue;
      anyRegion = true;
      const o = d.oursSelected[r.id]   ?? [];
      const t = d.theirsSelected[r.id] ?? [];
      const oursAll    = o.length === r.oursLines.length   && o.every(Boolean);
      const oursNone   = o.every(b => !b);
      const theirsAll  = t.length === r.theirsLines.length && t.every(Boolean);
      const theirsNone = t.every(b => !b);
      if (!(oursAll && theirsNone)) allOursOnly   = false;
      if (!(theirsAll && oursNone)) allTheirsOnly = false;
    }
    // No conflict regions (binary or identical content) — treat confirm as
    // "use stash" by default, matching the unviewed-file fallback used by
    // `handleForceApply`.
    const decision = !anyRegion
      ? 'use_stash'
      : allOursOnly
        ? 'keep_mine'
        : allTheirsOnly
          ? 'use_stash'
          : 'custom';
    markBlockingDecision(p, decision);
  }

  // ---------------------------------------------------------------------------
  // Effects
  // ---------------------------------------------------------------------------

  $effect(() => {
    if (isMerge && tab && !mergeMessage) {
      getMergeMessage(tab.id).then(msg => { mergeMessage = msg; }).catch(() => {});
    }
  });

  // Pre-fetch presence for every conflicted file so the sidebar badges
  // (A/D) appear immediately on open instead of only after the user
  // clicks each row.  We tried the dedicated `getConflictPresence` IPC
  // first but it was failing silently for some users (stage extraction
  // off the live index returns no rows in certain repo states) — fall
  // back to the proven path: kick off `getConflictPresence` for the
  // sidebar AND, if it returns nothing or errors, fan out
  // `getConflictContent` calls in parallel.  Both paths converge on the
  // same `mergeFilePresence` map.
  let presencePrefetchedFor = $state<string | null>(null);
  $effect(() => {
    // Run for BOTH merge mode AND stash post-apply conflicts (stash mode
    // with index entries at stages 1/2/3 but no blocker list).  Gating
    // only on `isMerge` left stash post-apply badges stuck on "C" until
    // the user clicked each one.  We skip only when we're in the
    // dedicated blocker UI (`isBlockingMode`) — that has its own icon
    // scheme and doesn't read from `mergeFilePresence`.
    if (isBlockingMode || !tab || conflictedFiles.length === 0) return;
    // Only run the prefetch ONCE per tab open — subsequent reactive
    // re-runs after we patch mergeFilePresence would otherwise re-fire
    // a full sweep on every per-file write.
    if (presencePrefetchedFor === tab.id) return;
    presencePrefetchedFor = tab.id;
    const tabId = tab.id;
    const paths = conflictedFiles.map(f => f.path);

    const fanOutContent = () => {
      // Per-file lazy presence — uses the same IPC the click handler
      // relies on, so if click works, this works too.
      for (const p of paths) {
        if (p in mergeFilePresence) continue;
        const override = encodingOverrides.get(tab!.path, p);
        getConflictContent(tabId, p, override)
          .then(c => {
            // Only patch if we haven't already loaded full content
            // for this path via an explicit click.
            if (mergeFileData[p]) return;
            mergeFilePresence = {
              ...mergeFilePresence,
              [p]: { ours: c.ours_present, theirs: c.theirs_present },
            };
          })
          .catch(err => {
            console.warn(`[conflict] presence fan-out for ${p} failed:`, err);
          });
      }
    };

    getConflictPresence(tabId)
      .then(rows => {
        if (!rows || rows.length === 0) {
          console.warn('[conflict] getConflictPresence returned empty — falling back to per-file content fetch');
          fanOutContent();
          return;
        }
        const patch: Record<string, { ours: boolean; theirs: boolean }> = {};
        for (const r of rows) {
          if (!(r.path in mergeFilePresence)) {
            patch[r.path] = { ours: r.ours_present, theirs: r.theirs_present };
          }
        }
        if (Object.keys(patch).length > 0) {
          mergeFilePresence = { ...mergeFilePresence, ...patch };
        }
        // Any conflicted files the bulk IPC didn't cover (e.g. live
        // index drift between the two scans) get filled in by the
        // per-file path so the sidebar never stays stuck on "C".
        const stillMissing = paths.filter(p => !(p in mergeFilePresence));
        if (stillMissing.length > 0) {
          console.warn(`[conflict] getConflictPresence missed ${stillMissing.length} path(s) — fanning out`);
          fanOutContent();
        }
      })
      .catch(err => {
        console.warn('[conflict] getConflictPresence failed — falling back to per-file content fetch:', err);
        fanOutContent();
      });
  });

  $effect(() => {
    if (!isBlockingMode && conflictedFiles.length > 0 && !selectedPath) {
      selectConflictFile(conflictedFiles[0].path);
    }
  });

  $effect(() => {
    if (isBlockingMode && blockingFiles.length > 0 && !blockingSelectedPath) {
      selectBlockingFile(blockingFiles[0]);
    }
  });

  // ---------------------------------------------------------------------------
  // Conflict mode: file selection
  // ---------------------------------------------------------------------------

  async function selectConflictFile(path: string) {
    if (!tab || selectedPath === path) return;
    selectedPath = path;
    if (mergeFileData[path]) return;
    await loadConflictFile(path);
  }

  /** Fetch (or re-fetch) a file's three-way content. Honors the per-file
   *  encoding override stored in `encodingOverrides`. Used both by initial
   *  selection and by the encoding-pill flow which re-loads the file with
   *  a different encoding label. */
  async function loadConflictFile(path: string) {
    if (!tab) return;
    isLoading = true;
    try {
      const override = encodingOverrides.get(tab.path, path);
      const c = await getConflictContent(tab.id, path, override);
      const regs = parseConflicts(c.working_content);
      const { os, ts } = initBlockingSelections(regs);
      mergeFileData = { ...mergeFileData, [path]: { content: null, regions: regs, oursSelected: os, theirsSelected: ts, manualResult: null } };
      mergeFileLabels = { ...mergeFileLabels, [path]: { ours: c.ours_label, theirs: c.theirs_label } };
      mergeFileEncoding = { ...mergeFileEncoding, [path]: c.encoding };
      mergeFilePresence = { ...mergeFilePresence, [path]: { ours: c.ours_present, theirs: c.theirs_present } };
    } catch (err) {
      uiStore.showToast(`Could not load ${path}: ${err}`, 'error');
    } finally {
      isLoading = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Blocking mode: file selection + diff computation
  // ---------------------------------------------------------------------------

  async function selectBlockingFile(path: string) {
    if (!tab || blockingSelectedPath === path) return;
    blockingSelectedPath = path;
    if (blockingFileData[path]) return;
    await loadBlockingFile(path);
  }

  /** Update the encoding override for the currently selected file and
   *  re-load it so the diff view reflects the new decoding. `nextEncoding`
   *  is `undefined` when the user picks "Auto-detect" (clears the pin). */
  async function changeEncoding(nextEncoding: string | undefined) {
    if (!tab || !activeEncodingPath) return;
    if (nextEncoding === undefined) {
      encodingOverrides.clear(tab.path, activeEncodingPath);
    } else {
      encodingOverrides.set(tab.path, activeEncodingPath, nextEncoding);
    }
    if (isBlockingMode) {
      await loadBlockingFile(activeEncodingPath);
    } else {
      await loadConflictFile(activeEncodingPath);
    }
  }

  /** Fetch (or re-fetch) a blocking-stash file's content with the current
   *  encoding override. Symmetric to `loadConflictFile` for the merge flow. */
  async function loadBlockingFile(path: string) {
    if (!tab) return;
    isLoadingBlocking = true;
    try {
      const override = encodingOverrides.get(tab.path, path);
      const bc = await getStashFileContent(tab.id, stash?.index ?? 0, path, override);
      const rgns = computeDiff(bc.current_content, bc.stash_content);
      const { os, ts } = initBlockingSelections(rgns);
      blockingFileData = {
        ...blockingFileData,
        [path]: { content: bc, regions: rgns, oursSelected: os, theirsSelected: ts, manualResult: null },
      };
    } catch {
      blockingFileData = {
        ...blockingFileData,
        [path]: { content: null, regions: [], oursSelected: {}, theirsSelected: {}, manualResult: null },
      };
    } finally {
      isLoadingBlocking = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Blocking mode: line selection
  // ---------------------------------------------------------------------------

  function toggleOursLine(regionId: number, lineIdx: number) {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    const arr = [...(d.oursSelected[regionId] ?? [])];
    arr[lineIdx] = !arr[lineIdx];
    blockingFileData = {
      ...blockingFileData,
      [p]: { ...d, oursSelected: { ...d.oursSelected, [regionId]: arr }, manualResult: null },
    };
  }

  function toggleTheirsLine(regionId: number, lineIdx: number) {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    const arr = [...(d.theirsSelected[regionId] ?? [])];
    arr[lineIdx] = !arr[lineIdx];
    blockingFileData = {
      ...blockingFileData,
      [p]: { ...d, theirsSelected: { ...d.theirsSelected, [regionId]: arr }, manualResult: null },
    };
  }

  function acceptBlockingOurs(regionId: number) {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    const r = d.regions.find((r): r is ConflictRegion => r.kind === 'conflict' && r.id === regionId);
    if (!r) return;
    blockingFileData = {
      ...blockingFileData,
      [p]: {
        ...d,
        oursSelected:   { ...d.oursSelected,   [regionId]: r.oursLines.map(() => true) },
        theirsSelected: { ...d.theirsSelected,  [regionId]: r.theirsLines.map(() => false) },
        manualResult: null,
      },
    };
  }

  function acceptBlockingTheirs(regionId: number) {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    const r = d.regions.find((r): r is ConflictRegion => r.kind === 'conflict' && r.id === regionId);
    if (!r) return;
    blockingFileData = {
      ...blockingFileData,
      [p]: {
        ...d,
        oursSelected:   { ...d.oursSelected,  [regionId]: r.oursLines.map(() => false) },
        theirsSelected: { ...d.theirsSelected, [regionId]: r.theirsLines.map(() => true) },
        manualResult: null,
      },
    };
  }

  function acceptBlockingBoth(regionId: number) {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    const r = d.regions.find((r): r is ConflictRegion => r.kind === 'conflict' && r.id === regionId);
    if (!r) return;
    blockingFileData = {
      ...blockingFileData,
      [p]: {
        ...d,
        oursSelected:   { ...d.oursSelected,  [regionId]: r.oursLines.map(() => true) },
        theirsSelected: { ...d.theirsSelected, [regionId]: r.theirsLines.map(() => true) },
        manualResult: null,
      },
    };
  }

  // Bulk toggle for blocking (stash) mode — same semantics as setAllMergeSide.
  function setAllBlockingSide(side: 'ours' | 'theirs', checked: boolean) {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    const next: Record<number, boolean[]> = {};
    for (const r of d.regions) {
      if (r.kind !== 'conflict') continue;
      const lines = side === 'ours' ? r.oursLines : r.theirsLines;
      next[r.id] = lines.map(() => checked);
    }
    const patch = side === 'ours'
      ? { oursSelected: next }
      : { theirsSelected: next };
    blockingFileData = { ...blockingFileData, [p]: { ...d, ...patch, manualResult: null } };
  }

  function setBlockingManualResult(value: string) {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    blockingFileData = { ...blockingFileData, [p]: { ...d, manualResult: value } };
    markBlockingDecision(p, 'custom');
  }

  function resetBlockingResult() {
    const p = blockingSelectedPath!;
    const d = blockingFileData[p];
    if (!d) return;
    blockingFileData = { ...blockingFileData, [p]: { ...d, manualResult: null } };
    // Reverting to the computed result undoes the explicit decision.
    if (blockingDecided[p] === 'custom') {
      const next = { ...blockingDecided };
      delete next[p];
      blockingDecided = next;
    }
  }

  // ---------------------------------------------------------------------------
  // Conflict mode (merge): line selection
  // ---------------------------------------------------------------------------

  function toggleMergeOursLine(regionId: number, lineIdx: number) {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    const arr = [...(d.oursSelected[regionId] ?? [])]; arr[lineIdx] = !arr[lineIdx];
    mergeFileData = { ...mergeFileData, [p]: { ...d, oursSelected: { ...d.oursSelected, [regionId]: arr }, manualResult: null } };
  }

  function toggleMergeTheirsLine(regionId: number, lineIdx: number) {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    const arr = [...(d.theirsSelected[regionId] ?? [])]; arr[lineIdx] = !arr[lineIdx];
    mergeFileData = { ...mergeFileData, [p]: { ...d, theirsSelected: { ...d.theirsSelected, [regionId]: arr }, manualResult: null } };
  }

  function acceptMergeOurs(regionId: number) {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    const r = d.regions.find((r): r is ConflictRegion => r.kind === 'conflict' && r.id === regionId); if (!r) return;
    mergeFileData = { ...mergeFileData, [p]: { ...d, oursSelected: { ...d.oursSelected, [regionId]: r.oursLines.map(() => true) }, theirsSelected: { ...d.theirsSelected, [regionId]: r.theirsLines.map(() => false) }, manualResult: null } };
  }

  function acceptMergeTheirs(regionId: number) {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    const r = d.regions.find((r): r is ConflictRegion => r.kind === 'conflict' && r.id === regionId); if (!r) return;
    mergeFileData = { ...mergeFileData, [p]: { ...d, oursSelected: { ...d.oursSelected, [regionId]: r.oursLines.map(() => false) }, theirsSelected: { ...d.theirsSelected, [regionId]: r.theirsLines.map(() => true) }, manualResult: null } };
  }

  function acceptMergeBoth(regionId: number) {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    const r = d.regions.find((r): r is ConflictRegion => r.kind === 'conflict' && r.id === regionId); if (!r) return;
    mergeFileData = { ...mergeFileData, [p]: { ...d, oursSelected: { ...d.oursSelected, [regionId]: r.oursLines.map(() => true) }, theirsSelected: { ...d.theirsSelected, [regionId]: r.theirsLines.map(() => true) }, manualResult: null } };
  }

  // Resolve a whole file by accepting one side for every conflict region,
  // then stage it. Used by the file-list right-click context menu.
  async function acceptAllForFile(path: string, side: 'ours' | 'theirs') {
    if (!tab) return;
    try {
      // Load the file's conflict content if we haven't seen it yet.
      if (!mergeFileData[path]) {
        const c = await getConflictContent(tab.id, path);
        const regs = parseConflicts(c.working_content);
        const { os, ts } = initBlockingSelections(regs);
        mergeFileData = { ...mergeFileData, [path]: { content: null, regions: regs, oursSelected: os, theirsSelected: ts, manualResult: null } };
        mergeFileLabels = { ...mergeFileLabels, [path]: { ours: c.ours_label, theirs: c.theirs_label } };
        mergeFileEncoding = { ...mergeFileEncoding, [path]: c.encoding };
        mergeFilePresence = { ...mergeFilePresence, [path]: { ours: c.ours_present, theirs: c.theirs_present } };
      }
      const d = mergeFileData[path];
      // Build the resolved content directly from the regions, AND update
      // per-line selections so the side-by-side view reflects the choice
      // if the user later opens the file in the editor.
      const lines: string[] = [];
      const newOurs:   Record<number, boolean[]> = {};
      const newTheirs: Record<number, boolean[]> = {};
      for (const r of d.regions) {
        if (r.kind === 'context') { lines.push(...r.lines); }
        else {
          const src = side === 'ours' ? r.oursLines : r.theirsLines;
          lines.push(...src);
          newOurs[r.id]   = r.oursLines.map(()   => side === 'ours');
          newTheirs[r.id] = r.theirsLines.map(() => side === 'theirs');
        }
      }
      const result = lines.join('\n');
      mergeFileData = {
        ...mergeFileData,
        [path]: { ...d, oursSelected: newOurs, theirsSelected: newTheirs, manualResult: null },
      };
      const encoding = mergeFileEncoding[path];
      if (isMerge) {
        await resolveConflict(tab.id, path, result, encoding);
      } else {
        await resolveStashConflict(tab.id, path, result, encoding);
      }
      resolvedPaths = new Set([...resolvedPaths, path]);
      const s = await getStatus(tab.id);
      repoStore.setStatus(s);
      uiStore.showToast(
        `${path.split('/').pop()}: took ${side === 'ours' ? (mergeFileLabels[path]?.ours ?? 'ours') : (mergeFileLabels[path]?.theirs ?? 'theirs')} version`,
        'success',
      );
    } catch (err) {
      uiStore.showToast(`Resolution error on ${path}: ${err}`, 'error');
    }
  }

  // Bulk toggle: flag/unflag every checkbox on ONE side across ALL conflict
  // regions of the active file — without touching the opposite side.
  function setAllMergeSide(side: 'ours' | 'theirs', checked: boolean) {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    const next: Record<number, boolean[]> = {};
    for (const r of d.regions) {
      if (r.kind !== 'conflict') continue;
      const lines = side === 'ours' ? r.oursLines : r.theirsLines;
      next[r.id] = lines.map(() => checked);
    }
    const patch = side === 'ours'
      ? { oursSelected: next }
      : { theirsSelected: next };
    mergeFileData = { ...mergeFileData, [p]: { ...d, ...patch, manualResult: null } };
  }

  function setMergeManualResult(value: string) {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    mergeFileData = { ...mergeFileData, [p]: { ...d, manualResult: value } };
  }

  function resetMergeResult() {
    const p = selectedPath!; const d = mergeFileData[p]; if (!d) return;
    mergeFileData = { ...mergeFileData, [p]: { ...d, manualResult: null } };
  }

  // ---------------------------------------------------------------------------
  // LCS diff: produces Region[] from two file contents
  // ---------------------------------------------------------------------------

  function computeDiff(a: string | null, b: string | null): Region[] {
    const aLines = (a ?? '').split('\n');
    const bLines = (b ?? '').split('\n');
    if (aLines.at(-1) === '') aLines.pop();
    if (bLines.at(-1) === '') bLines.pop();

    if (aLines.length === 0 && bLines.length === 0) return [];
    if (aLines.length === 0) {
      return [{ kind: 'conflict', id: 0, oursLines: [], theirsLines: bLines, oursLabel: 'Corrente', theirsLabel: 'Stash' }];
    }
    if (bLines.length === 0) {
      return [{ kind: 'conflict', id: 0, oursLines: aLines, theirsLines: [], oursLabel: 'Corrente', theirsLabel: 'Stash' }];
    }

    if (aLines.length * bLines.length > 250_000) {
      return [{ kind: 'conflict', id: 0, oursLines: aLines, theirsLines: bLines, oursLabel: 'Corrente', theirsLabel: 'Stash' }];
    }

    const n = aLines.length, m = bLines.length;
    const dp = Array.from({ length: n + 1 }, () => new Int32Array(m + 1));
    for (let i = 1; i <= n; i++) {
      for (let j = 1; j <= m; j++) {
        dp[i][j] = aLines[i-1] === bLines[j-1]
          ? dp[i-1][j-1] + 1
          : Math.max(dp[i-1][j], dp[i][j-1]);
      }
    }

    type Op = { k: 'eq' | 'rm' | 'add'; l: string };
    const ops: Op[] = [];
    let i = n, j = m;
    while (i > 0 || j > 0) {
      if (i > 0 && j > 0 && aLines[i-1] === bLines[j-1]) {
        ops.push({ k: 'eq',  l: aLines[i-1] }); i--; j--;
      } else if (j > 0 && (i === 0 || dp[i][j-1] >= dp[i-1][j])) {
        ops.push({ k: 'add', l: bLines[j-1] }); j--;
      } else {
        ops.push({ k: 'rm',  l: aLines[i-1] }); i--;
      }
    }
    ops.reverse();

    const result: Region[] = [];
    let ctx: string[] = [];
    let rmLines: string[] = [];
    let addLines: string[] = [];
    let id = 0;

    function flush() {
      if (rmLines.length === 0 && addLines.length === 0) return;
      if (ctx.length) { result.push({ kind: 'context', lines: [...ctx] }); ctx = []; }
      result.push({ kind: 'conflict', id: id++, oursLines: [...rmLines], theirsLines: [...addLines], oursLabel: 'Corrente', theirsLabel: 'Stash' });
      rmLines = []; addLines = [];
    }

    for (const op of ops) {
      if (op.k === 'eq')       { flush(); ctx.push(op.l); }
      else if (op.k === 'rm')  rmLines.push(op.l);
      else                     addLines.push(op.l);
    }
    flush();
    if (ctx.length) result.push({ kind: 'context', lines: [...ctx] });
    return result;
  }

  // ---------------------------------------------------------------------------
  // Conflict mode: staging
  // ---------------------------------------------------------------------------

  async function stageMergeFile() {
    if (!tab || !selectedPath || isStagingFile) return;
    isStagingFile = true;
    try {
      // Modify/delete and add/modify cases route differently: there's no
      // line-merge to compose — the user picks "keep file" (resolve with
      // the existing-side content) or "accept deletion" (drop file + index
      // entries entirely).
      if (isPresenceConflict) {
        const decision = presenceDecisions[selectedPath] ?? 'keep';
        if (decision === 'remove') {
          await removeConflictFile(tab.id, selectedPath);
        } else {
          // "Keep" — stage whatever git left in the workdir as the kept
          // content.  For modify/delete this is ours' modifications; for
          // add/modify it's theirs' new file body.  We always re-fetch
          // the raw working_content so we don't lose any bytes that the
          // region parser might have eaten (the parser splits on \n and
          // could drop a trailing newline / BOM).
          const c = await getConflictContent(tab.id, selectedPath);
          if (isMerge) {
            await resolveConflict(tab.id, selectedPath, c.working_content, c.encoding);
          } else {
            await resolveStashConflict(tab.id, selectedPath, c.working_content, c.encoding);
          }
        }
      } else {
        const data = mergeFileData[selectedPath];
        if (!data) return;
        const result = data.manualResult ?? computeBlockingResult(data);
        const encoding = mergeFileEncoding[selectedPath];
        if (isMerge) {
          await resolveConflict(tab.id, selectedPath, result, encoding);
        } else {
          await resolveStashConflict(tab.id, selectedPath, result, encoding);
        }
      }
      resolvedPaths = new Set([...resolvedPaths, selectedPath]);
      const s = await getStatus(tab.id);
      repoStore.setStatus(s);
    } catch (err) {
      uiStore.showToast(`Staging error on ${selectedPath}: ${err}`, 'error');
    } finally {
      isStagingFile = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Complete (merge commit / stash finalize)
  // ---------------------------------------------------------------------------

  async function handleComplete() {
    if (!tab || isCompleting || isMerging) return;
    if (isMerge) {
      isMerging = true;
      try {
        const oid = await completeMerge(tab.id, mergeMessage);
        // Empty OID = orphan-conflict state (no MERGE_HEAD). The backend
        // already staged the resolutions via resolve_conflict, so there's
        // nothing to commit as a merge — just close with a friendly note.
        if (oid === '') {
          uiStore.showToast('Conflicts resolved — changes staged. Commit from the Stage area.', 'success');
        } else {
          uiStore.showToast('Merge completed', 'success');
        }
        const s = await getStatus(tab.id);
        repoStore.setStatus(s);
        graphStore.refresh();
        uiStore.closeMergeModal();
      } catch (err) {
        uiStore.showToast(`Merge failed: ${err}`, 'error');
      } finally {
        isMerging = false;
      }
    } else {
      // ── Stash post-apply Complete ──────────────────────────────────────
      // Snapshot tabId BEFORE any await so we don't read a stale value
      // out of the `tab` $derived after handleClose() unmounts the modal
      // (same gotcha that bit CheckoutConflictModal earlier).  Snapshot
      // `blockingPop` too — uiStore.stashBlockingPop is cleared by
      // closeStashConflictModal().
      const localTabId    = tab.id;
      const wasPop        = blockingPop;
      const stashIndex    = stash?.index ?? null;
      console.debug('[conflict] handleComplete (stash) firing', { tabId: localTabId, wasPop, stashIndex });
      isCompleting = true;
      try {
        // If the original op was a pop, drop the stash now that the user
        // has resolved every conflict — without this, the stash entry
        // hangs around forever even though the user's intent was a pop.
        // Best-effort: drop failures don't block the close.
        if (wasPop && stashIndex !== null) {
          try { await stashDrop(localTabId, stashIndex); }
          catch (e) { console.warn('[conflict] post-apply stash drop failed:', e); }
        }
        const s = await getStatus(localTabId);
        repoStore.setStatus(s);
        graphStore.refresh();
        uiStore.setActiveBottomSection('stage');
        uiStore.showToast(
          wasPop
            ? 'Conflicts resolved — stash dropped'
            : 'Conflicts resolved — review and commit from the Stage area',
          'success',
        );
        handleClose();
      } catch (err) {
        uiStore.showToast(`Complete failed: ${err}`, 'error');
      } finally {
        isCompleting = false;
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Force apply (blocking mode)
  // ---------------------------------------------------------------------------

  async function handleForceApply() {
    if (!tab || !stash || isForcing) return;
    isForcing = true;
    try {
      const toDelete: string[] = [];
      const toKeep:   string[] = [];
      const toMerge:  Record<string, string> = {};

      for (const f of blockingFiles) {
        // Explicit per-file decision (set by the "Tieni il mio" / "Usa stash"
        // buttons or by typing a custom merged result) wins over the
        // line-checkbox inference — the user clicked it on purpose.
        const decision = blockingDecided[f];
        if (decision === 'keep_mine') { toKeep.push(f); continue; }
        if (decision === 'use_stash') { toDelete.push(f); continue; }

        const data = blockingFileData[f];
        if (!data) {
          // Not viewed and no explicit decision — default to using the
          // stash (delete current).  This matches the typical reason the
          // user opened the modal: they wanted the stash applied.
          toDelete.push(f);
          continue;
        }
        const result = data.manualResult ?? computeBlockingResult(data);
        const current = data.content?.current_content ?? null;
        const stashContent = data.content?.stash_content ?? null;
        if (result === current) {
          toKeep.push(f);
        } else if (result === stashContent || stashContent === null) {
          toDelete.push(f);
        } else {
          toMerge[f] = result;
        }
      }

      for (const [path, mergedContent] of Object.entries(toMerge)) {
        const enc = blockingFileData[path]?.content?.encoding ?? undefined;
        await writeWorkdirFile(tab.id, path, mergedContent, enc);
        toKeep.push(path);
      }

      const applyResult = await forceStashApply(tab.id, stash.index, toDelete, toKeep, blockingPop);
      if (applyResult.blocking_untracked.length > 0) {
        // Re-open the modal with the still-blocking list so the user can
        // resolve them in another pass instead of dumping a generic error.
        const stillBlocking = applyResult.blocking_untracked;
        uiStore.openStashConflictModal(stash, [], stillBlocking, blockingPop);
        uiStore.showToast(
          `${stillBlocking.length} remaining blocking file${stillBlocking.length === 1 ? '' : 's'} — resolve and reapply`,
          'warning',
          7000,
        );
        return;
      }
      const s = await getStatus(tab.id);
      repoStore.setStatus(s);
      const conflictPaths = applyResult.conflicted_files.length > 0
        ? applyResult.conflicted_files
        : s.conflicted.map(f => f.path);
      if (applyResult.has_conflicts && conflictPaths.length > 0) {
        uiStore.openStashConflictModal(stash, conflictPaths, [], blockingPop);
        uiStore.showToast(
          `Stash applied with ${conflictPaths.length} conflict${conflictPaths.length === 1 ? '' : 's'} — resolution required`,
          'warning',
        );
      } else if (applyResult.no_changes) {
        uiStore.showToast(
          blockingPop
            ? 'No changes — working tree already matches the stash. Stash dropped.'
            : 'No changes — working tree already matches the stash.',
          'info',
        );
        graphStore.refresh();
        handleClose();
      } else {
        uiStore.showToast(blockingPop ? 'Stash applied and dropped' : 'Stash applied', 'success');
        graphStore.refresh();
        handleClose();
      }
    } catch (err) {
      uiStore.showToast(`Force apply failed: ${err}`, 'error');
    } finally {
      isForcing = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Abort
  // ---------------------------------------------------------------------------

  async function handleAbort() {
    if (!tab || isAborting) return;
    isAborting = true;
    confirmAbort = false;
    try {
      if (isMerge) {
        const wasRealMerge = repoStore.status?.is_merging ?? false;
        await abortMerge(tab.id);
        uiStore.showToast(
          wasRealMerge ? 'Merge aborted' : 'Resolution discarded — files restored from HEAD',
          'warning',
        );
      } else {
        await abortStashApply(tab.id);

        // If this was a pre-checkout stash, go back to the source branch and
        // re-apply the stash there (where it was originally created from).
        const sourceBranch = stash?.message?.match(/^WIP on ([^:]+):/)?.[1]?.trim();
        const isPrecheckout = !!stash?.message?.includes('pre-checkout stash');

        if (isPrecheckout && sourceBranch) {
          try {
            await checkoutBranch(tab.id, sourceBranch);
            const applyResult = await stashApply(tab.id, 0);
            if (!applyResult.has_conflicts && !applyResult.blocking_untracked?.length) {
              await stashDrop(tab.id, 0);
              uiStore.showToast(`Switched back to '${sourceBranch}' — changes restored`, 'success');
            } else if (applyResult.has_conflicts) {
              uiStore.showToast(`Switched back to '${sourceBranch}' — stash reapplied with conflicts to resolve`, 'warning');
            } else {
              uiStore.showToast(`Switched back to '${sourceBranch}' — stash awaiting manual apply`, 'warning');
            }
          } catch (checkoutErr) {
            uiStore.showToast(`Working tree restored — checkout '${sourceBranch}' failed: ${checkoutErr}`, 'warning');
          }
        } else {
          uiStore.showToast('Stash aborted — working tree restored', 'warning');
        }
      }
      // Light refresh — checkout (when it ran) only moves HEAD, no new
      // commits / refs / topology to recompute.  applyPostCheckout pulls
      // status + sidebar lists in one round-trip and skips getGraph.
      await applyPostCheckout(tab.id);
      handleClose();
    } catch (err) {
      uiStore.showToast(`Abort failed: ${err}`, 'error');
    } finally {
      isAborting = false;
    }
  }

  function handleClose() {
    selectedPath = null; mergeFileData = {}; mergeFileLabels = {}; mergeFileEncoding = {}; mergeFilePresence = {}; presenceDecisions = {};
    presencePrefetchedFor = null;
    resolvedPaths = new Set(); seenConflictedPaths = new Set(); confirmAbort = false;
    blockingSelectedPath = null; blockingFileData = {}; blockingDecided = {};
    resultHeight = null;
    if (isMerge) uiStore.closeMergeModal();
    else         uiStore.closeStashConflictModal();
  }

  // ---------------------------------------------------------------------------
  // Navigation
  // ---------------------------------------------------------------------------

  function nextUnresolved() {
    const unresolved = conflictedFiles.filter(f => !resolvedPaths.has(f.path));
    if (unresolved.length > 0) selectConflictFile(unresolved[0].path);
  }

  // ---------------------------------------------------------------------------
  // Conflict marker parser (conflict mode only)
  // ---------------------------------------------------------------------------

  function parseConflicts(raw: string): Region[] {
    const lines = raw.split('\n');
    const result: Region[] = [];
    let ctx: string[] = [];
    let state: 'context' | 'ours' | 'theirs' = 'context';
    let oursLines: string[] = [], theirsLines: string[] = [];
    let oursLabel = 'HEAD', theirsLabel = isMerge ? 'THEIRS' : 'STASH';
    let id = 0;
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (i === lines.length - 1 && line === '') continue;
      if      (state === 'context' && line.startsWith('<<<<<<<')) {
        if (ctx.length) { result.push({ kind: 'context', lines: [...ctx] }); ctx = []; }
        oursLabel = line.slice(8).trim() || 'HEAD';
        state = 'ours'; oursLines = [];
      } else if (state === 'ours'    && line.startsWith('=======')) {
        state = 'theirs'; theirsLines = [];
      } else if (state === 'theirs'  && line.startsWith('>>>>>>>')) {
        theirsLabel = line.slice(8).trim() || (isMerge ? 'THEIRS' : 'STASH');
        result.push({ kind: 'conflict', id: id++, oursLines: [...oursLines], theirsLines: [...theirsLines], oursLabel, theirsLabel });
        state = 'context';
      } else if (state === 'ours')   oursLines.push(line);
      else if   (state === 'theirs') theirsLines.push(line);
      else                           ctx.push(line);
    }
    if (ctx.length) result.push({ kind: 'context', lines: ctx });
    return result;
  }

  // ---------------------------------------------------------------------------
  // Derived — conflict mode
  // ---------------------------------------------------------------------------

  // The merge-message gate only applies to a *real* merge — orphan-conflict
  // resolution (`isMerge && !isRealMerge`) doesn't create a commit, so
  // there's nothing to require a message for.  Without this exception the
  // "Apply resolution" button silently stays disabled because MERGE_MSG
  // doesn't exist when there's no MERGE_HEAD, leaving the user clicking
  // an apparently-active button that does nothing.
  const canComplete = $derived(
    allFilesResolved && !isCompleting && !isMerging &&
    (!isRealMerge || mergeMessage.trim().length > 0)
  );

  // ---------------------------------------------------------------------------
  // Result panel — resize + highlight overlay
  // ---------------------------------------------------------------------------

  let resultHeight = $state<number | null>(null); // null = 50% flex split
  let _resizeStart: { y: number; h: number } | null = null;
  let resultTextareaEl = $state<HTMLTextAreaElement | null>(null);
  let resultPreEl      = $state<HTMLPreElement | null>(null);

  // ── Shared horizontal scrollbar for the diff cells ──────────────────────────
  // Mirrors the pattern in DiffViewer (`.split-hscroll`): each row cell has
  // `overflow-x: hidden` and the long line of code inside it sits at its
  // intrinsic `max-content` width. A single sticky scrollbar at the bottom of
  // the diff area drives `scrollLeft` on every cell in lock-step, so the
  // columns visually stay at their grid widths and only the contents scroll.
  //
  // TODO(future-refactor): if we ever drop the shared `bcol-scroll` grid in
  // favour of two truly independent side panes (IntelliJ-style — see option
  // B in the conflict-resolver scroll discussion), this whole block goes
  // away in favour of two `<div overflow:auto>` containers + a vertical
  // sync handler. That's a bigger restructuring of the row markup (region
  // headers + asymmetric line-count padding) and it should be done for
  // DiffViewer at the same time to keep the two panels consistent.
  let bcolScrollEl       = $state<HTMLElement | null>(null);
  let bcolHscrollInnerEl = $state<HTMLElement | null>(null);
  let bcolScrollX        = $state(0);

  function bcolApplyScrollX(x: number) {
    if (!bcolScrollEl) return;
    const cells = bcolScrollEl.querySelectorAll<HTMLElement>('.brow-left, .brow-right, .bline');
    for (const el of cells) el.scrollLeft = x;
  }

  function onBcolHscroll(e: Event) {
    const x = (e.currentTarget as HTMLElement).scrollLeft;
    bcolScrollX = x;
    bcolApplyScrollX(x);
  }

  $effect(() => {
    if (!bcolScrollEl || !bcolHscrollInnerEl) return;
    const containerEl = bcolScrollEl;
    const innerEl     = bcolHscrollInnerEl;

    function update() {
      const cells = containerEl.querySelectorAll<HTMLElement>('.brow-left, .brow-right, .bline');
      let maxRange = 0;
      for (const el of cells) maxRange = Math.max(maxRange, el.scrollWidth - el.clientWidth);
      const hscrollW = innerEl.parentElement?.clientWidth ?? 0;
      innerEl.style.width = (hscrollW + maxRange) + 'px';
      // Re-sync any newly mounted cells to the current scroll offset.
      for (const el of cells) el.scrollLeft = bcolScrollX;
    }

    const ro = new ResizeObserver(update);
    ro.observe(containerEl);
    // Region expand/collapse and conflict-region updates add/remove cells —
    // recompute the scrollable range whenever the subtree changes.
    const mo = new MutationObserver(update);
    mo.observe(containerEl, { childList: true, subtree: true });
    update();
    return () => { ro.disconnect(); mo.disconnect(); };
  });

  const highlightedResult = $derived.by(() => {
    if (isBlockingMode) {
      const path = blockingSelectedPath ?? '';
      return (activeBlockingResult || '').split('\n').map(line => highlight(line, path)).join('\n');
    } else {
      const path = selectedPath ?? '';
      return (activeMergeResult || '').split('\n').map(line => highlight(line, path)).join('\n');
    }
  });

  function startResultResize(e: MouseEvent) {
    // Measure actual rendered height before switching to pixel mode
    const el = (e.currentTarget as HTMLElement).parentElement;
    const h = el ? el.getBoundingClientRect().height : 200;
    _resizeStart = { y: e.clientY, h };
    e.preventDefault();
  }

  function onGlobalMouseMove(e: MouseEvent) {
    if (!_resizeStart) return;
    const delta = _resizeStart.y - e.clientY;
    resultHeight = Math.max(80, Math.min(560, _resizeStart.h + delta));
  }

  // Modal-scoped keyboard shortcuts. Registered in the capture phase via
  // $effect below so they run *before* AppShell's window listener, with
  // `stopImmediatePropagation()` to keep the event from leaking into any
  // other handler on the same target. Otherwise Ctrl+B / Ctrl+Shift+S
  // would end up toggling the main app's sidebar / bottom panel while
  // the user is mid-conflict-resolution.
  function handleBackdropKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      // Hijack Escape so the host Modal doesn't close: instead the user is
      // prompted to confirm aborting the in-progress resolution. (Closing
      // mid-merge would silently lose the user's work.)
      e.preventDefault();
      e.stopImmediatePropagation();
      if (!confirmAbort) confirmAbort = true;
      return;
    }

    if (matchesBinding(e, keybindingsStore.getBinding('toggle_sidebar'))) {
      e.preventDefault();
      e.stopImmediatePropagation();
      sidebarCollapsed = !sidebarCollapsed;
      return;
    }

    // stage_view: same binding used to toggle the bottom panel in the
    // main layout. Inside the modal it collapses/expands the "Risultato
    // merge" preview panel, which plays the same structural role.
    if (matchesBinding(e, keybindingsStore.getBinding('stage_view'))) {
      e.preventDefault();
      e.stopImmediatePropagation();
      resultCollapsed = !resultCollapsed;
      return;
    }

    // Chunk navigation also works inside the modal.
    if (matchesBinding(e, keybindingsStore.getBinding('next_chunk'))) {
      e.preventDefault();
      e.stopImmediatePropagation();
      nextConflict();
      return;
    }
    if (matchesBinding(e, keybindingsStore.getBinding('prev_chunk'))) {
      e.preventDefault();
      e.stopImmediatePropagation();
      prevConflict();
      return;
    }
  }

  $effect(() => {
    const onKey = (e: KeyboardEvent) => handleBackdropKeydown(e);
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  });

  function onGlobalMouseUp() {
    _resizeStart = null;
  }

  function syncResultScroll() {
    if (resultTextareaEl && resultPreEl) {
      resultPreEl.scrollTop  = resultTextareaEl.scrollTop;
      resultPreEl.scrollLeft = resultTextareaEl.scrollLeft;
    }
  }

  // Collapse animation for the result editor — mirrors the bottom-panel
  // slide in AppShell. Uses `tick` rather than `css` because we also need
  // to animate the parent panel's flex-grow + min-height inline. Going
  // through CSS transitions on `flex-grow` is unreliable across browser
  // versions (the user reported no visible animation), so we drive every
  // frame from JS to guarantee motion.
  function resultSlide(node: HTMLElement, { duration = 200 }: { duration?: number } = {}) {
    const h = node.getBoundingClientRect().height;
    const parent = node.parentElement;
    return {
      duration,
      easing: cubicOut,
      tick: (t: number) => {
        node.style.height = `${t * h}px`;
        node.style.minHeight = '0';
        node.style.overflow = 'hidden';
        if (parent) {
          parent.style.flexGrow = String(t);
          parent.style.minHeight = `${t * 80}px`;
        }
      },
    };
  }

  // Collapse animation for the sidebar — CSS flex-basis transitions were
  // unreliable (browser quirks when flex + min-width are swapping), so we
  // let Svelte drive inline styles explicitly.
  function sidebarSlide(node: HTMLElement, { duration = 200 }: { duration?: number } = {}) {
    const w = node.getBoundingClientRect().width;
    return {
      duration,
      easing: cubicOut,
      css: (t: number) =>
        `width: ${t * w}px; min-width: 0; margin-right: ${t * 4}px; opacity: ${t}; overflow: hidden; flex: 0 0 auto;`,
    };
  }
</script>

<svelte:window onmousemove={onGlobalMouseMove} onmouseup={onGlobalMouseUp} />

<!-- Capture-phase keydown listener registered via $effect below. We can't
     use `<svelte:window>` here because it binds in the bubble phase —
     AppShell's own window listener (same phase) fires first for keys like
     Ctrl+B / Ctrl+Shift+S and eats them before the modal can react. With
     `capture: true` we run before any bubble listener and can
     `stopImmediatePropagation()` so nothing downstream sees the event. -->

<Modal onClose={handleClose} size="full" padBody={false} ariaLabel="Conflict resolution">
  {#snippet header()}
    <ModalHeader onClose={handleClose}>
      <ModalSidebarToggle
        collapsed={sidebarCollapsed}
        onToggle={() => sidebarCollapsed = !sidebarCollapsed}
        label={sidebarCollapsed ? 'Show file list' : 'Hide file list'}
      />
      <span class="header-icon">
        {#if isRealMerge}<GitMerge size={15} />{:else if !isMerge}<Archive size={15} />{:else}<TriangleAlert size={15} />{/if}
      </span>
      <span class="header-title">
        {#if isBlockingMode}
          Blocking files — compose merge
        {:else if isRealMerge}
          Merge resolution
        {:else if isMerge}
          Conflict resolution
        {:else}
          Stash conflict resolution
        {/if}
      </span>
      {#if !isMerge && stash}
        <span class="stash-chip">stash@{'{' + stash.index + '}'}</span>
      {/if}
      {#if !isBlockingMode && activeMerge}
        <span class="header-labels">
          <span class="label-chip label-chip-ours" use:tooltip={'Current version (HEAD)'}>
            <GitBranch size={10} />
            <span class="label-chip-text">{activeMergeLabels.ours}</span>
          </span>
          <span class="label-chip-vs" aria-hidden="true">vs</span>
          <span class="label-chip label-chip-theirs" use:tooltip={'Incoming version'}>
            <GitBranch size={10} />
            <span class="label-chip-text">{activeMergeLabels.theirs}</span>
          </span>
        </span>
      {/if}
      {#if activeEncoding && activeEncodingPath}
        <EncodingPill
          encoding={activeEncoding}
          overridden={activeEncodingOverridden}
          onChange={changeEncoding}
        />
      {/if}

      {#snippet actions()}
        <button
          class="header-fullfile-btn"
          class:on={diffStore.fullFile}
          use:tooltip={diffStore.fullFile ? 'Showing full file context (click to disable)' : 'Show full file context'}
          aria-pressed={diffStore.fullFile}
          onclick={() => diffStore.setFullFile(!diffStore.fullFile)}
        >
          <FileText size={12} />
        </button>
        {#if isBlockingMode}
          <span class="header-progress">
            {blockingFiles.filter(isBlockingFileDecided).length}/{blockingFiles.length} confirmed
          </span>
        {:else}
          <span class="header-progress">
            {resolvedPaths.size}/{conflictedFiles.length} resolved
          </span>
        {/if}
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="cr-modal" class:mode-merge={isMerge} class:mode-stash={!isMerge}>
    <!-- ── Body ────────────────────────────────────────────────────────────── -->
    <div class="cr-body">

      <!-- File sidebar. Wrapped in {#if} so Svelte drives the
           intro/outro transition inline — CSS transitions on flex-basis /
           width were not animating reliably across browser versions. -->
      {#if !sidebarCollapsed}
      <div class="file-sidebar" transition:sidebarSlide={{ duration: animStore.dPanel }}>
        {#if isBlockingMode}
          <div class="sidebar-label-row">
            <span class="sidebar-label">Blocking files</span>
            <button
              class="sidebar-toggle-btn"
              class:active={filesViewMode === 'list'}
              use:tooltip={'Show file names'}
              onclick={() => filesViewMode = 'list'}
              aria-label="List view"
            >
              <List size={12} />
            </button>
            <button
              class="sidebar-toggle-btn"
              class:active={filesViewMode === 'tree'}
              use:tooltip={'Show tree structure'}
              onclick={() => filesViewMode = 'tree'}
              aria-label="Tree view"
            >
              <FolderTree size={12} />
            </button>
          </div>
          <div class="sidebar-divider"></div>
          <div class="files-list" class:tree-mode={filesViewMode === 'tree'}>
            {#if filesViewMode === 'list'}
              {#each blockingFiles as f}
                {@const isViewed   = isBlockingFileViewed(f)}
                {@const isDecided  = isBlockingFileDecided(f)}
                {@const decision   = blockingDecided[f]}
                {@const isActive   = blockingSelectedPath === f}
                <button
                  class="file-item"
                  class:active={isActive}
                  class:resolved={isDecided || isViewed}
                  onclick={() => selectBlockingFile(f)}
                  oncontextmenu={(e) => openBlockingFileCtxMenu(e, f)}
                  use:tooltip={f}
                >
                  <span class="file-status-icon">
                    {#if isDecided}
                      <CheckCircle2 size={12} class="icon-resolved" />
                    {:else if isViewed}
                      <Eye size={12} class="icon-viewed" />
                    {:else}
                      <AlertTriangle size={12} class="icon-conflict" />
                    {/if}
                  </span>
                  <span class="file-name truncate">{f.split('/').pop()}</span>
                  {#if decision}
                    <span
                      class="blocking-decision-badge"
                      class:keep={decision === 'keep_mine'}
                      class:stash={decision === 'use_stash'}
                      class:custom={decision === 'custom'}
                      use:tooltip={decision === 'keep_mine'
                        ? 'Keep current version'
                        : decision === 'use_stash'
                          ? 'Use stash version'
                          : 'Custom merge'}
                    >{decision === 'keep_mine' ? 'M' : decision === 'use_stash' ? 'S' : '✎'}</span>
                  {/if}
                </button>
              {/each}
            {:else}
              {@const tree = buildConflictTree(blockingFiles.map(p => ({ path: p })))}
              {@const rows = flattenConflictTree(tree, filesTreeExpanded)}
              {#each rows as row}
                {#if row.kind === 'dir'}
                  <button
                    class="tree-dir"
                    style="padding-left: {6 + row.depth * 12}px"
                    onclick={() => toggleTreeDir(row.fullPath)}
                    use:tooltip={row.fullPath}
                  >
                    {#if filesTreeExpanded.has(row.fullPath)}
                      <ChevronDown size={11} class="tree-chev" />
                    {:else}
                      <ChevronRight size={11} class="tree-chev" />
                    {/if}
                    <Folder size={12} class="tree-folder-icon" />
                    <span class="tree-dir-name truncate">{row.name}</span>
                  </button>
                {:else}
                  {@const isViewed   = isBlockingFileViewed(row.path)}
                  {@const isDecided  = isBlockingFileDecided(row.path)}
                  {@const decision   = blockingDecided[row.path]}
                  {@const isActive   = blockingSelectedPath === row.path}
                  <button
                    class="file-item tree-file"
                    class:active={isActive}
                    class:resolved={isDecided || isViewed}
                    style="padding-left: {6 + row.depth * 12}px"
                    onclick={() => selectBlockingFile(row.path)}
                    oncontextmenu={(e) => openBlockingFileCtxMenu(e, row.path)}
                    use:tooltip={row.path}
                  >
                    <span class="file-status-icon">
                      {#if isDecided}
                        <CheckCircle2 size={12} class="icon-resolved" />
                      {:else if isViewed}
                        <Eye size={12} class="icon-viewed" />
                      {:else}
                        <AlertTriangle size={12} class="icon-conflict" />
                      {/if}
                    </span>
                    <span class="file-name truncate">{row.name}</span>
                    {#if decision}
                      <span
                        class="blocking-decision-badge"
                        class:keep={decision === 'keep_mine'}
                        class:stash={decision === 'use_stash'}
                        class:custom={decision === 'custom'}
                        use:tooltip={decision === 'keep_mine'
                          ? 'Keep current version'
                          : decision === 'use_stash'
                            ? 'Use stash version'
                            : 'Custom merge'}
                      >{decision === 'keep_mine' ? 'M' : decision === 'use_stash' ? 'S' : '✎'}</span>
                    {/if}
                  </button>
                {/if}
              {/each}
            {/if}
          </div>
        {:else}
          <div class="sidebar-label-row">
            <span class="sidebar-label">Conflicting files</span>
            <button
              class="sidebar-toggle-btn"
              class:active={filesViewMode === 'list'}
              use:tooltip={'Show file names'}
              onclick={() => filesViewMode = 'list'}
              aria-label="List view"
            >
              <List size={12} />
            </button>
            <button
              class="sidebar-toggle-btn"
              class:active={filesViewMode === 'tree'}
              use:tooltip={'Show tree structure'}
              onclick={() => filesViewMode = 'tree'}
              aria-label="Tree view"
            >
              <FolderTree size={12} />
            </button>
          </div>
          <div class="sidebar-divider"></div>
          <div class="files-list" class:tree-mode={filesViewMode === 'tree'}>
            {#if filesViewMode === 'list'}
              {#each conflictedFiles as file}
                {@const isResolved = resolvedPaths.has(file.path)}
                {@const isActive   = selectedPath === file.path}
                {@const pres       = mergeFilePresence[file.path]}
                {@const sideHint   = pres && !pres.ours ? 'added' : pres && !pres.theirs ? 'deleted' : null}
                {@const monogram   = isResolved ? '✓' : sideHint === 'added' ? 'A' : sideHint === 'deleted' ? 'D' : 'C'}
                {@const monoTip    = isResolved
                  ? 'Resolved'
                  : sideHint === 'added'
                    ? 'Added on incoming side — no version on current branch'
                    : sideHint === 'deleted'
                      ? 'Deleted on incoming side — no version on incoming branch'
                      : 'Conflict — both sides modified'}
                <button
                  class="file-item"
                  class:active={isActive}
                  class:resolved={isResolved}
                  onclick={() => selectConflictFile(file.path)}
                  oncontextmenu={(e) => openFileCtxMenu(e, file.path)}
                  use:tooltip={file.path}
                >
                  <span class="status-badge"
                    class:s-resolved={isResolved}
                    class:s-added={!isResolved && sideHint === 'added'}
                    class:s-deleted={!isResolved && sideHint === 'deleted'}
                    class:s-conflict={!isResolved && !sideHint}
                    use:tooltip={monoTip}
                  >{monogram}</span>
                  <span class="file-name truncate">{file.path.split('/').pop()}</span>
                  {#if isActive && isStagingFile}<span class="file-saving">…</span>{/if}
                </button>
              {/each}
            {:else}
              {@const tree = buildConflictTree(conflictedFiles)}
              {@const rows = flattenConflictTree(tree, filesTreeExpanded)}
              {#each rows as row}
                {#if row.kind === 'dir'}
                  <button
                    class="tree-dir"
                    style="padding-left: {6 + row.depth * 12}px"
                    onclick={() => toggleTreeDir(row.fullPath)}
                    use:tooltip={row.fullPath}
                  >
                    {#if filesTreeExpanded.has(row.fullPath)}
                      <ChevronDown size={11} class="tree-chev" />
                    {:else}
                      <ChevronRight size={11} class="tree-chev" />
                    {/if}
                    <Folder size={12} class="tree-folder-icon" />
                    <span class="tree-dir-name truncate">{row.name}</span>
                  </button>
                {:else}
                  {@const isResolved = resolvedPaths.has(row.path)}
                  {@const isActive   = selectedPath === row.path}
                  {@const pres       = mergeFilePresence[row.path]}
                  {@const sideHint   = pres && !pres.ours ? 'added' : pres && !pres.theirs ? 'deleted' : null}
                  {@const monogram   = isResolved ? '✓' : sideHint === 'added' ? 'A' : sideHint === 'deleted' ? 'D' : 'C'}
                  {@const monoTip    = isResolved
                    ? 'Resolved'
                    : sideHint === 'added'
                      ? 'Added on incoming side — no version on current branch'
                      : sideHint === 'deleted'
                        ? 'Deleted on incoming side — no version on incoming branch'
                        : 'Conflict — both sides modified'}
                  <button
                    class="file-item tree-file"
                    class:active={isActive}
                    class:resolved={isResolved}
                    style="padding-left: {6 + row.depth * 12}px"
                    onclick={() => selectConflictFile(row.path)}
                    oncontextmenu={(e) => openFileCtxMenu(e, row.path)}
                    use:tooltip={row.path}
                  >
                    <span class="status-badge"
                      class:s-resolved={isResolved}
                      class:s-added={!isResolved && sideHint === 'added'}
                      class:s-deleted={!isResolved && sideHint === 'deleted'}
                      class:s-conflict={!isResolved && !sideHint}
                      use:tooltip={monoTip}
                    >{monogram}</span>
                    <span class="file-name truncate">{row.name}</span>
                    {#if isActive && isStagingFile}<span class="file-saving">…</span>{/if}
                  </button>
                {/if}
              {/each}
            {/if}
            {#if conflictedFiles.length > 1}
              <button class="next-btn" onclick={nextUnresolved} disabled={allFilesResolved}>
                <ChevronRight size={12} /> Next conflict
              </button>
            {/if}
          </div>
        {/if}
      </div>
      {/if}

      <!-- Editor area -->
      <div class="editor-area">

        {#if isBlockingMode}
          <!-- ── Blocking mode: 2-column diff + result panel ── -->
          {#if !blockingSelectedPath}
            <div class="editor-empty">Select a file from the list</div>
          {:else if isLoadingBlocking}
            <div class="editor-empty"><span class="spinner"></span> Loading…</div>
          {:else if !activeBlocking || activeBlocking.regions.length === 0}
            <!-- No diff regions (binary file, identical content, or a
                 stash entry the user has nothing to compose) — still show
                 the Confirm action so the user can lock in a default
                 decision and move on. -->
            <div class="blocking-editor blocking-editor-empty">
              <div class="conflict-nav">
                <div class="cnav-spacer"></div>
                {#if activeBlockingDecision}
                  <span class="cnav-staged">
                    <CheckCircle2 size={12} />
                    {activeBlockingDecision === 'keep_mine'
                      ? 'Resolution: keep mine'
                      : activeBlockingDecision === 'use_stash'
                        ? 'Resolution: use stash'
                        : 'Resolution: custom merge'}
                  </span>
                {:else}
                  <button
                    class="cnav-stage"
                    onclick={confirmBlockingFile}
                    use:tooltip={'Confirm the default resolution (use stash) for this file'}
                  >
                    <PackageCheck size={12} />
                    Confirm
                  </button>
                {/if}
              </div>
              <div class="editor-empty">
                <AlertTriangle size={16} class="icon-conflict" />
                No differences detected or binary file
              </div>
            </div>
          {:else}
            <div class="blocking-editor">

              <!-- File-level action bar — mirrors the merge mode's
                   conflict-nav so the "Confirm" affordance lives in the
                   same place the user already looks for prev/next. -->
              <div class="conflict-nav">
                {#if activeConflictIds.length > 0}
                  <button class="cnav-btn" onclick={prevConflict} use:tooltip={'Previous diff'}>
                    <ChevronUp size={12} />
                  </button>
                  <span class="cnav-counter">
                    {activeConflictIdx >= 0 ? activeConflictIdx + 1 : '—'} / {activeConflictIds.length}
                  </span>
                  <button class="cnav-btn" onclick={nextConflict} use:tooltip={'Next diff'}>
                    <ChevronDown size={12} />
                  </button>
                {/if}
                <div class="cnav-spacer"></div>
                {#if activeBlockingDecision}
                  <span class="cnav-staged">
                    <CheckCircle2 size={12} />
                    {activeBlockingDecision === 'keep_mine'
                      ? 'Resolution: keep mine'
                      : activeBlockingDecision === 'use_stash'
                        ? 'Resolution: use stash'
                        : 'Resolution: custom merge'}
                  </span>
                {:else if blockingSelectedPath}
                  <button
                    class="cnav-stage"
                    onclick={confirmBlockingFile}
                    use:tooltip={'Confirm the current resolution for this file'}
                  >
                    <PackageCheck size={12} />
                    Confirm
                  </button>
                {/if}
              </div>

              <!-- Two-column synchronized view -->
              <div class="bcol-scroll" bind:this={bcolScrollEl}>
                <!-- Column headers: inside scroll so they participate in the shared grid.
                     Master checkbox flags / unflags every line on its side. -->
                <div class="bcol-headers">
                  <div class="bcol-header bcol-header-ours">
                    <input
                      type="checkbox"
                      class="bcol-header-cb"
                      checked={blockingOursState === 'all'}
                      indeterminate={blockingOursState === 'partial'}
                      onchange={(e) => setAllBlockingSide('ours', e.currentTarget.checked)}
                      use:tooltip={'Toggle all Current lines'}
                    />
                    <span class="bcol-header-title">Current</span>
                    <span class="bcol-header-sub">current file in the working tree</span>
                  </div>
                  <div class="bcol-header-divider"></div>
                  <div class="bcol-header bcol-header-theirs">
                    <input
                      type="checkbox"
                      class="bcol-header-cb"
                      checked={blockingTheirsState === 'all'}
                      indeterminate={blockingTheirsState === 'partial'}
                      onchange={(e) => setAllBlockingSide('theirs', e.currentTarget.checked)}
                      use:tooltip={'Toggle all Stash lines'}
                    />
                    <span class="bcol-header-title">Stash</span>
                    <span class="bcol-header-sub">version in the stash</span>
                  </div>
                </div>
                {#each activeBlockingDisplay as item}
                  {#if item.kind === 'context'}
                    <!-- Context lines: shown in both columns -->
                    {#each item.lines as line, i}
                      <div class="brow brow-context">
                        <div class="brow-left">
                          <span class="linenum">{item.oursStart + i}</span>
                          <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), blockingSelectedPath ?? '')}</code>
                        </div>
                        <div class="brow-divider"></div>
                        <div class="brow-right">
                          <span class="linenum">{item.theirsStart + i}</span>
                          <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), blockingSelectedPath ?? '')}</code>
                        </div>
                      </div>
                    {/each}
                  {:else if item.kind === 'collapsed'}
                    <button
                      type="button"
                      class="brow-collapsed"
                      onclick={() => expandCollapsed(item.contextKey)}
                      use:tooltip={`Show ${item.hiddenLines} hidden lines`}
                    >
                      <ChevronDown size={11} />
                      <span>… {item.hiddenLines} hidden context lines — click to expand</span>
                    </button>
                  {:else}
                    <!-- Conflict region: 2-column with per-line selection -->
                    <div
                      class="bregion"
                      class:bregion-active={activeConflictId === item.regionId}
                      data-conflict-id={item.regionId}
                      onclick={() => activeConflictId = item.regionId}
                      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); activeConflictId = item.regionId; } }}
                      role="button"
                      tabindex="0"
                      aria-label="Conflict block {item.regionId + 1}"
                    >
                      <div class="bregion-header">
                        <div class="bregion-header-left">
                          <span class="bregion-label">Diff {item.regionId + 1}</span>
                          <button class="bregion-icon bregion-icon-ours"
                            onclick={(e) => { e.stopPropagation(); acceptBlockingOurs(item.regionId); }}
                            use:tooltip={'Take Current'}>
                            <ChevronLeft size={11} />
                            <span>Current</span>
                          </button>
                        </div>
                        <button class="bregion-icon bregion-icon-both"
                          onclick={(e) => { e.stopPropagation(); acceptBlockingBoth(item.regionId); }}
                          use:tooltip={'Take both'}>
                          <Equal size={10} />
                          <span>Both</span>
                        </button>
                        <div class="bregion-header-right">
                          <button class="bregion-icon bregion-icon-theirs"
                            onclick={(e) => { e.stopPropagation(); acceptBlockingTheirs(item.regionId); }}
                            use:tooltip={'Take Stash'}>
                            <span>Stash</span>
                            <ChevronRight size={11} />
                          </button>
                        </div>
                      </div>
                      <div class="bregion-cols">
                        <!-- Left: ours lines -->
                        <div class="bregion-col bregion-col-ours">
                          {#if item.oursLines.length === 0}
                            <div class="bregion-empty">— not present —</div>
                          {:else}
                            {#each item.oursLines as line, i}
                              {@const sel = item.oursSelected[i] ?? false}
                              <div
                                class="bline"
                                class:bline-selected={sel}
                                onclick={() => toggleOursLine(item.regionId, i)}
                                role="button"
                                tabindex="0"
                                onkeydown={(e) => e.key === 'Enter' && toggleOursLine(item.regionId, i)}
                              >
                                <input
                                  type="checkbox"
                                  class="bline-cb"
                                  checked={sel}
                                  onchange={() => toggleOursLine(item.regionId, i)}
                                  onclick={(e) => e.stopPropagation()}
                                />
                                <span class="linenum">{item.oursStart + i}</span>
                                <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), blockingSelectedPath ?? '')}</code>
                              </div>
                            {/each}
                          {/if}
                        </div>
                        <div class="bregion-sep"></div>
                        <!-- Right: theirs lines -->
                        <div class="bregion-col bregion-col-theirs">
                          {#if item.theirsLines.length === 0}
                            <div class="bregion-empty">— rimosso nello stash —</div>
                          {:else}
                            {#each item.theirsLines as line, i}
                              {@const sel = item.theirsSelected[i] ?? false}
                              <div
                                class="bline"
                                class:bline-selected={sel}
                                onclick={() => toggleTheirsLine(item.regionId, i)}
                                role="button"
                                tabindex="0"
                                onkeydown={(e) => e.key === 'Enter' && toggleTheirsLine(item.regionId, i)}
                              >
                                <span class="linenum">{item.theirsStart + i}</span>
                                <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), blockingSelectedPath ?? '')}</code>
                                <input
                                  type="checkbox"
                                  class="bline-cb"
                                  checked={sel}
                                  onchange={() => toggleTheirsLine(item.regionId, i)}
                                  onclick={(e) => e.stopPropagation()}
                                />
                              </div>
                            {/each}
                          {/if}
                        </div>
                      </div>
                    </div>
                  {/if}
                {/each}
                <div class="bcol-hscroll" onscroll={onBcolHscroll}>
                  <div bind:this={bcolHscrollInnerEl}></div>
                </div>
              </div>

              <!-- Result panel -->
              <div class="blocking-result" class:is-collapsed={resultCollapsed} style={!resultCollapsed && resultHeight !== null ? `flex: 0 0 ${resultHeight}px` : ''}>
                <!-- Resize handle -->
                <div
                  class="result-resize-handle"
                  onmousedown={startResultResize}
                  role="separator"
                  aria-orientation="horizontal"
                  tabindex="-1"
                  aria-hidden="true"
                  use:tooltip={'Trascina per ridimensionare'}
                ></div>
                <div class="result-header">
                  <button
                    class="result-collapse-btn"
                    onclick={() => resultCollapsed = !resultCollapsed}
                    use:tooltip={resultCollapsed ? 'Espandi il pannello risultato' : 'Comprimi il pannello risultato'}
                    aria-label={resultCollapsed ? 'Espandi risultato' : 'Comprimi risultato'}
                  >
                    {#if resultCollapsed}
                      <ChevronUp size={12} />
                    {:else}
                      <ChevronDown size={12} />
                    {/if}
                  </button>
                  <span class="result-header-title">Merge result</span>
                  <span class="result-header-hint">
                    {#if activeBlocking?.manualResult !== null}
                      <span class="result-manual-badge">manually edited</span>
                    {:else}
                      based on selection
                    {/if}
                  </span>
                  {#if activeBlocking?.manualResult !== null && !resultCollapsed}
                    <button class="result-reset-btn" onclick={resetBlockingResult} use:tooltip={'Reset to selection'}>
                      ↩ Reset
                    </button>
                  {/if}
                </div>
                {#if !resultCollapsed}
                  <div class="result-editor-wrap" transition:resultSlide={{ duration: animStore.dPanel }}>
                    <pre class="result-highlight" bind:this={resultPreEl} aria-hidden="true">{@html highlightedResult}</pre>
                    <textarea
                      class="result-textarea"
                      bind:this={resultTextareaEl}
                      value={activeBlockingResult}
                      oninput={(e) => { setBlockingManualResult(e.currentTarget.value); syncResultScroll(); }}
                      onscroll={syncResultScroll}
                      spellcheck="false"
                    ></textarea>
                  </div>
                {/if}
              </div>

            </div>
          {/if}

        {:else}
          <!-- ── Conflict mode: 2-column + result (same as blocking) ── -->
          {#if !selectedPath}
            <div class="editor-empty">Select a file from the list</div>
          {:else if isLoading}
            <div class="editor-empty"><span class="spinner"></span> Loading…</div>
          {:else if !activeMerge || activeMerge.regions.length === 0}
            <div class="editor-empty">
              <AlertTriangle size={16} class="icon-conflict" />
              No conflicts found
            </div>
          {:else}
            <div class="blocking-editor">

              <!-- File-level action bar.  Always visible when a file is
                   selected so the Stage button reaches modify/delete and
                   add/modify cases too (no conflict regions ⇒ activeConflictIds
                   is empty, but the user still needs to confirm their
                   Keep/Remove choice). The prev/next conflict counter
                   only renders when there are actual <<<<<< regions. -->
              {#if selectedPath}
                <div class="conflict-nav">
                  {#if activeConflictIds.length > 0}
                    <button class="cnav-btn" onclick={prevConflict} use:tooltip={'Previous conflict'}>
                      <ChevronUp size={12} />
                    </button>
                    <span class="cnav-counter">
                      {activeConflictIdx >= 0 ? activeConflictIdx + 1 : '—'} / {activeConflictIds.length}
                    </span>
                    <button class="cnav-btn" onclick={nextConflict} use:tooltip={'Next conflict'}>
                      <ChevronDown size={12} />
                    </button>
                  {/if}
                  <div class="cnav-spacer"></div>
                  {#if resolvedPaths.has(selectedPath)}
                    <span class="cnav-staged"><CheckCircle2 size={12} /> File staged</span>
                  {:else}
                    <button
                      class="cnav-stage"
                      onclick={stageMergeFile}
                      disabled={isStagingFile}
                      use:tooltip={isPresenceConflict
                        ? 'Apply the chosen Keep / Remove decision'
                        : 'Apply the current resolution and stage the file'}
                    >
                      <PackageCheck size={12} />
                      {isStagingFile
                        ? 'Staging…'
                        : isPresenceConflict
                          ? (presenceDecisions[selectedPath] === 'remove' ? 'Remove file' : 'Keep file')
                          : 'Stage file'}
                    </button>
                  {/if}
                </div>
              {/if}

              <!-- Modify/delete & add/modify get a dedicated resolver:
                   the regular 2-col diff duplicates "context" lines on
                   both sides (since there are no <<<<<< markers in the
                   workdir for these cases) which is genuinely misleading.
                   Keep/Remove is a single binary choice with a live
                   preview; no line-by-line composition needed. -->
              {#if isPresenceConflict && selectedPath}
                {@const pres = mergeFilePresence[selectedPath]}
                {@const labels = mergeFileLabels[selectedPath] ?? { ours: 'HEAD', theirs: 'MERGE_HEAD' }}
                {@const decision = presenceDecisions[selectedPath] ?? 'keep'}
                {@const sideLabel = pres && !pres.ours ? labels.theirs : labels.ours}
                {@const previewLines =
                  mergeFileData[selectedPath]
                    ? mergeFileData[selectedPath].regions
                        .flatMap(r => r.kind === 'context' ? r.lines : [])
                    : []}
                <div class="presence-resolver">
                  <div class="presence-banner" class:added={!pres.ours} class:deleted={!pres.theirs}>
                    <TriangleAlert size={13} />
                    {#if !pres.ours}
                      <span>
                        <strong>Added on {labels.theirs}</strong> — this file does not exist on
                        <code>{labels.ours}</code>. Choose to keep the incoming file or discard it.
                      </span>
                    {:else}
                      <span>
                        <strong>Deleted on {labels.theirs}</strong> — this file no longer exists on
                        <code>{labels.theirs}</code>. Choose to keep your current version or accept the deletion.
                      </span>
                    {/if}
                  </div>

                  <div class="presence-choice-row">
                    <button
                      type="button"
                      class="presence-choice"
                      class:active={decision === 'keep'}
                      onclick={() => setPresenceDecision(selectedPath!, 'keep')}
                    >
                      <span class="presence-choice-title">
                        <CheckCircle2 size={13} />
                        Keep file
                      </span>
                      <span class="presence-choice-sub">Use the version from <code>{sideLabel}</code></span>
                    </button>
                    <button
                      type="button"
                      class="presence-choice danger"
                      class:active={decision === 'remove'}
                      onclick={() => setPresenceDecision(selectedPath!, 'remove')}
                    >
                      <span class="presence-choice-title">
                        <X size={13} />
                        Accept deletion
                      </span>
                      <span class="presence-choice-sub">Remove the file from workdir and index</span>
                    </button>
                  </div>

                  <div class="presence-preview">
                    <div class="presence-preview-header">
                      {#if decision === 'remove'}
                        <span class="presence-preview-title removed">
                          <X size={12} /> Resolution: file will be removed
                        </span>
                      {:else}
                        <span class="presence-preview-title kept">
                          <CheckCircle2 size={12} /> Resolution: file will be kept ({sideLabel})
                        </span>
                      {/if}
                    </div>
                    {#if decision === 'remove'}
                      <div class="presence-preview-empty">
                        — the file will be removed from the working tree and the index —
                      </div>
                    {:else}
                      <div class="presence-preview-body">
                        {#each previewLines as line, i}
                          <div class="presence-preview-row">
                            <span class="presence-preview-linenum">{i + 1}</span>
                            <code class="presence-preview-code">{@html highlight(line.replace(/\n$/, ''), selectedPath ?? '')}</code>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>

              {:else}
              <!-- Two-column synchronized view -->
              <div class="bcol-scroll" bind:this={bcolScrollEl}>
                <!-- Column headers: inside scroll so they participate in the shared grid.
                     Each header has a master checkbox that flags / unflags every line
                     on its side across all conflict regions of the active file. -->
                <div class="bcol-headers">
                  <div class="bcol-header bcol-header-ours">
                    <input
                      type="checkbox"
                      class="bcol-header-cb"
                      checked={mergeOursState === 'all'}
                      indeterminate={mergeOursState === 'partial'}
                      onchange={(e) => setAllMergeSide('ours', e.currentTarget.checked)}
                      use:tooltip={`Toggle all ${activeMergeLabels.ours} lines`}
                    />
                    <span class="bcol-header-title">{activeMergeLabels.ours}</span>
                    <span class="bcol-header-sub">ours</span>
                  </div>
                  <div class="bcol-header-divider"></div>
                  <div class="bcol-header bcol-header-merge-theirs">
                    <input
                      type="checkbox"
                      class="bcol-header-cb"
                      checked={mergeTheirsState === 'all'}
                      indeterminate={mergeTheirsState === 'partial'}
                      onchange={(e) => setAllMergeSide('theirs', e.currentTarget.checked)}
                      use:tooltip={`Toggle all ${activeMergeLabels.theirs} lines`}
                    />
                    <span class="bcol-header-title">{activeMergeLabels.theirs}</span>
                    <span class="bcol-header-sub">theirs</span>
                  </div>
                </div>
                {#each activeMergeDisplay as item}
                  {#if item.kind === 'context'}
                    {#each item.lines as line, i}
                      <div class="brow brow-context">
                        <div class="brow-left">
                          <span class="linenum">{item.oursStart + i}</span>
                          <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), selectedPath ?? '')}</code>
                        </div>
                        <div class="brow-divider"></div>
                        <div class="brow-right">
                          <span class="linenum">{item.theirsStart + i}</span>
                          <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), selectedPath ?? '')}</code>
                        </div>
                      </div>
                    {/each}
                  {:else if item.kind === 'collapsed'}
                    <button
                      type="button"
                      class="brow-collapsed"
                      onclick={() => expandCollapsed(item.contextKey)}
                      use:tooltip={`Show ${item.hiddenLines} hidden lines`}
                    >
                      <ChevronDown size={11} />
                      <span>… {item.hiddenLines} hidden context lines — click to expand</span>
                    </button>
                  {:else}
                    <div
                      class="bregion"
                      class:bregion-active={activeConflictId === item.regionId}
                      data-conflict-id={item.regionId}
                      onclick={() => activeConflictId = item.regionId}
                      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); activeConflictId = item.regionId; } }}
                      role="button"
                      tabindex="0"
                      aria-label="Conflict {item.regionId + 1}"
                    >
                      <div class="bregion-header">
                        <!-- Left column: conflict label + Take-Ours button -->
                        <div class="bregion-header-left">
                          <span class="bregion-label">Conflict {item.regionId + 1}</span>
                          <button class="bregion-icon bregion-icon-ours"
                            onclick={(e) => { e.stopPropagation(); acceptMergeOurs(item.regionId); }}
                            use:tooltip={`Take ${activeMergeLabels.ours} and go to next`}>
                            <ChevronLeft size={11} />
                            <span>Ours</span>
                          </button>
                        </div>
                        <!-- Center: Take-Both -->
                        <button class="bregion-icon bregion-icon-both"
                          onclick={(e) => { e.stopPropagation(); acceptMergeBoth(item.regionId); }}
                          use:tooltip={'Take both'}>
                          <Equal size={10} />
                          <span>Both</span>
                        </button>
                        <!-- Right column: Take-Theirs -->
                        <div class="bregion-header-right">
                          <button class="bregion-icon bregion-icon-theirs"
                            onclick={(e) => { e.stopPropagation(); acceptMergeTheirs(item.regionId); }}
                            use:tooltip={`Take ${activeMergeLabels.theirs} and go to next`}>
                            <span>Theirs</span>
                            <ChevronRight size={11} />
                          </button>
                        </div>
                      </div>
                      <div class="bregion-cols">
                        <!-- Left: ours lines -->
                        <div class="bregion-col bregion-col-ours">
                          {#if item.oursLines.length === 0}
                            <div class="bregion-empty">— vuoto —</div>
                          {:else}
                            {#each item.oursLines as line, i}
                              {@const sel = item.oursSelected[i] ?? false}
                              <div
                                class="bline"
                                class:bline-selected={sel}
                                onclick={() => toggleMergeOursLine(item.regionId, i)}
                                role="button"
                                tabindex="0"
                                onkeydown={(e) => e.key === 'Enter' && toggleMergeOursLine(item.regionId, i)}
                              >
                                <input
                                  type="checkbox"
                                  class="bline-cb"
                                  checked={sel}
                                  onchange={() => toggleMergeOursLine(item.regionId, i)}
                                  onclick={(e) => e.stopPropagation()}
                                />
                                <span class="linenum">{item.oursStart + i}</span>
                                <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), selectedPath ?? '')}</code>
                              </div>
                            {/each}
                          {/if}
                        </div>
                        <div class="bregion-sep"></div>
                        <!-- Right: theirs lines -->
                        <div class="bregion-col bregion-col-merge-theirs">
                          {#if item.theirsLines.length === 0}
                            <div class="bregion-empty">— not present —</div>
                          {:else}
                            {#each item.theirsLines as line, i}
                              {@const sel = item.theirsSelected[i] ?? false}
                              <div
                                class="bline"
                                class:bline-selected={sel}
                                onclick={() => toggleMergeTheirsLine(item.regionId, i)}
                                role="button"
                                tabindex="0"
                                onkeydown={(e) => e.key === 'Enter' && toggleMergeTheirsLine(item.regionId, i)}
                              >
                                <span class="linenum">{item.theirsStart + i}</span>
                                <code class="brow-code">{@html highlight(line.replace(/\n$/, ''), selectedPath ?? '')}</code>
                                <input
                                  type="checkbox"
                                  class="bline-cb"
                                  checked={sel}
                                  onchange={() => toggleMergeTheirsLine(item.regionId, i)}
                                  onclick={(e) => e.stopPropagation()}
                                />
                              </div>
                            {/each}
                          {/if}
                        </div>
                      </div>
                    </div>
                  {/if}
                {/each}
                <div class="bcol-hscroll" onscroll={onBcolHscroll}>
                  <div bind:this={bcolHscrollInnerEl}></div>
                </div>
              </div>

              <!-- Result panel -->
              <div class="blocking-result" class:is-collapsed={resultCollapsed} style={!resultCollapsed && resultHeight !== null ? `flex: 0 0 ${resultHeight}px` : ''}>
                <div class="result-resize-handle"
                  onmousedown={startResultResize}
                  role="separator"
                  aria-orientation="horizontal"
                  tabindex="-1"
                  aria-hidden="true"
                  use:tooltip={'Trascina per ridimensionare'}
                ></div>
                <div class="result-header">
                  <button
                    class="result-collapse-btn"
                    onclick={() => resultCollapsed = !resultCollapsed}
                    use:tooltip={resultCollapsed ? 'Espandi il pannello risultato' : 'Comprimi il pannello risultato'}
                    aria-label={resultCollapsed ? 'Espandi risultato' : 'Comprimi risultato'}
                  >
                    {#if resultCollapsed}
                      <ChevronUp size={12} />
                    {:else}
                      <ChevronDown size={12} />
                    {/if}
                  </button>
                  <span class="result-header-title">Merge result</span>
                  <span class="result-header-hint">
                    {#if activeMerge?.manualResult !== null}
                      <span class="result-manual-badge">manually edited</span>
                    {:else}
                      based on selection
                    {/if}
                  </span>
                  {#if activeMerge?.manualResult !== null && !resultCollapsed}
                    <button class="result-reset-btn" onclick={resetMergeResult} use:tooltip={'Reset to selection'}>
                      ↩ Reset
                    </button>
                  {/if}
                  <div class="result-header-spacer"></div>
                </div>
                {#if !resultCollapsed}
                  <div class="result-editor-wrap" transition:resultSlide={{ duration: animStore.dPanel }}>
                    <pre class="result-highlight" bind:this={resultPreEl} aria-hidden="true">{@html highlightedResult}</pre>
                    <textarea
                      class="result-textarea"
                      bind:this={resultTextareaEl}
                      value={activeMergeResult}
                      oninput={(e) => { setMergeManualResult(e.currentTarget.value); syncResultScroll(); }}
                      onscroll={syncResultScroll}
                      spellcheck="false"
                    ></textarea>
                  </div>
                {/if}
              </div>
              {/if}
              <!-- /isPresenceConflict -->

            </div>
          {/if}
        {/if}

      </div>
    </div>

  </div>

  {#snippet footer()}
    <div class="cr-footer-inner" class:mode-merge={isMerge} class:mode-stash={!isMerge}>
      {#if isBlockingMode}
        <span class="footer-info">
          Compose the resolution for each file and click <strong>Confirm</strong>. Unconfirmed files will use the stash version.
        </span>
        <div class="footer-spacer"></div>
        <Button variant="danger" onclick={handleClose} disabled={isForcing}>
          {#snippet iconStart()}<XCircle size={13} />{/snippet}
          Cancel
        </Button>
        <Button variant="primary" color="var(--success)" onclick={handleForceApply} loading={isForcing}>
          {#snippet iconStart()}<PackageCheck size={13} />{/snippet}
          {isForcing ? 'Applying…' : 'Apply stash →'}
        </Button>

      {:else if confirmAbort}
        <div class="abort-confirm">
          <TriangleAlert size={13} />
          <span>
            {#if isRealMerge}
              Abort the merge? All changes will be lost.
            {:else if isMerge}
              Discard the resolution? Files will revert to the HEAD version.
            {:else}
              Abort the stash? The working tree will be restored to HEAD.
            {/if}
          </span>
          <Button variant="danger" size="sm" onclick={handleAbort} loading={isAborting}>
            {isAborting ? 'Aborting…' : 'Yes, abort'}
          </Button>
          <Button variant="secondary" size="sm" onclick={() => confirmAbort = false}>No</Button>
        </div>

      {:else}
        {#if isRealMerge}
          <div class="footer-message-wrap">
            <input
              class="merge-msg-input"
              type="text"
              bind:value={mergeMessage}
              placeholder="Merge commit message…"
              disabled={isMerging}
            />
          </div>
        {:else}
          <div class="footer-spacer"></div>
        {/if}

        <!-- In the orphan-conflict case, once every file has been resolved
             there are no unmerged index entries left to revert — the abort
             fallback would just error with "Nothing to abort". Disable the
             button in that specific case with an explanatory tooltip. -->
        {@const orphanAllResolved = isMerge && !isRealMerge && allFilesResolved}
        <Button
          variant="danger"
          onclick={() => confirmAbort = true}
          disabled={isAborting || orphanAllResolved}
          title={orphanAllResolved
            ? 'All conflicts are already resolved — nothing to discard. Use "Apply resolution" or close.'
            : ''}
        >
          {#snippet iconStart()}<XCircle size={13} />{/snippet}
          {#if isRealMerge}
            Abort Merge
          {:else if isMerge}
            Discard resolution
          {:else}
            Abort Stash
          {/if}
        </Button>

        <Button
          variant="primary"
          color={isMerge ? undefined : 'var(--success)'}
          onclick={handleComplete}
          disabled={!canComplete}
          loading={isMerging || isCompleting}
          title={
            !allFilesResolved
              ? 'Resolve all conflicts before completing'
              : (isRealMerge && mergeMessage.trim().length === 0)
                ? 'Enter a merge commit message first'
                : ''
          }
        >
          {#snippet iconStart()}
            {#if isRealMerge}<GitMerge size={13} />{:else}<PackageCheck size={13} />{/if}
          {/snippet}
          {#if isRealMerge}
            {isMerging ? 'Merging…' : 'Merge →'}
          {:else if isMerge}
            {isMerging ? 'Applying…' : 'Apply resolution →'}
          {:else}
            {isCompleting ? 'Updating…' : 'Complete →'}
          {/if}
        </Button>
      {/if}
    </div>
  {/snippet}

</Modal>

<!-- ── File-list right-click context menu ────────────────────────────────── -->
{#if fileCtxMenu}
  {@const labels = mergeFileLabels[fileCtxMenu.path] ?? { ours: 'ours', theirs: 'theirs' }}
  <ContextMenu
    items={[
      { id: 'ours',   label: `Take ours (${labels.ours})`,      icon: ChevronLeft,  iconColor: 'var(--accent)' },
      { id: 'theirs', label: `Take theirs (${labels.theirs})`,  icon: ChevronRight, iconColor: 'var(--color-tag)' },
    ] as MenuItem[]}
    x={fileCtxMenu.x}
    y={fileCtxMenu.y}
    onSelect={(id) => {
      const path = fileCtxMenu!.path;
      if (id === 'ours' || id === 'theirs') {
        void acceptAllForFile(path, id);
      }
    }}
    onClose={() => fileCtxMenu = null}
  />
{/if}

<!-- ── Blocking-files right-click context menu ───────────────────────────── -->
{#if blockingFileCtxMenu}
  <ContextMenu
    items={[
      { id: 'keep_mine', label: 'Keep mine (current)', icon: ChevronLeft,  iconColor: 'var(--accent)' },
      { id: 'use_stash', label: 'Use stash',           icon: ChevronRight, iconColor: 'var(--color-stash)' },
    ] as MenuItem[]}
    x={blockingFileCtxMenu.x}
    y={blockingFileCtxMenu.y}
    onSelect={(id) => {
      const path = blockingFileCtxMenu!.path;
      if (id === 'keep_mine' || id === 'use_stash') {
        quickResolveBlockingFile(path, id);
      }
    }}
    onClose={() => blockingFileCtxMenu = null}
  />
{/if}

<style>
  .cr-modal {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* Rounded tinted badge housing the mode icon. Replaces the bare,
     tight-against-title icon that looked like an afterthought. */
  .header-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    flex-shrink: 0;
  }
  .mode-merge .header-icon {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .mode-stash .header-icon {
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 28%, transparent);
  }
  /* Orphan-conflict state (non-merge, non-stash) uses the warning triangle
     icon — give it a matching amber halo so it reads as a state indicator
     rather than a hazard sign stuck next to text. */
  .mode-merge .header-icon:has(:global(.lucide-triangle-alert)) {
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    border-color: color-mix(in srgb, var(--warning) 28%, transparent);
  }

  .header-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    letter-spacing: 0.1px;
    line-height: 1.2;
  }

  .stash-chip {
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--warning);
    background: rgba(226, 163, 53, 0.10);
    border: 1px solid rgba(226, 163, 53, 0.25);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
  }

  .header-labels {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: 2px;
    font-family: var(--font-ui-sans);
  }

  /* Branch-like chips for the two sides of the merge. Icon + monospace
     name inside a tinted pill so they read as refs rather than plain text
     with decorative arrows. */
  .label-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-code);
    letter-spacing: 0.2px;
    line-height: 1.4;
    white-space: nowrap;
  }
  .label-chip-text { max-width: 160px; overflow: hidden; text-overflow: ellipsis; }

  .label-chip-ours {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--success) 28%, transparent);
  }
  .mode-merge .label-chip-theirs {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .mode-stash .label-chip-theirs {
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 28%, transparent);
  }

  /* Small "vs" separator between the two chips — subtler than the
     previous GitMerge icon but still signals "two sides of a conflict". */
  .label-chip-vs {
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    color: var(--text-disabled);
    font-family: var(--font-ui-sans);
    padding: 0 2px;
  }

  .header-progress {
    font-size: 11px;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
  }

  /* ── Body ────────────────────────────────────────────────────────────────
     Acts as the "panels container" of the modal, mirroring the main app's
     workspace layout: bg-elevated reveals as 4px gaps around floating
     bg-base panel cards (sidebar + editor) with rounded corners. */
  .cr-body {
    display: flex;
    flex: 1;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
  }

  /* ── File sidebar ──────────────────────────────────────────────────── */
  .file-sidebar {
    /* Fixed-size panel. Open/close animation is driven by the
       `sidebarSlide` Svelte transition — no CSS transitions needed. */
    flex: 0 0 220px;
    margin-right: 4px;
    background: var(--bg-base);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }

  .sidebar-label-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 6px 12px;
  }

  .sidebar-label {
    flex: 1;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }

  /* List / tree view toggle — mirrors the stage panel's view-mode pair. */
  .sidebar-toggle-btn {
    display: flex; align-items: center; justify-content: center;
    width: 20px; height: 20px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .sidebar-toggle-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .sidebar-toggle-btn.active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  /* Tree view — denser than the card-list, no gap + directory rows. */
  .files-list.tree-mode {
    gap: 0;
    padding: 4px 4px;
  }
  .files-list.tree-mode .file-item {
    padding: 3px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    box-shadow: none;
  }
  .files-list.tree-mode .file-item:hover {
    background: var(--bg-hover);
    border-color: transparent;
    box-shadow: none;
  }
  .mode-merge .files-list.tree-mode .file-item.active {
    background: rgba(77,120,204,.14);
    border-color: rgba(77,120,204,.4);
  }
  .mode-stash .files-list.tree-mode .file-item.active {
    background: rgba(226,163,53,.12);
    border-color: rgba(226,163,53,.4);
  }

  .tree-dir {
    display: flex; align-items: center; gap: 4px;
    padding: 3px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    width: 100%;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tree-dir:hover { background: var(--bg-hover); color: var(--text-primary); }
  :global(.tree-chev)        { color: var(--text-muted); flex-shrink: 0; }
  :global(.tree-folder-icon) { color: var(--text-muted); flex-shrink: 0; }
  .tree-dir-name {
    flex: 1; min-width: 0;
    font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans);
  }

  /* Card-list pattern (mirrors Reflog / Issues sidebars). */
  .files-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    overflow-y: auto;
    flex: 1;
  }

  .file-item {
    display: flex; align-items: center; gap: 7px;
    padding: 7px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer; text-align: left; width: 100%;
    color: inherit;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                box-shadow var(--transition-fast);
  }
  .file-item:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0,0,0,0.15);
  }
  .mode-merge .file-item.active {
    background: rgba(77,120,204,.14);
    border-color: rgba(77,120,204,.55);
  }
  .mode-stash .file-item.active {
    background: rgba(226,163,53,.12);
    border-color: rgba(226,163,53,.55);
  }
  .file-item.resolved .file-name { color: var(--text-muted); }

  .file-status-icon { display: flex; flex-shrink: 0; }
  :global(.icon-conflict) { color: var(--warning); }
  :global(.icon-resolved) { color: var(--success); }
  :global(.icon-viewed)   { color: var(--accent); }

  /* Monogram status badge — matches StageArea's pattern so the two file
     lists feel like the same widget at a glance. A/D/M/C/✓ shown big and
     in the right color instead of a near-invisible secondary chip. */
  .status-badge {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    background: var(--bg-overlay);
    color: var(--text-muted);
    letter-spacing: 0;
    font-family: var(--font-code);
  }
  .status-badge.s-added    { background: color-mix(in srgb, var(--color-file-added) 22%, transparent);    color: var(--color-file-added); }
  .status-badge.s-deleted  { background: color-mix(in srgb, var(--color-file-deleted) 22%, transparent);  color: var(--color-file-deleted); }
  .status-badge.s-conflict { background: color-mix(in srgb, var(--warning) 22%, transparent);             color: var(--warning); }
  .status-badge.s-resolved { background: color-mix(in srgb, var(--success) 22%, transparent);             color: var(--success); }

  .file-name {
    font-size: var(--font-size-sm); color: var(--text-primary);
    font-family: var(--font-ui-sans); min-width: 0; flex: 1;
  }
  .file-saving { font-size: 10px; color: var(--text-muted); }

  .presence-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    font-size: 11px;
    line-height: 1.45;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .presence-banner code {
    font-family: var(--font-code);
    padding: 1px 5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
  }
  .presence-banner.added {
    background: color-mix(in srgb, var(--success) 10%, transparent);
    color: var(--text-primary);
    border-bottom-color: color-mix(in srgb, var(--success) 35%, transparent);
  }
  .presence-banner.added :global(svg) { color: var(--success); }
  .presence-banner.deleted {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    color: var(--text-primary);
    border-bottom-color: color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .presence-banner.deleted :global(svg) { color: var(--danger); }

  .presence-resolver {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .presence-choice-row {
    display: flex;
    gap: 10px;
    padding: 12px;
    flex-shrink: 0;
  }
  .presence-choice {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 14px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
    text-align: left;
    font-family: var(--font-ui-sans);
  }
  .presence-choice:hover { background: var(--bg-hover); }
  .presence-choice.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .presence-choice.danger.active {
    border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }
  .presence-choice-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .presence-choice.active .presence-choice-title { color: var(--accent); }
  .presence-choice.danger.active .presence-choice-title { color: var(--danger); }
  .presence-choice-sub {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .presence-choice-sub code {
    font-family: var(--font-code);
    padding: 0 4px;
    border-radius: 2px;
    background: var(--bg-base);
  }

  .presence-preview {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    margin: 0 12px 12px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .presence-preview-header {
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    font-size: 11px;
    flex-shrink: 0;
  }
  .presence-preview-title {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-weight: 600;
  }
  .presence-preview-title.kept    { color: var(--success); }
  .presence-preview-title.removed { color: var(--danger); }

  .presence-preview-body {
    margin: 0;
    padding: 6px 0;
    flex: 1;
    overflow: auto;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    line-height: 1.45;
    color: var(--text-primary);
  }
  .presence-preview-row {
    display: flex;
    gap: 12px;
    padding: 0 12px;
    min-width: max-content;
  }
  .presence-preview-row:hover {
    background: color-mix(in srgb, var(--bg-overlay) 60%, transparent);
  }
  .presence-preview-linenum {
    flex-shrink: 0;
    width: 36px;
    text-align: right;
    color: var(--text-muted);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }
  .presence-preview-code {
    flex: 1;
    white-space: pre;
    font-family: inherit;
    background: none;
  }
  .presence-preview-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-style: italic;
    font-size: 12px;
    padding: 24px;
  }

  .blocking-decision-badge {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 14px;
    height: 14px;
    padding: 0 4px;
    font-size: 9px;
    font-weight: 700;
    font-family: var(--font-code);
    border-radius: var(--radius-sm);
    line-height: 1;
  }
  .blocking-decision-badge.keep {
    background: color-mix(in srgb, var(--accent) 24%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .blocking-decision-badge.stash {
    background: color-mix(in srgb, var(--warning) 24%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 45%, transparent);
  }
  .blocking-decision-badge.custom {
    background: color-mix(in srgb, var(--success) 24%, transparent);
    color: var(--success);
    border: 1px solid color-mix(in srgb, var(--success) 45%, transparent);
  }


  .sidebar-divider { height: 1px; background: var(--border-subtle); margin: 0; flex-shrink: 0; }

  .next-btn {
    display: flex; align-items: center; gap: 4px;
    padding: 6px 12px; background: none; border: none; cursor: pointer;
    font-size: 11px; font-family: var(--font-ui-sans); width: 100%;
    transition: background var(--transition-fast);
  }
  .mode-merge .next-btn { color: var(--accent); }
  .mode-stash .next-btn { color: var(--warning); }
  .next-btn:hover:not(:disabled) { background: var(--bg-hover); }
  .next-btn:disabled { color: var(--text-disabled); cursor: default; }

  /* ── Editor area ─────────────────────────────────────────────────────── */
  .editor-area {
    flex: 1; display: flex; flex-direction: column;
    overflow: hidden; min-width: 0; position: relative;
    background: var(--bg-base);
    border-radius: 12px;
  }

  .editor-empty {
    flex: 1; display: flex; align-items: center; justify-content: center;
    color: var(--text-muted); font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans); gap: 8px;
  }

  /* ═══════════════════════════════════════════════════════════════════════
     BLOCKING MODE — 2-column diff + result panel
  ═══════════════════════════════════════════════════════════════════════ */

  .blocking-editor {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
  }

  /* Scroll area — also the shared CSS grid that all rows subgrid from.
     Columns are fixed at 1fr each (50/50) so the panes never widen beyond
     the viewport. Horizontal scroll happens INSIDE each row cell (via
     `overflow: hidden` on .brow-left / .brow-right / .bline plus a max-content
     code element) and is driven in lock-step by the sticky `.bcol-hscroll`
     scrollbar at the bottom — same pattern as DiffViewer's `.split-hscroll`. */
  .bcol-scroll {
    flex: 1; overflow: hidden auto; min-height: 0; min-width: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 4px minmax(0, 1fr);
    align-content: start;
  }

  /* Sticky shared horizontal scrollbar at the bottom of .bcol-scroll. Spans
     the full grid width and drives `scrollLeft` on every row cell via the
     `onBcolHscroll` handler. Inner div is sized programmatically (in the
     $effect) to `containerWidth + maxOverflow` so the scrollbar reflects
     the actual scrollable distance. Mirrors `.split-hscroll` in DiffViewer. */
  .bcol-hscroll {
    grid-column: 1 / -1;
    position: sticky;
    bottom: 0;
    left: 0;
    width: 100%;
    height: 12px;
    overflow-x: auto;
    overflow-y: hidden;
    background: var(--bg-elevated);
    z-index: 3;
    scrollbar-width: thin;
    scrollbar-color: var(--border) var(--bg-elevated);
  }
  .bcol-hscroll::-webkit-scrollbar       { height: 10px; }
  .bcol-hscroll::-webkit-scrollbar-track { background: var(--bg-elevated); }
  .bcol-hscroll::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: var(--radius-sm);
    border: 2px solid var(--bg-elevated);
  }
  .bcol-hscroll::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }
  .bcol-hscroll > div { height: 1px; }

  /* Column headers: sticky so they remain visible while scrolling vertically;
     they scroll horizontally with the content since they live inside .bcol-scroll. */
  .bcol-headers {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
    position: sticky; top: 0; z-index: 2;
    border-bottom: 1px solid var(--border-subtle);
  }

  .bcol-header {
    display: flex; align-items: center; gap: 8px;
    padding: 7px 12px; background: var(--bg-elevated);
  }

  .bcol-header-divider { background: var(--border-subtle); }

  .bcol-header-ours   { border-top: 2px solid rgba(95,173,86,.5); }
  .bcol-header-theirs { border-top: 2px solid rgba(226,163,53,.6); }

  .bcol-header-title {
    font-size: 11px; font-weight: 600; color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  .bcol-header-sub {
    font-size: 10px; color: var(--text-muted); font-family: var(--font-ui-sans);
  }

  /* Context row: spans all 3 columns, inherits column widths via subgrid */
  .brow {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
    border-bottom: 1px solid var(--border-subtle);
    min-height: 20px;
  }
  .brow:last-child { border-bottom: none; }

  .brow-context { background: var(--bg-base); }

  /* Collapsed-context placeholder — replaces huge stretches of context that
     would otherwise tank rendering performance. Spans the full width of
     the bcol-scroll subgrid. Click to expand. */
  .brow-collapsed {
    grid-column: 1 / -1;
    display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 5px 12px;
    background: var(--bg-elevated);
    border: none;
    border-top: 1px dashed var(--border-subtle);
    border-bottom: 1px dashed var(--border-subtle);
    color: var(--text-muted);
    font-size: 10px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .brow-collapsed:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .brow-left, .brow-right {
    display: flex; align-items: baseline; gap: 0;
    padding: 1px 0; min-width: 0; overflow: hidden;
  }

  .brow-divider {
    width: 4px; background: var(--border-subtle); flex-shrink: 0;
  }

  .linenum {
    flex-shrink: 0;
    display: inline-block;
    min-width: 36px;
    padding: 0 8px;
    text-align: right;
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-disabled);
    user-select: none;
    line-height: 1.6;
  }

  .brow-code {
    font-family: var(--font-code); font-size: 12px; line-height: 1.6;
    color: var(--text-primary); white-space: pre;
    /* Stay at intrinsic max-content width: the surrounding cell
       (.brow-left / .brow-right / .bline) has `overflow: hidden` and is
       scrolled programmatically by the shared bottom scrollbar. */
    flex: 0 0 auto;
  }

  /* Conflict region block: spans all 3 columns, uses subgrid for internal layout */
  .bregion {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
    align-content: start;
    border-bottom: 2px solid rgba(226,163,53,.25);
    margin-bottom: 2px;
  }
  .bregion:last-child { margin-bottom: 0; }

  .bregion-header {
    grid-column: 1 / -1;
    /* 3-column grid so action buttons align with the columns below:
       Ours on the left edge, Both centred, Theirs on the right edge. */
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 8px;
    padding: 3px 10px;
    background: rgba(226,163,53,.06);
    border-bottom: 1px solid rgba(226,163,53,.20);
    border-top: 1px solid rgba(226,163,53,.20);
  }
  .bregion-header-left {
    display: flex; align-items: center; gap: 8px;
    min-width: 0;
  }
  .bregion-header-right {
    display: flex; align-items: center;
    justify-content: flex-end;
  }

  .bregion-label {
    font-size: 10px; font-weight: 600; color: var(--warning);
    font-family: var(--font-ui-sans);
    white-space: nowrap;
  }

  /* ── Compact icon buttons (per-region accept) ─────────────────────────────
     Replaces the verbose ours / Entrambi / theirs labelled buttons. The
     branch labels live in the column headers above; here we just need the
     direction. Buttons are subtle until hover. */
  /* These are the *canonical* per-conflict Take-Ours / Take-Both /
     Take-Theirs buttons. They used to be transparent by default and only
     reveal colour on hover — basically invisible unless you knew they
     existed. Now they carry a permanent semantic background + label so
     the action is obvious at a glance. */
  .bregion-icon {
    display: inline-flex; align-items: center; justify-content: center;
    gap: 4px;
    height: 18px;
    padding: 0 7px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-ui-sans);
    letter-spacing: 0.03em;
    text-transform: uppercase;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .bregion-icon-ours {
    background: rgba(95,173,86,.12);
    border-color: rgba(95,173,86,.35);
    color: var(--success);
  }
  .bregion-icon-ours:hover {
    background: rgba(95,173,86,.22);
    border-color: rgba(95,173,86,.55);
  }
  /* Purple — distinct from the green/blue semantic columns so "Both"
     reads as its own third option rather than a neutral disabled-looking
     chip. Matches --color-tag already used elsewhere in the app. */
  .bregion-icon-both {
    background: rgba(198,120,221,.14);
    border-color: rgba(198,120,221,.40);
    color: var(--color-tag);
  }
  .bregion-icon-both:hover {
    background: rgba(198,120,221,.24);
    border-color: rgba(198,120,221,.60);
  }
  .bregion-icon-theirs {
    background: rgba(77,120,204,.12);
    border-color: rgba(77,120,204,.35);
    color: var(--accent);
  }
  .bregion-icon-theirs:hover {
    background: rgba(77,120,204,.22);
    border-color: rgba(77,120,204,.55);
  }

  /* ── Master "select all" checkbox in column headers ──────────────────── */
  .bcol-header-cb {
    width: 14px; height: 14px;
    margin: 0 4px 0 0;
    cursor: pointer;
    accent-color: var(--accent);
    flex-shrink: 0;
  }

  /* Two-column layout for conflict regions: subgrid from .bregion (which subgrids from .bcol-scroll) */
  .bregion-cols {
    grid-column: 1 / -1;
    display: grid; grid-template-columns: subgrid;
  }

  .bregion-col {
    display: flex; flex-direction: column;
  }

  .bregion-col-ours   { background: rgba(95,173,86,.04); }
  .bregion-col-theirs { background: rgba(226,163,53,.04); }

  .bregion-sep {
    background: var(--border-subtle); width: 4px;
  }

  .bregion-empty {
    padding: 6px 12px 6px 42px;
    font-size: 11px; color: var(--text-disabled);
    font-family: var(--font-ui-sans); font-style: italic;
  }

  /* Per-line rows with checkboxes */
  .bline {
    display: flex; align-items: baseline; gap: 0;
    padding: 1px 0; cursor: pointer;
    border-left: 2px solid transparent;
    transition: background var(--transition-fast), border-left-color var(--transition-fast);
    user-select: none;
    /* Each .bline is its own horizontal scroll container; scrollLeft is
       driven by the shared bottom scrollbar. The checkbox stays visible
       via `position: sticky` on `.bline-cb`. */
    overflow: hidden;
  }

  .bregion-col-ours   .bline:hover { background: rgba(95,173,86,.10); }
  .bregion-col-theirs .bline:hover { background: rgba(226,163,53,.10); }

  .bregion-col-ours .bline-selected {
    background: rgba(95,173,86,.15);
    border-left-color: rgba(95,173,86,.6);
  }
  .bregion-col-theirs .bline-selected {
    background: rgba(226,163,53,.15);
    border-left-color: rgba(226,163,53,.6);
  }

  .bline-cb {
    flex-shrink: 0;
    width: 14px; height: 14px;
    margin: 0 4px;
    cursor: pointer;
    accent-color: var(--accent);
    /* Pin the checkbox so it stays visible while the row scrolls
       horizontally. Per-side direction is set in the .bregion-col-* rules
       below (Ours: left; Theirs: right). The checkbox's own opaque
       `var(--bg-input)` background occludes any code that scrolls behind. */
    position: sticky;
    left: 4px;
    z-index: 1;
  }
  /* Per-column semantic colours. The global checkbox style uses
     `appearance: none`, which disables `accent-color` — re-apply the
     colour via background + border on the `:checked` state instead. */
  .bregion-col-ours   .bline-cb:checked { background: var(--success); border-color: var(--success); }
  .bregion-col-theirs .bline-cb:checked { background: var(--warning); border-color: var(--warning); }

  /* Theirs column: checkbox on the right side. DOM order is
     linenum + brow-code + bline-cb, so the checkbox naturally sits at the
     end of the flex content; sticky `right: 4px` keeps it pinned to the
     row's right edge during horizontal scroll. */
  .bregion-col-theirs .bline-cb {
    left: auto;
    right: 4px;
  }

  /* Result panel */
  .blocking-result {
    /* Default expanded layout: takes 50% of the editor area via
       `flex: 1 1 0`. The `resultSlide` Svelte transition overwrites
       flex-grow + min-height inline during open/close so no CSS
       transition is needed here. */
    flex-grow: 1;
    flex-shrink: 1;
    flex-basis: 0;
    min-height: 80px;
    border-top: 1px solid var(--border-subtle);
    display: flex; flex-direction: column;
    background: var(--bg-elevated);
    position: relative;
    overflow: hidden;
  }
  .blocking-result.is-collapsed .result-resize-handle { display: none; }

  /* Drag-to-resize handle */
  .result-resize-handle {
    position: absolute; top: -4px; left: 0; right: 0; height: 8px;
    cursor: ns-resize; z-index: 10;
    display: flex; align-items: center; justify-content: center;
  }
  .result-resize-handle::after {
    content: '';
    width: 40px; height: 3px;
    border-radius: 2px;
    background: var(--border);
    opacity: 0;
    transition: opacity var(--transition-fast);
  }
  .result-resize-handle:hover::after { opacity: 1; }

  .result-header {
    display: flex; align-items: center; gap: 8px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    cursor: default;
  }
  /* Chevron affordance: collapses / expands the whole result panel.
     Also bound to Ctrl+Shift+S. */
  .result-collapse-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 20px;
    padding: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 50%;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .result-collapse-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }

  .result-header-title {
    font-size: 11px; font-weight: 600; color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }

  .result-header-hint {
    font-size: 10px; color: var(--text-muted); font-family: var(--font-ui-sans);
  }

  .result-manual-badge {
    font-size: 10px;
    background: rgba(77,120,204,.15);
    color: var(--accent);
    border: 1px solid rgba(77,120,204,.3);
    border-radius: 999px;
    padding: 0 6px;
    font-family: var(--font-ui-sans);
  }

  .result-reset-btn {
    margin-left: auto;
    background: none; border: none; cursor: pointer;
    font-size: 10px; color: var(--text-muted); font-family: var(--font-ui-sans);
    padding: 2px 6px; border-radius: var(--radius-sm);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .result-reset-btn:hover { color: var(--text-secondary); background: var(--bg-hover); }

  /* Editor wrap: highlight pre + textarea overlay */
  .result-editor-wrap {
    flex: 1; position: relative; overflow: hidden; min-height: 0;
  }

  .result-highlight {
    position: absolute; inset: 0;
    margin: 0; padding: 8px 12px;
    font-family: var(--font-code); font-size: 12px; line-height: 1.6;
    color: var(--text-primary); background: var(--bg-base);
    white-space: pre; overflow: hidden;
    pointer-events: none; user-select: none;
    border: none;
    /* Match textarea tab size */
    tab-size: 2;
  }

  .result-textarea {
    position: absolute; inset: 0;
    resize: none; outline: none;
    background: transparent; border: none;
    color: transparent; caret-color: var(--text-primary);
    font-family: var(--font-code); font-size: 12px; line-height: 1.6;
    padding: 8px 12px; min-height: 0;
    overflow: auto; z-index: 1;
    tab-size: 2;
  }
  .result-textarea::selection { background: color-mix(in srgb, var(--accent) calc(35% * var(--selection-strength, 1)), transparent); }

  /* ── Merge theirs column (accent/blue) ───────────────────────────────── */
  .bcol-header-merge-theirs { border-top: 2px solid rgba(77,120,204,.5); }
  .bregion-col-merge-theirs { background: rgba(77,120,204,.04); }
  .bregion-col-merge-theirs .bline:hover { background: rgba(77,120,204,.10); }
  .bregion-col-merge-theirs .bline-selected { background: rgba(77,120,204,.15); border-left-color: rgba(77,120,204,.6); }
  .bregion-col-merge-theirs .bline-cb { accent-color: var(--accent); }
  .bregion-col-merge-theirs .bline-cb { left: auto; right: 4px; }

  /* ── Result header extras ─────────────────────────────────────────────── */
  .result-header-spacer { flex: 1; }

  /* ── Footer ──────────────────────────────────────────────────────────── */
  .cr-footer-inner {
    display: flex; align-items: center; gap: 8px; flex: 1;
  }

  .footer-spacer { flex: 1; }

  .footer-info {
    font-size: 11px; color: var(--text-muted); font-family: var(--font-ui-sans);
  }

  .abort-confirm {
    display: flex; align-items: center; gap: 8px; flex: 1;
    font-size: var(--font-size-sm); color: var(--warning); font-family: var(--font-ui-sans);
  }
  .abort-confirm span { color: var(--text-primary); }

  .footer-message-wrap { flex: 1; min-width: 0; }
  .merge-msg-input {
    width: 100%; background: var(--bg-base); border: 1px solid var(--border);
    border-radius: 5px; color: var(--text-primary); font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm); padding: 5px 10px; outline: none;
    transition: border-color var(--transition-fast);
  }
  .merge-msg-input:focus { border-color: var(--accent); }
  .merge-msg-input:disabled { opacity: .5; }

  /* ── Utilities ────────────────────────────────────────────────────────── */
  .truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .spinner {
    width: 14px; height: 14px; border: 2px solid var(--border); border-radius: 50%;
    animation: spin .8s linear infinite;
  }
  .mode-merge .spinner { border-top-color: var(--accent); }
  .mode-stash .spinner { border-top-color: var(--warning); }

  /* ── Conflict navigation bar ─────────────────────────────────────────── */
  .conflict-nav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-base);
    flex-shrink: 0;
    font-family: var(--font-ui-sans);
  }

  .cnav-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .cnav-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .cnav-counter {
    font-size: 11px;
    color: var(--text-muted);
    min-width: 40px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .cnav-spacer { flex: 1; }

  /* File-level "Stage file" action in the nav bar. Primary accent so it
     stands out as the next-step action once conflicts in the file are
     resolved. Replaces the duplicate Take-Ours / Take-Theirs buttons. */
  .cnav-stage {
    display: flex; align-items: center; gap: 5px;
    padding: 3px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--text-on-accent);
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), opacity var(--transition-fast);
  }
  .cnav-stage:hover:not(:disabled) { background: var(--accent-hover); }
  .cnav-stage:disabled { opacity: 0.5; cursor: not-allowed; }

  .cnav-staged {
    display: flex; align-items: center; gap: 5px;
    padding: 3px 10px;
    font-size: 11px;
    font-weight: 500;
    color: var(--success);
    font-family: var(--font-ui-sans);
  }

  /* ── Active conflict block highlight ────────────────────────────────── */
  .bregion-active {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
    border-radius: 2px;
  }
  .bregion-active .bregion-header {
    background: rgba(77,120,204,.06);
  }

  .header-fullfile-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px; height: 22px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .header-fullfile-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .header-fullfile-btn.on {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
