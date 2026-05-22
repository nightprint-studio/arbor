<script lang="ts">
  import { tick } from 'svelte';
  import {
    ArrowLeft, ArrowRight, ArrowUp, HardDrive, Folder,
    Home, Monitor, LayoutDashboard, FileText, Download, ChevronRight, AlertCircle,
    RefreshCw, FolderPlus, FilePlus, Pencil, Trash2,
    Star, Save, Search, X, Eye, History, GitBranch, Box, ChevronDown,
  } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import { getFileIcon, getFolderIcon } from '$lib/utils/file-icons';
  import { fsReadDir, listFsRoots, fsCreateDir, fsCreateFile, fsRename, fsDelete } from '$lib/ipc/fs';
  import type { FsEntry, FsRoot } from '$lib/ipc/fs';
  import { listRegistryRepos, listWorkspaces } from '$lib/ipc/workspace';
  import type { RepoRegistryEntry, WorkspaceDef } from '$lib/types/workspace';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import ContextMenu, { type MenuItem } from './ContextMenu.svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';
  import ModalSidebarToggle from './ui/ModalSidebarToggle.svelte';
  import Button from './ui/Button.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { matchesBinding } from '$lib/utils/keybindings';

  // ── Props ────────────────────────────────────────────────────────────────
  let {
    mode            = 'folder',
    extensions,
    title,
    initialPath,
    initialFilename,
    multiple        = false,
    onConfirm,
    onConfirmMulti,
    onCancel,
  }: {
    mode?:            'folder' | 'file' | 'save';
    extensions?:      string[];
    title?:           string;
    initialPath?:     string;
    /** Initial filename shown in the "save as" footer input (only used when mode='save'). */
    initialFilename?: string;
    /** Allow selecting multiple files (only honoured when mode='file').
     *  Caller must provide `onConfirmMulti` when this is true. */
    multiple?:        boolean;
    onConfirm?:       (path: string) => void;
    onConfirmMulti?:  (paths: string[]) => void;
    onCancel:         () => void;
  } = $props();

  /** True only when multi-selection is active in a way the picker honours
   *  (mode='file' + caller asked for it + provided a multi-confirm callback). */
  const multiSelectActive = $derived(mode === 'file' && multiple && !!onConfirmMulti);

  // ── State ────────────────────────────────────────────────────────────────
  let currentPath  = $state('');
  /** Raw directory contents from `fsReadDir`. The displayed `entries` are
   *  derived so toggling `showHidden` re-filters live without a refetch. */
  let rawEntries   = $state<FsEntry[]>([]);
  let selectedPath = $state('');
  /** Multi-selection: paths picked via ctrl/shift-click in `mode='file'+multiple`.
   *  The "anchor" tracks the last-clicked file for shift-range expansion. */
  let selectedPaths = $state<Set<string>>(new Set());
  let selectionAnchor = $state<string>('');
  let loading      = $state(false);
  let loadError    = $state('');
  let isThisPc     = $state(false);

  let history    = $state<string[]>([]);
  let historyIdx = $state(-1);

  type SortKey = 'name' | 'modified' | 'size';
  let sortKey = $state<SortKey>('name');
  let sortAsc = $state(true);

  let roots       = $state<FsRoot[]>([]);
  let drives      = $state<FsRoot[]>([]);
  let quickAccess = $state<FsRoot[]>([]);
  /** Registered git repos from arbor's repo registry — surfaced as a
   *  sidebar section below Devices so the user can hop into a known
   *  project without manually navigating from drive roots. */
  let projects    = $state<RepoRegistryEntry[]>([]);
  /** Workspaces (with their repo ids) — used to group projects under
   *  collapsible workspace headers. */
  let workspaces  = $state<WorkspaceDef[]>([]);
  let activeWorkspaceId = $state<string | null>(null);

  // Per-workspace expanded state. Persisted across opens; collapsed by
  // default. Special id "__active__" is the always-pinned active-tab row
  // (no header — always expanded, not user-collapsible).
  const WS_EXPANDED_KEY = 'arbor:filepicker-ws-expanded';
  let wsExpanded = $state<Set<string>>(loadWsExpanded());
  function loadWsExpanded(): Set<string> {
    if (typeof localStorage === 'undefined') return new Set();
    try {
      const raw = localStorage.getItem(WS_EXPANDED_KEY);
      if (!raw) return new Set();
      const arr = JSON.parse(raw);
      return Array.isArray(arr) ? new Set(arr.filter((s): s is string => typeof s === 'string')) : new Set();
    } catch { return new Set(); }
  }
  function persistWsExpanded() {
    if (typeof localStorage === 'undefined') return;
    try { localStorage.setItem(WS_EXPANDED_KEY, JSON.stringify([...wsExpanded])); } catch { /* ignore */ }
  }
  function toggleWs(id: string) {
    const next = new Set(wsExpanded);
    if (next.has(id)) next.delete(id); else next.add(id);
    wsExpanded = next;
    persistWsExpanded();
  }

  let addressEditing = $state(false);
  let addressInput   = $state('');
  /** Cache of the parent directory listing, scoped to the current
   *  `addressInput`. Used to compute the autocomplete ghost suffix
   *  without re-hitting the FS on every keystroke. */
  let addressParentCache = $state<{ parent: string; entries: FsEntry[] }>({ parent: '\0', entries: [] });
  let addressFetchSeq    = 0;
  // Save-mode filename (editable in footer)
  // svelte-ignore state_referenced_locally
  let saveFilename   = $state(initialFilename ?? '');

  // Sidebar collapsed state — persists across opens (ActivityBar-style icon rail).
  let sidebarCollapsed = $state(
    typeof localStorage !== 'undefined'
      && localStorage.getItem('arbor:filepicker-sidebar-collapsed') === '1',
  );
  $effect(() => {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('arbor:filepicker-sidebar-collapsed', sidebarCollapsed ? '1' : '0');
    }
  });

  // Quick filter applied on top of the current directory listing.
  let filterQuery = $state('');

  // Show-hidden toggle — defaults ON (git workflow: you usually want to see
  // .gitignore/.github/.env). Persisted in sessionStorage so a single picker
  // session keeps the choice without leaking it across app restarts.
  let showHidden = $state(
    typeof sessionStorage === 'undefined'
      ? true
      : sessionStorage.getItem('arbor:filepicker-show-hidden') !== '0',
  );
  $effect(() => {
    if (typeof sessionStorage !== 'undefined') {
      sessionStorage.setItem('arbor:filepicker-show-hidden', showHidden ? '1' : '0');
    }
  });

  // Recents — last directories the user navigated to. Persisted across
  // sessions in localStorage; capped at MAX_RECENTS, dedupe on insert.
  const MAX_RECENTS = 8;
  const RECENTS_KEY = 'arbor:filepicker-recents';
  let recentPaths = $state<string[]>(loadRecents());
  function loadRecents(): string[] {
    if (typeof localStorage === 'undefined') return [];
    try {
      const raw = localStorage.getItem(RECENTS_KEY);
      if (!raw) return [];
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed) ? parsed.filter((p): p is string => typeof p === 'string') : [];
    } catch {
      return [];
    }
  }
  function persistRecents() {
    if (typeof localStorage === 'undefined') return;
    try { localStorage.setItem(RECENTS_KEY, JSON.stringify(recentPaths)); } catch { /* quota / private mode */ }
  }
  /** Push a directory path to the front of recents. Files must never reach
   *  this — callers always pass the *folder* the user navigated to. */
  function addRecent(path: string) {
    if (!path || path === '__PC__') return;
    const next = [path, ...recentPaths.filter(p => p !== path)].slice(0, MAX_RECENTS);
    recentPaths = next;
    persistRecents();
  }
  function clearRecents() {
    recentPaths = [];
    persistRecents();
  }

  // Modal-scoped Ctrl+B (toggle_sidebar) — capture-phase listener hijacks the
  // shortcut so the main layout's window listener doesn't also fire and toggle
  // the app sidebar underneath the modal. Mirrors RepoBrowserModal.
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (matchesBinding(e, keybindingsStore.getBinding('toggle_sidebar'))) {
        e.preventDefault();
        e.stopImmediatePropagation();
        sidebarCollapsed = !sidebarCollapsed;
      }
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  });

  // ── Focus zone cycling (F6 / Shift+F6) ───────────────────────────────────
  // Mirrors Windows Explorer / browser convention: F6 rotates focus between
  // the file list, the sidebar (locations), and the address bar.
  let pickerEl = $state<HTMLElement | null>(null);

  type FocusZone = 'list' | 'sidebar' | 'address';

  function detectZone(): FocusZone {
    const active = document.activeElement as HTMLElement | null;
    if (!active) return 'list';
    if (pickerEl?.querySelector('.sidebar')?.contains(active)) return 'sidebar';
    if (active.id === 'addr-input' || active.closest('.hdr-address')) return 'address';
    return 'list';
  }

  function focusSidebarZone() {
    const sidebar = pickerEl?.querySelector('.sidebar');
    if (!sidebar) return;
    const target = sidebar.querySelector<HTMLElement>('.sb-item.active, .sb-ws-header.active')
                ?? sidebar.querySelector<HTMLElement>('.sb-item, .sb-ws-header');
    target?.focus();
  }

  function focusListZone() {
    if (!listEl) {
      // Falls back to filter input when the list isn't mounted (e.g. This PC).
      pickerEl?.querySelector<HTMLElement>('.filter-input')?.focus();
      return;
    }
    const target = listEl.querySelector<HTMLElement>('.row.selected')
                ?? listEl.querySelector<HTMLElement>('.row');
    if (target) { target.focus(); return; }
    pickerEl?.querySelector<HTMLElement>('.filter-input')?.focus();
  }

  function focusAddressZone() {
    if (isThisPc) { focusSidebarZone(); return; }
    startAddressEdit();
  }

  /** Cancel transient sub-states (rename, create, address-edit, delete-confirm,
   *  context menu) WITHOUT triggering their commit-on-blur side-effects. */
  function bailTransientStates() {
    if (addressEditing) { addressInput = ''; addressEditing = false; }
    if (renamingPath)   cancelRename();
    if (createKind)     cancelCreate();
    if (deletingPath)   deletingPath = '';
    if (ctxMenu)        ctxMenu = null;
  }

  function cycleFocus(dir: 1 | -1) {
    const cur = detectZone();
    bailTransientStates();
    const order: FocusZone[] = isThisPc ? ['list', 'sidebar'] : ['list', 'sidebar', 'address'];
    const idx  = order.indexOf(cur);
    const next = order[(idx + dir + order.length) % order.length];
    tick().then(() => {
      if (next === 'list')    focusListZone();
      if (next === 'sidebar') focusSidebarZone();
      if (next === 'address') focusAddressZone();
    });
  }

  // Capture-phase listener so F6 fires even when focus is in an input that
  // stops propagation (address-bar editor, filter input, rename/create input).
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'F6') {
        e.preventDefault();
        e.stopImmediatePropagation();
        cycleFocus(e.shiftKey ? -1 : 1);
        return;
      }
      // Once focus is on a sidebar button, ↑↓ should walk the locations list
      // (browsers don't do this natively for buttons). We don't preventDefault
      // unless the focus is actually inside the sidebar so the file-list arrow
      // navigation isn't disturbed.
      if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
      const sidebar = pickerEl?.querySelector('.sidebar');
      const active = document.activeElement as HTMLElement | null;
      if (!sidebar || !active || !sidebar.contains(active)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      const items = Array.from(sidebar.querySelectorAll<HTMLElement>('.sb-item, .sb-ws-header'));
      const i = items.indexOf(active);
      if (i < 0) return;
      const step = e.key === 'ArrowDown' ? 1 : -1;
      items[(i + step + items.length) % items.length].focus();
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  });

  // ── Virtual scroll ────────────────────────────────────────────────────────
  const VS_ROW_HEIGHT = 28;
  const VS_OVERSCAN   = 20;
  let vsScrollTop    = $state(0);
  let vsClientHeight = $state(0);
  let listEl         = $state<HTMLElement | null>(null);
  /** Bound to the breadcrumb container so we can auto-scroll to the right
   *  edge whenever the path deepens — keeps the current dir visible
   *  instead of getting clipped off the right. */
  let breadcrumbEl   = $state<HTMLElement | null>(null);
  let _vsRafId       = 0;

  function onListScroll() {
    if (_vsRafId) return; // coalesce into one update per frame
    _vsRafId = requestAnimationFrame(() => {
      _vsRafId = 0;
      vsScrollTop = listEl?.scrollTop ?? 0;
    });
  }

  // rename state
  let renamingPath  = $state('');
  let renameValue   = $state('');

  // delete confirm
  let deletingPath  = $state('');

  // "create" inline state
  type CreateKind = 'folder' | 'file' | null;
  let createKind  = $state<CreateKind>(null);
  let createName  = $state('');

  // context menu
  let ctxMenu = $state<{ x: number; y: number; path: string; isDir: boolean } | null>(null);

  function normPath(p: string): string {
    return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
  }

  const activeRepoPath = $derived(tabsStore.activeTab?.path ?? null);

  /** The single registry entry that matches the active tab. Pinned above
   *  the workspace groups; identified by path-equality so it survives the
   *  registry reorder. */
  const activeProject = $derived.by<RepoRegistryEntry | null>(() => {
    if (!activeRepoPath || projects.length === 0) return null;
    const target = normPath(activeRepoPath);
    return projects.find(p => normPath(p.path) === target) ?? null;
  });

  type WsGroup = {
    id:         string;
    name:       string;
    repos:      RepoRegistryEntry[];
    /** True for the synthetic "Unassigned" bucket (repos not in any
     *  workspace). Rendered with a muted style. */
    synthetic:  boolean;
    isActive:   boolean;
  };

  /** Projects grouped by workspace, sorted by workspace `.order`. Repos
   *  inside each group are alphabetical. A synthetic "Unassigned" group
   *  captures repos that aren't part of any workspace. The active-tab
   *  repo (if any) is rendered as a pinned row OUTSIDE these groups
   *  rather than being duplicated inside them. */
  const wsGroups = $derived.by<WsGroup[]>(() => {
    if (projects.length === 0) return [];
    const byId = new Map<string, RepoRegistryEntry>();
    for (const p of projects) byId.set(p.id, p);

    const groups: WsGroup[] = [];
    const seen = new Set<string>();
    const orderedWs = [...workspaces].sort((a, b) => a.order - b.order);
    for (const ws of orderedWs) {
      const repos = ws.repo_ids
        .map(id => byId.get(id))
        .filter((r): r is RepoRegistryEntry => !!r)
        .sort((a, b) => a.display_name.localeCompare(b.display_name, undefined, { sensitivity: 'base' }));
      if (repos.length === 0) continue;
      for (const r of repos) seen.add(r.id);
      groups.push({
        id:        ws.id,
        name:      ws.name,
        repos,
        synthetic: false,
        isActive:  ws.id === activeWorkspaceId,
      });
    }
    const unassigned = projects
      .filter(p => !seen.has(p.id))
      .sort((a, b) => a.display_name.localeCompare(b.display_name, undefined, { sensitivity: 'base' }));
    if (unassigned.length > 0) {
      groups.push({
        id:        '__unassigned__',
        name:      'Unassigned',
        repos:     unassigned,
        synthetic: true,
        isActive:  false,
      });
    }
    return groups;
  });

  function isWsExpanded(id: string): boolean { return wsExpanded.has(id); }

  /** Whenever the breadcrumb segments change (new directory entered), scroll
   *  the container fully right so the deepest crumb stays visible, and
   *  toggle the .is-overflowing class so the left-edge fade-mask only
   *  shows up when the path actually exceeds the visible width. */
  $effect(() => {
    void breadcrumbs;
    const el = breadcrumbEl;
    if (!el) return;
    requestAnimationFrame(() => {
      el.scrollLeft = el.scrollWidth;
      el.classList.toggle('is-overflowing', el.scrollWidth > el.clientWidth + 1);
    });
  });

  // ── Derived ──────────────────────────────────────────────────────────────
  const dialogTitle = $derived(
    title ?? (mode === 'folder' ? 'Select Folder' : mode === 'save' ? 'Save As' : 'Select File')
  );

  /** Visible entries: drop nameless ones, optionally hide dotfiles, and in
   *  file-mode honour the allowed-extensions whitelist. */
  const entries = $derived.by(() => {
    return rawEntries.filter(e => {
      if (!e.name) return false;
      if (!showHidden && e.name.startsWith('.')) return false;
      if (!e.is_dir && mode === 'file' && extensions && extensions.length > 0) {
        return extensions.includes(extOf(e.name));
      }
      return true;
    });
  });

  const breadcrumbs = $derived.by(() => {
    if (isThisPc || !currentPath) return [] as { label: string; path: string }[];
    const clean = currentPath.replace(/[\\/]+$/, '');
    const parts = clean.split(/[\\/]/);
    const isWin = /^[A-Za-z]:$/.test(parts[0]);
    const result: { label: string; path: string }[] = [];
    let acc = isWin ? parts[0] + '\\' : '/';
    result.push({ label: isWin ? parts[0] + '\\' : '/', path: acc });
    for (let i = 1; i < parts.length; i++) {
      if (!parts[i]) continue;
      acc = acc.replace(/[\\/]+$/, '') + (isWin ? '\\' : '/') + parts[i];
      result.push({ label: parts[i], path: acc });
    }
    return result;
  });

  const sorted = $derived.by(() => {
    const q = filterQuery.trim().toLowerCase();
    const list = q
      ? entries.filter(e => e.name.toLowerCase().includes(q))
      : [...entries];
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

  // Virtual scroll: total rows = sorted items + optional create row
  const vsTotalItems  = $derived(sorted.length + (createKind !== null ? 1 : 0));
  const vsTotalHeight = $derived(vsTotalItems * VS_ROW_HEIGHT);
  const vsStart       = $derived(Math.max(0, Math.floor(vsScrollTop / VS_ROW_HEIGHT) - VS_OVERSCAN));
  const vsEnd         = $derived(Math.min(vsTotalItems, Math.ceil((vsScrollTop + Math.max(vsClientHeight, VS_ROW_HEIGHT)) / VS_ROW_HEIGHT) + VS_OVERSCAN));
  const vsOffsetTop   = $derived(vsStart * VS_ROW_HEIGHT);
  const vsItems       = $derived(sorted.slice(vsStart, Math.min(vsEnd, sorted.length)));
  // True if the create row (always last) is in the visible window
  const vsShowCreate  = $derived(createKind !== null && sorted.length >= vsStart && sorted.length < vsEnd);

  const canBack    = $derived(historyIdx > 0);
  const canForward = $derived(historyIdx < history.length - 1);
  const canUp      = $derived(!isThisPc);
  const canConfirm = $derived(
    mode === 'save'
      ? currentPath !== '' && !isThisPc && saveFilename.trim() !== ''
      : multiSelectActive
        ? selectedPaths.size > 0
        : selectedPath !== '' && selectedPath !== '__PC__'
  );

  /** True when saving over a file that already exists in the current directory. */
  const saveFileExists = $derived(
    mode === 'save' && saveFilename.trim() !== '' &&
    entries.some(e => !e.is_dir && e.name.toLowerCase() === saveFilename.trim().toLowerCase())
  );

  const footerInfo = $derived.by(() => {
    if (isThisPc || !selectedPath) {
      const n = entries.filter(e => e.is_dir).length;
      const f = entries.filter(e => !e.is_dir).length;
      const parts: string[] = [];
      if (n > 0) parts.push(`${n} folder${n !== 1 ? 's' : ''}`);
      if (f > 0) parts.push(`${f} file${f !== 1 ? 's' : ''}`);
      return parts.join(', ');
    }
    const entry = entries.find(e => e.path === selectedPath);
    if (!entry) return '';
    if (entry.is_dir) return 'Folder';
    const parts: string[] = ['File'];
    if (entry.size   != null) parts.push(formatSize(entry.size));
    if (entry.modified != null) parts.push(formatDate(entry.modified));
    return parts.join(' · ');
  });

  // ── Utils ────────────────────────────────────────────────────────────────
  /** Normalise OS-native path separators to a single forward-slash style for
   *  display. Tauri returns Windows paths with backslashes; mixing both in
   *  the footer pill looked inconsistent. The actual `selectedPath` value
   *  passed back to the caller stays untouched.
   *
   *  Uses the platform-native separator (backslash on Windows, forward on
   *  POSIX) so the picker doesn't mix slash styles between the breadcrumb
   *  (which already builds with backslashes on Windows) and other displays. */
  const IS_WIN = typeof navigator !== 'undefined' && /win/i.test(navigator.platform);
  const PATH_SEP = IS_WIN ? '\\' : '/';
  function displayPath(p: string): string {
    return p.replace(/[\\/]+/g, PATH_SEP);
  }

  function formatSize(b: number): string {
    if (b < 1024)      return `${b} B`;
    if (b < 1024 ** 2) return `${(b / 1024).toFixed(1)} KB`;
    if (b < 1024 ** 3) return `${(b / 1024 ** 2).toFixed(1)} MB`;
    return `${(b / 1024 ** 3).toFixed(2)} GB`;
  }

  function formatDate(ms: number): string {
    const d = new Date(ms);
    return d.toLocaleDateString('it-IT', { day: '2-digit', month: '2-digit', year: 'numeric' })
      + ' '
      + d.toLocaleTimeString('it-IT', { hour: '2-digit', minute: '2-digit' });
  }

  function parentOf(p: string): string | null {
    const clean = p.replace(/[\\/]+$/, '');
    const last  = Math.max(clean.lastIndexOf('\\'), clean.lastIndexOf('/'));
    if (last <= 0) return null;
    const parent = clean.slice(0, last);
    if (/^[A-Za-z]:$/.test(parent)) return parent + '\\';
    return parent || null;
  }

  function joinPath(base: string, name: string): string {
    const s = base.includes('\\') ? '\\' : '/';
    return base.replace(/[\\/]+$/, '') + s + name;
  }

  function extOf(name: string): string {
    return name.split('.').pop()?.toLowerCase() ?? '';
  }

  function isSelectable(e: FsEntry): boolean {
    if (mode === 'folder') return e.is_dir;
    if (mode === 'save')   return false; // save mode: clicking populates filename, not selectedPath
    if (!e.is_dir) {
      if (!extensions || extensions.length === 0) return true;
      return extensions.includes(extOf(e.name));
    }
    return false; // dir in file mode: navigable but not selectable
  }

  // ── Navigation ───────────────────────────────────────────────────────────
  async function navigate(path: string, pushHist = true) {
    cancelCreate();
    cancelRename();
    deletingPath = '';
    ctxMenu = null;
    filterQuery = '';

    if (path === '__PC__') {
      isThisPc     = true;
      currentPath  = '';
      rawEntries   = [];
      selectedPath = '';
      loadError    = '';
      if (pushHist) pushHistory('__PC__');
      return;
    }

    loading   = true;
    loadError = '';
    isThisPc  = false;

    try {
      const raw = await fsReadDir(path, showHidden);
      rawEntries   = raw;
      currentPath  = path;
      selectedPath = mode === 'folder' ? path : ''; // save mode: confirm uses currentPath+saveFilename
      resetScroll();
      if (pushHist) pushHistory(path);
    } catch (err) {
      loadError = String(err).split('\n')[0].replace(/^.*error:/i, '').trim();
    } finally {
      loading = false;
    }
  }

  function pushHistory(path: string) {
    history    = [...history.slice(0, historyIdx + 1), path];
    historyIdx = history.length - 1;
  }

  async function goBack()    { if (!canBack)    return; historyIdx--; await navigate(history[historyIdx], false); }
  async function goForward() { if (!canForward) return; historyIdx++; await navigate(history[historyIdx], false); }
  async function goUp()      { await navigate(parentOf(currentPath) ?? '__PC__'); }

  function refresh() { return isThisPc ? Promise.resolve() : navigate(currentPath, false); }

  // ── Selection ────────────────────────────────────────────────────────────
  function clickEntry(e: FsEntry, ev?: MouseEvent) {
    cancelCreate(); deletingPath = '';
    if (mode === 'save') {
      // Clicking a file populates the filename input; folders just get highlighted
      if (!e.is_dir) saveFilename = e.name;
      return;
    }

    // Multi-selection (ctrl/shift) — only on selectable files in multi mode.
    if (multiSelectActive && isSelectable(e) && ev && (ev.ctrlKey || ev.metaKey)) {
      const next = new Set(selectedPaths);
      if (next.has(e.path)) next.delete(e.path); else next.add(e.path);
      selectedPaths   = next;
      selectionAnchor = e.path;
      selectedPath    = e.path;
      return;
    }
    if (multiSelectActive && isSelectable(e) && ev?.shiftKey && selectionAnchor) {
      const list = sorted.filter(x => isSelectable(x));
      const a = list.findIndex(x => x.path === selectionAnchor);
      const b = list.findIndex(x => x.path === e.path);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        const next = new Set(selectedPaths);
        for (let i = lo; i <= hi; i++) next.add(list[i].path);
        selectedPaths = next;
        selectedPath  = e.path;
        return;
      }
    }

    if (isSelectable(e)) {
      selectedPath = e.path;
      if (multiSelectActive) {
        selectedPaths   = new Set([e.path]);
        selectionAnchor = e.path;
      }
    } else if (e.is_dir) {
      selectedPath = e.path; // folder in file mode: select for nav but don't allow confirm
      if (multiSelectActive) selectedPaths = new Set();
    }
  }

  async function openEntry(e: FsEntry) {
    if (e.is_dir) await navigate(e.path);
    else if (mode === 'file' && isSelectable(e)) {
      if (multiSelectActive) onConfirmMulti!([e.path]);
      else onConfirm?.(e.path);
    }
    else if (mode === 'save') saveFilename = e.name; // double-click file → populate filename
  }

  // ── Address bar ──────────────────────────────────────────────────────────
  function startAddressEdit() {
    addressInput   = currentPath;
    addressEditing = true;
    tick().then(() => (document.getElementById('addr-input') as HTMLInputElement)?.select());
  }

  async function commitAddress() {
    addressEditing = false;
    if (addressInput.trim() && addressInput.trim() !== currentPath)
      await navigate(addressInput.trim());
  }

  // ── Address autocomplete (ghost text) ───────────────────────────────────
  /** Sentinel parent comparison key — `\\` and `/` are interchangeable on
   *  Windows, so normalise before comparing. */
  function normParentKey(p: string): string {
    return p.replace(/\\/g, '/').replace(/\/+$/, '');
  }

  function lastSepIdx(s: string): number {
    return Math.max(s.lastIndexOf('\\'), s.lastIndexOf('/'));
  }

  /** Resolve a parent path (as typed in the address bar) into an
   *  `fsReadDir`-friendly form. Critical on Windows: `C:` alone means
   *  "current dir on C:" (the process CWD), not the root of the C drive
   *  — we must keep a trailing backslash for drive letters. */
  function resolveReadDirPath(parent: string): string {
    if (/^[A-Za-z]:[\\/]?$/.test(parent)) return parent[0] + ':\\';
    return parent.replace(/[\\/]+$/, '') || parent;
  }

  /** Fetch the parent listing for the current `addressInput` if the cache
   *  is stale. Races resolved via `addressFetchSeq`. */
  async function refreshAddressCache(parent: string) {
    const seq = ++addressFetchSeq;
    try {
      const entries = await fsReadDir(resolveReadDirPath(parent), showHidden);
      if (seq !== addressFetchSeq) return;
      addressParentCache = { parent, entries };
    } catch {
      if (seq !== addressFetchSeq) return;
      addressParentCache = { parent, entries: [] };
    }
  }

  // Keep the cache in sync with what the user is typing.
  $effect(() => {
    if (!addressEditing) return;
    const idx = lastSepIdx(addressInput);
    if (idx < 0) return;
    const parent = addressInput.slice(0, idx + 1);
    if (normParentKey(parent) === normParentKey(addressParentCache.parent)) return;
    refreshAddressCache(parent);
  });

  /** Find the first matching directory in the cached parent listing.
   *  Used by both the ghost-text computation and Tab-completion. Sorted
   *  alphabetically so the suggestion is deterministic regardless of the
   *  raw `fsReadDir` ordering on the host OS. */
  function findAddressMatch(partial: string): FsEntry | undefined {
    const lp = partial.toLowerCase();
    return addressParentCache.entries
      .filter(e =>
        e.is_dir
        && e.name.toLowerCase().startsWith(lp)
        && e.name.length > partial.length,
      )
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))[0];
  }

  /** Ghost suffix shown after the cursor — derived from the cached parent
   *  listing. When the path ends with a separator the partial is empty and
   *  we suggest the first folder of that directory. */
  const addressGhost = $derived.by(() => {
    if (!addressEditing || !addressInput) return '';
    const idx = lastSepIdx(addressInput);
    if (idx < 0) return '';
    const parent  = addressInput.slice(0, idx + 1);
    const partial = addressInput.slice(idx + 1);
    if (normParentKey(parent) !== normParentKey(addressParentCache.parent)) return '';
    const match = findAddressMatch(partial);
    if (!match) return '';
    return match.name.slice(partial.length);
  });

  /** Tab-complete the ghost suffix. Replaces the typed prefix with the
   *  actual entry name so case is preserved (Windows is case-insensitive). */
  function completeAddress() {
    if (!addressGhost) return;
    const idx = lastSepIdx(addressInput);
    if (idx < 0) return;
    const parent  = addressInput.slice(0, idx + 1);
    const partial = addressInput.slice(idx + 1);
    const match = findAddressMatch(partial);
    if (!match) return;
    addressInput = parent + match.name;
    tick().then(() => {
      const inp = document.getElementById('addr-input') as HTMLInputElement | null;
      if (inp) inp.setSelectionRange(addressInput.length, addressInput.length);
    });
  }

  // ── Sort ────────────────────────────────────────────────────────────────
  function toggleSort(key: SortKey) {
    if (sortKey === key) sortAsc = !sortAsc;
    else { sortKey = key; sortAsc = true; }
  }

  // ── Rename ───────────────────────────────────────────────────────────────
  function startRename(path: string, currentName: string) {
    cancelCreate();
    renamingPath = path;
    renameValue  = currentName;
    ctxMenu = null;
    tick().then(() => {
      const inp = document.getElementById('rename-input') as HTMLInputElement | null;
      if (inp) { inp.focus(); inp.select(); }
    });
  }

  async function commitRename() {
    const trimmed = renameValue.trim();
    if (!trimmed || !renamingPath) { cancelRename(); return; }
    const entry = entries.find(e => e.path === renamingPath);
    if (!entry || entry.name === trimmed) { cancelRename(); return; }
    const newPath = joinPath(currentPath, trimmed);
    try {
      await fsRename(renamingPath, newPath);
      if (selectedPath === renamingPath) selectedPath = newPath;
      cancelRename();
      await refresh();
    } catch (err) {
      loadError = String(err).replace(/^.*error:/i, '').trim();
      cancelRename();
    }
  }

  function cancelRename() { renamingPath = ''; renameValue = ''; }

  // ── Delete ────────────────────────────────────────────────────────────────
  function askDelete(path: string) {
    cancelCreate(); cancelRename();
    deletingPath = path;
    ctxMenu = null;
  }

  async function confirmDelete() {
    if (!deletingPath) return;
    try {
      await fsDelete(deletingPath);
      if (selectedPath === deletingPath) selectedPath = mode === 'folder' ? currentPath : '';
      deletingPath = '';
      await refresh();
    } catch (err) {
      loadError = String(err).replace(/^.*error:/i, '').trim();
      deletingPath = '';
    }
  }

  // ── Create ────────────────────────────────────────────────────────────────
  function startCreate(kind: CreateKind) {
    cancelRename();
    createKind = kind;
    createName = kind === 'folder' ? 'New Folder' : 'new-file.txt';
    ctxMenu = null;
    tick().then(() => {
      const inp = document.getElementById('create-input') as HTMLInputElement | null;
      if (inp) { inp.focus(); inp.select(); }
    });
  }

  async function commitCreate() {
    const name = createName.trim();
    if (!name || !createKind) { cancelCreate(); return; }
    const path = joinPath(currentPath, name);
    try {
      if (createKind === 'folder') await fsCreateDir(path);
      else                          await fsCreateFile(path);
      cancelCreate();
      await refresh();
      selectedPath = path;
    } catch (err) {
      loadError = String(err).replace(/^.*error:/i, '').trim();
      cancelCreate();
    }
  }

  function cancelCreate() { createKind = null; createName = ''; }

  // ── Context menu ─────────────────────────────────────────────────────────
  function openCtxOnEntry(e: MouseEvent, entry: FsEntry) {
    e.preventDefault();
    e.stopPropagation();
    cancelCreate(); cancelRename(); deletingPath = '';
    ctxMenu = { x: e.clientX, y: e.clientY, path: entry.path, isDir: entry.is_dir };
  }

  function openCtxOnBackground(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.row')) return;
    e.preventDefault();
    cancelCreate(); cancelRename(); deletingPath = '';
    ctxMenu = { x: e.clientX, y: e.clientY, path: '', isDir: false };
  }

  function ctxItems(path: string, isDir: boolean): MenuItem[] {
    const items: MenuItem[] = [];
    if (path) {
      items.push(
        { id: 'rename', label: 'Rename', icon: Pencil, iconColor: '#ffc66d' },
        { id: 'delete', label: 'Delete', icon: Trash2, danger: true },
        { id: 'sep1',   label: '',       separator: true },
      );
    }
    items.push(
      { id: 'new-folder', label: 'New Folder', icon: FolderPlus, iconColor: 'var(--success)' },
      { id: 'new-file',   label: 'New File',   icon: FilePlus,   iconColor: 'var(--success)' },
    );
    return items;
  }

  async function handleCtxSelect(id: string) {
    if (!ctxMenu) return;
    const { path, isDir } = ctxMenu;
    ctxMenu = null;
    switch (id) {
      case 'rename': {
        const entry = entries.find(e => e.path === path);
        if (entry) startRename(entry.path, entry.name);
        break;
      }
      case 'delete':     askDelete(path);        break;
      case 'new-folder': startCreate('folder');  break;
      case 'new-file':   startCreate('file');    break;
    }
  }

  // ── Keyboard ────────────────────────────────────────────────────────────
  /** Modal's onClose receives this guard so any transient sub-state
   *  (delete confirm, context menu, address bar edit, inline rename)
   *  swallows the close before the modal would dismiss. */
  function guardedClose() {
    if (addressEditing) { addressEditing = false; return; }
    if (renamingPath)   { renamingPath   = '';    return; }
    if (createKind)     { createKind     = null;  return; }
    if (deletingPath)   { deletingPath   = '';    return; }
    if (ctxMenu)        { ctxMenu        = null;  return; }
    onCancel();
  }

  function onKeydown(e: KeyboardEvent) {
    if (addressEditing || renamingPath || createKind) return;
    const inInput = (e.target as HTMLElement).tagName === 'INPUT' || (e.target as HTMLElement).tagName === 'TEXTAREA';

    // Ctrl/⌘ + N → new file · Ctrl/⌘ + Shift + N → new folder
    // We use stopImmediatePropagation so any global app binding stays clear
    // of the picker while it's open.
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
      e.preventDefault();
      e.stopImmediatePropagation();
      startCreate(e.shiftKey ? 'folder' : 'file');
      return;
    }

    // Ctrl/⌘ + L → focus the address bar in edit mode (browser/Explorer convention)
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 'l') {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (!isThisPc) startAddressEdit();
      return;
    }

    switch (e.key) {
      case 'Escape':
        // Handled by Modal's window listener → guardedClose() above. We
        // intentionally do NOT call onCancel here so we don't bypass the
        // sub-state guard (delete-confirm, ctx menu, etc.).
        return;
      case 'Enter':
        e.preventDefault();
        if (deletingPath) { confirmDelete(); return; }
        if (canConfirm) handleConfirm();
        return;
      case 'F2': {
        if (inInput) return;
        e.preventDefault();
        const entry = entries.find(e => e.path === selectedPath);
        if (entry) startRename(entry.path, entry.name);
        return;
      }
      case 'Delete': {
        if (inInput) return;
        e.preventDefault();
        if (selectedPath && selectedPath !== currentPath) askDelete(selectedPath);
        return;
      }
      case 'Backspace':
        if (inInput) return;
        e.preventDefault(); goUp(); return;
      case 'ArrowLeft':  if (e.altKey) { e.preventDefault(); goBack(); }    return;
      case 'ArrowRight': if (e.altKey) { e.preventDefault(); goForward(); } return;
      case 'ArrowDown':
      case 'ArrowUp': {
        if (inInput) return;
        e.preventDefault();
        const list = sorted;
        const idx  = list.findIndex(e => e.path === selectedPath);
        const next = e.key === 'ArrowDown'
          ? list[Math.min(idx + 1, list.length - 1)]
          : list[Math.max(idx - 1, 0)];
        if (next) {
          clickEntry(next);
          tick().then(() => {
            scrollToSelected();
            // Keep DOM focus on the newly selected row so the :focus-visible
            // ring tracks selection — otherwise focus stays on the previously
            // focused row and the user can't tell which panel/item is live.
            listEl?.querySelector<HTMLElement>('.row.selected')?.focus({ preventScroll: true });
          });
        }
        return;
      }
    }

    // Type-ahead: a printable single character with no modifiers, fired
    // outside any input → route it to the filter input. The user always sees
    // what they typed (it lands in the filter field), can backspace, can
    // press ArrowDown to jump to the matching list, etc. — no separate
    // "search buffer" UI to learn.
    if (!inInput && !e.ctrlKey && !e.metaKey && !e.altKey
        && e.key.length === 1 && !isThisPc) {
      e.preventDefault();
      filterQuery = filterQuery + e.key;
      tick().then(() => {
        const filterInput = document.querySelector('.filter-input') as HTMLInputElement | null;
        if (filterInput) {
          filterInput.focus();
          const len = filterQuery.length;
          filterInput.setSelectionRange(len, len);
        }
      });
    }
  }

  // ── Confirm ──────────────────────────────────────────────────────────────
  function handleConfirm() {
    if (!canConfirm) return;
    if (mode === 'save') {
      addRecent(currentPath);
      onConfirm?.(joinPath(currentPath, saveFilename.trim()));
    } else if (multiSelectActive) {
      // Files in multi-select all live under currentPath — store the folder.
      addRecent(currentPath);
      onConfirmMulti!(Array.from(selectedPaths));
    } else if (mode === 'folder') {
      addRecent(selectedPath);
      onConfirm?.(selectedPath);
    } else {
      // file mode (single): store the parent directory, never the file.
      addRecent(parentOf(selectedPath) ?? currentPath);
      onConfirm?.(selectedPath);
    }
  }

  // ── Virtual scroll helpers ────────────────────────────────────────────────
  function scrollToIndex(idx: number) {
    if (!listEl) return;
    const rowTop    = idx * VS_ROW_HEIGHT;
    const rowBottom = rowTop + VS_ROW_HEIGHT;
    const { scrollTop: st, clientHeight: ch } = listEl;
    if (rowTop < st)
      listEl.scrollTop = rowTop;
    else if (rowBottom > st + ch)
      listEl.scrollTop = rowBottom - ch;
    vsScrollTop = listEl.scrollTop;
  }

  function scrollToSelected() {
    const idx = sorted.findIndex(e => e.path === selectedPath);
    if (idx >= 0) scrollToIndex(idx);
  }

  function resetScroll() {
    vsScrollTop = 0;
    if (listEl) listEl.scrollTop = 0;
  }

  // ── Sidebar icon ─────────────────────────────────────────────────────────
  function sidebarIcon(kind: FsRoot['kind']) {
    return { home: Home, desktop: LayoutDashboard, documents: FileText, downloads: Download, drive: HardDrive }[kind] ?? Folder;
  }

  // ── Mount ────────────────────────────────────────────────────────────────
  import { onMount } from 'svelte';
  onMount(async () => {
    try {
      const r = await listFsRoots();
      roots       = r;
      quickAccess = r.filter(x => x.kind !== 'drive');
      drives      = r.filter(x => x.kind === 'drive');
    } catch { /* ignore */ }

    // Pull registered repos + workspaces in parallel so the "Projects"
    // sidebar section populates without blocking the initial paint.
    // Best-effort — picker stays fully usable if either fails.
    try {
      const [repos, snap] = await Promise.all([
        listRegistryRepos(),
        listWorkspaces(),
      ]);
      projects          = repos;
      workspaces        = snap.workspaces;
      activeWorkspaceId = snap.active_workspace_id;
      // First time the picker opens: auto-expand the active workspace so
      // the user has at least one group ready. Persists from then on.
      if (activeWorkspaceId && wsExpanded.size === 0) {
        const next = new Set(wsExpanded);
        next.add(activeWorkspaceId);
        wsExpanded = next;
        persistWsExpanded();
      }
    } catch { /* ignore */ }

    // Try paths in priority order, falling back until one works.  The
    // caller can pass anything (including a stale / non-existent path); we
    // must never end up stranded in an error state on first open.
    const fallback = quickAccess[0]?.path ?? drives[0]?.path ?? '';
    const candidates = [initialPath, fallback].filter((p): p is string => !!p);

    for (const candidate of candidates) {
      await navigate(candidate);
      if (!loadError) return;   // success
      loadError = '';           // reset and try next
    }
    // Everything failed — land on "This PC" so the user still has a way
    // to navigate rather than a dead error screen.
    await navigate('__PC__');
  });
