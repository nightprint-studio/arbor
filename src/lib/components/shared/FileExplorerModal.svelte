<script lang="ts">
  /**
   * FileExplorerModal — built-in file explorer (real filesystem).
   *
   * Multi-tab browse over the real FS (via the `fs_*` IPC), with a
   * Spacedrive-style Overview dashboard, multi-selection, copy/cut/paste
   * (internal clipboard), delete (Recycle Bin / Shift = permanent), a live
   * `notify` watcher, an editable address bar with ghost-text autocomplete,
   * and a collapsible sidebar with collapsible sections + workspace groups.
   *
   * NOTE: this does NOT replace the production file/folder pickers yet.
   *
   * Performance: the dir is read once (incl. hidden) and filtered on the FE;
   * the list is virtualised; the watcher is debounced; sort/filter memoised.
   */
  import { tick, untrack, onMount, onDestroy } from 'svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { highlightCode } from '$lib/utils/highlight';
  import {
    ArrowLeft, ArrowRight, ArrowUp, RefreshCw, Search, X, Eye, Info,
    Folder, FolderOpen, FileText, ChevronRight, ChevronDown, Home, LayoutDashboard,
    Download, HardDrive, Monitor, Star, History, GitBranch, Box,
    FolderPlus, FilePlus, Pencil, Trash2, Scissors, Copy, ClipboardPaste,
    ExternalLink, Link2, AlertCircle, Maximize2, Minimize2, Settings2,
    Archive, PackageOpen, Image as ImageIcon, FolderSearch,
    List, LayoutGrid, Grid2x2, Grid3x3,
    Plus, Minus, Undo2, EyeOff, GitCompare, CheckCircle2,
  } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import { getFileIcon, getFolderIcon } from '$lib/utils/file-icons';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import {
    fsReadDir, listFsRoots, fsRename, fsCreateDir, fsCreateFile,
    fsCopy, fsMove, fsTrash, fsDeleteMany, fsOpenDefault, fsRevealInDir, fsShowProperties, fsIcon,
    fsZip, fsUnzip, fsSetWallpaper, fsSearch,
    fsReadTextFile, fsWatchStart, fsWatchStop,
    fsGitStatus, fsGitStage, fsGitUnstage, fsGitDiscard, fsGitIgnore, fsOpenInArbor, fsGitChanges,
    fsGitBranches, fsGitCheckout,
    type FsEntry, type FsRoot, type FsGitStatus, type GitBadge, type GitRepoMarker, type GitChange, type GitChanges, type FsBranch,
  } from '$lib/ipc/fs';
  import { listRegistryRepos, listWorkspaces } from '$lib/ipc/workspace';
  import { workspaceColorVar, type RepoRegistryEntry, type WorkspaceDef } from '$lib/types/workspace';
  import { explorerStore, mergeSidebarSections, EXPLORER_SECTIONS } from '$lib/stores/explorer.svelte';
  import Modal from './Modal.svelte';
  import FileExplorerSettings from './FileExplorerSettings.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import BranchSwitchPopup from './BranchSwitchPopup.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';
  // Standalone-window chrome (only used when `standalone` — the dedicated
  // File Explorer window opened via the global Ctrl+Shift+E shortcut).
  import WindowControls from '$lib/components/layout/WindowControls.svelte';
  import ActivityBar from '$lib/components/layout/ActivityBar.svelte';
  import ModalSidebarToggle from './ui/ModalSidebarToggle.svelte';
  import Button from './ui/Button.svelte';
  import Card from './ui/Card.svelte';
  import Spinner from './ui/Spinner.svelte';
  import Dropdown, { type DropdownItem } from './ui/Dropdown.svelte';
  import Tabs, { type TabItem } from './ui/Tabs.svelte';
  import ContextMenu, { type MenuItem, type MenuAction } from './ContextMenu.svelte';
  import { overviewStats, kindBreakdown, totalFiles } from '$lib/mocks/fileExplorerMock';

  type View = 'overview' | 'browse' | 'settings';
  interface ExpTab { id: string; view: View; path: string; history: string[]; historyIdx: number; }

  // `onClose` drives the modal usage (Esc / backdrop / header close button).
  // In `standalone` mode the explorer fills a dedicated OS window and is
  // dismissed via its own WindowControls, so `onClose` defaults to a no-op.
  let { onClose = () => {}, standalone = false }: { onClose?: () => void; standalone?: boolean } = $props();

  // ── localStorage helpers ──────────────────────────────────────────────────
  function lsGet<T>(key: string, fallback: T): T {
    if (typeof localStorage === 'undefined') return fallback;
    try { const v = localStorage.getItem(key); return v == null ? fallback : JSON.parse(v); } catch { return fallback; }
  }
  function lsSet(key: string, v: unknown) { try { localStorage?.setItem(key, JSON.stringify(v)); } catch { /* quota */ } }

  // ── Icon source — single swap-point for future system icons ───────────────
  function entryIcon(name: string, isDir: boolean, open = false) {
    return isDir ? getFolderIcon(name, open) : getFileIcon(name);
  }

  // ── Tabs (explorer locations) ──────────────────────────────────────────────
  let tabSeq = 0;
  const mkTab = (view: View, path: string): ExpTab =>
    ({ id: `t${tabSeq++}`, view, path, history: path ? [path] : [], historyIdx: path ? 0 : -1 });
  const initialTab = mkTab('overview', '');
  let tabs        = $state<ExpTab[]>([initialTab]);
  let activeTabId = $state(initialTab.id);
  const activeTab = $derived(tabs.find(t => t.id === activeTabId) ?? tabs[0]);

  // ── State ────────────────────────────────────────────────────────────────
  let rawEntries  = $state<FsEntry[]>([]);
  let loading     = $state(false);
  let loadError   = $state('');

  let selected = $state<Set<string>>(new Set());
  let anchor   = $state('');
  let lead     = $state('');
  let clipboard = $state<{ op: 'copy' | 'cut'; paths: string[] } | null>(null);
  let rightPanel = $state<'preview' | 'info' | 'changes' | null>(null);
  // Right panel width (resizable, persisted) + expand toggle (ephemeral).
  let previewWidth = $state<number>(lsGet('arbor:explorer-preview-width', 300));
  $effect(() => lsSet('arbor:explorer-preview-width', previewWidth));
  let previewExpanded = $state(false);
  function startPreviewResize(e: MouseEvent) {
    e.preventDefault();
    const startX = e.clientX, startW = previewWidth;
    const onMove = (ev: MouseEvent) => { previewWidth = Math.max(240, Math.min(760, startW + (startX - ev.clientX))); };
    const onUp = () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); document.body.style.cursor = ''; document.body.style.userSelect = ''; };
    window.addEventListener('mousemove', onMove); window.addEventListener('mouseup', onUp);
    document.body.style.cursor = 'col-resize'; document.body.style.userSelect = 'none';
  }
  const previewBig = $derived(rightPanel !== null && view === 'browse' && previewExpanded);

  let filterQuery = $state('');
  // Show-hidden + recursive-search are global toggles persisted in ExplorerConfig
  // (FS, via explorerStore) — not path-based, they apply across all folders.
  // Read through the store; flip via its setters (see the toggle handlers).
  const showHidden      = $derived(explorerStore.showHidden);
  // Recursive search: when on (with a non-empty query) the list shows matches
  // from all subfolders (backend walk) instead of filtering the current dir.
  const recursiveSearch = $derived(explorerStore.recursiveSearch);
  let searchResults = $state<FsEntry[]>([]);
  let searching     = $state(false);

  // ── Git awareness (TortoiseGit-style overlays + inline actions) ────────────
  // Status for the current directory's entries (badges + branch / ahead-behind),
  // fetched per navigation and refreshed off the fs watcher. `null` outside a repo.
  let gitStatus = $state<FsGitStatus | null>(null);
  let gitSeq = 0;
  // Glyph + colour + tooltip label per badge. Rendered as a tiny corner overlay
  // on the entry icon (details + grid). 'ignored' dims the row instead.
  const GIT_BADGE_GLYPH: Record<GitBadge, string> = {
    modified: '●', added: '+', untracked: '?', deleted: '−', renamed: '»', conflicted: '!', ignored: '',
  };
  const GIT_BADGE_LABEL: Record<GitBadge, string> = {
    modified: 'Modified', added: 'Staged', untracked: 'Untracked', deleted: 'Deleted',
    renamed: 'Renamed', conflicted: 'Conflicted', ignored: 'Ignored',
  };

  type SortKey = 'name' | 'modified' | 'size';
  let sortKey = $state<SortKey>('name');
  let sortAsc = $state(true);

  // View mode: details list vs medium / large / extra-large icon grids.
  // Persisted per-folder (LRU map) layered over a global default — like Windows
  // Explorer's per-folder memory, minus folder-type templates. Changing a
  // folder's view also updates the default applied to not-yet-visited folders.
  type ViewMode = 'details' | 'medium' | 'large' | 'xlarge';
  const VIEW_CAP = 200;
  // Global default lives in ExplorerConfig (FS); the per-folder LRU memory below
  // is ephemeral UI state and stays in localStorage.
  let viewByPath  = $state<Map<string, ViewMode>>(new Map(lsGet<[string, ViewMode][]>('arbor:explorer-view-modes', [])));
  const viewKey  = $derived(currentPath ? normParentKey(currentPath) : '');
  const viewMode = $derived(viewByPath.get(viewKey) ?? explorerStore.defaultView);
  function setViewMode(m: ViewMode) {
    explorerStore.setDefaultView(m);
    const key = viewKey;
    if (key) {
      const map = new Map(viewByPath);
      map.delete(key);                  // re-insert at end → most-recently-used
      map.set(key, m);
      while (map.size > VIEW_CAP) {      // evict oldest until under the cap
        const oldest = map.keys().next().value;
        if (oldest === undefined) break;
        map.delete(oldest);
      }
      viewByPath = map;
    }
  }
  $effect(() => lsSet('arbor:explorer-view-modes', [...viewByPath]));
  // Single titlebar dropdown (Windows-style) listing the four view modes.
  const VIEW_OPTIONS = [
    { mode: 'details', icon: List,       label: 'Details' },
    { mode: 'medium',  icon: Grid3x3,    label: 'Medium icons' },
    { mode: 'large',   icon: Grid2x2,    label: 'Large icons' },
    { mode: 'xlarge',  icon: LayoutGrid, label: 'Extra large icons' },
  ] as const;
  const currentView = $derived(VIEW_OPTIONS.find(o => o.mode === viewMode) ?? VIEW_OPTIONS[0]);
  const viewItems = $derived<DropdownItem[]>(VIEW_OPTIONS.map(o => ({
    kind: 'item' as const, id: o.mode, label: o.label, icon: o.icon,
    active: viewMode === o.mode, onclick: () => setViewMode(o.mode),
  })));

  let roots       = $state<FsRoot[]>([]);
  let projects    = $state<RepoRegistryEntry[]>([]);
  let workspaces  = $state<WorkspaceDef[]>([]);
  let activeWorkspaceId = $state<string | null>(null);

  let renamingPath = $state('');
  let renameValue  = $state('');
  let createKind   = $state<'folder' | 'file' | null>(null);
  let createName   = $state('');

  let ctxMenu   = $state<{ x: number; y: number; entry: FsEntry | null } | null>(null);
  let deleteReq = $state<{ paths: string[]; permanent: boolean } | null>(null);
  // Discard is destructive (overwrites working-tree edits / deletes untracked
  // files) → confirm first. Backed by a Recovery snapshot, but the modal makes
  // the intent explicit. Holds the selection paths while the dialog is open.
  let discardReq = $state<string[] | null>(null);
  let discardBusy = $state(false);

  let listEl = $state<HTMLElement | null>(null);

  // ── Sidebar collapse + per-section collapse (persisted) ───────────────────
  let sidebarCollapsed = $state<boolean>(lsGet('arbor:explorer-sidebar-collapsed', false));
  $effect(() => lsSet('arbor:explorer-sidebar-collapsed', sidebarCollapsed));
  let collapsedSections = $state<Set<string>>(new Set(lsGet<string[]>('arbor:explorer-collapsed-sections', [])));
  function toggleSection(id: string) {
    const next = new Set(collapsedSections);
    if (next.has(id)) next.delete(id); else next.add(id);
    collapsedSections = next;
    lsSet('arbor:explorer-collapsed-sections', [...next]);
  }
  const sectionOpen = (id: string) => sidebarCollapsed || !collapsedSections.has(id);

  // Workspace groups expanded state (persisted; default expanded on first run).
  let wsExpanded = $state<Set<string>>(new Set(lsGet<string[]>('arbor:explorer-ws-expanded', [])));
  function toggleWs(id: string) {
    const next = new Set(wsExpanded);
    if (next.has(id)) next.delete(id); else next.add(id);
    wsExpanded = next;
    lsSet('arbor:explorer-ws-expanded', [...next]);
  }
  const isWsExpanded = (id: string) => wsExpanded.has(id);

  // ── Recents ────────────────────────────────────────────────────────────────
  const RECENTS_KEY = 'arbor:explorer-recents';
  let recents = $state<string[]>(lsGet<string[]>(RECENTS_KEY, []).filter(x => typeof x === 'string'));
  function addRecent(path: string) {
    if (!path) return;
    recents = [path, ...recents.filter(p => p !== path)].slice(0, explorerStore.maxRecents);
    lsSet(RECENTS_KEY, recents);
  }
  // Trim live when the cap is lowered from settings.
  $effect(() => {
    const cap = explorerStore.maxRecents;
    if (recents.length > cap) { recents = recents.slice(0, cap); lsSet(RECENTS_KEY, recents); }
  });

  // ── Active-tab projections ────────────────────────────────────────────────
  const view        = $derived(activeTab.view);
  const currentPath = $derived(activeTab.path);
  const favourites  = $derived(roots.filter(r => r.kind !== 'drive'));
  const drives      = $derived(roots.filter(r => r.kind === 'drive'));
  const activeRepoPath = $derived(tabsStore.activeTab?.path ?? null);
  const defaultBrowsePath = $derived(favourites[0]?.path ?? drives[0]?.path ?? '');

  function normPath(p: string): string { return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase(); }

  // ── Git status fetch + lookup ──────────────────────────────────────────────
  // Master switch (ExplorerConfig.git_awareness): when off, the explorer issues
  // no git IPC at all — no status walk, no overlays, no Changes/branch actions.
  const gitOn = $derived(explorerStore.gitAwareness);
  const inRepo = $derived(!!gitStatus?.in_repo);
  /** Overlay badge for `entry`, or null when clean / outside a repo. Keyed by
   *  `normPath` to match the backend, which mirrors this normalization. */
  function badgeFor(entry: FsEntry): GitBadge | null {
    return gitStatus?.badges[normPath(entry.path)] ?? null;
  }
  /** Load git status for `path` (overlays + branch). `refresh` busts the backend
   *  per-repo cache — passed when reacting to the fs watcher. Guarded by `gitSeq`
   *  so a slow response from a previous folder can't clobber the current one. */
  async function loadGitStatus(path: string, refresh = false) {
    const seq = ++gitSeq;
    try {
      const s = await fsGitStatus(path, refresh);
      if (seq === gitSeq) gitStatus = s;
    } catch {
      if (seq === gitSeq) gitStatus = null;
    }
  }

  // ── Repo-root + Arbor-registration markers (folder-of-projects awareness) ──
  /** Registry entries keyed by normalized path → which folders are repos Arbor
   *  knows about. */
  const projectByPath = $derived(new Map(projects.map(p => [normPath(p.path), p] as const)));
  /** Combined "this folder is a project" info for a row: its git branch (when a
   *  detected repo root) and Arbor registration (entry + the workspaces holding
   *  it). `null` when the folder is neither a repo root nor registered. */
  interface RepoFolderInfo {
    marker:     GitRepoMarker | null;
    registered: boolean;
    entry:      RepoRegistryEntry | null;
    workspaces: WorkspaceDef[];
  }
  function repoInfoFor(entry: FsEntry): RepoFolderInfo | null {
    if (!entry.is_dir) return null;
    const key = normPath(entry.path);
    const marker = gitStatus?.repos?.[key] ?? null;
    const reg = projectByPath.get(key) ?? null;
    if (!marker && !reg) return null;
    const wss = reg ? workspaces.filter(w => w.repo_ids.includes(reg.id)) : [];
    return { marker, registered: !!reg, entry: reg, workspaces: wss };
  }
  /** Tooltip / Info-panel summary line for a repo-root folder. */
  function repoTip(info: RepoFolderInfo): string {
    const parts: string[] = [info.registered ? 'Git project · in Arbor' : 'Git repository (not in Arbor)'];
    if (info.marker?.branch) parts.push(info.marker.detached ? 'detached HEAD' : info.marker.branch);
    if (info.registered && info.workspaces.length) parts.push('Workspaces: ' + info.workspaces.map(w => w.name).join(', '));
    return parts.join(' · ');
  }

  // ── Changes panel (staged / unstaged file list) ───────────────────────────
  // Full working-tree change list for the current repo, loaded on demand when
  // the Changes panel is open and refreshed off the fs watcher.
  let gitChanges = $state<GitChanges | null>(null);
  let changesSeq = 0;
  async function loadGitChanges(path: string) {
    const seq = ++changesSeq;
    try {
      const c = await fsGitChanges(path);
      if (seq === changesSeq) gitChanges = c;
    } catch {
      if (seq === changesSeq) gitChanges = null;
    }
  }
  // Load whenever the Changes panel is showing for a browsed folder; re-runs on
  // navigation (currentPath dep). Watcher-driven refreshes go through
  // refreshCurrent(), which reloads explicitly (same path → no dep change here).
  $effect(() => {
    if (gitOn && rightPanel === 'changes' && view === 'browse' && currentPath) void loadGitChanges(currentPath);
  });
  // Stepping out of a repo hides both the rail button and the footer chip that
  // toggle this panel, so close it to avoid a stuck, un-dismissable state.
  $effect(() => {
    if (rightPanel === 'changes' && gitStatus && !gitStatus.in_repo) rightPanel = null;
  });
  /** Total changed entries (staged + unstaged) — a file can count in both. */
  const changeCount = $derived((gitChanges?.staged.length ?? 0) + (gitChanges?.unstaged.length ?? 0));
  /** Split a repo-relative path into [dir-with-trailing-slash, basename]. */
  function splitRel(rel: string): [string, string] {
    const i = rel.lastIndexOf('/');
    return i >= 0 ? [rel.slice(0, i + 1), rel.slice(i + 1)] : ['', rel];
  }
  /** Reveal a changed file in the list: navigate to its parent folder (if not
   *  already there), then select + scroll it into view. Deleted files just
   *  navigate to the folder (the row no longer exists). */
  async function revealPath(absPath: string) {
    const parent = absPath.replace(/[\\/]+$/, '').replace(/[\\/][^\\/]*$/, '');
    if (parent && normPath(parent) !== normPath(currentPath)) await navigate(parent);
    await tick();
    const match = sorted.find(e => normPath(e.path) === normPath(absPath));
    if (match) {
      selected = new Set([match.path]); lead = match.path; anchor = match.path;
      scrollToIndex(sorted.indexOf(match));
    }
  }

  // ── Branch switch (checkout) ───────────────────────────────────────────────
  // Filterable, keyboard-first branch picker anchored at a screen point, opened
  // from the footer branch chip or the repo context menu. `repoPath` is any path
  // inside (or at the root of) the target repo — the backend discovers it.
  let branchPopup = $state<{ x: number; y: number; repoPath: string; branches: FsBranch[] } | null>(null);
  let branchBusy = $state(false);
  async function openBranchSwitch(repoPath: string, x: number, y: number) {
    try {
      const branches = await fsGitBranches(repoPath);
      branchPopup = { x, y, repoPath, branches };
    } catch (e) {
      uiStore.showToast(`Branches: ${e}`, 'error');
    }
  }
  async function doCheckout(name: string) {
    if (!branchPopup) return;
    branchBusy = true;
    try {
      await fsGitCheckout(branchPopup.repoPath, name);
      branchPopup = null;
      await refreshCurrent();
      uiStore.showToast(`Switched to ${name}`, 'success');
    } catch (e) {
      uiStore.showToast(`Switch branch: ${e}`, 'error');
    } finally {
      branchBusy = false;
    }
  }

  // ── Workspace grouping for the Projects section (ported pattern) ──────────
  const activeProject = $derived.by<RepoRegistryEntry | null>(() => {
    if (!activeRepoPath || !projects.length) return null;
    const target = normPath(activeRepoPath);
    return projects.find(p => normPath(p.path) === target) ?? null;
  });
  type WsGroup = { id: string; name: string; repos: RepoRegistryEntry[]; synthetic: boolean; isActive: boolean; };
  const wsGroups = $derived.by<WsGroup[]>(() => {
    if (!projects.length) return [];
    const byId = new Map(projects.map(p => [p.id, p]));
    const groups: WsGroup[] = [];
    const seen = new Set<string>();
    for (const ws of [...workspaces].sort((a, b) => a.order - b.order)) {
      const repos = ws.repo_ids.map(id => byId.get(id)).filter((r): r is RepoRegistryEntry => !!r)
        .sort((a, b) => a.display_name.localeCompare(b.display_name, undefined, { sensitivity: 'base' }));
      if (!repos.length) continue;
      for (const r of repos) seen.add(r.id);
      groups.push({ id: ws.id, name: ws.name, repos, synthetic: false, isActive: ws.id === activeWorkspaceId });
    }
    const unassigned = projects.filter(p => !seen.has(p.id))
      .sort((a, b) => a.display_name.localeCompare(b.display_name, undefined, { sensitivity: 'base' }));
    if (unassigned.length) groups.push({ id: '__unassigned__', name: 'Unassigned', repos: unassigned, synthetic: true, isActive: false });
    return groups;
  });

  // ── Explorer tab strip ────────────────────────────────────────────────────
  const tabItems = $derived<TabItem[]>(tabs.map(t => ({
    id: t.id,
    label: t.view === 'overview' ? 'Overview' : t.view === 'settings' ? 'Settings' : (t.path.split(/[\\/]/).filter(Boolean).pop() || t.path || 'Browse'),
    icon: t.view === 'overview' ? LayoutDashboard : t.view === 'settings' ? Settings2 : Folder,
    closable: tabs.length > 1,
  })));
  function syncActive() {
    cancelCreate(); cancelRename(); ctxMenu = null; filterQuery = ''; addressEditing = false; clearSelection();
    if (activeTab.view === 'browse') navigate(activeTab.path, false);
    else { rawEntries = []; loadError = ''; gitStatus = null; fsWatchStop().catch(() => {}); }
  }
  function switchTab(id: string) { activeTabId = id; syncActive(); }
  function addTab() { const t = mkTab('overview', ''); tabs = [...tabs, t]; activeTabId = t.id; syncActive(); }
  function closeTab(id: string) {
    if (tabs.length <= 1) return;
    const idx = tabs.findIndex(t => t.id === id);
    tabs = tabs.filter(t => t.id !== id);
    if (activeTabId === id) { activeTabId = tabs[Math.min(idx, tabs.length - 1)].id; syncActive(); }
  }

  // ── Navigation ─────────────────────────────────────────────────────────
  let navSeq = 0;
  async function navigate(path: string, pushHist = true) {
    if (!path) return;
    cancelCreate(); cancelRename(); ctxMenu = null; deleteReq = null; filterQuery = ''; addressEditing = false;
    const t = activeTab;
    t.view = 'browse'; t.path = path;
    if (pushHist) { t.history = [...t.history.slice(0, t.historyIdx + 1), path]; t.historyIdx = t.history.length - 1; }
    clearSelection();
    const seq = ++navSeq;
    loading = true; loadError = '';
    resetScroll();
    try {
      const raw = await fsReadDir(path, true);
      if (seq !== navSeq) return;
      rawEntries = raw;
    } catch (e) {
      if (seq !== navSeq) return;
      loadError = String(e).split('\n')[0].replace(/^.*error:/i, '').trim();
      rawEntries = [];
    } finally {
      if (seq === navSeq) loading = false;
    }
    addRecent(path);
    fsWatchStart(path).catch(() => {});
    if (gitOn) void loadGitStatus(path);
    else gitStatus = null;
  }
  async function refreshCurrent() {
    if (activeTab.view !== 'browse' || !activeTab.path) return;
    const seq = ++navSeq;
    try {
      const raw = await fsReadDir(activeTab.path, true);
      if (seq !== navSeq) return;
      rawEntries = raw;
      const exist = new Set(raw.map(e => e.path));
      selected = new Set([...selected].filter(p => exist.has(p)));
      if (lead && !exist.has(lead)) lead = '';
      if (gitOn) {
        void loadGitStatus(activeTab.path, true);
        if (rightPanel === 'changes') void loadGitChanges(activeTab.path);
      }
    } catch { /* keep stale view */ }
  }
  function showOverview() {
    cancelCreate(); cancelRename(); ctxMenu = null; deleteReq = null; addressEditing = false; clearSelection();
    activeTab.view = 'overview';
    gitStatus = null;
    fsWatchStop().catch(() => {});
  }
  // Settings view (Windows-Terminal style: swaps the whole body). Keeps the
  // tab's `path` so exitSettings() can return to the folder it came from.
  function showSettings() {
    cancelCreate(); cancelRename(); ctxMenu = null; deleteReq = null; addressEditing = false; clearSelection();
    activeTab.view = 'settings';
    gitStatus = null;
    fsWatchStop().catch(() => {});
  }
  function exitSettings() {
    if (activeTab.path) navigate(activeTab.path, false);
    else showOverview();
  }

  // Reset actions for the in-explorer settings page. These own the ephemeral
  // localStorage state the modal holds in memory (view memory, recents, layout).
  function resetViewMemory() { viewByPath = new Map(); }          // $effect persists []
  function resetRecents()    { recents = []; lsSet(RECENTS_KEY, []); }
  function resetLayout() {
    collapsedSections = new Set(); lsSet('arbor:explorer-collapsed-sections', []);
    wsExpanded        = new Set(); lsSet('arbor:explorer-ws-expanded', []);
    sidebarCollapsed  = false;     // persisted by its $effect
    previewWidth      = 300;       // persisted by its $effect
  }

  // ── Sidebar section order + visibility (configurable) ─────────────────────
  // Resolved list (built-in order + saved overrides). The sidebar renders in
  // this order, skipping hidden sections; the settings page edits it.
  const orderedSidebar = $derived(mergeSidebarSections(explorerStore.sidebarSections));
  let sectionCtx = $state<{ x: number; y: number; id: string } | null>(null);
  function openSectionCtx(e: MouseEvent, id: string) {
    e.preventDefault(); e.stopPropagation();
    ctxMenu = null;
    sectionCtx = { x: e.clientX, y: e.clientY, id };
  }
  function hideSection(id: string) {
    explorerStore.setSidebarSections(orderedSidebar.map(s => s.id === id ? { ...s, visible: false } : s));
  }
  const sectionLabel = (id: string) => EXPLORER_SECTIONS.find(x => x.id === id)?.label ?? id;

  // Seed the session sort + startup folder from ExplorerConfig once it has
  // loaded — one-shot, so in-session sort toggles aren't clobbered and we only
  // auto-open the last folder before the user has navigated anywhere.
  let _bootApplied = false;
  $effect(() => {
    if (_bootApplied || !explorerStore.loaded) return;
    _bootApplied = true;
    untrack(() => {
      sortKey = explorerStore.defaultSort as SortKey;
      sortAsc = explorerStore.sortAscending;
      if (explorerStore.startup === 'last' && activeTab.view === 'overview' && recents.length) {
        navigate(recents[0]);
      }
    });
  });

  const canBack    = $derived(activeTab.historyIdx > 0);
  const canForward = $derived(activeTab.historyIdx < activeTab.history.length - 1);
  const canUp      = $derived(view === 'browse' && parentOf(currentPath) !== null);
  function goBack()    { if (canBack)    { activeTab.historyIdx--; navigate(activeTab.history[activeTab.historyIdx], false); } }
  function goForward() { if (canForward) { activeTab.historyIdx++; navigate(activeTab.history[activeTab.historyIdx], false); } }
  function goUp()      { const p = parentOf(currentPath); if (p) navigate(p); }
  function refresh()   { if (view === 'browse') refreshCurrent(); }

  function parentOf(p: string): string | null {
    const clean = p.replace(/[\\/]+$/, '');
    const last = Math.max(clean.lastIndexOf('\\'), clean.lastIndexOf('/'));
    if (last <= 0) return null;
    const parent = clean.slice(0, last);
    if (/^[A-Za-z]:$/.test(parent)) return parent + '\\';
    return parent || null;
  }
  function joinPath(base: string, name: string): string {
    const sep = base.includes('\\') ? '\\' : '/';
    return base.replace(/[\\/]+$/, '') + sep + name;
  }
  function extOf(name: string): string { return name.split('.').pop()?.toLowerCase() ?? ''; }

  // ── Native system icons ───────────────────────────────────────────────────
  // Lazily fetch the real shell/desktop icon (PNG data-URI) for each file and
  // cache it by type. The cache is reactive, so rows re-render the moment an
  // icon arrives; folders keep the themed Iconify icon (the backend resolves
  // by file attribute and has no folder icon). Falls back to Iconify on error.
  const ICON_PX = 32;                            // fetch 32px → crisp at 16px render
  const PER_FILE_ICON = new Set(['exe']);        // types with a per-file (path) icon
  const iconCache = new SvelteMap<string, string>(); // key → data URI (reactive)
  const iconInflight = new Set<string>();        // dedupe concurrent fetches (non-reactive)
  const iconFailed = new Set<string>();          // give up after one failure → Iconify

  /** Native icon data-URI for `entry`, or null to fall back to the Iconify icon.
   *  Kicks off a one-shot async fetch on a cache miss (resolves into the
   *  reactive `iconCache`, re-rendering this row). */
  function nativeIconSrc(entry: FsEntry): string | null {
    if (entry.is_dir) return null;
    const dot = entry.name.lastIndexOf('.');
    const ext = dot > 0 ? entry.name.slice(dot + 1).toLowerCase() : '';
    const perFile = PER_FILE_ICON.has(ext);
    const key = perFile ? entry.path : (ext || ':noext');
    const cached = iconCache.get(key);
    if (cached) return cached;
    if (iconFailed.has(key) || iconInflight.has(key)) return null;
    iconInflight.add(key);
    const query = perFile ? entry.path : (ext ? `.${ext}` : 'file');
    fsIcon(query, ICON_PX)
      .then(uri => iconCache.set(key, uri))
      .catch(() => { iconFailed.add(key); })
      .finally(() => { iconInflight.delete(key); });
    return null;
  }

  // ── Address bar (editable path + ghost autocomplete) ──────────────────────
  let addressEditing = $state(false);
  let addressInput   = $state('');
  let addressParentCache = $state<{ parent: string; entries: FsEntry[] }>({ parent: '\0', entries: [] });
  let addressFetchSeq = 0;

  function startAddressEdit() {
    if (view === 'overview') return;
    addressInput = view === 'settings' ? 'arbor://settings' : currentPath;
    addressEditing = true;
    tick().then(() => (document.getElementById('fx-addr') as HTMLInputElement)?.select());
  }
  async function commitAddress() {
    addressEditing = false;
    const t = addressInput.trim();
    if (!t) return;
    // `arbor://settings` opens the in-explorer settings view (body swap), like
    // a browser's settings page — instead of navigating to a filesystem path.
    if (/^arbor:\/\/settings\/?$/i.test(t)) { showSettings(); return; }
    if (t !== currentPath) await navigate(t);
  }
  function normParentKey(p: string): string { return p.replace(/\\/g, '/').replace(/\/+$/, ''); }
  function lastSepIdx(s: string): number { return Math.max(s.lastIndexOf('\\'), s.lastIndexOf('/')); }
  function resolveReadDirPath(parent: string): string {
    if (/^[A-Za-z]:[\\/]?$/.test(parent)) return parent[0] + ':\\';
    return parent.replace(/[\\/]+$/, '') || parent;
  }
  async function refreshAddressCache(parent: string) {
    const seq = ++addressFetchSeq;
    try {
      const entries = await fsReadDir(resolveReadDirPath(parent), true);
      if (seq !== addressFetchSeq) return;
      addressParentCache = { parent, entries };
    } catch {
      if (seq !== addressFetchSeq) return;
      addressParentCache = { parent, entries: [] };
    }
  }
  $effect(() => {
    if (!addressEditing) return;
    const idx = lastSepIdx(addressInput);
    if (idx < 0) return;
    const parent = addressInput.slice(0, idx + 1);
    if (normParentKey(parent) === normParentKey(addressParentCache.parent)) return;
    refreshAddressCache(parent);
  });
  function findAddressMatch(partial: string): FsEntry | undefined {
    const lp = partial.toLowerCase();
    return addressParentCache.entries
      .filter(e => e.is_dir && e.name.toLowerCase().startsWith(lp) && e.name.length > partial.length)
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))[0];
  }
  const addressGhost = $derived.by(() => {
    if (!addressEditing || !addressInput) return '';
    const idx = lastSepIdx(addressInput);
    if (idx < 0) return '';
    const parent = addressInput.slice(0, idx + 1);
    const partial = addressInput.slice(idx + 1);
    if (normParentKey(parent) !== normParentKey(addressParentCache.parent)) return '';
    const match = findAddressMatch(partial);
    return match ? match.name.slice(partial.length) : '';
  });
  function completeAddress() {
    if (!addressGhost) return;
    const idx = lastSepIdx(addressInput);
    if (idx < 0) return;
    const parent = addressInput.slice(0, idx + 1);
    const match = findAddressMatch(addressInput.slice(idx + 1));
    if (!match) return;
    addressInput = parent + match.name;
    tick().then(() => { const i = document.getElementById('fx-addr') as HTMLInputElement | null; i?.setSelectionRange(addressInput.length, addressInput.length); });
  }

  // ── Breadcrumbs ──────────────────────────────────────────────────────────
  const breadcrumbs = $derived.by(() => {
    if (!currentPath) return [] as { label: string; path: string }[];
    const clean = currentPath.replace(/[\\/]+$/, '');
    const parts = clean.split(/[\\/]/);
    const isWin = /^[A-Za-z]:$/.test(parts[0]);
    const out: { label: string; path: string }[] = [];
    let acc = isWin ? parts[0] + '\\' : '/';
    out.push({ label: isWin ? parts[0] + '\\' : '/', path: acc });
    for (let i = 1; i < parts.length; i++) {
      if (!parts[i]) continue;
      acc = acc.replace(/[\\/]+$/, '') + (isWin ? '\\' : '/') + parts[i];
      out.push({ label: parts[i], path: acc });
    }
    return out;
  });

  // ── Filtering (wildcard) + derived list ─────────────────────────────────────
  // A query with `*`/`?` is an anchored, case-insensitive glob; otherwise it's
  // a plain case-insensitive substring match. Mirrors the backend matcher used
  // by recursive search so both modes behave identically.
  function globToRegExp(glob: string): RegExp {
    const esc = glob.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.');
    return new RegExp(`^${esc}$`, 'i');
  }
  const matchName = $derived.by<(name: string) => boolean>(() => {
    const t = filterQuery.trim();
    if (!t) return () => true;
    if (/[*?]/.test(t)) {
      try { const re = globToRegExp(t); return (name) => re.test(name); } catch { /* fall through */ }
    }
    const lc = t.toLowerCase();
    return (name) => name.toLowerCase().includes(lc);
  });

  /** Recursive mode is active only with a non-empty query in browse view. */
  const recursiveActive = $derived(recursiveSearch && view === 'browse' && filterQuery.trim().length > 0);

  const entries = $derived(rawEntries.filter(e => showHidden || !e.name.startsWith('.')));
  const sorted = $derived.by(() => {
    // Recursive results arrive already filtered (and hidden-aware) from the
    // backend; the local list is filtered here with the same matcher.
    const list = recursiveActive ? [...searchResults] : entries.filter(e => matchName(e.name));
    list.sort((a, b) => {
      if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
      let cmp = 0;
      if (sortKey === 'name')     cmp = a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
      if (sortKey === 'modified') cmp = (a.modified ?? 0) - (b.modified ?? 0);
      if (sortKey === 'size')     cmp = (a.size ?? 0) - (b.size ?? 0);
      return sortAsc ? cmp : -cmp;
    });
    return list;
  });
  function toggleSort(key: SortKey) { if (sortKey === key) sortAsc = !sortAsc; else { sortKey = key; sortAsc = true; } }

  // ── Recursive search (debounced backend walk) ───────────────────────────────
  let searchSeq = 0;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    // Track deps explicitly so the search re-runs on query/dir/hidden change.
    const active = recursiveActive;
    const root = currentPath, q = filterQuery, hidden = showHidden;
    if (searchTimer) { clearTimeout(searchTimer); searchTimer = null; }
    if (!active) { searching = false; return; }
    searching = true;
    searchTimer = setTimeout(async () => {
      const seq = ++searchSeq;
      try {
        const res = await fsSearch(root, q, hidden);
        if (seq === searchSeq) searchResults = res;
      } catch {
        if (seq === searchSeq) searchResults = [];
      } finally {
        if (seq === searchSeq) searching = false;
      }
    }, 250);
    return () => { if (searchTimer) { clearTimeout(searchTimer); searchTimer = null; } };
  });
  /** Directory of `path` relative to the current folder (for recursive hits). */
  function relLocation(path: string): string {
    const parent = parentOf(path) ?? '';
    const base = currentPath.replace(/[\\/]+$/, '');
    let rel = parent.replace(/[\\/]+$/, '');
    if (rel.toLowerCase().startsWith(base.toLowerCase())) rel = rel.slice(base.length);
    return rel.replace(/^[\\/]+/, '').replace(/\\/g, '/');
  }

  const cutSet = $derived(clipboard?.op === 'cut' ? new Set(clipboard.paths) : new Set<string>());
  const leadEntry = $derived(sorted.find(e => e.path === lead) ?? null);

  // ── Selection ──────────────────────────────────────────────────────────
  function clearSelection() { selected = new Set(); anchor = ''; lead = ''; }
  function clickEntry(entry: FsEntry, ev?: MouseEvent) {
    if (consumeDragClick()) return;
    cancelCreate();
    if (ev && (ev.ctrlKey || ev.metaKey)) {
      const next = new Set(selected);
      if (next.has(entry.path)) next.delete(entry.path); else next.add(entry.path);
      selected = next; anchor = entry.path; lead = entry.path;
    } else if (ev && ev.shiftKey && anchor) {
      const a = sorted.findIndex(e => e.path === anchor);
      const b = sorted.findIndex(e => e.path === entry.path);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        const next = new Set(selected);
        for (let i = lo; i <= hi; i++) next.add(sorted[i].path);
        selected = next; lead = entry.path;
      }
    } else {
      selected = new Set([entry.path]); anchor = entry.path; lead = entry.path;
    }
  }
  function selectAll() { selected = new Set(sorted.map(e => e.path)); if (sorted.length) { anchor = sorted[0].path; lead = sorted[sorted.length - 1].path; } }
  function actionPaths(): string[] { return selected.size > 0 ? [...selected] : (lead ? [lead] : []); }

  function openEntry(entry: FsEntry) {
    if (entry.is_dir) navigate(entry.path);
    else fsOpenDefault(entry.path).catch(e => uiStore.showToast(`Cannot open: ${e}`, 'error'));
  }
  /** Open the OS-native Properties sheet for a path (Windows). */
  function openSystemProperties(path: string) {
    fsShowProperties(path).catch(e => uiStore.showToast(`${e}`, 'error'));
  }

  // ── Clipboard ──────────────────────────────────────────────────────────
  function doCopy() { const p = actionPaths(); if (!p.length) return; clipboard = { op: 'copy', paths: p }; uiStore.showToast(`Copied ${p.length} item${p.length !== 1 ? 's' : ''}`, 'info'); }
  function doCut()  { const p = actionPaths(); if (!p.length) return; clipboard = { op: 'cut', paths: p }; }
  async function doPaste() {
    if (!clipboard || !currentPath || view !== 'browse') return;
    const { op, paths } = clipboard;
    try {
      if (op === 'copy') await fsCopy(paths, currentPath);
      else { await fsMove(paths, currentPath); clipboard = null; }
      await refreshCurrent();
    } catch (e) { uiStore.showToast(`Paste failed: ${e}`, 'error'); }
  }

  // ── Drag & drop (move entries into a folder) ───────────────────────────────
  // Mouse-event based, NOT native HTML5 DnD: on Windows WebView2 the OS drag
  // handler (Tauri `dragDropEnabled`, on by default) swallows the DOM `drop`
  // event, so native DnD silently fails. Same approach as Tabs.svelte. Bonus:
  // re-rendering the dragged row mid-drag (selection/virtualization) can't
  // cancel the gesture, since the listeners live on `window`, not the node.
  let dragging      = $state(false);
  let dragOverDir   = $state<string | null>(null);
  let dragGhost     = $state<{ x: number; y: number; label: string } | null>(null);
  let dragPaths: string[] = [];
  let dragPathSet   = new Set<string>();
  let suppressClick = false;

  /** True (and clears the flag) when a click is the tail of a finished drag. */
  function consumeDragClick(): boolean {
    if (!suppressClick) return false;
    suppressClick = false;
    return true;
  }

  function startRowDrag(e: MouseEvent, entry: FsEntry) {
    if (e.button !== 0 || renamingPath || createKind) return;
    suppressClick = false;
    const startX = e.clientX, startY = e.clientY;
    let started = false;
    const onMove = (ev: MouseEvent) => {
      if (!started) {
        if (Math.abs(ev.clientX - startX) < 5 && Math.abs(ev.clientY - startY) < 5) return;
        started = true;
        // Drag the whole selection if the grabbed row is part of it, else just it.
        if (!selected.has(entry.path)) { selected = new Set([entry.path]); anchor = entry.path; lead = entry.path; }
        dragPaths   = selected.has(entry.path) && selected.size > 0 ? [...selected] : [entry.path];
        dragPathSet = new Set(dragPaths);
        dragging    = true;
        dragGhost   = { x: ev.clientX, y: ev.clientY, label: dragPaths.length === 1 ? baseName(dragPaths[0]) : `${dragPaths.length} items` };
        document.body.style.cursor = 'grabbing';
        document.body.style.userSelect = 'none';
      }
      if (dragGhost) dragGhost = { ...dragGhost, x: ev.clientX, y: ev.clientY };
      updateDropTarget(ev.clientX, ev.clientY);
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      if (started) {
        const target = dragOverDir;
        suppressClick = true;             // swallow the click the browser fires next
        if (target) void moveInto(target);
      }
      dragging = false; dragOverDir = null; dragGhost = null;
      dragPaths = []; dragPathSet = new Set();
      document.body.style.cursor = ''; document.body.style.userSelect = '';
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  /** Hit-test the row under the cursor; highlight it only when it's a folder
   *  outside the dragged set. Reads `data-path`/`data-dir` off the row — no
   *  per-move list scan. */
  function updateDropTarget(x: number, y: number) {
    const row  = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>('.fx-row');
    const path = row?.dataset.path;
    dragOverDir = (path && row?.dataset.dir === 'true' && !dragPathSet.has(path)) ? path : null;
  }

  async function moveInto(target: string) {
    const paths = dragPaths.filter(p => p && p !== target);
    if (!paths.length) return;
    try { await fsMove(paths, target); clearSelection(); await refreshCurrent(); }
    catch (err) { uiStore.showToast(`Move failed: ${err}`, 'error'); }
  }

  // ── Compress / extract (ZIP) ────────────────────────────────────────────────
  function baseName(p: string): string { return p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || p; }
  function isZip(name: string): boolean { return extOf(name) === 'zip'; }
  // Already-compressed formats: re-zipping a single one of these is pointless,
  // so the "Compress to ZIP" entry is hidden for it.
  const COMPRESSED_EXTS = new Set(['zip','7z','rar','gz','tgz','bz2','tbz2','xz','txz','zst','lz','lzma','cab','tar','jar','war','apk']);
  function isCompressed(name: string): boolean { return COMPRESSED_EXTS.has(extOf(name)); }
  /** A single selected entry that is a .zip file, or null. */
  const zipTarget = $derived.by<FsEntry | null>(() => {
    const p = actionPaths();
    if (p.length !== 1) return null;
    const e = sorted.find(x => x.path === p[0]);
    return e && !e.is_dir && isZip(e.name) ? e : null;
  });
  /** True when the action targets exactly one already-compressed file. */
  const singleCompressed = $derived.by(() => {
    const p = actionPaths();
    if (p.length !== 1) return false;
    const e = sorted.find(x => x.path === p[0]);
    return !!e && !e.is_dir && isCompressed(e.name);
  });
  /** Default archive name: `<item>.zip` for one item, `<folder>.zip` for many. */
  function defaultZipName(paths: string[]): string {
    if (paths.length === 1) {
      const n = baseName(paths[0]);
      const dot = n.lastIndexOf('.');
      return `${dot > 0 ? n.slice(0, dot) : n}.zip`;
    }
    return `${baseName(currentPath) || 'archive'}.zip`;
  }
  async function doCompress() {
    const paths = actionPaths();
    if (!paths.length || view !== 'browse' || !currentPath) return;
    try {
      const created = await fsZip(paths, currentPath, defaultZipName(paths));
      await refreshCurrent();
      selected = new Set([created]); anchor = created; lead = created;
      uiStore.showToast(`Compressed ${paths.length} item${paths.length !== 1 ? 's' : ''}`, 'success');
    } catch (e) { uiStore.showToast(`Compress failed: ${e}`, 'error'); }
  }
  async function doExtract(path?: string) {
    const target = path ?? zipTarget?.path;
    if (!target || view !== 'browse') return;
    try {
      const out = await fsUnzip(target);
      await refreshCurrent();
      selected = new Set([out]); anchor = out; lead = out;
      uiStore.showToast('Archive extracted', 'success');
    } catch (e) { uiStore.showToast(`Extract failed: ${e}`, 'error'); }
  }
  function setWallpaper(path: string) {
    fsSetWallpaper(path)
      .then(() => uiStore.showToast('Set as desktop background', 'success'))
      .catch(e => uiStore.showToast(`${e}`, 'error'));
  }

  // ── Context-menu action state ───────────────────────────────────────────────
  const singleSelection = $derived(selected.size <= 1 && !!lead);
  /** A single selected image file (for "Set as desktop background"), or null. */
  const imageTarget = $derived.by<FsEntry | null>(() => {
    if (!singleSelection) return null;
    const e = leadEntry;
    return e && !e.is_dir && isImage(e.name) ? e : null;
  });

  // ── Delete ──────────────────────────────────────────────────────────────
  function askDelete(permanent: boolean) {
    const paths = actionPaths().filter(p => p !== currentPath);
    if (paths.length) deleteReq = { paths, permanent };
  }
  async function confirmDelete() {
    if (!deleteReq) return;
    const { paths, permanent } = deleteReq;
    deleteReq = null;
    try {
      if (permanent) await fsDeleteMany(paths); else await fsTrash(paths);
      clearSelection();
      await refreshCurrent();
      uiStore.showToast(permanent ? `Deleted ${paths.length} item${paths.length !== 1 ? 's' : ''}` : `Moved ${paths.length} item${paths.length !== 1 ? 's' : ''} to Recycle Bin`, 'success');
    } catch (e) { uiStore.showToast(`Delete failed: ${e}`, 'error'); }
  }

  // ── Rename / create ───────────────────────────────────────────────────────
  function startRename(entry: FsEntry) {
    cancelCreate(); ctxMenu = null;
    renamingPath = entry.path; renameValue = entry.name;
    tick().then(() => { const i = document.getElementById('fx-rename') as HTMLInputElement | null; i?.focus(); i?.select(); });
  }
  async function commitRename() {
    const trimmed = renameValue.trim();
    const target = rawEntries.find(e => e.path === renamingPath);
    const old = renamingPath;
    cancelRename();
    if (!trimmed || !target || target.name === trimmed) return;
    try { await fsRename(old, joinPath(currentPath, trimmed)); await refreshCurrent(); }
    catch (e) { uiStore.showToast(`Rename failed: ${e}`, 'error'); }
  }
  function cancelRename() { renamingPath = ''; renameValue = ''; }
  function startCreate(kind: 'folder' | 'file') {
    cancelRename(); ctxMenu = null;
    createKind = kind; createName = kind === 'folder' ? 'New Folder' : 'new-file.txt';
    tick().then(() => { const i = document.getElementById('fx-create') as HTMLInputElement | null; i?.focus(); i?.select(); });
  }
  async function commitCreate() {
    const name = createName.trim(); const kind = createKind;
    cancelCreate();
    if (!name || !kind || !currentPath) return;
    const path = joinPath(currentPath, name);
    try {
      if (kind === 'folder') await fsCreateDir(path); else await fsCreateFile(path);
      await refreshCurrent();
      selected = new Set([path]); lead = path; anchor = path;
    } catch (e) { uiStore.showToast(`Create failed: ${e}`, 'error'); }
  }
  function cancelCreate() { createKind = null; createName = ''; }

  // ── Context menu ──────────────────────────────────────────────────────────
  function openCtx(e: MouseEvent, entry: FsEntry | null) {
    e.preventDefault(); e.stopPropagation();
    cancelCreate(); cancelRename();
    if (entry && !selected.has(entry.path)) { selected = new Set([entry.path]); anchor = entry.path; lead = entry.path; }
    ctxMenu = { x: e.clientX, y: e.clientY, entry };
  }
  /** Top icon-bar quick actions (Cut/Copy/Rename/Delete) — only on an entry. */
  function ctxActions(entry: FsEntry | null): MenuAction[] | undefined {
    if (!entry) return undefined;
    const multi = selected.size > 1;
    return [
      { id: 'cut',    label: 'Cut',    icon: Scissors, shortcut: 'Ctrl+X' },
      { id: 'copy',   label: 'Copy',   icon: Copy,     shortcut: 'Ctrl+C' },
      { id: 'rename', label: 'Rename', icon: Pencil,   shortcut: 'F2', disabled: multi },
      { id: 'delete', label: multi ? `Move ${selected.size} to Recycle Bin` : 'Move to Recycle Bin', icon: Trash2, shortcut: 'Del', danger: true },
    ];
  }
  function ctxItems(entry: FsEntry | null): MenuItem[] {
    const items: MenuItem[] = [];
    const multi = selected.size > 1;
    if (entry) {
      items.push(
        { id: 'open',   label: entry.is_dir ? 'Open' : 'Open with default app', icon: entry.is_dir ? FolderOpen : ExternalLink },
        { id: 'reveal', label: 'Reveal in File Explorer', icon: ExternalLink },
      );
    }
    if (clipboard) {
      if (entry) items.push({ id: 'sep-paste', label: '', separator: true });
      items.push({ id: 'paste', label: `Paste${clipboard.paths.length > 1 ? ` (${clipboard.paths.length})` : ''}`, icon: ClipboardPaste });
    }
    if (entry) {
      items.push(
        { id: 'sep1',      label: '', separator: true },
        { id: 'copy-path', label: 'Copy Path', icon: Link2, disabled: multi },
        { id: 'sep-arch',  label: '', separator: true },
      );
      if (zipTarget) items.push({ id: 'extract', label: 'Extract here', icon: PackageOpen, iconColor: 'var(--info)' });
      if (!singleCompressed) items.push({ id: 'compress', label: multi ? `Compress ${selected.size} to ZIP` : 'Compress to ZIP', icon: Archive });
      if (imageTarget) items.push({ id: 'wallpaper', label: 'Set as desktop background', icon: ImageIcon });
    }
    // Git section. File-level actions (stage/unstage/discard/ignore) show when
    // the current directory is inside a repo; repo-level ones (Switch branch,
    // Open in Arbor) also show on a folder that IS a repo root — so you can act
    // on a project from its parent without entering it first.
    const repoFolder = entry?.is_dir ? repoInfoFor(entry) : null;
    if (entry && (inRepo || repoFolder)) {
      const n = multi ? ` ${selected.size}` : '';
      items.push({ id: 'sep-git', label: '', separator: true });
      if (inRepo) {
        items.push(
          { id: 'git-stage',   label: `Stage${n}`,            icon: Plus,  iconColor: 'var(--success)' },
          { id: 'git-unstage', label: `Unstage${n}`,          icon: Minus },
          { id: 'git-discard', label: `Discard changes${n}`,  icon: Undo2, danger: true },
          { id: 'git-ignore',  label: 'Add to .gitignore',    icon: EyeOff },
        );
      }
      // Switch branch is a git action → only with git awareness on. Open in
      // Arbor is a plain convenience (no git checks) → always available here.
      if (gitOn) items.push({ id: 'git-switch', label: 'Switch branch…', icon: GitBranch });
      items.push({ id: 'git-open', label: 'Open in Arbor', icon: ExternalLink, iconColor: 'var(--accent)' });
    }
    items.push(
      { id: 'sep2', label: '', separator: true },
      { id: 'new-folder', label: 'New Folder', icon: FolderPlus, iconColor: 'var(--success)' },
      { id: 'new-file',   label: 'New File',   icon: FilePlus,   iconColor: 'var(--success)' },
    );
    if (entry) items.push({ id: 'props', label: 'Properties', icon: Info });
    return items;
  }
  function handleCtx(id: string) {
    const entry = ctxMenu?.entry ?? null;
    const px = ctxMenu?.x ?? 0, py = ctxMenu?.y ?? 0;
    ctxMenu = null;
    switch (id) {
      case 'open':       if (entry) openEntry(entry); break;
      case 'reveal':     if (entry) fsRevealInDir(entry.path).catch(e => uiStore.showToast(`${e}`, 'error')); break;
      case 'cut':        doCut(); break;
      case 'copy':       doCopy(); break;
      case 'paste':      doPaste(); break;
      case 'rename':     if (entry) startRename(entry); break;
      case 'delete':     askDelete(false); break;
      case 'copy-path':  if (entry) copyToClipboard(entry.path, { successToast: 'Path copied' }); break;
      case 'new-folder': startCreate('folder'); break;
      case 'new-file':   startCreate('file'); break;
      case 'compress':   doCompress(); break;
      case 'extract':    if (entry) doExtract(entry.path); break;
      case 'wallpaper':  if (entry) setWallpaper(entry.path); break;
      case 'props':      rightPanel = 'info'; break;
      case 'git-stage':   void runGit(fsGitStage,   'Staged'); break;
      case 'git-unstage': void runGit(fsGitUnstage, 'Unstaged'); break;
      case 'git-discard': { const p = actionPaths(); if (p.length) discardReq = p; break; }
      case 'git-ignore':  void runGit(fsGitIgnore,  'Added to .gitignore'); break;
      case 'git-switch':  if (entry) void openBranchSwitch(entry.path, px, py); break;
      case 'git-open':    if (entry) fsOpenInArbor(entry.path).catch(e => uiStore.showToast(`${e}`, 'error')); break;
    }
  }

  /** Run a path-based git action over the current selection, then refresh the
   *  view + overlays. Discard is snapshotted to Arbor's Recovery tab backend-side,
   *  so it stays undoable. */
  async function runGit(fn: (paths: string[]) => Promise<void>, verb: string) {
    const paths = actionPaths();
    if (!paths.length) return;
    try {
      await fn(paths);
      await refreshCurrent();
      uiStore.showToast(`${verb} ${paths.length} item${paths.length !== 1 ? 's' : ''}`, 'success');
    } catch (e) {
      uiStore.showToast(`Git: ${e}`, 'error');
    }
  }

  /** Run the discard confirmed in the ConfirmModal, then refresh + close. */
  async function confirmDiscard() {
    const paths = discardReq;
    if (!paths?.length) { discardReq = null; return; }
    discardBusy = true;
    try {
      await fsGitDiscard(paths);
      await refreshCurrent();
      uiStore.showToast(`Discarded ${paths.length} item${paths.length !== 1 ? 's' : ''}`, 'success');
    } catch (e) {
      uiStore.showToast(`Git: ${e}`, 'error');
    } finally {
      discardBusy = false;
      discardReq = null;
    }
  }

  // ── Keyboard ──────────────────────────────────────────────────────────────
  function onKeydown(e: KeyboardEvent) {
    if (renamingPath || createKind || addressEditing) return;
    const inInput = (e.target as HTMLElement).tagName === 'INPUT';
    const mod = e.ctrlKey || e.metaKey;
    // Panel toggles — work in any view. Ctrl+B: left sidebar; Ctrl+Shift+B: right panel.
    if (mod && !inInput && e.key.toLowerCase() === 'b') {
      e.preventDefault(); e.stopImmediatePropagation();
      if (e.shiftKey) { if (view === 'browse') rightPanel = rightPanel === null ? 'preview' : null; }
      else sidebarCollapsed = !sidebarCollapsed;
      return;
    }
    // Ctrl+, — open the in-explorer settings view (toggles back out if already there).
    if (mod && !inInput && e.key === ',') {
      e.preventDefault(); e.stopImmediatePropagation();
      if (view === 'settings') exitSettings(); else showSettings();
      return;
    }
    if (view !== 'browse') return;
    if (mod && !inInput) {
      const k = e.key.toLowerCase();
      if (k === 'a') { e.preventDefault(); e.stopImmediatePropagation(); selectAll(); return; }
      if (k === 'c') { e.preventDefault(); e.stopImmediatePropagation(); doCopy(); return; }
      if (k === 'x') { e.preventDefault(); e.stopImmediatePropagation(); doCut();  return; }
      if (k === 'v') { e.preventDefault(); e.stopImmediatePropagation(); doPaste(); return; }
      if (k === 'l') { e.preventDefault(); e.stopImmediatePropagation(); startAddressEdit(); return; }
    }
    switch (e.key) {
      case 'Enter': { if (inInput) return; e.preventDefault(); if (deleteReq) { confirmDelete(); return; } if (leadEntry) openEntry(leadEntry); return; }
      case 'Delete': { if (inInput) return; e.preventDefault(); askDelete(e.shiftKey); return; }
      case 'F2': { if (inInput) return; e.preventDefault(); if (leadEntry) startRename(leadEntry); return; }
      case 'Backspace': if (!inInput) { e.preventDefault(); goUp(); } return;
      case 'ArrowLeft':
        if (e.altKey) { e.preventDefault(); goBack(); return; }
        if (!inInput && isGrid) { e.preventDefault(); moveLead(-1, e.shiftKey); }
        return;
      case 'ArrowRight':
        if (e.altKey) { e.preventDefault(); goForward(); return; }
        if (!inInput && isGrid) { e.preventDefault(); moveLead(1, e.shiftKey); }
        return;
      case 'ArrowDown':
        if (inInput) return;
        e.preventDefault(); moveLead(isGrid ? cols : 1, e.shiftKey); return;
      case 'ArrowUp':
        if (inInput) return;
        e.preventDefault(); moveLead(isGrid ? -cols : -1, e.shiftKey); return;
    }
    if (!inInput && !mod && !e.altKey && e.key.length === 1) {
      e.preventDefault();
      filterQuery += e.key;
      tick().then(() => { const i = document.querySelector('.fx-filter-input') as HTMLInputElement | null; i?.focus(); if (i) i.setSelectionRange(filterQuery.length, filterQuery.length); });
    }
  }

  // ── Virtual scroll ─────────────────────────────────────────────────────────
  // Generalised to a grid: details mode is just a 1-column grid of tall rows.
  // `cellH` is the height of one virtual row; `cols` is how many entries fit
  // across (1 in details). Visible window is always whole rows of `cols` items.
  const VS_ROW = 28, VS_OVERSCAN = 12;
  // `ico` = vector-icon size; `thumb` = image-preview box (large/xlarge only).
  const TILE = {
    medium: { w: 112, h: 104, ico: 40, thumb: 40 },
    large:  { w: 156, h: 150, ico: 64, thumb: 84 },
    xlarge: { w: 208, h: 196, ico: 96, thumb: 132 },
  } as const;
  let vsScrollTop = $state(0), vsClient = $state(0), vsListW = $state(0), _raf = 0;
  function onScroll() { if (_raf) return; _raf = requestAnimationFrame(() => { _raf = 0; vsScrollTop = listEl?.scrollTop ?? 0; }); }
  const isGrid   = $derived(viewMode !== 'details');
  const tile     = $derived(viewMode === 'xlarge' ? TILE.xlarge : viewMode === 'large' ? TILE.large : TILE.medium);
  // Image thumbnails kick in from large icons upward; medium keeps vector icons.
  const thumbsOn = $derived(viewMode === 'large' || viewMode === 'xlarge');
  const cols     = $derived(isGrid ? Math.max(1, Math.floor((vsListW || 1) / tile.w)) : 1);
  const cellH    = $derived(isGrid ? tile.h : VS_ROW);
  const ovRows   = $derived(isGrid ? 3 : VS_OVERSCAN);
  const rowCount = $derived(Math.ceil(sorted.length / cols));
  const vsTotalH = $derived(rowCount * cellH);
  const vsStartRow = $derived(Math.max(0, Math.floor(vsScrollTop / cellH) - ovRows));
  const vsEndRow   = $derived(Math.min(rowCount, Math.ceil((vsScrollTop + Math.max(vsClient, cellH)) / cellH) + ovRows));
  const vsStart  = $derived(vsStartRow * cols);
  const vsEnd    = $derived(Math.min(sorted.length, vsEndRow * cols));
  const vsItems  = $derived(sorted.slice(vsStart, vsEnd));
  const vsOffset = $derived(vsStartRow * cellH);
  function resetScroll() { vsScrollTop = 0; if (listEl) listEl.scrollTop = 0; }
  function scrollToIndex(i: number) {
    if (!listEl) return;
    const row = isGrid ? Math.floor(i / cols) : i;
    const top = row * cellH, bottom = top + cellH;
    if (top < listEl.scrollTop) listEl.scrollTop = top;
    else if (bottom > listEl.scrollTop + listEl.clientHeight) listEl.scrollTop = bottom - listEl.clientHeight;
    vsScrollTop = listEl.scrollTop;
  }
  // Move the lead/selection by `delta` entries (±1 horizontally, ±cols vertically
  // in grid mode). `extend` grows a contiguous selection from the anchor.
  function moveLead(delta: number, extend: boolean) {
    if (!sorted.length) return;
    const idx = sorted.findIndex(x => x.path === lead);
    const nextIdx = idx < 0 ? 0 : Math.max(0, Math.min(idx + delta, sorted.length - 1));
    const next = sorted[nextIdx];
    if (!next) return;
    if (extend && anchor) {
      const a = sorted.findIndex(x => x.path === anchor);
      if (a >= 0) { const [lo, hi] = a < nextIdx ? [a, nextIdx] : [nextIdx, a]; const set = new Set(selected); for (let i = lo; i <= hi; i++) set.add(sorted[i].path); selected = set; }
      lead = next.path;
    } else { selected = new Set([next.path]); anchor = next.path; lead = next.path; }
    tick().then(() => scrollToIndex(nextIdx));
  }

  // ── Preview (async + spinner) ───────────────────────────────────────────────
  type Preview =
    | { kind: 'image';  src: string }
    | { kind: 'video';  src: string }
    | { kind: 'audio';  src: string }
    | { kind: 'text';   text: string; lang: string | null }
    | { kind: 'folder'; items: FsEntry[] }
    | { kind: 'none' };
  let preview = $state<Preview | null>(null);
  let previewLoading = $state(false);
  let pvSeq = 0;

  // Image extensions that the asset protocol is scoped to (tauri.conf.json) —
  // these render via convertFileSrc into a real <img>.
  const IMAGE_EXTS = ['png','jpg','jpeg','gif','webp','svg','bmp','ico','avif'];
  function isImage(name: string): boolean { return IMAGE_EXTS.includes(extOf(name)); }
  // Media extensions matching the asset-protocol scope (tauri.conf.json).
  const VIDEO_EXTS = ['mp4','webm','ogv','mov','m4v','mkv'];
  const AUDIO_EXTS = ['mp3','wav','m4a','flac','aac','opus','oga','ogg'];
  function isVideo(name: string): boolean { return VIDEO_EXTS.includes(extOf(name)); }
  function isAudio(name: string): boolean { return AUDIO_EXTS.includes(extOf(name)); }

  const TEXT_EXTS = ['md','txt','rs','toml','json','jsonc','csv','tsv','env','log','lua','ts','tsx','js','jsx','mjs','cjs','css','scss','sass','html','htm','xml','svelte','vue','yaml','yml','ini','conf','cfg','properties','sh','bash','zsh','bat','cmd','py','go','sql','c','h','cpp','cc','cxx','hpp','kt','kts','cs','swift','java','rb','php','glsl','vert','frag','ps1','psm1','diff','patch','make','cmake','gradle','dockerfile','gitignore','gitattributes','editorconfig',''];
  function isTextual(name: string): boolean { return name.startsWith('.') || TEXT_EXTS.includes(extOf(name)); }

  // Extension → Prism language id (grammars registered in prism-shared.ts).
  // Unmapped → null → highlightCode falls back to plain escaped text.
  const EXT_LANG: Record<string, string> = {
    ts: 'typescript', tsx: 'typescript', mts: 'typescript', cts: 'typescript',
    js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
    ini: 'ini', cfg: 'ini', conf: 'ini', properties: 'ini',
    rs: 'rust', py: 'python', json: 'json', jsonc: 'json', css: 'css',
    scss: 'scss', sass: 'scss', sh: 'bash', bash: 'bash', zsh: 'bash',
    bat: 'batch', cmd: 'batch', toml: 'toml', md: 'markdown', markdown: 'markdown',
    yaml: 'yaml', yml: 'yaml', java: 'java', swift: 'swift', go: 'go', sql: 'sql',
    c: 'c', h: 'c', cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp', hxx: 'cpp',
    kt: 'kotlin', kts: 'kotlin', cs: 'csharp', lua: 'lua', glsl: 'glsl',
    vert: 'glsl', frag: 'glsl', ps1: 'powershell', psm1: 'powershell',
    html: 'markup', htm: 'markup', xml: 'markup', svelte: 'svelte', dockerfile: 'docker',
  };
  function langFor(name: string): string | null {
    if (name.toLowerCase() === 'dockerfile') return 'docker';
    return EXT_LANG[extOf(name)] ?? null;
  }

  $effect(() => {
    const entry = leadEntry;
    if (rightPanel !== 'preview' || view !== 'browse' || !entry) { preview = null; previewLoading = false; return; }
    const seq = ++pvSeq;
    previewLoading = true; preview = null;
    (async () => {
      try {
        let result: Preview;
        if (entry.is_dir) result = { kind: 'folder', items: (await fsReadDir(entry.path, true)).slice(0, 300) };
        else if (isImage(entry.name)) result = { kind: 'image', src: convertFileSrc(entry.path) };
        else if (isVideo(entry.name)) result = { kind: 'video', src: convertFileSrc(entry.path) };
        else if (isAudio(entry.name)) result = { kind: 'audio', src: convertFileSrc(entry.path) };
        else if (isTextual(entry.name) && (entry.size ?? 0) < 2_000_000) result = { kind: 'text', text: (await fsReadTextFile(entry.path)).slice(0, 50_000), lang: langFor(entry.name) };
        else result = { kind: 'none' };
        if (seq === pvSeq) { preview = result; previewLoading = false; }
      } catch { if (seq === pvSeq) { preview = { kind: 'none' }; previewLoading = false; } }
    })();
  });

  // ── Formatting / icons ──────────────────────────────────────────────────
  function formatSize(b: number): string {
    if (b < 1024) return `${b} B`;
    if (b < 1024 ** 2) return `${(b / 1024).toFixed(1)} KB`;
    if (b < 1024 ** 3) return `${(b / 1024 ** 2).toFixed(1)} MB`;
    return `${(b / 1024 ** 3).toFixed(2)} GB`;
  }
  function formatDate(ms: number): string {
    const d = new Date(ms);
    return d.toLocaleDateString('it-IT', { day: '2-digit', month: '2-digit', year: 'numeric' }) + ' ' + d.toLocaleTimeString('it-IT', { hour: '2-digit', minute: '2-digit' });
  }
  function rootIcon(kind: FsRoot['kind']) {
    return { home: Home, desktop: Monitor, documents: FileText, downloads: Download, drive: HardDrive }[kind] ?? Folder;
  }
  const SEARCH_CAP = 5000;
  const footerInfo = $derived.by(() => {
    if (selected.size > 1) return `${selected.size} selected`;
    if (recursiveActive) {
      const n = sorted.length;
      if (searching && !n) return 'Searching…';
      const capped = n >= SEARCH_CAP ? ` (first ${SEARCH_CAP})` : '';
      return `${n} result${n !== 1 ? 's' : ''}${capped}`;
    }
    // Count the displayed list (`sorted`), not the whole folder, so an active
    // filter is reflected here instead of reporting the unfiltered total.
    const dirs = sorted.filter(e => e.is_dir).length;
    const files = sorted.length - dirs;
    const parts: string[] = [];
    if (dirs) parts.push(`${dirs} folder${dirs !== 1 ? 's' : ''}`);
    if (files) parts.push(`${files} file${files !== 1 ? 's' : ''}`);
    return parts.join(', ') || (filterQuery.trim() ? 'No matches' : 'Empty folder');
  });

  // ── Lifecycle ──────────────────────────────────────────────────────────
  let unlisten: UnlistenFn | null = null;
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleRefresh() { if (refreshTimer) clearTimeout(refreshTimer); refreshTimer = setTimeout(() => { refreshTimer = null; refreshCurrent(); }, 200); }
  onMount(async () => {
    try { roots = await listFsRoots(); } catch { /* ignore */ }
    try {
      const [repos, snap] = await Promise.all([listRegistryRepos(), listWorkspaces()]);
      projects = repos; workspaces = snap.workspaces; activeWorkspaceId = snap.active_workspace_id;
      // First ever run: expand all workspace groups by default (persisted via a
      // one-shot flag so collapsing them all later isn't undone on reopen).
      if (!lsGet('arbor:explorer-ws-init', false)) {
        lsSet('arbor:explorer-ws-init', true);
        const all = new Set<string>(['__unassigned__', ...snap.workspaces.map(w => w.id)]);
        wsExpanded = all; lsSet('arbor:explorer-ws-expanded', [...all]);
      }
    } catch { /* ignore */ }
    try { unlisten = await listen('arbor://fs-changed', () => scheduleRefresh()); } catch { /* ignore */ }
  });
  onDestroy(() => { unlisten?.(); if (refreshTimer) clearTimeout(refreshTimer); fsWatchStop().catch(() => {}); });
</script>

{#snippet sbLabel(id: string, Ico: typeof Folder, text: string)}
  <button class="fx-sb-label" class:rail={sidebarCollapsed} onclick={() => { if (!sidebarCollapsed) toggleSection(id); }}
          oncontextmenu={(e) => openSectionCtx(e, id)}
          use:tooltip={sidebarCollapsed ? '' : { content: text, description: 'Right-click to hide · reorder in Settings', delay: 1400 }}
          aria-expanded={!collapsedSections.has(id)}>
    {#if !sidebarCollapsed}<span class="fx-sb-chev">{#if collapsedSections.has(id)}<ChevronRight size={10} strokeWidth={2.4} />{:else}<ChevronDown size={10} strokeWidth={2.4} />{/if}</span>{/if}
    <Ico size={11} class="fx-sb-label-ico" />
    {#if !sidebarCollapsed}<span class="fx-sb-label-text">{text}</span>{/if}
  </button>
{/snippet}

<!-- Sidebar sections, each wrapped so the configured order/visibility loop can
     render them by id. Availability {#if}s stay inside so an empty section
     renders nothing. -->
{#snippet secLibrary()}
  {@render sbLabel('library', LayoutDashboard, 'Library')}
  {#if sectionOpen('library')}
    <div class="fx-sb-list">
      <button class="fx-sb-item" class:active={view === 'overview'} onclick={showOverview} use:tooltip={sidebarCollapsed ? 'Overview' : ''}>
        <span class="fx-sb-ico"><LayoutDashboard size={14} /></span><span class="fx-sb-text">Overview</span>
      </button>
      <button class="fx-sb-item" class:active={view === 'settings'} onclick={showSettings} use:tooltip={sidebarCollapsed ? { content: 'Settings', shortcut: 'Ctrl+,' } : ''}>
        <span class="fx-sb-ico"><Settings2 size={14} /></span><span class="fx-sb-text">Settings</span>
      </button>
    </div>
  {/if}
{/snippet}

{#snippet secRecents()}
  {#if recents.length}
    {@render sbLabel('recents', History, 'Recents')}
    {#if sectionOpen('recents')}
      <div class="fx-sb-list">
        {#each recents as p (p)}
          {@const name = p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || p}
          <button class="fx-sb-item" class:active={view === 'browse' && currentPath === p} onclick={() => navigate(p)} use:tooltip={sidebarCollapsed ? `${name} — ${p}` : p}>
            <span class="fx-sb-ico"><Folder size={14} /></span><span class="fx-sb-text">{name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
{/snippet}

{#snippet secFavourites()}
  {#if favourites.length}
    {@render sbLabel('favourites', Star, 'Favourites')}
    {#if sectionOpen('favourites')}
      <div class="fx-sb-list">
        {#each favourites as r (r.path)}
          {@const I = rootIcon(r.kind)}
          <button class="fx-sb-item" class:active={view === 'browse' && currentPath === r.path} onclick={() => navigate(r.path)} use:tooltip={sidebarCollapsed ? `${r.name} — ${r.path}` : r.path}>
            <span class="fx-sb-ico"><I size={14} /></span><span class="fx-sb-text">{r.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
{/snippet}

{#snippet secDevices()}
  {#if drives.length}
    {@render sbLabel('devices', HardDrive, 'Devices')}
    {#if sectionOpen('devices')}
      <div class="fx-sb-list">
        {#each drives as r (r.path)}
          <button class="fx-sb-item" class:active={view === 'browse' && currentPath.startsWith(r.path)} onclick={() => navigate(r.path)} use:tooltip={sidebarCollapsed ? `${r.name} — ${r.path}` : r.path}>
            <span class="fx-sb-ico"><HardDrive size={14} /></span><span class="fx-sb-text">{r.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
{/snippet}

{#snippet secProjects()}
  {#if activeProject || wsGroups.length}
    {@render sbLabel('projects', GitBranch, 'Projects')}
    {#if sectionOpen('projects')}
      {#if sidebarCollapsed}
        <!-- icon-rail: flat project icons, no workspace headers -->
        <div class="fx-sb-list">
          {#each projects as p (p.id)}
            {@const isActive = activeRepoPath != null && normPath(p.path) === normPath(activeRepoPath)}
            <button class="fx-sb-item fx-sb-project" class:fx-active-repo={isActive} onclick={() => navigate(p.path)} use:tooltip={`${p.display_name}${isActive ? ' (active)' : ''} — ${p.path}`}>
              <span class="fx-sb-ico">{#if isActive}<Box size={14} />{:else}<GitBranch size={14} />{/if}</span>
            </button>
          {/each}
        </div>
      {:else}
        {#if activeProject}
          {@const ap = activeProject}
          {@const isCurrent = normPath(currentPath) === normPath(ap.path) || normPath(currentPath).startsWith(normPath(ap.path) + '/')}
          <div class="fx-sb-list">
            <button class="fx-sb-item fx-sb-project fx-active-repo" class:active={view === 'browse' && isCurrent} onclick={() => navigate(ap.path)} use:tooltip={`${ap.path} — open in active tab`}>
              <span class="fx-sb-ico"><Box size={14} /></span><span class="fx-sb-text">{ap.display_name}</span><span class="fx-sb-dot" aria-hidden="true"></span>
            </button>
          </div>
        {/if}
        {#each wsGroups as ws (ws.id)}
          {@const expanded = isWsExpanded(ws.id)}
          <button class="fx-ws-header" class:active={ws.isActive} class:synthetic={ws.synthetic} onclick={() => toggleWs(ws.id)} aria-expanded={expanded}>
            <span class="fx-ws-chev">{#if expanded}<ChevronDown size={11} strokeWidth={2.2} />{:else}<ChevronRight size={11} strokeWidth={2.2} />{/if}</span>
            <span class="fx-ws-name">{ws.name}</span><span class="fx-ws-count">{ws.repos.length}</span>
          </button>
          {#if expanded}
            <div class="fx-sb-list fx-ws-list">
              {#each ws.repos as p (p.id)}
                {@const isActiveRepo = activeRepoPath != null && normPath(p.path) === normPath(activeRepoPath)}
                {@const isCurrent = normPath(currentPath) === normPath(p.path) || normPath(currentPath).startsWith(normPath(p.path) + '/')}
                <button class="fx-sb-item fx-sb-project" class:fx-active-repo={isActiveRepo} class:active={view === 'browse' && isCurrent} onclick={() => navigate(p.path)} use:tooltip={isActiveRepo ? `${p.path} — open in active tab` : p.path}>
                  <span class="fx-sb-ico">{#if isActiveRepo}<Box size={14} />{:else}<GitBranch size={14} />{/if}</span>
                  <span class="fx-sb-text">{p.display_name}</span>{#if isActiveRepo}<span class="fx-sb-dot" aria-hidden="true"></span>{/if}
                </button>
              {/each}
            </div>
          {/if}
        {/each}
      {/if}
    {/if}
  {/if}
{/snippet}

{#snippet gitBadge(b: GitBadge)}
  <span class="fx-badge fx-badge-{b}" use:tooltip={GIT_BADGE_LABEL[b]}>{GIT_BADGE_GLYPH[b]}</span>
{/snippet}

<!-- Details-view chip: marks a folder that is a git repo root, with its branch
     and (when registered in Arbor) the owning workspaces as coloured dots. -->
{#snippet repoChip(info: RepoFolderInfo)}
  <span class="fx-repo-chip" class:registered={info.registered} use:tooltip={repoTip(info)}>
    <GitBranch size={11} />
    {#if info.marker?.branch}<span class="fx-repo-branch">{info.marker.detached ? 'detached' : info.marker.branch}</span>{/if}
    {#each info.workspaces.slice(0, 4) as ws (ws.id)}
      <span class="fx-ws-dot" style="background: {workspaceColorVar(ws.color_idx)}"></span>
    {/each}
  </span>
{/snippet}

<!-- Grid-view corner overlay: compact repo-root marker (top-left, clear of the
     bottom-right status badge). Accent-filled when registered in Arbor. -->
{#snippet repoOverlay(info: RepoFolderInfo)}
  <span class="fx-repo-ov" class:registered={info.registered} use:tooltip={repoTip(info)}><GitBranch size={9} /></span>
{/snippet}

<!-- One row in the Changes panel: status glyph + filename + muted parent dir.
     Click jumps to the file in the list (navigating into its folder if needed). -->
{#snippet changeRow(ch: GitChange)}
  {@const parts = splitRel(ch.rel)}
  <button class="fx-ch-row" type="button" onclick={() => revealPath(ch.path)} use:tooltip={{ content: ch.rel, description: GIT_BADGE_LABEL[ch.badge], placement: 'left' }}>
    <span class="fx-ch-badge fx-badge-{ch.badge}">{GIT_BADGE_GLYPH[ch.badge] || '•'}</span>
    <span class="fx-ch-text"><span class="fx-ch-name">{parts[1]}</span>{#if parts[0]}<span class="fx-ch-dir">{parts[0]}</span>{/if}</span>
  </button>
{/snippet}

{#snippet railButtons()}
    <button class="ab-btn" class:ab-active={rightPanel === 'preview'} aria-pressed={rightPanel === 'preview'}
            onclick={() => rightPanel = rightPanel === 'preview' ? null : 'preview'}
            use:tooltip={{ content: 'Preview', description: 'Render the selected file', placement: 'left' }} aria-label="Preview"><Eye size={20} /></button>
    <button class="ab-btn" class:ab-active={rightPanel === 'info'} aria-pressed={rightPanel === 'info'}
            onclick={() => rightPanel = rightPanel === 'info' ? null : 'info'}
            use:tooltip={{ content: 'Info', description: 'File details', placement: 'left' }} aria-label="Info"><Info size={20} /></button>
    {#if inRepo}
      <button class="ab-btn" class:ab-active={rightPanel === 'changes'} aria-pressed={rightPanel === 'changes'}
              onclick={() => rightPanel = rightPanel === 'changes' ? null : 'changes'}
              use:tooltip={{ content: 'Changes', description: 'Staged & unstaged files', placement: 'left' }} aria-label="Changes"><GitCompare size={20} /></button>
    {/if}
{/snippet}

{#snippet headerNavButtons()}
      <div class="fx-nav-btns">
        <button class="fx-nav-btn" onclick={goBack}    disabled={!canBack}    use:tooltip={{ content: 'Back', shortcut: 'Alt+←' }} aria-label="Back"><ArrowLeft size={14} /></button>
        <button class="fx-nav-btn" onclick={goForward} disabled={!canForward} use:tooltip={{ content: 'Forward', shortcut: 'Alt+→' }} aria-label="Forward"><ArrowRight size={14} /></button>
        <button class="fx-nav-btn" onclick={goUp}      disabled={!canUp}      use:tooltip={{ content: 'Up', shortcut: 'Backspace' }} aria-label="Up"><ArrowUp size={14} /></button>
      </div>
{/snippet}

{#snippet headerAddress()}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div class="fx-address" class:editing={addressEditing} onclick={startAddressEdit} role="button" tabindex="-1"
           onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); startAddressEdit(); } }}
           use:tooltip={{ content: 'Edit path', description: 'Type a path with Tab autocomplete', shortcut: 'Ctrl+L', delay: 1200, disabled: addressEditing || view === 'overview' }}>
        {#if addressEditing}
          <div class="fx-addr-wrap">
            <input id="fx-addr" class="fx-addr-input" type="text" bind:value={addressInput} onblur={commitAddress} autocomplete="off" spellcheck="false"
                   onkeydown={(e) => { e.stopPropagation();
                     if (e.key === 'Enter') { e.preventDefault(); commitAddress(); return; }
                     if (e.key === 'Escape') { addressEditing = false; return; }
                     if (e.key === 'Tab' && addressGhost) { e.preventDefault(); completeAddress(); return; }
                     if (e.key === 'ArrowRight' && addressGhost) { const i = e.currentTarget as HTMLInputElement; if (i.selectionStart === addressInput.length) { e.preventDefault(); completeAddress(); } }
                   }} />
            {#if addressGhost}
              <span class="fx-addr-ghost" aria-hidden="true"><span class="fx-ghost-typed">{addressInput}</span><span class="fx-ghost-suffix">{addressGhost}</span></span>
              <kbd class="fx-addr-tab">Tab</kbd>
            {/if}
          </div>
        {:else if view === 'overview'}
          <span class="fx-crumb-single">Overview</span>
        {:else if view === 'settings'}
          <span class="fx-crumb-single">Settings</span>
        {:else}
          <div class="fx-breadcrumb">
            {#each breadcrumbs as crumb, i (crumb.path)}
              {#if i > 0}<span class="fx-crumb-sep"><ChevronRight size={10} /></span>{/if}
              <button class="fx-crumb-item" onclick={(e) => { e.stopPropagation(); navigate(crumb.path); }}>{crumb.label}</button>
            {/each}
          </div>
        {/if}
      </div>
{/snippet}

{#snippet headerActions()}
        {#if view === 'browse'}
          <Dropdown items={viewItems} position="fixed" width="200px">
            {#snippet trigger({ open, toggle })}
              {@const CurIco = currentView.icon}
              <button class="fx-icon-btn fx-view-btn" class:active={open} aria-haspopup="menu" aria-expanded={open}
                      onclick={toggle} use:tooltip={'Change view'} aria-label="Change view">
                <CurIco size={13} /><ChevronDown size={10} class="fx-view-caret" />
              </button>
            {/snippet}
          </Dropdown>
        {/if}
        <button class="fx-icon-btn" onclick={refresh} disabled={view !== 'browse'} use:tooltip={'Refresh'} aria-label="Refresh"><RefreshCw size={13} class={loading ? 'spin' : ''} /></button>
{/snippet}

{#snippet bodyContent()}
  <div class="fx-root">
  <div class="fx-body">
    <!-- ══ Sidebar ══ -->
    <aside class="fx-sidebar" class:collapsed={sidebarCollapsed}>
      <div class="fx-sb-top">
        {#if !sidebarCollapsed}<span class="fx-sb-title">Locations</span>{/if}
        <ModalSidebarToggle collapsed={sidebarCollapsed} onToggle={() => sidebarCollapsed = !sidebarCollapsed} />
      </div>

      {#each orderedSidebar as s (s.id)}
        {#if s.visible}
          {#if s.id === 'library'}{@render secLibrary()}
          {:else if s.id === 'recents'}{@render secRecents()}
          {:else if s.id === 'favourites'}{@render secFavourites()}
          {:else if s.id === 'devices'}{@render secDevices()}
          {:else if s.id === 'projects'}{@render secProjects()}
          {/if}
        {/if}
      {/each}
    </aside>

    <!-- ══ Main ══ -->
    <div class="fx-main" class:fx-narrow={previewBig}>
      <div class="fx-tabbar">
        <Tabs items={tabItems} value={activeTabId} variant="panel" overflow onSelect={switchTab} onClose={closeTab} onAdd={addTab} addLabel="New tab" ariaLabel="Explorer tabs" />
      </div>

      {#if view === 'overview'}
        <div class="fx-overview">
          <div class="fx-stat-row">
            {#each overviewStats as s (s.label)}
              <Card variant="flat" padding="md" class="fx-stat"><div class="fx-stat-val">{s.value}</div><div class="fx-stat-label">{s.label}</div></Card>
            {/each}
            <Card variant="flat" padding="md" class="fx-stat fx-stat-files"><div class="fx-stat-val">{totalFiles.toLocaleString('en-US')}</div><div class="fx-stat-label">Total files</div></Card>
          </div>
          <section class="fx-section">
            <h3 class="fx-h3">File kinds <span class="fx-demo">demo data</span></h3>
            <div class="fx-bar" role="img" aria-label="File kind breakdown">
              {#each kindBreakdown as k (k.kind)}<div class="fx-bar-seg" style="flex: {k.count}; background: {k.color};" use:tooltip={`${k.kind}: ${k.count.toLocaleString('en-US')}`}></div>{/each}
            </div>
            <div class="fx-legend">
              {#each kindBreakdown as k (k.kind)}<span class="fx-legend-item"><span class="fx-legend-dot" style="background: {k.color};"></span>{k.kind}<span class="fx-legend-count">{k.count.toLocaleString('en-US')}</span></span>{/each}
            </div>
          </section>
          {#if drives.length}
            <section class="fx-section"><h3 class="fx-h3">Devices</h3>
              <div class="fx-grid">{#each drives as d (d.path)}<Card variant="flat" padding="md" hoverable class="fx-tile"><button class="fx-loc-btn" onclick={() => navigate(d.path)}><span class="fx-tile-ico"><HardDrive size={18} /></span><span class="fx-tile-col"><span class="fx-tile-name">{d.name}</span><span class="fx-tile-sub">Local Disk</span></span></button></Card>{/each}</div>
            </section>
          {/if}
          {#if favourites.length}
            <section class="fx-section"><h3 class="fx-h3">Locations</h3>
              <div class="fx-grid">{#each favourites as loc (loc.path)}{@const I = rootIcon(loc.kind)}<Card variant="flat" padding="md" hoverable class="fx-tile"><button class="fx-loc-btn" onclick={() => navigate(loc.path)}><span class="fx-tile-ico"><I size={18} /></span><span class="fx-tile-col"><span class="fx-tile-name">{loc.name}</span><span class="fx-tile-sub">{loc.path}</span></span></button></Card>{/each}</div>
            </section>
          {/if}
        </div>
      {:else if view === 'settings'}
        <FileExplorerSettings
          onExit={exitSettings}
          onResetViewMemory={resetViewMemory}
          onResetRecents={resetRecents}
          onResetLayout={resetLayout}
          viewMemoryCount={viewByPath.size}
          recentsCount={recents.length} />
      {:else}
        <!-- ── Browser ── -->
        <div class="fx-filter-row">
          <Search size={11} class="fx-filter-ico" />
          <input class="fx-filter-input" type="text" placeholder={recursiveSearch ? 'Search subfolders — wildcards * ?' : 'Filter files — wildcards * ?'} bind:value={filterQuery} spellcheck="false" autocomplete="off"
                 onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Escape' && filterQuery) { filterQuery = ''; e.preventDefault(); } }} />
          {#if filterQuery}<button class="fx-filter-clear" onclick={() => filterQuery = ''} aria-label="Clear filter" use:tooltip={'Clear filter'}><X size={11} /></button>{/if}
          <span class="fx-filter-divider"></span>
          <button class="fx-toggle" class:active={recursiveSearch} aria-pressed={recursiveSearch} onclick={() => explorerStore.setRecursiveSearch(!recursiveSearch)}
                  use:tooltip={{ content: recursiveSearch ? 'Recursive search on' : 'Recursive search off', description: 'Search inside all subfolders' }}><FolderSearch size={13} /></button>
          <button class="fx-toggle" class:active={showHidden} aria-pressed={showHidden} onclick={() => explorerStore.setShowHidden(!showHidden)}
                  use:tooltip={{ content: showHidden ? 'Hide hidden files' : 'Show hidden files', description: 'Files starting with a dot' }}><Eye size={13} /></button>
        </div>

        {#if !isGrid}
          <div class="fx-col-head">
            <div class="fx-col fx-col-name"><button class="fx-ch" onclick={() => toggleSort('name')}>Name {#if sortKey === 'name'}<span class="fx-sort">{sortAsc ? '↑' : '↓'}</span>{/if}</button></div>
            <div class="fx-col fx-col-date"><button class="fx-ch" onclick={() => toggleSort('modified')}>Date modified {#if sortKey === 'modified'}<span class="fx-sort">{sortAsc ? '↑' : '↓'}</span>{/if}</button></div>
            <div class="fx-col fx-col-type"><span class="fx-ch-static">Type</span></div>
            <div class="fx-col fx-col-size"><button class="fx-ch fx-ch-right" onclick={() => toggleSort('size')}>Size {#if sortKey === 'size'}<span class="fx-sort">{sortAsc ? '↑' : '↓'}</span>{/if}</button></div>
          </div>
        {/if}

        {#if createKind}
          <div class="fx-row fx-create-row" style="height: {VS_ROW}px;">
            <div class="fx-col fx-col-name">
              <span class="fx-entry-ico"><Icon icon={entryIcon(createKind === 'folder' ? '' : (createName || 'untitled'), createKind === 'folder')} width={16} height={16} /></span>
              <input id="fx-create" class="fx-inline" type="text" bind:value={createName} onclick={(e) => e.stopPropagation()} onblur={commitCreate}
                     onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') commitCreate(); if (e.key === 'Escape') cancelCreate(); }} />
            </div>
            <div class="fx-col fx-col-date"></div><div class="fx-col fx-col-type">{createKind === 'folder' ? 'Folder' : 'File'}</div><div class="fx-col fx-col-size"></div>
          </div>
        {/if}

        <div class="fx-list" class:fx-dragging={dragging} bind:this={listEl} bind:clientHeight={vsClient} bind:clientWidth={vsListW} role="presentation" onscroll={onScroll}
             oncontextmenu={(e) => { if (!(e.target as HTMLElement).closest('.fx-row')) openCtx(e, null); }}
             onclick={(e) => { if (consumeDragClick()) return; if (!(e.target as HTMLElement).closest('.fx-row')) clearSelection(); }}>
          {#if loading}
            <div class="fx-state"><Spinner size="sm" /> Loading…</div>
          {:else if loadError}
            <div class="fx-state error"><AlertCircle size={14} /> {loadError}</div>
          {:else if recursiveActive && searching && sorted.length === 0}
            <div class="fx-state"><Spinner size="sm" /> Searching subfolders…</div>
          {:else if sorted.length === 0}
            {#if filterQuery}<div class="fx-state"><Search size={14} /> No {recursiveActive ? 'matches' : 'entries'} for “{filterQuery}”<button class="fx-state-clear" onclick={() => filterQuery = ''}>Clear filter</button></div>
            {:else}<div class="fx-state">This folder is empty</div>{/if}
          {:else}
            <div class="fx-vs" style="height: {vsTotalH}px;">
              <div class="fx-vs-win" class:grid={isGrid} style="transform: translateY({vsOffset}px); {isGrid ? `grid-template-columns: repeat(${cols}, 1fr); grid-auto-rows: ${cellH}px;` : ''}">
                {#each vsItems as entry (entry.path)}
                  {@const nIco = isGrid ? null : nativeIconSrc(entry)}
                  {@const badge = inRepo ? badgeFor(entry) : null}
                  {@const repoInfo = repoInfoFor(entry)}
                  <div class="fx-row" class:fx-gi={isGrid} class:fx-ignored={badge === 'ignored'} style={isGrid ? '' : `height: ${VS_ROW}px;`}
                       data-path={entry.path} data-dir={entry.is_dir}
                       class:selected={selected.has(entry.path)} class:lead={lead === entry.path} class:cut={cutSet.has(entry.path)} class:drop-target={dragOverDir === entry.path}
                       onmousedown={(e) => startRowDrag(e, entry)}
                       onclick={(ev) => clickEntry(entry, ev)} ondblclick={() => openEntry(entry)} oncontextmenu={(e) => openCtx(e, entry)}
                       role="option" aria-selected={selected.has(entry.path)} tabindex="0"
                       onkeydown={(e) => { if (e.key === 'Enter') openEntry(entry); if (e.key === 'F2') startRename(entry); }}>
                    {#if isGrid}
                      {@const showThumb = thumbsOn && !entry.is_dir && isImage(entry.name)}
                      <span class="fx-gi-img" class:thumb={showThumb} style={showThumb ? `width:${tile.thumb}px;height:${tile.thumb}px` : ''}>
                        {#if showThumb}<img class="fx-gi-thumb" src={convertFileSrc(entry.path)} alt="" loading="lazy" draggable="false" />
                        {:else}<Icon icon={entryIcon(entry.name, entry.is_dir)} width={tile.ico} height={tile.ico} />{/if}
                        {#if badge && badge !== 'ignored'}{@render gitBadge(badge)}{/if}
                        {#if repoInfo}{@render repoOverlay(repoInfo)}{/if}
                      </span>
                      {#if renamingPath === entry.path}
                        <input id="fx-rename" class="fx-inline fx-inline-grid" type="text" bind:value={renameValue} onclick={(e) => e.stopPropagation()} onblur={commitRename}
                               onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') cancelRename(); }} />
                      {:else}
                        <span class="fx-gi-label" use:tooltip={recursiveActive ? entry.path : entry.name}>{entry.name}</span>
                      {/if}
                    {:else}
                      <div class="fx-col fx-col-name">
                        <span class="fx-entry-ico">
                          {#if nIco}<img class="fx-native-ico" src={nIco} alt="" draggable="false" />
                          {:else}<Icon icon={entryIcon(entry.name, entry.is_dir)} width={16} height={16} />{/if}
                          {#if badge && badge !== 'ignored'}{@render gitBadge(badge)}{/if}
                        </span>
                        {#if renamingPath === entry.path}
                          <input id="fx-rename" class="fx-inline" type="text" bind:value={renameValue} onclick={(e) => e.stopPropagation()} onblur={commitRename}
                                 onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') cancelRename(); }} />
                        {:else}
                          <span class="fx-entry-name" use:tooltip={entry.name}>{entry.name}</span>
                          {#if repoInfo}{@render repoChip(repoInfo)}{/if}
                          {#if recursiveActive}
                            {@const loc = relLocation(entry.path)}
                            {#if loc}<span class="fx-entry-loc" use:tooltip={entry.path}>{loc}</span>{/if}
                          {/if}
                        {/if}
                      </div>
                      <div class="fx-col fx-col-date">{entry.modified != null ? formatDate(entry.modified) : ''}</div>
                      <div class="fx-col fx-col-type">{entry.is_dir ? 'Folder' : (extOf(entry.name).toUpperCase() || 'File') + ' file'}</div>
                      <div class="fx-col fx-col-size">{!entry.is_dir && entry.size != null ? formatSize(entry.size) : ''}</div>
                    {/if}
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- ══ Right rail panel (Preview / Info / Changes) ══ -->
    {#if rightPanel && view === 'browse'}
      <aside class="fx-preview" class:fx-expanded={previewExpanded} style={previewExpanded ? '' : `width: ${previewWidth}px`}>
        {#if !previewExpanded}
          <!-- svelte-ignore a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
          <div class="fx-pv-resize" onmousedown={startPreviewResize} role="separator" aria-orientation="vertical" aria-label="Resize preview"></div>
        {/if}
        <div class="fx-pv-head">
          <span class="fx-pv-head-title">{rightPanel === 'preview' ? 'Preview' : rightPanel === 'changes' ? 'Changes' : 'Info'}</span>
          <button class="fx-pv-expand" onclick={() => previewExpanded = !previewExpanded}
                  use:tooltip={previewExpanded ? 'Restore' : 'Expand'} aria-label="Toggle expand" aria-pressed={previewExpanded}>
            {#if previewExpanded}<Minimize2 size={13} />{:else}<Maximize2 size={13} />{/if}
          </button>
        </div>
        {#if rightPanel === 'changes'}
          {#if !gitChanges?.repo_root}
            <div class="fx-pv-empty"><GitCompare size={22} /><span>Not a git repository</span><small>This folder isn't inside a repo</small></div>
          {:else if changeCount === 0}
            <div class="fx-pv-empty"><CheckCircle2 size={22} /><span>Working tree clean</span><small>No staged or unstaged changes</small></div>
          {:else}
            {@const gc = gitChanges!}
            <div class="fx-ch">
              {#if gc.staged.length}
                <div class="fx-ch-group-head">Staged <span class="fx-ch-count">{gc.staged.length}</span></div>
                {#each gc.staged as ch (ch.path + ':s')}{@render changeRow(ch)}{/each}
              {/if}
              {#if gc.unstaged.length}
                <div class="fx-ch-group-head">Unstaged <span class="fx-ch-count">{gc.unstaged.length}</span></div>
                {#each gc.unstaged as ch (ch.path + ':u')}{@render changeRow(ch)}{/each}
              {/if}
            </div>
          {/if}
          {#if gitChanges?.repo_root}
            <div class="fx-info-actions">
              <button type="button" class="fx-info-link" onclick={() => fsOpenInArbor(gitChanges!.repo_root!).catch(err => uiStore.showToast(`${err}`, 'error'))}>
                <GitBranch size={13} /> Open in Arbor
              </button>
            </div>
          {/if}
        {:else if !leadEntry}
          <div class="fx-pv-empty"><FileText size={22} /><span>No selection</span><small>Select an item from the list</small></div>
        {:else if rightPanel === 'preview'}
          {@const e = leadEntry}
          <div class="fx-pv-view">
            {#if previewLoading}
              <div class="fx-pv-loading"><Spinner size="md" /><span>Loading preview…</span></div>
            {:else if preview?.kind === 'image'}
              <img class="fx-pv-image" src={preview.src} alt={e.name} loading="lazy" />
            {:else if preview?.kind === 'video'}
              <!-- svelte-ignore a11y_media_has_caption -->
              <video class="fx-pv-media" src={preview.src} controls preload="metadata"></video>
            {:else if preview?.kind === 'audio'}
              <div class="fx-pv-audio">
                <span class="fx-pv-bigico"><Icon icon={entryIcon(e.name, false)} width={44} height={44} /></span>
                <audio class="fx-pv-audio-el" src={preview.src} controls preload="metadata"></audio>
              </div>
            {:else if preview?.kind === 'text'}
              <pre class="fx-pv-code"><code>{@html highlightCode(preview.text, preview.lang)}</code></pre>
            {:else if preview?.kind === 'folder'}
              <div class="fx-pv-folder">{#each preview.items as it (it.path)}<div class="fx-pv-folder-item"><span class="fx-pv-folder-ico"><Icon icon={entryIcon(it.name, it.is_dir)} width={14} height={14} /></span><span class="fx-pv-folder-name">{it.name}</span></div>{:else}<div class="fx-pv-noprev"><span>Empty folder</span></div>{/each}</div>
            {:else}
              <div class="fx-pv-noprev"><span class="fx-pv-bigico"><Icon icon={entryIcon(e.name, e.is_dir)} width={52} height={52} /></span><span>No preview available</span></div>
            {/if}
          </div>
          <div class="fx-pv-name" title={e.name}>{e.name}</div>
        {:else}
          {@const e = leadEntry}
          <div class="fx-info-thumb">
            {#if isImage(e.name)}<img class="fx-info-img" src={convertFileSrc(e.path)} alt={e.name} loading="lazy" />
            {:else}<Icon icon={entryIcon(e.name, e.is_dir, true)} width={44} height={44} />{/if}
          </div>
          <div class="fx-pv-name" title={e.name}>{e.name}</div>
          <dl class="fx-pv-meta">
            <div class="fx-pv-row"><dt>Type</dt><dd>{e.is_dir ? 'Folder' : (extOf(e.name).toUpperCase() || 'File') + ' file'}</dd></div>
            {#if !e.is_dir && e.size != null}<div class="fx-pv-row"><dt>Size</dt><dd>{formatSize(e.size)}</dd></div>{/if}
            {#if e.modified != null}<div class="fx-pv-row"><dt>Modified</dt><dd>{formatDate(e.modified)}</dd></div>{/if}
            <div class="fx-pv-row fx-pv-path"><dt>Path</dt><dd>{e.path}</dd></div>
          </dl>
          {@const repoInfo = repoInfoFor(e)}
          {#if repoInfo}
            <div class="fx-info-git">
              <div class="fx-info-git-head"><GitBranch size={12} /> {repoInfo.registered ? 'Git project' : 'Git repository'}</div>
              <dl class="fx-pv-meta">
                {#if repoInfo.marker?.branch}
                  <div class="fx-pv-row"><dt>Branch</dt><dd>{repoInfo.marker.detached ? 'detached HEAD' : repoInfo.marker.branch}</dd></div>
                {/if}
                <div class="fx-pv-row"><dt>In Arbor</dt><dd>{repoInfo.registered ? 'Registered' : 'Not registered'}</dd></div>
              </dl>
              {#if repoInfo.registered}
                {#if repoInfo.workspaces.length}
                  <div class="fx-info-ws">
                    {#each repoInfo.workspaces as ws (ws.id)}
                      <span class="fx-ws-chip"><span class="fx-ws-dot" style="background: {workspaceColorVar(ws.color_idx)}"></span>{ws.name}</span>
                    {/each}
                  </div>
                {:else}
                  <div class="fx-info-ws-empty">In no workspace</div>
                {/if}
              {/if}
              <button type="button" class="fx-info-link" onclick={() => fsOpenInArbor(e.path).catch(err => uiStore.showToast(`${err}`, 'error'))}>
                <GitBranch size={13} /> Open in Arbor
              </button>
            </div>
          {/if}
          <div class="fx-info-actions">
            <button type="button" class="fx-info-link" onclick={() => openSystemProperties(e.path)}>
              <Settings2 size={13} /> System properties…
            </button>
          </div>
        {/if}
      </aside>
    {/if}
  </div>
  </div>
{/snippet}

{#snippet footerBody()}
    {#if deleteReq}
      <ModalFooter align="between">
        <span class="fx-del-confirm">
          <Trash2 size={13} class="fx-del-ico" />
          {#if deleteReq.permanent}Permanently delete <strong>{deleteReq.paths.length}</strong> item{deleteReq.paths.length !== 1 ? 's' : ''}? This cannot be undone.
          {:else}Move <strong>{deleteReq.paths.length}</strong> item{deleteReq.paths.length !== 1 ? 's' : ''} to the Recycle Bin?{/if}
        </span>
        <span class="fx-foot-actions">
          <Button variant="ghost" size="sm" onclick={() => deleteReq = null}>Cancel</Button>
          <Button variant="danger" size="sm" onclick={confirmDelete}>{deleteReq.permanent ? 'Delete' : 'Move to Recycle Bin'}</Button>
        </span>
      </ModalFooter>
    {:else if view === 'browse'}
      <ModalFooter align="between">
        <span class="fx-foot-info">{footerInfo}{#if clipboard}<span class="fx-clip"> · {clipboard.op === 'cut' ? 'Cut' : 'Copied'} {clipboard.paths.length}</span>{/if}</span>
        <span class="fx-foot-right">
          {#if gitStatus?.in_repo && gitStatus.branch}
            <button type="button" class="fx-foot-branch" class:active={!!branchPopup}
                    onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); void openBranchSwitch(gitStatus?.repo_root ?? currentPath, r.left, r.top); }}
                    use:tooltip={`On branch ${gitStatus.branch}${gitStatus.ahead ? ` · ${gitStatus.ahead} ahead` : ''}${gitStatus.behind ? ` · ${gitStatus.behind} behind` : ''} — click to switch branch`}>
              <GitBranch size={11} />{gitStatus.detached ? 'detached' : gitStatus.branch}{#if gitStatus.ahead}<span class="fx-ab">↑{gitStatus.ahead}</span>{/if}{#if gitStatus.behind}<span class="fx-ab">↓{gitStatus.behind}</span>{/if}
            </button>
          {/if}
          <span class="fx-foot-path">{currentPath}</span>
        </span>
      </ModalFooter>
    {:else}
      <ModalFooter align="between">
        <span class="fx-foot-info">Built-in explorer</span>
        <span class="fx-foot-path">{drives.length} device{drives.length !== 1 ? 's' : ''} · {favourites.length} location{favourites.length !== 1 ? 's' : ''}</span>
      </ModalFooter>
    {/if}
{/snippet}

{#if standalone}
  <!-- Dedicated window: frameless titlebar + WindowControls, body fills the OS window. -->
  <div class="fx-win">
    <header class="fx-win-bar" data-tauri-drag-region>
      <div class="fx-win-island fx-win-left">{@render headerNavButtons()}</div>
      <!-- Center zone: drag-region wrapper, address bar centered + capped. -->
      <div class="fx-win-center" data-tauri-drag-region>
        <div class="fx-win-addr">{@render headerAddress()}</div>
      </div>
      <div class="fx-win-island fx-win-right">{@render headerActions()}</div>
      <WindowControls />
    </header>
    <div class="fx-win-mid">
      <div class="fx-win-body">{@render bodyContent()}</div>
      {#if view === 'browse'}
        <ActivityBar side="right" ariaLabel="Explorer tool rail">
          {#snippet top()}{@render railButtons()}{/snippet}
        </ActivityBar>
      {/if}
    </div>
    <footer class="fx-win-foot">{@render footerBody()}</footer>
  </div>
{:else}
  <Modal onClose={onClose} width="1240px" height="720px" padBody={false} bodyBorder={false} topGap
         showRightRail={view === 'browse'} zIndex="var(--z-modal-picker)" ariaLabel="File Explorer">
    {#snippet rightRail()}{@render railButtons()}{/snippet}
    {#snippet header()}
      <ModalHeader onClose={onClose}>
        {@render headerNavButtons()}
        {@render headerAddress()}
        {#snippet actions()}{@render headerActions()}{/snippet}
      </ModalHeader>
    {/snippet}
    {@render bodyContent()}
    {#snippet footer()}{@render footerBody()}{/snippet}
  </Modal>
{/if}

<svelte:window onkeydown={onKeydown} />

{#if ctxMenu}
  <ContextMenu x={ctxMenu.x} y={ctxMenu.y} items={ctxItems(ctxMenu.entry)} actions={ctxActions(ctxMenu.entry)} onSelect={handleCtx} onClose={() => ctxMenu = null} />
{/if}

{#if sectionCtx}
  <ContextMenu x={sectionCtx.x} y={sectionCtx.y}
    items={[{ id: 'hide', label: `Hide “${sectionLabel(sectionCtx.id)}”`, icon: EyeOff }]}
    onSelect={(id) => { if (id === 'hide' && sectionCtx) hideSection(sectionCtx.id); sectionCtx = null; }}
    onClose={() => sectionCtx = null} />
{/if}

{#if discardReq}
  <ConfirmModal
    title="Discard changes?"
    message={`Discard local changes to ${discardReq.length} item${discardReq.length !== 1 ? 's' : ''}?`}
    detail="Modified files revert to the last commit and untracked files are deleted. A snapshot is saved to Arbor's Recovery tab so this can be undone."
    variant="danger"
    confirmLabel="Discard"
    busy={discardBusy}
    zIndex="var(--z-menu)"
    onConfirm={confirmDiscard}
    onCancel={() => { if (!discardBusy) discardReq = null; }}
  />
{/if}

{#if branchPopup}
  <BranchSwitchPopup
    x={branchPopup.x} y={branchPopup.y} branches={branchPopup.branches} busy={branchBusy}
    onSelect={doCheckout}
    onClose={() => { if (!branchBusy) branchPopup = null; }}
  />
{/if}

{#if dragGhost}
  <div class="fx-drag-ghost" style="left: {dragGhost.x}px; top: {dragGhost.y}px;">{dragGhost.label}</div>
{/if}

<style>
  /* ══ Standalone window shell (dedicated explorer window) ══ */
  /* Mirrors the modal's chrome rhythm: bg-elevated frame, body floats as a
     bg-base card with the same 4px insets that expose the rounded corners. */
  /* NO `overflow: hidden` here. On WebView2/Chromium a `position:fixed` flex
     column with `overflow:hidden` collapses its flex children (header + body)
     to ~0 height on some relayouts — e.g. the re-render after a settings
     toggle. It isn't needed at this level: the inner panels (.fx-win-mid /
     .fx-win-body) clip their own overflow and the page never scrolls. */
  .fx-win { position: fixed; inset: 0; display: flex; flex-direction: column; background: var(--bg-elevated); }
  .fx-win-bar { display: flex; align-items: center; gap: 8px; height: 38px; flex-shrink: 0; padding-left: 10px; }
  /* Interactive islands opt out of the titlebar drag region. */
  .fx-win-island { display: flex; align-items: center; gap: 8px; flex-shrink: 0; -webkit-app-region: no-drag; }
  /* Center zone fills the space between the left/right islands; the address
     bar sits centered inside it, capped at 70% of that span. The uncovered
     30% (and the zone's padding) stays draggable. */
  .fx-win-center { flex: 1 1 auto; min-width: 0; align-self: stretch; display: flex; align-items: center; justify-content: center; }
  .fx-win-addr { width: 70%; min-width: 0; max-width: 100%; display: flex; -webkit-app-region: no-drag; }
  .fx-win-mid { flex: 1; min-height: 0; display: flex; flex-direction: row; align-items: stretch; overflow: hidden; }
  .fx-win-body { flex: 1; min-width: 0; min-height: 0; margin: 0 4px 4px; overflow: hidden; }
  .fx-win-foot { display: flex; align-items: center; padding: var(--modal-footer-padding); background: var(--modal-chrome-bg); flex-shrink: 0; }

  /* ══ Header chrome ══ */
  .fx-nav-btns { display: inline-flex; gap: 2px; flex-shrink: 0; }
  .fx-nav-btn, .fx-icon-btn { display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; background: transparent; border: none; border-radius: var(--radius-sm); color: var(--text-secondary); cursor: pointer; transition: background var(--transition-fast), color var(--transition-fast); }
  .fx-nav-btn:hover:not(:disabled), .fx-icon-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .fx-nav-btn:disabled, .fx-icon-btn:disabled { opacity: 0.3; cursor: default; }

  .fx-address { flex: 1; min-width: 0; height: 26px; background: var(--bg-input); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); padding: 0 10px; display: flex; align-items: center; overflow: hidden; cursor: text; transition: border-color var(--transition-fast); }
  .fx-address:hover { border-color: var(--border); }
  .fx-address.editing { border-color: var(--border-focus); }
  .fx-crumb-single { font-size: 12px; color: var(--text-secondary); }
  .fx-breadcrumb { display: flex; align-items: center; overflow-x: auto; white-space: nowrap; width: 100%; scrollbar-width: none; }
  .fx-breadcrumb::-webkit-scrollbar { display: none; }
  .fx-crumb-item { background: transparent; border: none; cursor: pointer; font-family: var(--font-ui-sans); font-size: 12px; color: var(--text-secondary); padding: 0 3px; border-radius: var(--radius-sm); white-space: nowrap; flex-shrink: 0; transition: color var(--transition-fast), background var(--transition-fast); }
  .fx-crumb-item:hover { color: var(--text-primary); background: var(--bg-hover); }
  .fx-crumb-sep { color: var(--text-disabled); display: flex; align-items: center; flex-shrink: 0; }
  /* Address edit + ghost autocomplete */
  .fx-addr-wrap { position: relative; flex: 1; min-width: 0; display: flex; align-items: center; height: 100%; }
  .fx-addr-input { width: 100%; background: transparent; border: none; outline: none; color: var(--text-primary); font-family: var(--font-code); font-size: 12px; position: relative; z-index: 1; }
  .fx-addr-ghost { position: absolute; left: 0; top: 50%; transform: translateY(-50%); font-family: var(--font-code); font-size: 12px; line-height: 1; pointer-events: none; white-space: pre; z-index: 0; overflow: hidden; max-width: 100%; }
  .fx-ghost-typed { color: transparent; }
  .fx-ghost-suffix { color: var(--text-disabled); }
  .fx-addr-tab { position: absolute; right: 4px; top: 50%; transform: translateY(-50%); font-size: 9.5px; line-height: 1; padding: 2px 5px; background: var(--bg-overlay); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); color: var(--text-muted); pointer-events: none; font-family: var(--font-ui-sans); }

  /* ══ Body ══ */
  .fx-root { height: 100%; display: flex; flex-direction: column; overflow: hidden; }
  .fx-body { flex: 1; display: flex; overflow: hidden; background: var(--bg-elevated); gap: 6px; }

  /* ── Sidebar ── */
  .fx-sidebar { width: 196px; flex-shrink: 0; background: var(--bg-base); border-radius: var(--radius-lg); overflow-y: auto; padding-bottom: 8px; display: flex; flex-direction: column; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; transition: width var(--anim-dur-panel, 180ms) cubic-bezier(.16,1,.3,1); }
  .fx-sidebar.collapsed { width: 46px; }
  .fx-sb-top { display: flex; align-items: center; justify-content: space-between; height: 34px; padding: 0 8px 0 12px; box-sizing: border-box; border-bottom: 1px solid var(--border-subtle); flex-shrink: 0; }
  .fx-sb-title { font-size: 10px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-muted); user-select: none; white-space: nowrap; overflow: hidden; }
  .fx-sidebar.collapsed .fx-sb-top { padding: 0; justify-content: center; }

  .fx-sb-label { display: flex; align-items: center; gap: 5px; width: 100%; height: 24px; padding: 0 10px 0 8px; margin-top: 6px; background: transparent; border: none; cursor: pointer; font-family: var(--font-ui-sans); font-size: 10px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-disabled); user-select: none; text-align: left; transition: color var(--transition-fast); }
  .fx-sb-label:hover { color: var(--text-muted); }
  .fx-sb-label.rail { justify-content: center; padding: 0; cursor: default; }
  .fx-sb-chev { display: inline-flex; align-items: center; justify-content: center; width: 12px; flex-shrink: 0; color: var(--text-disabled); }
  :global(.fx-sb-label-ico) { color: var(--text-disabled); flex-shrink: 0; }
  .fx-sb-label-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .fx-sb-list { display: flex; flex-direction: column; gap: 1px; padding: 2px 6px; }
  .fx-sidebar.collapsed .fx-sb-list { padding: 2px 4px; }
  .fx-sb-item { display: flex; align-items: center; gap: 8px; width: 100%; height: 26px; padding: 0 8px 0 16px; background: transparent; border: none; border-radius: var(--radius-sm); cursor: pointer; font-family: var(--font-ui-sans); font-size: 12.5px; color: var(--text-secondary); text-align: left; overflow: hidden; white-space: nowrap; transition: background var(--transition-fast), color var(--transition-fast); }
  .fx-sidebar.collapsed .fx-sb-item { padding: 0; justify-content: center; gap: 0; }
  .fx-sidebar.collapsed .fx-sb-text { display: none; }
  .fx-sb-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .fx-sb-item.active { background: var(--accent-subtle); color: var(--accent); }
  .fx-sb-ico { display: flex; align-items: center; justify-content: center; width: 16px; height: 16px; flex-shrink: 0; color: var(--text-muted); }
  .fx-sb-item:hover .fx-sb-ico { color: var(--text-secondary); }
  .fx-sb-item.active .fx-sb-ico { color: var(--accent); }
  .fx-sb-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0; }
  .fx-sb-project { position: relative; }
  .fx-sb-project.fx-active-repo { background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--text-primary); }
  .fx-sb-project.fx-active-repo .fx-sb-ico { color: var(--accent); }
  .fx-sb-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); flex-shrink: 0; margin-left: auto; }
  .fx-sidebar.collapsed .fx-sb-dot { position: absolute; top: 4px; right: 4px; margin-left: 0; }

  /* Workspace sub-headers */
  .fx-ws-header { display: flex; align-items: center; gap: 6px; width: 100%; padding: 4px 10px; background: transparent; color: var(--text-secondary); border: none; cursor: pointer; font-size: 11px; font-weight: 500; text-align: left; transition: background var(--transition-fast), color var(--transition-fast); }
  .fx-ws-header:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .fx-ws-header.active .fx-ws-name { color: var(--accent); }
  .fx-ws-header.synthetic { color: var(--text-muted); font-style: italic; }
  .fx-ws-chev { display: inline-flex; align-items: center; justify-content: center; width: 14px; flex-shrink: 0; color: var(--text-muted); }
  .fx-ws-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fx-ws-count { font-family: var(--font-code); font-size: 10px; color: var(--text-disabled); background: var(--bg-overlay); padding: 1px 6px; border-radius: 8px; flex-shrink: 0; }
  .fx-ws-list { padding-left: 14px; }

  /* ── Main ── */
  .fx-main { flex: 1; min-width: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--bg-base); border-radius: var(--radius-lg); }
  /* Expanded preview: the list narrows (Date/Type columns drop) and the
     preview takes the rest of the body. */
  .fx-main.fx-narrow { flex: 0 0 300px; }
  .fx-main.fx-narrow .fx-col-date, .fx-main.fx-narrow .fx-col-type { display: none; }
  .fx-tabbar { display: flex; align-items: stretch; height: 34px; padding: 0 4px; border-bottom: 1px solid var(--border-subtle); flex-shrink: 0; }
  .fx-tabbar :global(.tabs) { flex: 1; min-width: 0; }

  /* ══ Overview ══ */
  .fx-overview { flex: 1; overflow-y: auto; padding: 18px 20px 24px; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; }
  .fx-stat-row { display: flex; gap: 10px; flex-wrap: wrap; }
  :global(.fx-stat) { flex: 1; min-width: 120px; }
  .fx-stat-val { font-size: 20px; font-weight: 650; color: var(--text-primary); font-variant-numeric: tabular-nums; }
  .fx-stat-label { font-size: 11px; color: var(--text-muted); margin-top: 3px; }
  :global(.fx-stat-files) { border-color: var(--accent-subtle) !important; }
  .fx-section { margin-top: 22px; }
  .fx-h3 { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin: 0 0 10px; text-transform: uppercase; letter-spacing: 0.04em; display: flex; align-items: center; gap: 8px; }
  .fx-demo { font-size: 9px; font-weight: 600; color: var(--text-disabled); background: var(--bg-overlay); padding: 1px 6px; border-radius: var(--radius-sm); letter-spacing: 0.02em; }
  .fx-bar { display: flex; height: 12px; border-radius: 6px; overflow: hidden; gap: 2px; }
  .fx-bar-seg { min-width: 4px; transition: filter var(--transition-fast); }
  .fx-bar-seg:hover { filter: brightness(1.2); }
  .fx-legend { display: flex; flex-wrap: wrap; gap: 14px; margin-top: 12px; }
  .fx-legend-item { display: inline-flex; align-items: center; gap: 6px; font-size: 11.5px; color: var(--text-secondary); }
  .fx-legend-dot { width: 9px; height: 9px; border-radius: 2px; flex-shrink: 0; }
  .fx-legend-count { color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .fx-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 10px; }
  :global(.fx-tile) { min-width: 0; }
  .fx-tile-ico { display: flex; align-items: center; justify-content: center; width: 32px; height: 32px; border-radius: var(--radius-sm); background: var(--bg-hover); color: var(--text-secondary); flex-shrink: 0; }
  .fx-tile-name { font-size: 13px; font-weight: 550; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fx-tile-sub { font-size: 11px; color: var(--text-muted); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fx-loc-btn { display: flex; align-items: center; gap: 10px; width: 100%; background: transparent; border: none; cursor: pointer; padding: 0; text-align: left; min-width: 0; }
  .fx-tile-col { display: flex; flex-direction: column; min-width: 0; }

  /* ══ Browser ══ */
  .fx-filter-row { position: relative; display: flex; align-items: center; height: 36px; padding: 0 10px; border-bottom: 1px solid var(--border-subtle); flex-shrink: 0; }
  :global(.fx-filter-ico) { position: absolute; left: 18px; top: 50%; transform: translateY(-50%); color: var(--text-muted); pointer-events: none; z-index: 1; }
  .fx-filter-input { width: 100%; height: 24px; background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); font-family: var(--font-ui-sans); font-size: 11.5px; padding: 0 24px; outline: none; transition: border-color var(--transition-fast); }
  .fx-filter-input:focus { border-color: var(--border-focus); }
  .fx-filter-input::placeholder { color: var(--text-disabled); }
  .fx-filter-clear { position: absolute; right: 14px; top: 50%; transform: translateY(-50%); display: flex; align-items: center; justify-content: center; width: 16px; height: 16px; border: none; background: transparent; color: var(--text-muted); cursor: pointer; border-radius: var(--radius-sm); transition: background var(--transition-fast), color var(--transition-fast); }
  .fx-filter-clear:hover { background: var(--bg-hover); color: var(--text-primary); }
  .fx-filter-divider { display: inline-block; width: 1px; height: 16px; background: var(--border-subtle); margin: 0 8px 0 6px; flex-shrink: 0; }
  .fx-toggle { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; border: none; background: transparent; color: var(--text-muted); cursor: pointer; border-radius: var(--radius-sm); flex-shrink: 0; transition: background var(--transition-fast), color var(--transition-fast); }
  .fx-toggle:hover { background: var(--bg-hover); color: var(--text-primary); }
  .fx-toggle.active { background: var(--accent-subtle); color: var(--accent); }

  /* View-mode dropdown trigger (Details / Medium / Large / Extra large) */
  .fx-view-btn { width: auto; gap: 1px; padding: 0 4px; }
  .fx-view-btn.active { background: var(--bg-hover); color: var(--text-primary); }
  :global(.fx-view-caret) { color: var(--text-muted); }

  .fx-col-head { display: flex; align-items: center; height: 26px; border-bottom: 1px solid var(--border-subtle); flex-shrink: 0; user-select: none; }
  .fx-col { display: flex; align-items: center; overflow: hidden; white-space: nowrap; height: 100%; }
  .fx-col-name { flex: 1; min-width: 0; padding: 0 8px; }
  .fx-col-date { width: 148px; flex-shrink: 0; padding: 0 6px; }
  .fx-col-type { width: 90px; flex-shrink: 0; padding: 0 6px; }
  .fx-col-size { width: 80px; flex-shrink: 0; padding: 0 8px; justify-content: flex-end; }
  .fx-ch { background: transparent; border: none; cursor: pointer; font-family: var(--font-ui-sans); font-size: 10.5px; font-weight: 600; color: var(--text-muted); letter-spacing: 0.4px; padding: 0; height: 100%; text-transform: uppercase; display: flex; align-items: center; gap: 4px; transition: color var(--transition-fast); }
  .fx-ch:hover { color: var(--text-secondary); }
  .fx-ch-right { margin-left: auto; }
  .fx-ch-static { font-size: 10.5px; font-weight: 600; color: var(--text-disabled); text-transform: uppercase; letter-spacing: 0.4px; }
  .fx-sort { color: var(--accent); font-size: 9px; }

  .fx-list { flex: 1; overflow-y: auto; overflow-x: hidden; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; position: relative; }
  .fx-vs { position: relative; }
  .fx-vs-win { position: absolute; top: 0; left: 0; right: 0; will-change: transform; }
  .fx-row { display: flex; align-items: center; cursor: default; outline: none; transition: background var(--transition-fast); }
  .fx-row:hover:not(.selected) { background: var(--bg-hover); }
  .fx-row.selected { background: var(--bg-selected); }
  .fx-row.lead { box-shadow: inset 0 0 0 1.5px var(--accent); }
  .fx-row.cut { opacity: 0.5; }
  .fx-row.drop-target { background: color-mix(in srgb, var(--accent) 18%, transparent); box-shadow: inset 0 0 0 1.5px var(--accent); }
  /* During a drag, only the actual drop-target folder lights up. */
  .fx-list.fx-dragging .fx-row:hover:not(.drop-target):not(.selected) { background: transparent; }
  .fx-create-row { background: color-mix(in srgb, var(--accent) 6%, transparent); border-bottom: 1px solid var(--border-subtle); }
  .fx-row:focus-visible { box-shadow: inset 0 0 0 1.5px var(--accent); }
  .fx-entry-ico { flex-shrink: 0; margin-right: 7px; display: inline-flex; align-items: center; justify-content: center; width: 16px; height: 16px; position: relative; overflow: visible; }
  .fx-native-ico { width: 16px; height: 16px; object-fit: contain; -webkit-user-drag: none; user-select: none; }

  /* Icon grid (medium / large) — virtual rows become a grid of fixed-height cells */
  .fx-vs-win.grid { display: grid; }
  .fx-row.fx-gi { flex-direction: column; align-items: center; justify-content: center; gap: 8px; height: 100%; padding: 8px 4px; box-sizing: border-box; border-radius: var(--radius-md); overflow: hidden; }
  .fx-gi-img { display: flex; align-items: center; justify-content: center; flex-shrink: 0; position: relative; overflow: visible; }
  .fx-gi-img.thumb { background: var(--bg-base); border-radius: var(--radius-sm); overflow: hidden; }
  .fx-gi-thumb { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: var(--radius-sm); box-shadow: 0 1px 4px rgba(0, 0, 0, 0.28); -webkit-user-drag: none; user-select: none; }
  .fx-gi-label { font-size: 11.5px; color: var(--text-primary); font-family: var(--font-ui-sans); text-align: center; line-height: 1.25; max-width: 100%; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; word-break: break-word; }
  .fx-inline-grid { width: 92%; text-align: center; }

  /* Floating drag indicator following the cursor (mouse-based DnD) */
  .fx-drag-ghost {
    position: fixed; z-index: 100000; pointer-events: none;
    transform: translate(12px, 10px);
    max-width: 280px; padding: 4px 10px;
    background: var(--accent); color: var(--text-on-accent);
    border-radius: var(--radius-sm); box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    font-family: var(--font-ui-sans); font-size: 11.5px; font-weight: 550;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .fx-entry-name { font-size: 12px; color: var(--text-primary); font-family: var(--font-ui-sans); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 0 1 auto; min-width: 0; }
  .fx-entry-loc { font-size: 10.5px; color: var(--text-disabled); font-family: var(--font-code); margin-left: 8px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1 1 auto; min-width: 0; }
  .fx-col-date, .fx-col-type, .fx-col-size { font-size: 11px; color: var(--text-muted); }
  .fx-inline { flex: 1; min-width: 0; background: var(--bg-input); border: 1px solid var(--border-focus); border-radius: var(--radius-sm); color: var(--text-primary); font-family: var(--font-ui-sans); font-size: 12px; padding: 1px 5px; outline: none; box-shadow: 0 0 0 2px rgba(61,127,255,0.2); }
  .fx-state { display: flex; align-items: center; gap: 8px; padding: 20px 16px; font-size: 12px; color: var(--text-muted); }
  .fx-state.error { color: var(--error); }
  .fx-state-clear { margin-left: 8px; background: transparent; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); color: var(--text-secondary); cursor: pointer; padding: 2px 8px; font-size: 11px; }
  .fx-state-clear:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ══ Right rail panel ══ */
  .fx-preview { flex-shrink: 0; background: var(--bg-base); border-radius: var(--radius-lg); overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 14px; position: relative; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; }
  .fx-preview.fx-expanded { flex: 1 1 auto; }
  /* Drag handle on the left edge (sits over the inter-panel gap). */
  .fx-pv-resize { position: absolute; left: 0; top: 0; bottom: 0; width: 7px; cursor: col-resize; z-index: 5; }
  .fx-pv-resize::after { content: ''; position: absolute; left: 2px; top: 0; bottom: 0; width: 2px; border-radius: 1px; background: transparent; transition: background var(--transition-fast); }
  .fx-pv-resize:hover::after, .fx-pv-resize:active::after { background: var(--accent); }
  .fx-pv-head { display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; margin-bottom: 2px; }
  .fx-pv-head-title { font-size: 10px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-muted); }
  .fx-pv-expand { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; border: none; background: transparent; color: var(--text-muted); cursor: pointer; border-radius: var(--radius-sm); transition: background var(--transition-fast), color var(--transition-fast); }
  .fx-pv-expand:hover { background: var(--bg-hover); color: var(--text-primary); }
  .fx-pv-view { flex: 1; min-height: 150px; display: flex; align-items: center; justify-content: center; background: var(--bg-elevated); border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; position: relative; }
  .fx-pv-loading { display: flex; flex-direction: column; align-items: center; gap: 10px; color: var(--text-muted); font-size: 11px; }
  .fx-pv-image { max-width: 100%; max-height: 100%; object-fit: contain; display: block; }
  .fx-pv-media { max-width: 100%; max-height: 100%; display: block; background: #000; border-radius: var(--radius-sm); }
  .fx-pv-audio { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 16px; width: 100%; padding: 18px; box-sizing: border-box; color: var(--text-muted); }
  .fx-pv-audio-el { width: 100%; }
  .fx-pv-code { margin: 0; width: 100%; height: 100%; overflow: auto; box-sizing: border-box; padding: 12px 14px; font-family: var(--font-code); font-size: 11px; line-height: 1.55; color: var(--text-secondary); white-space: pre; tab-size: 2; cursor: text; user-select: text; -webkit-user-select: text; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; }
  .fx-pv-code code { font-family: inherit; user-select: text; -webkit-user-select: text; }
  .fx-pv-folder { width: 100%; height: 100%; overflow-y: auto; padding: 8px; box-sizing: border-box; display: flex; flex-direction: column; gap: 1px; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; }
  .fx-pv-folder-item { display: flex; align-items: center; gap: 7px; padding: 3px 6px; border-radius: var(--radius-sm); font-size: 11.5px; color: var(--text-secondary); }
  .fx-pv-folder-ico { display: inline-flex; align-items: center; flex-shrink: 0; }
  .fx-pv-folder-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fx-pv-noprev { display: flex; flex-direction: column; align-items: center; gap: 8px; color: var(--text-muted); font-size: 11px; }
  .fx-pv-bigico { display: inline-flex; }
  .fx-pv-name { font-size: 13px; font-weight: 600; color: var(--text-primary); word-break: break-all; line-height: 1.3; flex-shrink: 0; user-select: text; -webkit-user-select: text; }
  .fx-info-thumb { display: flex; align-items: center; justify-content: center; height: 120px; background: var(--bg-elevated); border: 1px solid var(--border-subtle); border-radius: var(--radius-md); color: var(--text-secondary); overflow: hidden; flex-shrink: 0; }
  .fx-info-img { max-width: 100%; max-height: 100%; object-fit: contain; display: block; }
  .fx-pv-meta { display: flex; flex-direction: column; gap: 8px; margin: 0; }
  .fx-pv-row { display: flex; flex-direction: column; gap: 2px; }
  .fx-pv-row dt { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-disabled); }
  .fx-pv-row dd { margin: 0; font-size: 12px; color: var(--text-secondary); user-select: text; -webkit-user-select: text; }
  .fx-pv-path dd { font-family: var(--font-code); font-size: 11px; word-break: break-all; }
  .fx-info-actions { margin-top: 4px; padding-top: 10px; border-top: 1px solid var(--border-subtle); }
  .fx-info-link { display: inline-flex; align-items: center; gap: 6px; padding: 4px 0; background: none; border: none; color: var(--accent); font-size: 12px; cursor: pointer; transition: color var(--transition-fast); }
  .fx-info-link:hover { color: var(--accent-hover, var(--accent)); text-decoration: underline; }
  .fx-pv-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; color: var(--text-disabled); text-align: center; }
  .fx-pv-empty span { font-size: 12px; color: var(--text-muted); }
  .fx-pv-empty small { font-size: 11px; }

  /* ══ Footer ══ */
  .fx-foot-info { font-size: 11.5px; color: var(--text-secondary); }
  .fx-clip { color: var(--text-muted); }
  .fx-foot-path { font-size: 11px; color: var(--text-muted); font-family: var(--font-code); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 55%; }
  .fx-foot-actions { display: inline-flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .fx-del-confirm { display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-secondary); overflow: hidden; min-width: 0; }
  :global(.fx-del-ico) { color: var(--error); flex-shrink: 0; }
  .fx-del-confirm strong { color: var(--text-primary); }

  /* ── Git status overlays (TortoiseGit-style) ─────────────────────────────── */
  .fx-badge {
    position: absolute; right: -4px; bottom: -4px;
    min-width: 13px; height: 13px; padding: 0 2px; box-sizing: border-box;
    display: inline-flex; align-items: center; justify-content: center;
    font-size: 9px; font-weight: 700; line-height: 1; color: #fff;
    border-radius: 7px; pointer-events: none;
    /* Ring in the surrounding surface colour so the badge reads cleanly on top
       of any icon (light or dark). */
    box-shadow: 0 0 0 1.5px var(--bg-base), 0 1px 2px rgba(0, 0, 0, 0.35);
  }
  /* Grid icons are larger → give the badge a touch more presence. */
  .fx-gi .fx-badge { right: -3px; bottom: -3px; min-width: 15px; height: 15px; font-size: 10px; border-radius: 8px; }
  .fx-badge-modified   { background: var(--warning); }
  .fx-badge-added      { background: var(--success); }
  .fx-badge-untracked  { background: var(--info); }
  .fx-badge-deleted    { background: var(--error); }
  .fx-badge-renamed    { background: var(--accent); }
  .fx-badge-conflicted { background: var(--error); }
  /* Ignored entries are dimmed rather than badged (matches Explorer/Tortoise). */
  .fx-ignored { opacity: 0.5; }
  .fx-ignored.selected { opacity: 0.75; }

  /* Footer branch chip — clickable, toggles the Changes panel. */
  .fx-foot-right { display: inline-flex; align-items: center; gap: 10px; min-width: 0; overflow: hidden; }
  .fx-foot-branch {
    display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0;
    font-size: 11px; color: var(--text-secondary); font-family: var(--font-ui-sans);
    padding: 1px 7px 1px 6px; border-radius: var(--radius-pill, 999px);
    background: var(--bg-base); border: 1px solid var(--border-subtle, var(--border));
    cursor: pointer; transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .fx-foot-branch:hover { background: var(--bg-hover); border-color: var(--border); }
  .fx-foot-branch.active { background: var(--accent-subtle); border-color: transparent; color: var(--accent); }
  .fx-foot-branch :global(svg) { color: var(--accent); flex-shrink: 0; }
  .fx-ab { color: var(--text-muted); font-variant-numeric: tabular-nums; margin-left: 1px; }

  /* ── Changes panel (staged / unstaged list) ──────────────────────────────── */
  .fx-ch { display: flex; flex-direction: column; gap: 1px; }
  .fx-ch-group-head {
    display: flex; align-items: center; gap: 6px;
    font-size: 10px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--text-muted); margin: 8px 2px 3px;
  }
  .fx-ch-group-head:first-child { margin-top: 0; }
  .fx-ch-count {
    font-size: 10px; font-weight: 600; letter-spacing: 0; color: var(--text-secondary);
    background: var(--bg-elevated); border-radius: var(--radius-pill, 999px); padding: 0 6px; min-width: 16px; text-align: center;
  }
  .fx-ch-row {
    display: flex; align-items: center; gap: 7px; width: 100%;
    padding: 3px 6px; border: none; background: none; border-radius: var(--radius-sm);
    cursor: pointer; text-align: left; color: var(--text-primary); font-family: var(--font-ui-sans);
  }
  .fx-ch-row:hover { background: var(--bg-hover); }
  .fx-ch-badge {
    flex-shrink: 0; width: 14px; height: 14px; border-radius: 4px;
    display: inline-flex; align-items: center; justify-content: center;
    font-size: 9px; font-weight: 700; line-height: 1; color: #fff;
  }
  .fx-ch-text { display: flex; align-items: baseline; gap: 5px; min-width: 0; flex: 1; }
  .fx-ch-name { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .fx-ch-dir { font-size: 10.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex-shrink: 1; min-width: 0; }

  /* ── Repo-root + Arbor-registration markers ──────────────────────────────── */
  /* Details-view inline chip on a folder that is a git repo root. */
  .fx-repo-chip {
    display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0;
    margin-left: 8px; padding: 0 6px; height: 16px;
    border-radius: var(--radius-pill, 999px);
    font-size: 10.5px; font-family: var(--font-ui-sans); line-height: 1;
    color: var(--text-muted);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle, var(--border));
  }
  .fx-repo-chip :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  /* Registered in Arbor → accent treatment to distinguish from a plain repo. */
  .fx-repo-chip.registered {
    color: var(--accent); background: var(--accent-subtle); border-color: transparent;
  }
  .fx-repo-chip.registered :global(svg) { color: var(--accent); }
  .fx-repo-branch { font-variant-numeric: tabular-nums; max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Tiny workspace colour dots (which workspaces own the repo). */
  .fx-ws-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; box-shadow: 0 0 0 1px var(--bg-base); }

  /* Grid-view corner overlay (top-left, opposite the status badge). */
  .fx-repo-ov {
    position: absolute; left: -3px; top: -3px;
    width: 15px; height: 15px; border-radius: 50%;
    display: inline-flex; align-items: center; justify-content: center;
    color: var(--text-on-accent, #fff); background: var(--text-muted);
    box-shadow: 0 0 0 1.5px var(--bg-base), 0 1px 2px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }
  .fx-repo-ov.registered { background: var(--accent); }

  /* Info-panel repository section. */
  .fx-info-git { margin-top: 4px; padding-top: 10px; border-top: 1px solid var(--border-subtle); display: flex; flex-direction: column; gap: 8px; }
  .fx-info-git-head { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; color: var(--text-primary); }
  .fx-info-git-head :global(svg) { color: var(--accent); }
  .fx-info-git .fx-pv-meta { margin: 0; }
  .fx-info-ws { display: flex; flex-wrap: wrap; gap: 5px; }
  .fx-ws-chip {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 2px 8px; border-radius: var(--radius-pill, 999px);
    font-size: 11px; color: var(--text-secondary);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle, var(--border));
  }
  .fx-info-ws-empty { font-size: 11px; color: var(--text-muted); font-style: italic; }
</style>
