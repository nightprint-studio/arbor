<!--
  ConflictResolutionModal — shell for the merge / stash conflict resolver.

  Owns: orchestration (which file is selected, IPC calls, completion / abort
  flow), and the *shape* of state passed to the sub-components. The actual
  rendering is split across:
    · ConflictFileSidebar       — left file list (flat + tree)
    · ConflictActionBar         — file-level action toolbar (prev/next + stage)
    · ConflictDiffColumns       — 2-column synchronized diff with checkboxes
    · ConflictResultPanel       — bottom preview/edit pane
    · PresenceConflictResolver  — modify/delete & add/modify dedicated UI

  Pure logic lives in `$lib/utils/conflict/*`:
    · conflict-diff             — LCS region builder for stash blocking
    · conflict-marker-parser    — <<<<<<< / ======= / >>>>>>> parser
    · conflict-file-tree        — directory tree from path list
    · conflict-display          — DisplayItem walker + clipping
    · conflict-selection-ops    — immutable mutators for line selection
    · region-types              — shared Region/DisplayItem shapes

  Modes:
    · merge mode  → reads conflict markers from working content
    · stash mode  → either same as merge (post-apply conflicts) OR
                    "blocking mode" (pre-apply files that would be
                    overwritten — uses computeDiff because there are no
                    conflict markers in the workdir yet).
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalSidebarToggle from '$lib/components/shared/ui/ModalSidebarToggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import { encodingOverrides } from '$lib/stores/encodingOverrides.svelte';
  import {
    AlertTriangle, GitMerge, Archive, XCircle,
    ChevronLeft, ChevronRight, TriangleAlert, PackageCheck,
    GitBranch, FileText,
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
  import { highlight } from '$lib/utils/diff-formatter';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { matchesBinding } from '$lib/utils/keybindings';
  import type { StashBlockingContent } from '$lib/types/git';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  import type { Region, DisplayItem } from '$lib/utils/conflict/region-types';
  import { computeDiff } from '$lib/utils/conflict/conflict-diff';
  import { parseConflicts } from '$lib/utils/conflict/conflict-marker-parser';
  import {
    buildDisplayItems, computeSideState, computeRegionsResult, initBlockingSelections,
    isConflictItem, type SelectionMap,
  } from '$lib/utils/conflict/conflict-display';
  import {
    toggleLine, acceptSide, acceptBoth, setAllSide,
    setManualResult, resetManualResult,
  } from '$lib/utils/conflict/conflict-selection-ops';

  import ConflictFileSidebar from './ConflictFileSidebar.svelte';
  import type { FileItem, Status } from './types';
  import ConflictActionBar    from './ConflictActionBar.svelte';
  import ConflictDiffColumns  from './ConflictDiffColumns.svelte';
  import ConflictResultPanel  from './ConflictResultPanel.svelte';
  import PresenceConflictResolver from './PresenceConflictResolver.svelte';

  // ── Props ──────────────────────────────────────────────────────────────
  let { mode }: { mode: 'merge' | 'stash' } = $props();
  const isMerge = $derived(mode === 'merge');
  /** True only when there's a genuine merge in progress (MERGE_HEAD present).
   *  False when the modal opens on an "orphan" conflict state — unmerged
   *  index entries left over from an aborted op. */
  const isRealMerge = $derived(isMerge && (repoStore.status?.is_merging ?? false));

  // ── File data shape ─────────────────────────────────────────────────────
  // One entry per opened path; carries the parsed regions plus the per-line
  // selections + manual override. Used by both modes.
  type FileData = {
    content:        StashBlockingContent | null;
    regions:        Region[];
    oursSelected:   SelectionMap;
    theirsSelected: SelectionMap;
    manualResult:   string | null;
  };

  // ── Base reactive state ──────────────────────────────────────────────────
  const tab    = $derived(tabsStore.activeTab);
  const status = $derived(repoStore.status);

  const initialPaths   = $derived(uiStore.stashConflictFiles);
  const stash          = $derived(uiStore.stashConflictEntry);
  const blockingFiles  = $derived(uiStore.stashBlockingFiles);
  const blockingPop    = $derived(uiStore.stashBlockingPop);
  const isBlockingMode = $derived(!isMerge && blockingFiles.length > 0);

  // Snapshot of every path that was conflicted at any point during this
  // session. Without it, `conflictedFiles` would shrink as resolutions land
  // (git stops flagging resolved files) and the "X/Y resolved" counter
  // would underflow + the "Apply resolution" button disable mid-task.
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

  const conflictedFiles = $derived.by(() => {
    const live = new Map((status?.conflicted ?? []).map(f => [f.path, f]));
    return [...seenConflictedPaths].map(p => live.get(p) ?? {
      path: p,
      old_path: null,
      index_status: 'conflicted' as const,
      workdir_status: 'conflicted' as const,
    });
  });

  // ── Per-file state maps ─────────────────────────────────────────────────
  // Two separate maps (merge / blocking) so the user can switch back and
  // forth without losing in-progress selections.
  let selectedPath         = $state<string | null>(null);
  let blockingSelectedPath = $state<string | null>(null);
  let mergeFileData        = $state<Record<string, FileData>>({});
  let mergeFileLabels      = $state<Record<string, { ours: string; theirs: string }>>({});
  let mergeFileEncoding    = $state<Record<string, string>>({});
  let mergeFilePresence    = $state<Record<string, { ours: boolean; theirs: boolean }>>({});
  let presenceDecisions    = $state<Record<string, 'keep' | 'remove'>>({});
  let blockingFileData     = $state<Record<string, FileData>>({});
  let blockingDecided      = $state<Record<string, 'keep_mine' | 'use_stash' | 'custom'>>({});
  let blockingContent      = $state<Record<string, StashBlockingContent | null>>({});

  let isLoading         = $state(false);
  let isLoadingBlocking = $state(false);
  let resolvedPaths     = $state(new Set<string>());
  let mergeMessage      = $state('');
  let isMerging         = $state(false);
  let isCompleting      = $state(false);
  let isAborting        = $state(false);
  let isStagingFile     = $state(false);
  let isForcing         = $state(false);
  let confirmAbort      = $state(false);

  let activeConflictId  = $state<number | null>(null);
  let sidebarCollapsed  = $state(false);

  // Result panel collapsed state — persisted across modal re-opens.
  let resultCollapsed = $state<boolean>(
    localStorage.getItem('arbor:conflict-result-collapsed') === '1',
  );
  $effect(() => {
    localStorage.setItem('arbor:conflict-result-collapsed', resultCollapsed ? '1' : '0');
  });
  let resultHeight = $state<number | null>(null);

  // User-expanded context blocks (key: `${file}|${ctxIdx}`).
  let expandedContextKeys = $state(new Set<string>());

  // ── Context menus ───────────────────────────────────────────────────────
  type FileCtxMenu = { x: number; y: number; path: string };
  let fileCtxMenu         = $state<FileCtxMenu | null>(null);
  let blockingFileCtxMenu = $state<FileCtxMenu | null>(null);

  function openFileCtxMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    fileCtxMenu = { x: e.clientX, y: e.clientY, path };
  }
  function openBlockingFileCtxMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    blockingFileCtxMenu = { x: e.clientX, y: e.clientY, path };
  }

  // ── Active-file derivations ─────────────────────────────────────────────
  const activeMerge        = $derived(selectedPath ? (mergeFileData[selectedPath] ?? null) : null);
  const activeBlocking     = $derived(blockingSelectedPath ? (blockingFileData[blockingSelectedPath] ?? null) : null);
  const activeMergeLabels  = $derived(
    selectedPath ? (mergeFileLabels[selectedPath] ?? { ours: 'HEAD', theirs: 'THEIRS' }) : { ours: 'HEAD', theirs: 'THEIRS' },
  );
  const activeBlockingDecision = $derived(
    blockingSelectedPath ? blockingDecided[blockingSelectedPath] : undefined,
  );

  const activeEncoding = $derived.by(() => {
    if (isBlockingMode) {
      const p = blockingSelectedPath;
      const bc = p ? blockingContent[p] : null;
      return bc?.encoding ?? '';
    }
    return selectedPath ? (mergeFileEncoding[selectedPath] ?? '') : '';
  });
  const activeEncodingPath = $derived(isBlockingMode ? blockingSelectedPath : selectedPath);
  const activeEncodingOverridden = $derived.by(() => {
    if (!tab || !activeEncodingPath) return false;
    return encodingOverrides.get(tab.path, activeEncodingPath) !== undefined;
  });

  /** True when the active file is a modify/delete or add/modify presence
   *  conflict — drives the dedicated resolver instead of the 2-col diff. */
  const isPresenceConflict = $derived.by(() => {
    if (!selectedPath) return false;
    const p = mergeFilePresence[selectedPath];
    return !!p && (!p.ours || !p.theirs);
  });

  // ── DisplayItem streams ─────────────────────────────────────────────────
  const activeMergeDisplay: DisplayItem[] = $derived.by(() => {
    if (!activeMerge || !selectedPath) return [];
    return buildDisplayItems({
      regions: activeMerge.regions,
      oursSelected: activeMerge.oursSelected,
      theirsSelected: activeMerge.theirsSelected,
      fileKey: `${selectedPath}|merge`,
      fullFile: diffStore.fullFile,
      expandedKeys: expandedContextKeys,
    });
  });

  const activeBlockingDisplay: DisplayItem[] = $derived.by(() => {
    if (!activeBlocking || !blockingSelectedPath) return [];
    return buildDisplayItems({
      regions: activeBlocking.regions,
      oursSelected: activeBlocking.oursSelected,
      theirsSelected: activeBlocking.theirsSelected,
      fileKey: `${blockingSelectedPath}|stash`,
      fullFile: diffStore.fullFile,
      expandedKeys: expandedContextKeys,
    });
  });

  // Master-checkbox states for the column headers.
  const mergeOursState    = $derived(activeMerge
    ? computeSideState(activeMerge.regions, { ours: activeMerge.oursSelected, theirs: activeMerge.theirsSelected }, 'ours')
    : 'none');
  const mergeTheirsState  = $derived(activeMerge
    ? computeSideState(activeMerge.regions, { ours: activeMerge.oursSelected, theirs: activeMerge.theirsSelected }, 'theirs')
    : 'none');
  const blockingOursState   = $derived(activeBlocking
    ? computeSideState(activeBlocking.regions, { ours: activeBlocking.oursSelected, theirs: activeBlocking.theirsSelected }, 'ours')
    : 'none');
  const blockingTheirsState = $derived(activeBlocking
    ? computeSideState(activeBlocking.regions, { ours: activeBlocking.oursSelected, theirs: activeBlocking.theirsSelected }, 'theirs')
    : 'none');

  const activeMergeResult = $derived(
    activeMerge
      ? (activeMerge.manualResult ?? computeRegionsResult(activeMerge.regions, { ours: activeMerge.oursSelected, theirs: activeMerge.theirsSelected }))
      : '',
  );
  const activeBlockingResult = $derived(
    activeBlocking
      ? (activeBlocking.manualResult ?? computeRegionsResult(activeBlocking.regions, { ours: activeBlocking.oursSelected, theirs: activeBlocking.theirsSelected }))
      : '',
  );

  // ── Conflict navigation (prev/next) ────────────────────────────────────
  const activeConflictIds = $derived(
    (isBlockingMode ? activeBlockingDisplay : activeMergeDisplay)
      .filter(isConflictItem)
      .map(d => d.regionId),
  );
  const activeConflictIdx = $derived(
    activeConflictId !== null ? activeConflictIds.indexOf(activeConflictId) : -1,
  );

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

  // ── Blocking decision tracking ──────────────────────────────────────────
  function isBlockingFileViewed(path: string): boolean { return path in blockingFileData; }
  function isBlockingFileDecided(path: string): boolean { return path in blockingDecided; }

  function markBlockingDecision(path: string, decision: 'keep_mine' | 'use_stash' | 'custom') {
    blockingDecided = { ...blockingDecided, [path]: decision };
  }

  /** Confirm a blocking file's resolution. Infers decision flavour from the
   *  current line selection — explicit manual edit > all-ours > all-theirs >
   *  partial = custom. */
  function confirmBlockingFile() {
    const p = blockingSelectedPath; if (!p) return;
    const d = blockingFileData[p]; if (!d) return;

    if (d.manualResult !== null) { markBlockingDecision(p, 'custom'); return; }

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
    const decision = !anyRegion
      ? 'use_stash'
      : allOursOnly
        ? 'keep_mine'
        : allTheirsOnly
          ? 'use_stash'
          : 'custom';
    markBlockingDecision(p, decision);
  }

  function quickResolveBlockingFile(path: string, decision: 'keep_mine' | 'use_stash') {
    markBlockingDecision(path, decision);
  }

  function setPresenceDecision(path: string, decision: 'keep' | 'remove') {
    presenceDecisions = { ...presenceDecisions, [path]: decision };
  }

  // ── Presence prefetch (sidebar badges A/D before user clicks) ──────────
  let presencePrefetchedFor = $state<string | null>(null);
  $effect(() => {
    if (isBlockingMode || !tab || conflictedFiles.length === 0) return;
    if (presencePrefetchedFor === tab.id) return;
    presencePrefetchedFor = tab.id;
    const tabId = tab.id;
    const paths = conflictedFiles.map(f => f.path);

    const fanOutContent = () => {
      for (const p of paths) {
        if (p in mergeFilePresence) continue;
        const override = encodingOverrides.get(tab!.path, p);
        getConflictContent(tabId, p, override)
          .then(c => {
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

  // Auto-select first file on open.
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

  // Merge message fetched once for real merges.
  $effect(() => {
    if (isMerge && tab && !mergeMessage) {
      getMergeMessage(tab.id).then(msg => { mergeMessage = msg; }).catch(() => {});
    }
  });

  // ── File selection (merge mode) ─────────────────────────────────────────
  async function selectConflictFile(path: string) {
    if (!tab || selectedPath === path) return;
    selectedPath = path;
    if (mergeFileData[path]) return;
    await loadConflictFile(path);
  }
  async function loadConflictFile(path: string) {
    if (!tab) return;
    isLoading = true;
    try {
      const override = encodingOverrides.get(tab.path, path);
      const c = await getConflictContent(tab.id, path, override);
      const regs = parseConflicts(c.working_content, 'merge');
      const { os, ts } = initBlockingSelections(regs);
      mergeFileData     = { ...mergeFileData,     [path]: { content: null, regions: regs, oursSelected: os, theirsSelected: ts, manualResult: null } };
      mergeFileLabels   = { ...mergeFileLabels,   [path]: { ours: c.ours_label, theirs: c.theirs_label } };
      mergeFileEncoding = { ...mergeFileEncoding, [path]: c.encoding };
      mergeFilePresence = { ...mergeFilePresence, [path]: { ours: c.ours_present, theirs: c.theirs_present } };
    } catch (err) {
      uiStore.showToast(`Could not load ${path}: ${err}`, 'error');
    } finally {
      isLoading = false;
    }
  }

  // ── File selection (stash blocking) ─────────────────────────────────────
  async function selectBlockingFile(path: string) {
    if (!tab || blockingSelectedPath === path) return;
    blockingSelectedPath = path;
    if (blockingFileData[path]) return;
    await loadBlockingFile(path);
  }
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
      blockingContent = { ...blockingContent, [path]: bc };
    } catch {
      blockingFileData = {
        ...blockingFileData,
        [path]: { content: null, regions: [], oursSelected: {}, theirsSelected: {}, manualResult: null },
      };
      blockingContent = { ...blockingContent, [path]: null };
    } finally {
      isLoadingBlocking = false;
    }
  }

  /** Update the encoding override and re-load with the new decoding. */
  async function changeEncoding(nextEncoding: string | undefined) {
    if (!tab || !activeEncodingPath) return;
    if (nextEncoding === undefined) {
      encodingOverrides.clear(tab.path, activeEncodingPath);
    } else {
      encodingOverrides.set(tab.path, activeEncodingPath, nextEncoding);
    }
    if (isBlockingMode) await loadBlockingFile(activeEncodingPath);
    else                await loadConflictFile(activeEncodingPath);
  }

  // ── Selection ops (uniform wrappers around the .ts helpers) ─────────────
  function toggleMergeLine(side: 'ours' | 'theirs', regionId: number, lineIdx: number) {
    if (!selectedPath) return;
    mergeFileData = toggleLine(mergeFileData, selectedPath, side, regionId, lineIdx);
  }
  function toggleBlockingLine(side: 'ours' | 'theirs', regionId: number, lineIdx: number) {
    if (!blockingSelectedPath) return;
    blockingFileData = toggleLine(blockingFileData, blockingSelectedPath, side, regionId, lineIdx);
  }
  function setAllMergeSide(side: 'ours' | 'theirs', checked: boolean) {
    if (!selectedPath) return;
    mergeFileData = setAllSide(mergeFileData, selectedPath, side, checked);
  }
  function setAllBlockingSide(side: 'ours' | 'theirs', checked: boolean) {
    if (!blockingSelectedPath) return;
    blockingFileData = setAllSide(blockingFileData, blockingSelectedPath, side, checked);
  }

  // ── Whole-file resolution shortcut (right-click "Take ours / theirs") ───
  async function acceptAllForFile(path: string, side: 'ours' | 'theirs') {
    if (!tab) return;
    try {
      if (!mergeFileData[path]) {
        const c = await getConflictContent(tab.id, path);
        const regs = parseConflicts(c.working_content, 'merge');
        const { os, ts } = initBlockingSelections(regs);
        mergeFileData     = { ...mergeFileData,     [path]: { content: null, regions: regs, oursSelected: os, theirsSelected: ts, manualResult: null } };
        mergeFileLabels   = { ...mergeFileLabels,   [path]: { ours: c.ours_label, theirs: c.theirs_label } };
        mergeFileEncoding = { ...mergeFileEncoding, [path]: c.encoding };
        mergeFilePresence = { ...mergeFilePresence, [path]: { ours: c.ours_present, theirs: c.theirs_present } };
      }
      const d = mergeFileData[path];
      // Build the resolved content + update per-line selections so the side-
      // by-side view reflects the choice if the user later opens the file.
      const lines: string[] = [];
      const newOurs:   SelectionMap = {};
      const newTheirs: SelectionMap = {};
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
      if (isMerge) await resolveConflict(tab.id, path, result, encoding);
      else         await resolveStashConflict(tab.id, path, result, encoding);
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

  // ── Staging (merge mode) ────────────────────────────────────────────────
  async function stageMergeFile() {
    if (!tab || !selectedPath || isStagingFile) return;
    isStagingFile = true;
    try {
      // Modify/delete and add/modify cases route to a different flow.
      if (isPresenceConflict) {
        const decision = presenceDecisions[selectedPath] ?? 'keep';
        if (decision === 'remove') {
          await removeConflictFile(tab.id, selectedPath);
        } else {
          // Always re-fetch the raw working_content so we don't lose any bytes
          // the region parser might have eaten (trailing newline / BOM).
          const c = await getConflictContent(tab.id, selectedPath);
          if (isMerge) await resolveConflict(tab.id, selectedPath, c.working_content, c.encoding);
          else         await resolveStashConflict(tab.id, selectedPath, c.working_content, c.encoding);
        }
      } else {
        const data = mergeFileData[selectedPath];
        if (!data) return;
        const result = data.manualResult ?? computeRegionsResult(data.regions, { ours: data.oursSelected, theirs: data.theirsSelected });
        const encoding = mergeFileEncoding[selectedPath];
        if (isMerge) await resolveConflict(tab.id, selectedPath, result, encoding);
        else         await resolveStashConflict(tab.id, selectedPath, result, encoding);
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

  // ── Completion / force-apply / abort ────────────────────────────────────
  async function handleComplete() {
    if (!tab || isCompleting || isMerging) return;
    if (isMerge) {
      isMerging = true;
      try {
        const oid = await completeMerge(tab.id, mergeMessage);
        // Empty OID = orphan-conflict state (no MERGE_HEAD).
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
      // Snapshot tabId/blockingPop BEFORE any await: `tab` / `blockingPop`
      // are derived from uiStore and would read stale after handleClose().
      const localTabId  = tab.id;
      const wasPop      = blockingPop;
      const stashIndex  = stash?.index ?? null;
      isCompleting = true;
      try {
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

  async function handleForceApply() {
    if (!tab || !stash || isForcing) return;
    isForcing = true;
    try {
      const toDelete: string[] = [];
      const toKeep:   string[] = [];
      const toMerge:  Record<string, string> = {};

      for (const f of blockingFiles) {
        const decision = blockingDecided[f];
        if (decision === 'keep_mine') { toKeep.push(f); continue; }
        if (decision === 'use_stash') { toDelete.push(f); continue; }

        const data = blockingFileData[f];
        if (!data) {
          // Not viewed and no explicit decision — default to using the
          // stash. Matches the typical reason the user opened the modal.
          toDelete.push(f);
          continue;
        }
        const result = data.manualResult ?? computeRegionsResult(data.regions, { ours: data.oursSelected, theirs: data.theirsSelected });
        const bc = blockingContent[f];
        const current = bc?.current_content ?? null;
        const stashContent = bc?.stash_content ?? null;
        if (result === current)                                    toKeep.push(f);
        else if (result === stashContent || stashContent === null) toDelete.push(f);
        else                                                       toMerge[f] = result;
      }

      for (const [path, mergedContent] of Object.entries(toMerge)) {
        const enc = blockingContent[path]?.encoding ?? undefined;
        await writeWorkdirFile(tab.id, path, mergedContent, enc);
        toKeep.push(path);
      }

      const applyResult = await forceStashApply(tab.id, stash.index, toDelete, toKeep, blockingPop);
      if (applyResult.blocking_untracked.length > 0) {
        // Re-open the modal with the still-blocking list.
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

        // Pre-checkout stash: go back to the source branch and reapply.
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
      await applyPostCheckout(tab.id);
      handleClose();
    } catch (err) {
      uiStore.showToast(`Abort failed: ${err}`, 'error');
    } finally {
      isAborting = false;
    }
  }

  function handleClose() {
    selectedPath = null;
    mergeFileData = {}; mergeFileLabels = {}; mergeFileEncoding = {}; mergeFilePresence = {};
    presenceDecisions = {}; presencePrefetchedFor = null;
    resolvedPaths = new Set(); seenConflictedPaths = new Set();
    confirmAbort = false;
    blockingSelectedPath = null; blockingFileData = {}; blockingContent = {}; blockingDecided = {};
    resultHeight = null;
    if (isMerge) uiStore.closeMergeModal();
    else         uiStore.closeStashConflictModal();
  }

  function nextUnresolved() {
    const unresolved = conflictedFiles.filter(f => !resolvedPaths.has(f.path));
    if (unresolved.length > 0) selectConflictFile(unresolved[0].path);
  }

  // ── Completion gate ─────────────────────────────────────────────────────
  const allFilesResolved = $derived(
    conflictedFiles.length > 0 && conflictedFiles.every(f => resolvedPaths.has(f.path)),
  );
  const canComplete = $derived(
    allFilesResolved && !isCompleting && !isMerging &&
    (!isRealMerge || mergeMessage.trim().length > 0),
  );

  // ── Sidebar item builders ───────────────────────────────────────────────
  // Map per-mode state onto the uniform `FileItem` shape the sidebar wants.
  const blockingSidebarItems: FileItem[] = $derived(blockingFiles.map(f => {
    const isViewed  = isBlockingFileViewed(f);
    const isDecided = isBlockingFileDecided(f);
    const decision  = blockingDecided[f];
    const status: Status = isDecided ? 'resolved' : isViewed ? 'viewed' : 'conflict';
    return {
      path: f,
      status,
      decisionBadge: decision
        ? { kind: decision, tooltip: decision === 'keep_mine' ? 'Keep current version' : decision === 'use_stash' ? 'Use stash version' : 'Custom merge' }
        : undefined,
    };
  }));

  const mergeSidebarItems: FileItem[] = $derived(conflictedFiles.map(file => {
    const isResolved = resolvedPaths.has(file.path);
    const pres = mergeFilePresence[file.path];
    const sideHint = pres && !pres.ours ? 'added' : pres && !pres.theirs ? 'deleted' : null;
    const status: Status = isResolved
      ? 'resolved'
      : sideHint === 'added'   ? 'added'
      : sideHint === 'deleted' ? 'deleted'
      :                          'conflict';
    const monogram = isResolved ? '✓' : sideHint === 'added' ? 'A' : sideHint === 'deleted' ? 'D' : 'C';
    const monoTip = isResolved
      ? 'Resolved'
      : sideHint === 'added'
        ? 'Added on incoming side — no version on current branch'
        : sideHint === 'deleted'
          ? 'Deleted on incoming side — no version on incoming branch'
          : 'Conflict — both sides modified';
    return {
      path: file.path,
      status,
      monogram,
      monoTip,
      saving: isStagingFile && selectedPath === file.path,
    };
  }));

  // ── Highlighted result HTML ─────────────────────────────────────────────
  const highlightedResult = $derived.by(() => {
    const path = isBlockingMode ? (blockingSelectedPath ?? '') : (selectedPath ?? '');
    const text = isBlockingMode ? activeBlockingResult : activeMergeResult;
    return (text || '').split('\n').map(line => highlight(line, path)).join('\n');
  });

  // ── Shared bottom horizontal scrollbar ──────────────────────────────────
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
      for (const el of cells) el.scrollLeft = bcolScrollX;
    }

    const ro = new ResizeObserver(update);
    ro.observe(containerEl);
    const mo = new MutationObserver(update);
    mo.observe(containerEl, { childList: true, subtree: true });
    update();
    return () => { ro.disconnect(); mo.disconnect(); };
  });

  function expandCollapsed(key: string) {
    expandedContextKeys = new Set([...expandedContextKeys, key]);
  }

  // ── Modal-scoped shortcuts (capture phase) ──────────────────────────────
  // Registered in the capture phase so they run *before* AppShell's window
  // listener — otherwise Ctrl+B / Ctrl+Shift+S would end up toggling the
  // main app's sidebar / bottom panel while we're mid-conflict-resolution.
  function handleBackdropKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      // Hijack Escape so the host Modal doesn't close: instead prompt to
      // confirm aborting — closing mid-merge would silently lose work.
      e.preventDefault();
      e.stopImmediatePropagation();
      if (!confirmAbort) confirmAbort = true;
      return;
    }
    if (matchesBinding(e, keybindingsStore.getBinding('toggle_sidebar'))) {
      e.preventDefault(); e.stopImmediatePropagation();
      sidebarCollapsed = !sidebarCollapsed;
      return;
    }
    if (matchesBinding(e, keybindingsStore.getBinding('stage_view'))) {
      e.preventDefault(); e.stopImmediatePropagation();
      resultCollapsed = !resultCollapsed;
      return;
    }
    if (matchesBinding(e, keybindingsStore.getBinding('next_chunk'))) {
      e.preventDefault(); e.stopImmediatePropagation();
      nextConflict();
      return;
    }
    if (matchesBinding(e, keybindingsStore.getBinding('prev_chunk'))) {
      e.preventDefault(); e.stopImmediatePropagation();
      prevConflict();
      return;
    }
  }
  $effect(() => {
    const onKey = (e: KeyboardEvent) => handleBackdropKeydown(e);
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  });

  // Sidebar slide animation — JS-driven (CSS transitions on flex-basis
  // unreliable across browser versions).
  function sidebarSlide(node: HTMLElement, { duration = 200 }: { duration?: number } = {}) {
    const w = node.getBoundingClientRect().width;
    return {
      duration,
      easing: cubicOut,
      css: (t: number) =>
        `width: ${t * w}px; min-width: 0; margin-right: ${t * 4}px; opacity: ${t}; overflow: hidden; flex: 0 0 auto;`,
    };
  }

  // Label for the action bar 'done' state in blocking mode.
  const blockingDoneLabel = $derived.by(() => {
    if (activeBlockingDecision === 'keep_mine') return 'Resolution: keep mine';
    if (activeBlockingDecision === 'use_stash') return 'Resolution: use stash';
    if (activeBlockingDecision === 'custom')    return 'Resolution: custom merge';
    return '';
  });
</script>

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
        {#if isBlockingMode}      Blocking files — compose merge
        {:else if isRealMerge}    Merge resolution
        {:else if isMerge}        Conflict resolution
        {:else}                   Stash conflict resolution
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
        <EncodingPill encoding={activeEncoding} overridden={activeEncodingOverridden} onChange={changeEncoding} />
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
    <div class="cr-body">

      {#if !sidebarCollapsed}
        <div class="sidebar-wrap" transition:sidebarSlide={{ duration: animStore.dPanel }}>
          {#if isBlockingMode}
            <ConflictFileSidebar
              label="Blocking files"
              items={blockingSidebarItems}
              selectedPath={blockingSelectedPath}
              onSelect={selectBlockingFile}
              onContextMenu={openBlockingFileCtxMenu}
            />
          {:else}
            <ConflictFileSidebar
              label="Conflicting files"
              items={mergeSidebarItems}
              {selectedPath}
              onSelect={selectConflictFile}
              onContextMenu={openFileCtxMenu}
              showNextButton={conflictedFiles.length > 1}
              nextDisabled={allFilesResolved}
              onNext={nextUnresolved}
            />
          {/if}
        </div>
      {/if}

      <div class="editor-area">

        {#if isBlockingMode}
          {#if !blockingSelectedPath}
            <div class="editor-empty">Select a file from the list</div>
          {:else if isLoadingBlocking}
            <div class="editor-empty"><Spinner size={14} /> Loading…</div>
          {:else if !activeBlocking || activeBlocking.regions.length === 0}
            <!-- No diff regions (binary / identical) — still show Confirm. -->
            <div class="blocking-editor blocking-editor-empty">
              <ConflictActionBar
                regionCount={0}
                activeIndex={-1}
                onPrev={prevConflict}
                onNext={nextConflict}
                action={activeBlockingDecision ? 'done' : 'idle'}
                actionLabel="Confirm"
                actionTooltip="Confirm the default resolution (use stash) for this file"
                doneLabel={blockingDoneLabel}
                onAction={confirmBlockingFile}
                navTooltipNoun="diff"
              />
              <div class="editor-empty">
                <AlertTriangle size={16} class="icon-conflict" />
                No differences detected or binary file
              </div>
            </div>
          {:else}
            <div class="blocking-editor">
              <ConflictActionBar
                regionCount={activeConflictIds.length}
                activeIndex={activeConflictIdx}
                onPrev={prevConflict}
                onNext={nextConflict}
                action={activeBlockingDecision ? 'done' : 'idle'}
                actionLabel="Confirm"
                actionTooltip="Confirm the current resolution for this file"
                doneLabel={blockingDoneLabel}
                onAction={confirmBlockingFile}
                navTooltipNoun="diff"
              />

              <ConflictDiffColumns
                theme="stash"
                oursLabel="Current"
                theirsLabel="Stash"
                oursSub="current file in the working tree"
                theirsSub="version in the stash"
                takeOursLabel="Current"
                takeTheirsLabel="Stash"
                oursEmptyLabel="— not present —"
                theirsEmptyLabel="— removed in stash —"
                items={activeBlockingDisplay}
                activeId={activeConflictId}
                oursState={blockingOursState}
                theirsState={blockingTheirsState}
                path={blockingSelectedPath ?? ''}
                onToggleLine={toggleBlockingLine}
                onAcceptOurs={(id) => blockingSelectedPath && (blockingFileData = acceptSide(blockingFileData, blockingSelectedPath, id, 'ours'))}
                onAcceptTheirs={(id) => blockingSelectedPath && (blockingFileData = acceptSide(blockingFileData, blockingSelectedPath, id, 'theirs'))}
                onAcceptBoth={(id) => blockingSelectedPath && (blockingFileData = acceptBoth(blockingFileData, blockingSelectedPath, id))}
                onSetAllSide={setAllBlockingSide}
                onActivate={(id) => activeConflictId = id}
                onExpandCollapsed={expandCollapsed}
                bind:scrollEl={bcolScrollEl}
                bind:hscrollInnerEl={bcolHscrollInnerEl}
                onHscroll={onBcolHscroll}
              />

              <ConflictResultPanel
                highlightedHtml={highlightedResult}
                value={activeBlockingResult}
                isManual={activeBlocking?.manualResult !== null}
                collapsed={resultCollapsed}
                height={resultHeight}
                onCollapseToggle={() => resultCollapsed = !resultCollapsed}
                onHeightChange={(h) => resultHeight = h}
                onInput={(v) => {
                  if (!blockingSelectedPath) return;
                  blockingFileData = setManualResult(blockingFileData, blockingSelectedPath, v);
                  markBlockingDecision(blockingSelectedPath, 'custom');
                }}
                onReset={() => {
                  if (!blockingSelectedPath) return;
                  blockingFileData = resetManualResult(blockingFileData, blockingSelectedPath);
                  if (blockingDecided[blockingSelectedPath] === 'custom') {
                    const next = { ...blockingDecided };
                    delete next[blockingSelectedPath];
                    blockingDecided = next;
                  }
                }}
              />
            </div>
          {/if}

        {:else}
          {#if !selectedPath}
            <div class="editor-empty">Select a file from the list</div>
          {:else if isLoading}
            <div class="editor-empty"><Spinner size={14} /> Loading…</div>
          {:else if !activeMerge || activeMerge.regions.length === 0}
            <div class="editor-empty">
              <AlertTriangle size={16} class="icon-conflict" />
              No conflicts found
            </div>
          {:else}
            <div class="blocking-editor">
              <ConflictActionBar
                regionCount={activeConflictIds.length}
                activeIndex={activeConflictIdx}
                onPrev={prevConflict}
                onNext={nextConflict}
                action={resolvedPaths.has(selectedPath) ? 'done' : (isStagingFile ? 'busy' : 'idle')}
                actionLabel={isStagingFile
                  ? 'Staging…'
                  : isPresenceConflict
                    ? (presenceDecisions[selectedPath] === 'remove' ? 'Remove file' : 'Keep file')
                    : 'Stage file'}
                actionTooltip={isPresenceConflict
                  ? 'Apply the chosen Keep / Remove decision'
                  : 'Apply the current resolution and stage the file'}
                doneLabel="File staged"
                onAction={stageMergeFile}
                navTooltipNoun="conflict"
              />

              {#if isPresenceConflict && selectedPath}
                {@const pres = mergeFilePresence[selectedPath]}
                {@const labels = mergeFileLabels[selectedPath] ?? { ours: 'HEAD', theirs: 'MERGE_HEAD' }}
                {@const decision = presenceDecisions[selectedPath] ?? 'keep'}
                {@const previewLines = mergeFileData[selectedPath]
                  ? mergeFileData[selectedPath].regions.flatMap(r => r.kind === 'context' ? r.lines : [])
                  : []}
                <PresenceConflictResolver
                  presence={pres}
                  {labels}
                  {decision}
                  {previewLines}
                  path={selectedPath}
                  onPick={(d) => setPresenceDecision(selectedPath!, d)}
                />
              {:else}
                <ConflictDiffColumns
                  theme="merge"
                  oursLabel={activeMergeLabels.ours}
                  theirsLabel={activeMergeLabels.theirs}
                  oursSub="ours"
                  theirsSub="theirs"
                  takeOursLabel="Ours"
                  takeTheirsLabel="Theirs"
                  oursEmptyLabel="— empty —"
                  theirsEmptyLabel="— not present —"
                  items={activeMergeDisplay}
                  activeId={activeConflictId}
                  oursState={mergeOursState}
                  theirsState={mergeTheirsState}
                  path={selectedPath ?? ''}
                  onToggleLine={toggleMergeLine}
                  onAcceptOurs={(id) => selectedPath && (mergeFileData = acceptSide(mergeFileData, selectedPath, id, 'ours'))}
                  onAcceptTheirs={(id) => selectedPath && (mergeFileData = acceptSide(mergeFileData, selectedPath, id, 'theirs'))}
                  onAcceptBoth={(id) => selectedPath && (mergeFileData = acceptBoth(mergeFileData, selectedPath, id))}
                  onSetAllSide={setAllMergeSide}
                  onActivate={(id) => activeConflictId = id}
                  onExpandCollapsed={expandCollapsed}
                  bind:scrollEl={bcolScrollEl}
                  bind:hscrollInnerEl={bcolHscrollInnerEl}
                  onHscroll={onBcolHscroll}
                />

                <ConflictResultPanel
                  highlightedHtml={highlightedResult}
                  value={activeMergeResult}
                  isManual={activeMerge?.manualResult !== null}
                  collapsed={resultCollapsed}
                  height={resultHeight}
                  onCollapseToggle={() => resultCollapsed = !resultCollapsed}
                  onHeightChange={(h) => resultHeight = h}
                  onInput={(v) => selectedPath && (mergeFileData = setManualResult(mergeFileData, selectedPath, v))}
                  onReset={() => selectedPath && (mergeFileData = resetManualResult(mergeFileData, selectedPath))}
                />
              {/if}
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
            {#if isRealMerge}        Abort the merge? All changes will be lost.
            {:else if isMerge}       Discard the resolution? Files will revert to the HEAD version.
            {:else}                  Abort the stash? The working tree will be restored to HEAD.
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

        <!-- Orphan-conflict + all resolved → nothing left to abort. -->
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
          {#if isRealMerge}   Abort Merge
          {:else if isMerge}  Discard resolution
          {:else}             Abort Stash
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
          {#if isRealMerge}    {isMerging ? 'Merging…' : 'Merge →'}
          {:else if isMerge}   {isMerging ? 'Applying…' : 'Apply resolution →'}
          {:else}              {isCompleting ? 'Updating…' : 'Complete →'}
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

  /* ── Header chrome ────────────────────────────────────────────────────── */
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
  /* Orphan-conflict state uses the warning triangle — give it a matching
     amber halo so it reads as a state indicator rather than a hazard sign. */
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

  /* ── Body shell ───────────────────────────────────────────────────────── */
  .cr-body {
    display: flex;
    flex: 1;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
  }

  /* Wraps the sidebar so the slide transition can drive its width without
     fighting the child's `flex: 0 0 220px`. */
  .sidebar-wrap {
    display: flex;
    flex: 0 0 auto;
  }

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

  .blocking-editor {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
  }

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
</style>