</script>

<svelte:window onkeydown={onKeydown} />

<!-- We route Modal's escape through our guard so a pending delete-confirm /
     context menu / address-edit / rename gets a chance to swallow it before
     the modal would close. -->
<Modal onClose={guardedClose} width="860px" height="560px" padBody={false} topGap
       zIndex="var(--z-modal-picker)" ariaLabel={dialogTitle}>
  {#snippet header()}
    <ModalHeader onClose={guardedClose}>
      <span class="hdr-mode-icon" use:tooltip={dialogTitle}>
        {#if mode === 'folder'}
          <Folder size={14} />
        {:else if mode === 'save'}
          <Save size={14} />
        {:else}
          <FileText size={14} />
        {/if}
      </span>

      <div class="hdr-nav-btns">
        <button class="hdr-nav-btn" onclick={goBack}    disabled={!canBack}    use:tooltip={{ content: 'Back', shortcut: 'Alt+←' }} aria-label="Back">
          <ArrowLeft size={14} strokeWidth={2} />
        </button>
        <button class="hdr-nav-btn" onclick={goForward} disabled={!canForward} use:tooltip={{ content: 'Forward', shortcut: 'Alt+→' }} aria-label="Forward">
          <ArrowRight size={14} strokeWidth={2} />
        </button>
        <button class="hdr-nav-btn" onclick={goUp}      disabled={!canUp}      use:tooltip={{ content: 'Up', shortcut: 'Backspace' }} aria-label="Up">
          <ArrowUp size={14} strokeWidth={2} />
        </button>
      </div>

      <div
        class="hdr-address"
        onclick={startAddressEdit}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); startAddressEdit(); } }}
        role="button"
        tabindex="-1"
        aria-label="Address bar"
        use:tooltip={{
          content:     'Edit path',
          description: 'Type a path with ghost-text autocomplete',
          shortcut:    'Ctrl+L',
          delay:       1200,
          disabled:    addressEditing || isThisPc,
        }}
      >
        {#if addressEditing}
          <div class="addr-input-wrapper">
            <input
              id="addr-input"
              class="addr-input"
              type="text"
              bind:value={addressInput}
              onblur={commitAddress}
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === 'Enter')  { e.preventDefault(); commitAddress(); return; }
                if (e.key === 'Escape') { addressEditing = false; return; }
                if (e.key === 'Tab' && addressGhost) {
                  e.preventDefault();
                  completeAddress();
                  return;
                }
                if (e.key === 'ArrowRight' && addressGhost) {
                  // Right-arrow at end-of-input also completes — common in
                  // shells and address bars. Only when the caret is at the
                  // very end so we don't break left-of-cursor editing.
                  const inp = e.currentTarget as HTMLInputElement;
                  if (inp.selectionStart === addressInput.length) {
                    e.preventDefault();
                    completeAddress();
                  }
                }
              }}
              autocomplete="off"
              spellcheck="false"
            />
            {#if addressGhost}
              <span class="addr-ghost" aria-hidden="true">
                <span class="addr-ghost-typed">{addressInput}</span><span class="addr-ghost-suffix">{addressGhost}</span>
              </span>
              <kbd class="addr-tab-hint">Tab</kbd>
            {/if}
          </div>
        {:else if isThisPc}
          <span class="crumb-single">This PC</span>
        {:else}
          <div class="breadcrumb" bind:this={breadcrumbEl}>
            {#each breadcrumbs as crumb, i (crumb.path)}
              {#if i > 0}<span class="crumb-sep"><ChevronRight size={10} /></span>{/if}
              <button
                class="crumb-item"
                onclick={(e) => { e.stopPropagation(); navigate(crumb.path); }}
              >{crumb.label}</button>
            {/each}
          </div>
        {/if}
      </div>

      {#snippet actions()}
        <button
          class="hdr-icon-btn"
          onclick={refresh}
          disabled={isThisPc || loading}
          use:tooltip={'Refresh'}
          aria-label="Refresh"
        >
          <RefreshCw size={13} class={loading ? 'spin' : ''} />
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="picker"
  role="presentation"
  tabindex="-1"
  bind:this={pickerEl}
>

  <!-- ══ Body ══ -->
  <div class="body">

    <!-- Sidebar -->
    <aside class="sidebar" class:collapsed={sidebarCollapsed}>
      <!-- Sidebar mini-header: title + collapse toggle. Replaces the
           toggle that previously lived in the modal header. -->
      <div class="sb-header">
        <span class="sb-header-title">Locations</span>
        <ModalSidebarToggle
          collapsed={sidebarCollapsed}
          onToggle={() => sidebarCollapsed = !sidebarCollapsed}
        />
      </div>

      {#if recentPaths.length > 0}
        <div class="sb-section-label sb-recents-label">
          <History size={11} strokeWidth={2} class="sb-section-icon" />
          <span class="sb-section-text">Recents</span>
          <button
            class="sb-recents-clear"
            onclick={clearRecents}
            aria-label="Clear recents"
            use:tooltip={'Clear recents'}
          ><X size={10} /></button>
        </div>
        <div class="sb-list">
          {#each recentPaths as p (p)}
            {@const recentName = p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || p}
            {@const isActive = currentPath === p}
            <button
              class="sb-item"
              class:active={isActive}
              onclick={() => navigate(p)}
              use:tooltip={sidebarCollapsed ? `${recentName} — ${displayPath(p)}` : displayPath(p)}
            >
              <span class="sb-icon-wrap"><Folder size={14} strokeWidth={1.7} /></span>
              <span class="sb-label-text">{recentName}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if quickAccess.length > 0}
        <div class="sb-section-label sb-section-divided">
          <Star size={11} strokeWidth={2} class="sb-section-icon" />
          <span class="sb-section-text">Favourites</span>
        </div>
        <div class="sb-list">
          {#each quickAccess as r (r.path)}
            {@const Icon = sidebarIcon(r.kind)}
            {@const isActive = currentPath === r.path}
            <button
              class="sb-item"
              class:active={isActive}
              onclick={() => navigate(r.path)}
              use:tooltip={sidebarCollapsed ? `${r.name} — ${r.path}` : r.path}
            >
              <span class="sb-icon-wrap">
                <Icon size={14} strokeWidth={1.7} />
              </span>
              <span class="sb-label-text">{r.name}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if drives.length > 0}
        <div class="sb-section-label sb-section-divided">
          <HardDrive size={11} strokeWidth={2} class="sb-section-icon" />
          <span class="sb-section-text">Devices</span>
        </div>
        <div class="sb-list">
          <button
            class="sb-item"
            class:active={isThisPc}
            onclick={() => navigate('__PC__')}
            use:tooltip={sidebarCollapsed ? 'This PC' : ''}
          >
            <span class="sb-icon-wrap"><Monitor size={14} strokeWidth={1.7} /></span>
            <span class="sb-label-text">This PC</span>
          </button>
          {#each drives as d (d.path)}
            {@const isDriveActive = !isThisPc && currentPath.startsWith(d.path)}
            <button
              class="sb-item sb-drive"
              class:active={isDriveActive}
              onclick={() => navigate(d.path)}
              use:tooltip={sidebarCollapsed ? `${d.name} — ${d.path}` : d.path}
            >
              <span class="sb-icon-wrap"><HardDrive size={14} strokeWidth={1.7} /></span>
              <span class="sb-label-text">{d.name}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if activeProject || wsGroups.length > 0}
        <div class="sb-section-label sb-section-divided">
          <GitBranch size={11} strokeWidth={2} class="sb-section-icon" />
          <span class="sb-section-text">Projects</span>
        </div>

        {#if activeProject}
          {@const ap = activeProject}
          {@const isCurrent = normPath(currentPath) === normPath(ap.path) || normPath(currentPath).startsWith(normPath(ap.path) + '/')}
          <div class="sb-list">
            <button
              class="sb-item sb-project sb-project-active"
              class:active={isCurrent}
              onclick={() => navigate(ap.path)}
              use:tooltip={sidebarCollapsed
                ? `${ap.display_name} (active tab) — ${ap.path}`
                : `${ap.path} — open in active tab`}
            >
              <span class="sb-icon-wrap"><Box size={14} strokeWidth={1.9} /></span>
              <span class="sb-label-text">{ap.display_name}</span>
              <span class="sb-project-dot" aria-hidden="true"></span>
            </button>
          </div>
        {/if}

        {#each wsGroups as ws (ws.id)}
          {@const expanded = isWsExpanded(ws.id)}
          <button
            class="sb-ws-header"
            class:active={ws.isActive}
            class:synthetic={ws.synthetic}
            onclick={() => toggleWs(ws.id)}
            aria-expanded={expanded}
            use:tooltip={sidebarCollapsed ? `${ws.name} (${ws.repos.length})` : ''}
          >
            <span class="sb-ws-chev">
              {#if expanded}<ChevronDown size={11} strokeWidth={2.2} />{:else}<ChevronRight size={11} strokeWidth={2.2} />{/if}
            </span>
            <span class="sb-ws-name">{ws.name}</span>
            <span class="sb-ws-count">{ws.repos.length}</span>
          </button>
          {#if expanded}
            <div class="sb-list sb-ws-list">
              {#each ws.repos as p (p.id)}
                {@const isActiveRepo = activeRepoPath && normPath(p.path) === normPath(activeRepoPath)}
                {@const isCurrent    = normPath(currentPath) === normPath(p.path) || normPath(currentPath).startsWith(normPath(p.path) + '/')}
                <button
                  class="sb-item sb-project sb-project-nested"
                  class:active={isCurrent}
                  class:sb-project-active={isActiveRepo}
                  onclick={() => navigate(p.path)}
                  use:tooltip={sidebarCollapsed
                    ? `${p.display_name}${isActiveRepo ? ' (active tab)' : ''} — ${p.path}`
                    : (isActiveRepo ? `${p.path} — open in active tab` : p.path)}
                >
                  <span class="sb-icon-wrap">
                    {#if isActiveRepo}
                      <Box size={14} strokeWidth={1.9} />
                    {:else}
                      <GitBranch size={14} strokeWidth={1.7} />
                    {/if}
                  </span>
                  <span class="sb-label-text">{p.display_name}</span>
                  {#if isActiveRepo}<span class="sb-project-dot" aria-hidden="true"></span>{/if}
                </button>
              {/each}
            </div>
          {/if}
        {/each}
      {/if}
    </aside>

    <!-- Main area -->
    <div class="main">
      <!-- Quick filter — narrows the visible entries by name (case-insensitive). -->
      {#if !isThisPc}
        <div class="filter-row">
          <Search size={11} class="filter-icon" />
          <input
            class="filter-input"
            type="text"
            placeholder="Filter files…"
            bind:value={filterQuery}
            spellcheck="false"
            autocomplete="off"
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === 'Escape') {
                if (filterQuery) { filterQuery = ''; e.preventDefault(); }
                return;
              }
              // ArrowDown jumps focus to the file list, selecting the
              // first matching entry — pairs with type-ahead so the user
              // can type a few letters then arrow into the result.
              if (e.key === 'ArrowDown' && sorted.length > 0) {
                e.preventDefault();
                clickEntry(sorted[0]);
                (e.currentTarget as HTMLInputElement).blur();
                tick().then(() => scrollToSelected());
              }
            }}
          />
          {#if filterQuery}
            <button
              class="filter-clear"
              onclick={() => filterQuery = ''}
              aria-label="Clear filter"
              use:tooltip={'Clear filter'}
            ><X size={11} /></button>
          {/if}

          <span class="filter-divider" aria-hidden="true"></span>
          <button
            class="filter-toggle-btn"
            class:active={showHidden}
            onclick={() => {
              showHidden = !showHidden;
              // Backend now respects the flag too — refetch so newly visible
              // (or filtered-out) entries actually appear/disappear.
              if (currentPath) navigate(currentPath, false);
            }}
            aria-pressed={showHidden}
            use:tooltip={{ content: showHidden ? 'Hide hidden files' : 'Show hidden files', description: 'Files starting with a dot' }}
          >
            <Eye size={13} />
          </button>
        </div>
      {/if}

      <!-- Column headers -->
      <div class="col-head">
        <div class="col col-name">
          <button class="ch-btn" onclick={() => toggleSort('name')}>
            Name {#if sortKey === 'name'}<span class="sort-arrow">{sortAsc ? '↑' : '↓'}</span>{/if}
          </button>
        </div>
        <div class="col col-date">
          <button class="ch-btn" onclick={() => toggleSort('modified')}>
            Date modified {#if sortKey === 'modified'}<span class="sort-arrow">{sortAsc ? '↑' : '↓'}</span>{/if}
          </button>
        </div>
        <div class="col col-type"><span class="ch-static">Type</span></div>
        <div class="col col-size">
          <button class="ch-btn ch-right" onclick={() => toggleSort('size')}>
            Size {#if sortKey === 'size'}<span class="sort-arrow">{sortAsc ? '↑' : '↓'}</span>{/if}
          </button>
        </div>
      </div>

      <!-- File list — virtual scrolling: only visible rows are in the DOM -->
      <div
        class="file-list"
        role="presentation"
        bind:this={listEl}
        bind:clientHeight={vsClientHeight}
        onscroll={onListScroll}
        oncontextmenu={openCtxOnBackground}
        onclick={(e) => {
          if (!(e.target as HTMLElement).closest('.row')) {
            cancelCreate();
            if (mode === 'folder') selectedPath = currentPath;
            if (multiSelectActive)  { selectedPaths = new Set(); selectionAnchor = ''; }
          }
        }}
      >
        {#if loading}
          <div class="state-msg"><RefreshCw size={15} class="spin" /> Loading…</div>
        {:else if loadError}
          <div class="state-msg error"><AlertCircle size={14} />{loadError}</div>
        {:else if isThisPc}
          <!-- Drive list is small (≤26 items) — no virtual scroll needed -->
          {#each drives as d (d.path)}
            <div
              class="row"
              class:selected={selectedPath === d.path}
              onclick={() => { selectedPath = d.path; }}
              ondblclick={() => navigate(d.path)}
              onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); navigate(d.path); } else if (e.key === ' ') { e.preventDefault(); selectedPath = d.path; } }}
              role="option"
              aria-selected={selectedPath === d.path}
              tabindex="0"
            >
              <div class="col col-name">
                <div class="drive-entry-icon"><HardDrive size={13} /></div>
                <span class="entry-name">{d.name}</span>
              </div>
              <div class="col col-date"></div>
              <div class="col col-type">Local Disk</div>
              <div class="col col-size"></div>
            </div>
          {/each}
        {:else if sorted.length === 0 && !createKind}
          {#if filterQuery}
            <div class="state-msg">
              <Search size={14} />
              <span>No entries match “{filterQuery}”</span>
              <button class="state-clear" onclick={() => filterQuery = ''}>Clear filter</button>
            </div>
          {:else}
            <div class="state-msg">This folder is empty</div>
          {/if}
        {:else}
          <!-- Virtual scroll container: full logical height keeps the scrollbar correct -->
          <div class="vs-container" style="height: {vsTotalHeight}px;">
            <!-- Sliding window: only vsItems + optional create row are rendered -->
            <div class="vs-window" style="transform: translateY({vsOffsetTop}px);">

              {#each vsItems as entry (entry.path)}
                {@const selectable = isSelectable(entry)}
                {@const isSel = multiSelectActive ? selectedPaths.has(entry.path) : selectedPath === entry.path}
                <div
                  class="row"
                  class:selected={isSel}
                  class:dimmed={!selectable && !entry.is_dir && mode !== 'save'}
                  class:deleting={deletingPath === entry.path}
                  onclick={(ev) => clickEntry(entry, ev)}
                  ondblclick={() => openEntry(entry)}
                  oncontextmenu={(e) => openCtxOnEntry(e, entry)}
                  role="option"
                  aria-selected={isSel}
                  tabindex="0"
                  onkeydown={(e) => { if (e.key === 'Enter') openEntry(entry); if (e.key === 'F2') startRename(entry.path, entry.name); }}
                >
                  <div class="col col-name">
                    {#if entry.is_dir}
                      {@const folderIcon = getFolderIcon(entry.name, false)}
                      <span class="entry-icon"><Icon icon={folderIcon} width={16} height={16} /></span>
                    {:else}
                      {@const fileIconObj = getFileIcon(entry.name)}
                      <span class="entry-icon"><Icon icon={fileIconObj} width={16} height={16} /></span>
                    {/if}

                    {#if renamingPath === entry.path}
                      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                      <input
                        id="rename-input"
                        class="inline-input"
                        type="text"
                        bind:value={renameValue}
                        onclick={(e) => e.stopPropagation()}
                        ondblclick={(e) => e.stopPropagation()}
                        onblur={commitRename}
                        onkeydown={(e) => {
                          e.stopPropagation();
                          if (e.key === 'Enter')  commitRename();
                          if (e.key === 'Escape') cancelRename();
                        }}
                      />
                    {:else}
                      <span class="entry-name" use:tooltip={entry.name}>{entry.name}</span>
                    {/if}
                  </div>
                  <div class="col col-date">{entry.modified != null ? formatDate(entry.modified) : ''}</div>
                  <div class="col col-type">{entry.is_dir ? 'Folder' : (extOf(entry.name).toUpperCase() || 'File') + ' file'}</div>
                  <div class="col col-size">{!entry.is_dir && entry.size != null ? formatSize(entry.size) : ''}</div>
                </div>
              {/each}

              <!-- Create inline row (always last; rendered only when in view) -->
              {#if vsShowCreate}
                <div class="row create-row" style="margin-top: {(sorted.length - vsStart) * VS_ROW_HEIGHT - vsItems.length * VS_ROW_HEIGHT}px;">
                  <div class="col col-name">
                    {#if createKind === 'folder'}
                      {@const newFolderIcon = getFolderIcon('', false)}
                      <span class="entry-icon"><Icon icon={newFolderIcon} width={16} height={16} /></span>
                    {:else}
                      {@const newFileIcon = getFileIcon(createName || 'untitled')}
                      <span class="entry-icon"><Icon icon={newFileIcon} width={16} height={16} /></span>
                    {/if}
                    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                    <input
                      id="create-input"
                      class="inline-input"
                      type="text"
                      bind:value={createName}
                      onclick={(e) => e.stopPropagation()}
                      onblur={commitCreate}
                      onkeydown={(e) => {
                        e.stopPropagation();
                        if (e.key === 'Enter')  commitCreate();
                        if (e.key === 'Escape') cancelCreate();
                      }}
                    />
                  </div>
                  <div class="col col-date"></div>
                  <div class="col col-type">{createKind === 'folder' ? 'Folder' : 'File'}</div>
                  <div class="col col-size"></div>
                </div>
              {/if}

            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>


</div>

  {#snippet footer()}
    {#if deletingPath}
      {@const entry = entries.find(e => e.path === deletingPath)}
      <ModalFooter align="between">
        <span class="delete-confirm">
          <Trash2 size={13} class="delete-icon" />
          Delete <strong>{entry?.name ?? deletingPath}</strong>? This cannot be undone.
        </span>
        <span class="footer-actions">
          <Button variant="ghost" size="sm" onclick={() => deletingPath = ''}>Cancel</Button>
          <Button variant="danger" size="sm" onclick={confirmDelete}>Delete</Button>
        </span>
      </ModalFooter>
    {:else if mode === 'save'}
      <ModalFooter align="between">
        <span class="footer-row">
          <span class="footer-label">Name</span>
          <input
            class="footer-filename-input"
            type="text"
            bind:value={saveFilename}
            onkeydown={(e) => e.key === 'Enter' && handleConfirm()}
            spellcheck="false"
            autocomplete="off"
            placeholder="Enter file name…"
          />
          {#if saveFileExists}
            <span class="overwrite-warning" use:tooltip={'A file with this name already exists and will be overwritten.'}>
              <AlertCircle size={11} /> Will overwrite
            </span>
          {/if}
        </span>
        <span class="footer-actions">
          {#if mode === 'save' && extensions && extensions.length > 0}
            <span class="ext-badge" use:tooltip={'Allowed extensions'}>{extensions.map(e => `.${e}`).join(' ')}</span>
          {/if}
          <Button variant="secondary" onclick={onCancel}>Cancel</Button>
          <Button variant="primary" onclick={handleConfirm} disabled={!canConfirm}>Save</Button>
        </span>
      </ModalFooter>
    {:else}
      {@const pillPath =
        isThisPc
          ? ''
          : multiSelectActive
            ? (selectedPaths.size === 1 ? Array.from(selectedPaths)[0] : '')
            : (selectedPath || currentPath)}
      {@const pillTooltip = pillPath ? displayPath(pillPath) : ''}
      {@const pillLabel =
        mode === 'folder'
          ? 'Folder'
          : multiSelectActive
            ? 'Files'
            : 'File'}
      <ModalFooter align="between">
        <span class="footer-row">
          <span class="footer-label">{pillLabel}</span>
          <span class="footer-pill" class:is-empty={!pillPath} use:tooltip={pillTooltip}>
            {#if isThisPc}
              <span class="footer-pill-hint">Pick a drive to continue</span>
            {:else if multiSelectActive && selectedPaths.size === 0}
              <span class="footer-pill-hint">Ctrl/⌘ + click to add files · Shift + click for range</span>
            {:else if multiSelectActive && selectedPaths.size > 1}
              <span class="footer-pill-text">{selectedPaths.size} files selected</span>
            {:else}
              <span class="footer-pill-text">{displayPath(pillPath)}</span>
            {/if}
          </span>
        </span>
        <span class="footer-actions">
          {#if mode === 'file' && extensions && extensions.length > 0}
            <span class="ext-badge" use:tooltip={'Allowed extensions'}>{extensions.map(e => `.${e}`).join(' ')}</span>
          {/if}
          <Button variant="secondary" onclick={onCancel}>Cancel</Button>
          <Button variant="primary" onclick={handleConfirm} disabled={!canConfirm}>
            {mode === 'folder'
              ? 'Select Folder'
              : multiSelectActive
                ? (selectedPaths.size > 1 ? `Open ${selectedPaths.size} files` : 'Open')
                : 'Open'}
          </Button>
        </span>
      </ModalFooter>
    {/if}
  {/snippet}
</Modal>

<!-- Context menu -->
{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={ctxItems(ctxMenu.path, ctxMenu.isDir)}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

<style>
  /* Context menus float at --z-menu (above --z-modal-picker), so no
     local override needed — the picker can host its own right-click
     menu and the menu stays on top. */

  .picker {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    outline: none;
  }

  /* ── Header inline content ──
     The whole top toolbar (mode icon + nav buttons + breadcrumb/address +
     refresh + close) lives inside <ModalHeader>. */
  .hdr-mode-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .hdr-nav-btns { display: inline-flex; gap: 2px; flex-shrink: 0; }
  .hdr-nav-btn,
  .hdr-icon-btn {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: transparent; border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-nav-btn:hover:not(:disabled),
  .hdr-icon-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .hdr-nav-btn:disabled,
  .hdr-icon-btn:disabled { opacity: 0.3; cursor: default; }

  .hdr-address {
    flex: 1; min-width: 0;
    height: 26px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 0 10px;
    cursor: text;
    display: flex; align-items: center;
    overflow: hidden;
    transition: border-color var(--transition-fast);
  }
  .hdr-address:hover { border-color: var(--border); }

  .addr-input {
    width: 100%; background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-code); font-size: 12px;
    position: relative; z-index: 1;
  }

  /* Ghost-text autocomplete (mirrors the CommandPalette pattern). The
     overlay sits behind the input; the typed prefix is rendered transparent
     so only the suffix is visible after the user's caret. */
  .addr-input-wrapper {
    position: relative;
    flex: 1; min-width: 0;
    display: flex;
    align-items: center;
    height: 100%;
  }
  .addr-ghost {
    position: absolute;
    left: 0; top: 50%;
    transform: translateY(-50%);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1;
    pointer-events: none;
    white-space: pre;
    z-index: 0;
    overflow: hidden;
    max-width: 100%;
  }
  .addr-ghost-typed  { color: transparent; }
  .addr-ghost-suffix { color: var(--text-disabled); }
  .addr-tab-hint {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 9.5px;
    line-height: 1;
    padding: 2px 5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    pointer-events: none;
    font-family: var(--font-ui-sans);
  }

  .crumb-single { font-size: 12px; color: var(--text-secondary); }

  .breadcrumb {
    display: flex; align-items: center; gap: 0;
    /* Allow the path to scroll horizontally when it overflows. We auto-
       scroll to the end (current dir) via the `breadcrumbEl` effect, so
       the user always sees the deepest segment. The fade-mask on the left
       is only applied when there's actual overflow (via .is-overflowing)
       so short paths don't get their leading crumb dimmed for no reason. */
    overflow-x: auto; overflow-y: hidden;
    white-space: nowrap; width: 100%;
    scrollbar-width: none;
  }
  .breadcrumb::-webkit-scrollbar { display: none; }

  .crumb-item {
    background: transparent; border: none; cursor: pointer;
    font-family: var(--font-ui-sans); font-size: 12px;
    color: var(--text-secondary);
    padding: 0 3px; border-radius: var(--radius-sm);
    transition: color var(--transition-fast), background var(--transition-fast);
    white-space: nowrap; flex-shrink: 0;
  }
  .crumb-item:hover { color: var(--text-primary); background: var(--bg-hover); }
  .crumb-sep { color: var(--text-disabled); display: flex; align-items: center; flex-shrink: 0; }

  .ext-badge {
    font-size: 10px; font-family: var(--font-code);
    color: var(--accent); background: var(--accent-subtle);
    border-radius: var(--radius-sm); padding: 1px 6px;
    flex-shrink: 0;
  }

  /* ── Body ──
     The wrapper is `--bg-elevated` so it reads as one continuous chrome
     surface with the modal header, footer, and the rounded top-gap
     separator. Sidebar + main are `--bg-base` cards on top of it. */
  .body {
    flex: 1;
    display: flex;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }

  /* Sidebar */
  .sidebar {
    width: 160px; flex-shrink: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
    display: flex; flex-direction: column;
    transition:
      width    var(--anim-dur-panel, 180ms) cubic-bezier(.16, 1, .3, 1),
      min-width var(--anim-dur-panel, 180ms) cubic-bezier(.16, 1, .3, 1);
  }
  .sidebar.collapsed { width: 44px; }

  /* Sidebar mini-header — holds the collapse toggle (and a title when
     expanded). Same height as the filter row + section labels (36px) so
     the rhythm of horizontal dividers across the picker stays consistent. */
  .sb-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    padding: 0 8px 0 12px;
    box-sizing: border-box;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    overflow: hidden;
  }
  .sb-header-title {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    user-select: none;
    white-space: nowrap;
    overflow: hidden;
  }
  .sidebar.collapsed .sb-header {
    padding: 0;
    justify-content: center;
  }
  .sidebar.collapsed .sb-header-title { display: none; }

  /* ── Sidebar — macOS Finder style ──
     Flat rows on transparent surface, monochrome muted icons, light section
     labels separated by whitespace (no borders, no card outlines). */
  .sb-section-label {
    display: flex; align-items: center; gap: 6px;
    height: 22px;
    padding: 0 14px;
    box-sizing: border-box;
    font-family: var(--font-ui-sans);
    font-size: 10px; font-weight: 600; letter-spacing: 0.04em;
    text-transform: uppercase; color: var(--text-disabled);
    user-select: none;
    flex-shrink: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  :global(.sb-section-icon) { color: var(--text-disabled); flex-shrink: 0; }
  .sb-section-divided { margin-top: 8px; }

  /* Recents header — same look as the other section labels but with a
     trailing clear-all button revealed on hover. */
  .sb-recents-label { padding-right: 6px; }
  .sb-recents-label .sb-section-text { flex: 1; }
  .sb-recents-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--text-disabled);
    cursor: pointer;
    border-radius: var(--radius-sm);
    opacity: 0;
    transition: opacity var(--transition-fast), background var(--transition-fast),
                color var(--transition-fast);
  }
  .sb-recents-label:hover .sb-recents-clear { opacity: 1; }
  .sb-recents-clear:hover { background: var(--bg-hover); color: var(--text-primary); }
  .sidebar.collapsed .sb-recents-clear { display: none; }

  /* Collapsed (icon-rail) — section labels become a thin separator with
     just their icon centered, items lose their text and center the icon. */
  .sidebar.collapsed .sb-section-label {
    padding: 0;
    justify-content: center;
  }
  .sidebar.collapsed .sb-section-text { display: none; }

  .sb-list {
    display: flex; flex-direction: column; gap: 1px;
    padding: 2px 6px;
  }
  .sidebar.collapsed .sb-list { padding: 2px 4px; }

  .sb-item {
    display: flex; align-items: center; gap: 8px;
    width: 100%;
    height: 26px;
    /* Extra left padding nests items visually under their section label
       (Mac Finder vibe — items sit a few px to the right of the section). */
    padding: 0 8px 0 16px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: var(--font-ui-sans); font-size: 12.5px;
    color: var(--text-secondary); text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
    overflow: hidden; white-space: nowrap;
  }
  .sb-item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .sb-item.active {
    background: var(--accent-subtle);
    color: var(--accent);
  }
  /* Keyboard focus — inset accent ring distinguishes a focused item from a
     merely "active" one (e.g. the current directory shown in the sidebar).
     Mouse focus is intentionally not styled (:focus-visible only). */
  .sb-item:focus-visible,
  .sb-ws-header:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1.5px var(--accent);
    color: var(--text-primary);
  }
  .sb-item:focus-visible .sb-icon-wrap { color: var(--accent); }

  .sidebar.collapsed .sb-item {
    padding: 0;
    justify-content: center;
    gap: 0;
  }
  .sidebar.collapsed .sb-label-text { display: none; }

  .sb-icon-wrap {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    flex-shrink: 0;
    color: var(--text-muted);
  }
  .sb-item:hover .sb-icon-wrap { color: var(--text-secondary); }
  .sb-item.active .sb-icon-wrap { color: var(--accent); }

  .sb-label-text {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    flex: 1; min-width: 0;
  }

  .sb-drive { font-size: 12px; }

  /* Project entry — the active-tab repo gets a subtle accent ring + a dot
     so the user can see at a glance which one is "the one they're working
     on". Other projects look like normal sidebar items. */
  .sb-project { position: relative; }
  .sb-project.sb-project-active {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--text-primary);
  }
  .sb-project.sb-project-active .sb-icon-wrap { color: var(--accent); }
  .sb-project-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
    margin-left: auto;
  }
  .sidebar.collapsed .sb-project-dot {
    position: absolute; top: 4px; right: 4px;
    margin-left: 0;
  }

  /* Workspace section header — collapsible, sits between the global
     "Projects" label and the per-workspace repo lists. Hover shows the
     chevron more prominently; active workspace gets a subtle accent. */
  .sb-ws-header {
    display: flex; align-items: center; gap: 6px;
    width: 100%;
    padding: 4px 10px;
    background: transparent;
    color: var(--text-secondary);
    border: none;
    cursor: pointer;
    font-size: 11px;
    font-weight: 500;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .sb-ws-header:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .sb-ws-header.active .sb-ws-name { color: var(--accent); }
  .sb-ws-header.synthetic { color: var(--text-muted); font-style: italic; }
  .sb-ws-chev {
    display: inline-flex; align-items: center; justify-content: center;
    width: 14px; flex-shrink: 0;
    color: var(--text-muted);
  }
  .sb-ws-name {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .sb-ws-count {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-disabled);
    background: var(--bg-overlay);
    padding: 1px 6px;
    border-radius: 8px;
    flex-shrink: 0;
  }
  .sb-ws-list { padding-left: 14px; }
  .sb-project-nested { padding-left: 8px; }

  /* When the sidebar is collapsed to its icon rail, hide the workspace
     headers and the count chips — the structure stays in memory but the
     visual cost of stacked headers in a 40px rail is too high. The repos
     inside expanded workspaces still render as plain icon rows. */
  .sidebar.collapsed .sb-ws-header { display: none; }
  .sidebar.collapsed .sb-ws-list   { padding-left: 0; }
  .sidebar.collapsed .sb-project-nested { padding-left: 0; }

  /* Main */
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  /* Quick filter row — narrow strip above the column header.
     Height 36px aligns its bottom border with the sidebar's section labels. */
  .filter-row {
    position: relative;
    display: flex; align-items: center;
    height: 36px;
    padding: 0 10px;
    box-sizing: border-box;
    border-bottom: 1px solid var(--border-subtle);
    background: transparent;
    flex-shrink: 0;
  }
  :global(.filter-icon) {
    position: absolute;
    left: 18px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-muted);
    pointer-events: none;
    z-index: 1;
  }
  .filter-input {
    width: 100%;
    height: 24px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 11.5px;
    padding: 0 24px 0 24px;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .filter-input:focus { border-color: var(--border-focus); }
  .filter-input::placeholder { color: var(--text-disabled); }
  .filter-clear {
    position: absolute;
    right: 14px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .filter-clear:hover { background: var(--bg-hover); color: var(--text-primary); }

  .filter-divider {
    display: inline-block;
    width: 1px;
    height: 16px;
    background: var(--border-subtle);
    margin: 0 8px 0 6px;
    flex-shrink: 0;
  }
  .filter-toggle-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .filter-toggle-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .filter-toggle-btn.active {
    background: var(--accent-subtle);
    color: var(--accent);
  }

  .state-clear {
    margin-left: 8px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    padding: 2px 8px;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .state-clear:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border); }

  .col-head {
    display: flex; align-items: center;
    height: 26px;
    background: transparent;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0; user-select: none;
  }

  .col { display: flex; align-items: center; overflow: hidden; white-space: nowrap; height: 100%; }
  .col-name { flex: 1; min-width: 0; padding: 0 8px; }
  .col-date { width: 148px; flex-shrink: 0; padding: 0 6px; }
  .col-type { width: 90px;  flex-shrink: 0; padding: 0 6px; }
  .col-size { width: 72px;  flex-shrink: 0; padding: 0 8px; justify-content: flex-end; }

  .ch-btn {
    background: transparent; border: none; cursor: pointer;
    font-family: var(--font-ui-sans); font-size: 10.5px; font-weight: 600;
    color: var(--text-muted); letter-spacing: 0.4px;
    padding: 0; height: 100%;
    text-transform: uppercase;
    display: flex; align-items: center; gap: 4px;
    transition: color var(--transition-fast);
  }
  .ch-btn:hover { color: var(--text-secondary); }
  .ch-right { margin-left: auto; }
  .ch-static { font-size: 10.5px; font-weight: 600; color: var(--text-disabled); text-transform: uppercase; letter-spacing: 0.4px; }
  .sort-arrow { color: var(--accent); font-size: 9px; margin-left: 1px; }

  .file-list {
    flex: 1; overflow-y: auto; overflow-x: hidden;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
    position: relative;
  }

  /* Virtual scroll */
  .vs-container { position: relative; }
  .vs-window    { position: absolute; top: 0; left: 0; right: 0; will-change: transform; }

  .row {
    display: flex; align-items: center;
    min-height: 28px;
    cursor: default; outline: none;
    transition: background var(--transition-fast);
    border-bottom: 1px solid transparent;
  }
  .row:hover:not(.selected):not(.dimmed) { background: var(--bg-hover); }
  .row.selected  { background: var(--bg-selected); }
  .row.selected:hover { background: var(--bg-selected); }
  .row.dimmed    { opacity: 0.35; pointer-events: none; }
  .row.deleting  { background: var(--error-subtle); }
  .row.create-row { background: color-mix(in srgb, var(--accent) 6%, transparent); }
  /* Keyboard focus — inset accent ring. Layered on top of .selected so a row
     that is BOTH focused and selected is unmistakeably "the live one". */
  .row:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }

  .entry-name {
    font-size: 12px; color: var(--text-primary);
    font-family: var(--font-ui-sans);
    overflow: hidden; text-overflow: ellipsis;
    flex: 1; min-width: 0;
  }
  .row.selected .entry-name { color: var(--text-primary); }

  .entry-icon {
    flex-shrink: 0;
    margin-right: 7px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px; height: 16px;
  }

  /* Drive entry in "This PC" view */
  .drive-entry-icon {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px; flex-shrink: 0; margin-right: 7px;
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    color: var(--text-secondary);
  }

  .col-date, .col-type { font-size: 11px; color: var(--text-muted); }
  .col-size             { font-size: 11px; color: var(--text-muted); }

  /* Inline rename/create input */
  .inline-input {
    flex: 1; min-width: 0;
    background: var(--bg-input);
    border: 1px solid var(--border-focus);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 1px 5px;
    outline: none;
    box-shadow: 0 0 0 2px rgba(61,127,255,0.2);
  }

  .state-msg {
    display: flex; align-items: center; gap: 8px;
    padding: 20px 16px; font-size: 12px; color: var(--text-muted);
  }
  .state-msg.error { color: var(--error); }

  /* ── Footer (rendered into Modal's footer slot via <ModalFooter>) ──
     The pill is the always-visible "what will I get?" surface — shows the
     current/selected path (or a hint when there's nothing to show). */
  .footer-pill {
    flex: 1;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    height: 26px;
    padding: 0 10px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    overflow: hidden;
    box-sizing: border-box;
  }
  .footer-pill.is-empty { background: var(--bg-base); }
  .footer-pill-text {
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* Long paths get truncated at the START, not the end — the user cares
       about WHERE THEY ARE (deepest segment), not the drive prefix. The
       `direction: rtl` flips the overflow side; `text-align: left` keeps
       the text aligned to the left edge of the pill so it reads naturally. */
    direction: rtl;
    text-align: left;
    unicode-bidi: plaintext;
  }
  .footer-pill-hint {
    color: var(--text-disabled);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Wraps a leading [label] + the input/pill so they hug each other on the
     left side of the ModalFooter (which uses align="between"). */
  .footer-row {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
  }
  .footer-label {
    font-size: 11.5px; font-weight: 500; color: var(--text-secondary);
    white-space: nowrap; flex-shrink: 0;
  }

  .footer-filename-input {
    flex: 1;
    min-width: 0;
    background: var(--bg-input); border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary); font-family: var(--font-ui-sans);
    font-size: 12px; padding: 4px 8px; outline: none;
    transition: border-color var(--transition-fast);
  }
  .footer-filename-input:focus { border-color: var(--border-focus); }
  .footer-filename-input::placeholder { color: var(--text-disabled); }

  .overwrite-warning {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
    color: var(--warning, #e6a817);
    flex-shrink: 0;
    cursor: help;
  }

  .footer-actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  /* Delete confirm bar — rendered into the Modal footer slot. */
  .delete-confirm {
    display: inline-flex; align-items: center; gap: 8px;
    font-size: 12px; color: var(--text-secondary);
    overflow: hidden;
    min-width: 0;
  }
  :global(.delete-icon) { color: var(--error); flex-shrink: 0; }
  .delete-confirm strong { color: var(--text-primary); }
  :global(.spin) { animation: spin 0.75s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
