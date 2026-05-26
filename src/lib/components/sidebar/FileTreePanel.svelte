<script lang="ts">
  import { Search, X, RefreshCw, GitCommitHorizontal, History, FolderTree, FileText } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';

  import { onMount, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';

  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { getRepoFiles, startFileMetaScan, getRepoFingerprint } from '$lib/ipc/graph';
  import type { RepoFileEntry } from '$lib/types/git';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import GitBlameModal from '$lib/components/shared/GitBlameModal.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { markdownStore } from '$lib/stores/markdown.svelte';
  import { compactMiddleDirs } from '$lib/utils/file-tree/compact-middle-dirs';
  import { appearanceStore } from '$lib/stores/appearance.svelte';

  // ── File / folder icons ──────────────────────────────────────────────────────
  import { getFileIcon, getFolderIcon } from '$lib/utils/file-icons';

  // ── Tree data structures ──────────────────────────────────────────────────────

  interface FileNode { kind: 'file'; name: string; path: string; }
  interface DirNode  { kind: 'dir';  name: string; path: string; children: TreeNode[]; }
  type TreeNode = FileNode | DirNode;

  interface FileMeta {
    last_commit_short_oid?: string;
    last_commit_date?: number;
    last_commit_summary?: string;
  }

  // ── Component state ───────────────────────────────────────────────────────────

  const tab = $derived(tabsStore.activeTab);

  let rawPaths    = $state<string[]>([]);
  let fileMeta    = $state<Record<string, FileMeta>>({});
  let metaLoading    = $state(false);   // background date scan in progress
  let loading        = $state(false);
  let error          = $state<string | null>(null);
  let searchQuery    = $state('');
  let expanded       = $state<Set<string>>(new Set());
  let treeBodyEl     = $state<HTMLElement | undefined>(undefined);

  // Context menu state
  interface CtxMenu { x: number; y: number; path: string; }
  let ctxMenu     = $state<CtxMenu | null>(null);
  let blameTarget = $state<string | null>(null);

  function isMarkdownPath(p: string): boolean {
    const lower = p.toLowerCase();
    return lower.endsWith('.md') || lower.endsWith('.markdown');
  }

  /** Resolve a repo-relative path to an absolute filesystem path. Uses the
   *  repo root's native separator (`\` on Windows, `/` elsewhere) so the
   *  resulting string round-trips through Rust `Path` parsing unchanged. */
  function absolutePath(repoRoot: string, relative: string): string {
    const useBackslash = repoRoot.includes('\\') && !repoRoot.includes('/');
    const sep = useBackslash ? '\\' : '/';
    const normRel = useBackslash ? relative.replace(/\//g, '\\') : relative;
    const trimmed = repoRoot.endsWith(sep) ? repoRoot.slice(0, -1) : repoRoot;
    return `${trimmed}${sep}${normRel}`;
  }

  const ctxMenuItems = $derived.by((): MenuItem[] => {
    const items: MenuItem[] = [];
    if (ctxMenu && isMarkdownPath(ctxMenu.path)) {
      items.push({ id: 'open-md', label: 'Open in Markdown Editor', icon: FileText, iconColor: 'var(--accent)' });
    }
    items.push(
      { id: 'blame', label: 'Git Blame', icon: History, iconColor: 'var(--color-tag)' },
      { id: 'filter', label: 'Filter Graph by File', icon: GitCommitHorizontal, iconColor: '#20b2aa' },
    );
    return items;
  });

  function openContextMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenu = { x: e.clientX, y: e.clientY, path };
  }

  function handleCtxSelect(id: string) {
    if (!ctxMenu || !tab) return;
    const path = ctxMenu.path;
    ctxMenu = null;
    if (id === 'blame') {
      blameTarget = path;
    } else if (id === 'filter') {
      selectFile(path);
    } else if (id === 'open-md') {
      markdownStore.openFile({
        path:     absolutePath(tab.path, path),
        filename: path.split('/').pop() ?? path,
        tabId:    tab.id,
      });
    }
  }

  // Token to cancel a stale background scan when tab changes (plain var, not reactive)
  let metaToken = 0;
  // Cache key for the current scan (null = no caching for this scan)
  let pendingCacheKey: string | null = null;

  const activeFilter = $derived(graphStore.fileFilter);

  // ── Load ──────────────────────────────────────────────────────────────────────

  // ── Event listeners (set up once, live for component lifetime) ───────────────

  onMount(() => {
    function onNavigate(e: Event) {
      const path = (e as CustomEvent<{ path: string }>).detail?.path;
      if (path) navigateTo(path);
    }
    window.addEventListener('arbor:navigate-to-file', onNavigate);

    const unlistenBatch = listen<{ tab_id: string; entries: RepoFileEntry[] }>(
      'arbor://file-meta-batch',
      (ev) => {
        if (ev.payload.tab_id !== tab?.id) return;
        const next = { ...fileMeta };
        for (const e of ev.payload.entries) {
          next[e.path] = {
            last_commit_short_oid: e.last_commit_short_oid,
            last_commit_date:      e.last_commit_date,
            last_commit_summary:   e.last_commit_summary,
          };
        }
        fileMeta = next;
      }
    );

    const unlistenDone = listen<string>('arbor://file-meta-done', (ev) => {
      if (ev.payload !== tab?.id) return;
      metaLoading = false;
      // Persist completed scan to session cache.
      if (pendingCacheKey) {
        writeCache(pendingCacheKey, fileMeta);
        pendingCacheKey = null;
      }
    });

    return async () => {
      window.removeEventListener('arbor:navigate-to-file', onNavigate);
      (await unlistenBatch)();
      (await unlistenDone)();
    };
  });

  // ── Load ──────────────────────────────────────────────────────────────────────

  $effect(() => {
    const tabId = tab?.id;
    if (!tabId) { rawPaths = []; fileMeta = {}; return; }
    loadTree(tabId);
  });

  // ── Cache helpers ──────────────────────────────────────────────────────────────

  function cacheKey(repoPath: string, fingerprint: string): string {
    return `arbor:file-meta:${repoPath}:${fingerprint}`;
  }

  function readCache(key: string): Record<string, FileMeta> | null {
    try {
      const raw = sessionStorage.getItem(key);
      if (!raw) return null;
      return JSON.parse(raw) as Record<string, FileMeta>;
    } catch { return null; }
  }

  function writeCache(key: string, data: Record<string, FileMeta>) {
    try { sessionStorage.setItem(key, JSON.stringify(data)); } catch { /* quota */ }
  }

  // ── Load ──────────────────────────────────────────────────────────────────────

  async function loadTree(tabId: string) {
    loading = true; error = null;
    rawPaths = []; fileMeta = {};
    metaToken += 1;
    try {
      rawPaths = await getRepoFiles(tabId);  // instant — just reads the index
    } catch (err) {
      error = `${err}`;
      loading = false;
      return;
    }
    loading = false;

    // Check session cache — key is repoPath + HEAD fingerprint.
    try {
      const fingerprint = await getRepoFingerprint(tabId);
      const repoPath = tab?.path ?? tabId;
      const key = cacheKey(repoPath, fingerprint);
      const cached = readCache(key);
      if (cached) {
        fileMeta = cached;
        return;  // cache hit — no scan needed
      }
      // Cache miss: stream metadata and save when done.
      metaLoading = true;
      // Store key so the done-listener can persist the result.
      pendingCacheKey = key;
      startFileMetaScan(tabId).catch(() => { metaLoading = false; });
    } catch {
      // Fingerprint failed — start scan without caching.
      metaLoading = true;
      pendingCacheKey = null;
      startFileMetaScan(tabId).catch(() => { metaLoading = false; });
    }
  }

  async function handleRefresh() {
    if (!tab || loading) return;
    await loadTree(tab.id);
  }

  // ── Tree builder ──────────────────────────────────────────────────────────────

  function buildTree(paths: string[]): TreeNode[] {
    const root: DirNode = { kind: 'dir', name: '', path: '', children: [] };
    for (const path of paths) {
      const parts = path.split('/');
      let node = root;
      for (let i = 0; i < parts.length - 1; i++) {
        const seg = parts[i];
        const dirPath = parts.slice(0, i + 1).join('/');
        let child = node.children.find((c): c is DirNode => c.kind === 'dir' && c.name === seg);
        if (!child) {
          child = { kind: 'dir', name: seg, path: dirPath, children: [] };
          node.children.push(child);
        }
        node = child;
      }
      node.children.push({ kind: 'file', name: parts[parts.length - 1], path });
    }
    sortTree(root);
    return root.children;
  }

  function sortTree(node: DirNode) {
    node.children.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === 'dir' ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const child of node.children) {
      if (child.kind === 'dir') sortTree(child);
    }
  }

  // IntelliJ-style "compact middle packages": collapse single-child dir
  // chains into one row, then re-sort each level by the joined name.
  const FILE_TREE_ACCESSORS = {
    isDir:       (n: TreeNode) => n.kind === 'dir',
    getName:     (n: TreeNode) => n.name,
    setName:     (n: TreeNode, name: string) => { n.name = name; },
    getChildren: (n: TreeNode) => (n.kind === 'dir' ? n.children : []),
    setChildren: (n: TreeNode, kids: TreeNode[]) => {
      if (n.kind === 'dir') n.children = kids;
    },
  };
  function compactFileTree(roots: TreeNode[]): TreeNode[] {
    const out = compactMiddleDirs(roots, FILE_TREE_ACCESSORS);
    const reSort = (n: TreeNode) => {
      if (n.kind !== 'dir') return;
      n.children.sort((a, b) => {
        if (a.kind !== b.kind) return a.kind === 'dir' ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      for (const c of n.children) reSort(c);
    };
    out.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === 'dir' ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const n of out) reSort(n);
    return out;
  }

  const tree = $derived.by(() => {
    const base = buildTree(rawPaths);
    return appearanceStore.compactFileTreeDirs ? compactFileTree(base) : base;
  });

  // ── Search ────────────────────────────────────────────────────────────────────

  // Debounce searchQuery so scoring only runs ~150ms after the user stops typing.
  let debouncedQuery = $state('');
  $effect(() => {
    const q = searchQuery;
    const t = setTimeout(() => { debouncedQuery = q; }, 150);
    return () => clearTimeout(t);
  });

  // Pre-compute lowercase paths + filenames once (not on every keystroke).
  const rawPathsLower    = $derived(rawPaths.map(p => p.toLowerCase()));
  const rawFilenames     = $derived(rawPaths.map(p => p.split('/').pop() ?? p));
  const rawFilenamesLower = $derived(rawFilenames.map(f => f.toLowerCase()));

  function fuzzyMatch(str: string, pattern: string): boolean {
    let si = 0;
    for (let pi = 0; pi < pattern.length; pi++) {
      const idx = str.indexOf(pattern[pi], si);
      if (idx === -1) return false;
      si = idx + 1;
    }
    return true;
  }

  const MAX_RESULTS = 200;

  interface SearchResults { items: string[]; total: number; }

  // Bucket-based search: no intermediate score objects, no Array.sort().
  // Priority order: exact name → name prefix → name contains → path contains
  //                 → fuzzy name → fuzzy path.
  // Fuzzy checks are skipped once high-priority buckets already reach MAX_RESULTS.
  const searchResults = $derived.by((): SearchResults | null => {
    const q = debouncedQuery.trim().toLowerCase();
    if (!q) return null;

    const exact:     string[] = [];
    const prefix:    string[] = [];
    const fnContain: string[] = [];
    const ptContain: string[] = [];
    const fnFuzzy:   string[] = [];
    const ptFuzzy:   string[] = [];

    for (let i = 0; i < rawPaths.length; i++) {
      const fl = rawFilenamesLower[i];
      const pl = rawPathsLower[i];

      if (fl === q)         { exact.push(rawPaths[i]);     continue; }
      if (fl.startsWith(q)) { prefix.push(rawPaths[i]);    continue; }
      if (fl.includes(q))   { fnContain.push(rawPaths[i]); continue; }
      if (pl.includes(q))   { ptContain.push(rawPaths[i]); continue; }

      // Skip fuzzy entirely when high-quality buckets are already full.
      const hq = exact.length + prefix.length + fnContain.length + ptContain.length;
      if (hq >= MAX_RESULTS) continue;

      if (fuzzyMatch(fl, q)) { fnFuzzy.push(rawPaths[i]); continue; }
      if (hq + fnFuzzy.length < MAX_RESULTS) {
        if (fuzzyMatch(pl, q)) ptFuzzy.push(rawPaths[i]);
      }
    }

    const all = [...exact, ...prefix, ...fnContain, ...ptContain, ...fnFuzzy, ...ptFuzzy];
    return { items: all.slice(0, MAX_RESULTS), total: all.length };
  });

  // ── Interactions ──────────────────────────────────────────────────────────────

  function selectFile(path: string) {
    if (activeFilter === path) {
      graphStore.clearFileFilter();
    } else {
      graphStore.filterByFile(path);
      if (uiStore.activeBottomSection === null) uiStore.setActiveBottomSection('detail');
    }
  }

  /** Expand all ancestor folders for a given path and scroll the file row into view. */
  function navigateTo(path: string) {
    // Expand every ancestor directory
    const parts = path.split('/');
    const next = new Set(expanded);
    for (let i = 1; i < parts.length; i++) {
      next.add(parts.slice(0, i).join('/'));
    }
    expanded = next;

    // Scroll to the file row after DOM update
    tick().then(() => {
      const el = treeBodyEl?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`);
      el?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    });
  }

  // ── Formatting ────────────────────────────────────────────────────────────────

  function formatDate(ts?: number): string {
    if (!ts) return '';
    const diff = Math.floor((Date.now() - ts * 1000) / 86_400_000);
    if (diff === 0) return 'today';
    if (diff === 1) return 'yesterday';
    if (diff < 7)   return `${diff}d ago`;
    if (diff < 30)  return `${Math.floor(diff / 7)}w ago`;
    if (diff < 365) return `${Math.floor(diff / 30)}mo ago`;
    return `${Math.floor(diff / 365)}y ago`;
  }
</script>

<!-- Tree rendering — recursion / chevron / indentation owned by the
     shared <Tree> widget. The snippet only renders the in-row content
     (icon + name + optional last-commit date) and branches between
     dir / file. -->


<aside class="file-tree-panel">

  <!-- ── Header ── -->
  <div class="panel-header">
    <span class="header-icon"><FolderTree size={14} /></span>
    <span class="header-title">Files</span>
    {#if metaLoading}
      <span class="meta-loading-badge" use:tooltip={'Loading file history…'}>
        <RefreshCw size={10} />
      </span>
    {/if}
    {#if activeFilter}
      <button class="clear-btn" onclick={() => graphStore.clearFileFilter()} use:tooltip={'Clear file filter'}>
        <X size={10} /> Clear
      </button>
    {/if}
    <button
      class="refresh-btn"
      onclick={handleRefresh}
      disabled={loading}
      use:tooltip={'Refresh'}
    >
      <RefreshCw size={11} class={loading ? 'spin' : ''} />
    </button>
  </div>

  <!-- ── Filter banner ── -->
  {#if activeFilter}
    <div class="filter-banner">
      <GitCommitHorizontal size={11} />
      <span class="filter-path" use:tooltip={activeFilter}>{activeFilter}</span>
    </div>
  {/if}

  <!-- ── Search ── -->
  <div class="search-row">
    <Search size={11} class="search-icon" />
    <input
      class="search-input"
      type="text"
      placeholder="Filter files…"
      bind:value={searchQuery}
      spellcheck="false"
    />
    {#if searchQuery}
      <button class="search-clear" onclick={() => searchQuery = ''}>
        <X size={10} />
      </button>
    {/if}
  </div>

  <!-- ── Body ── -->
  <div class="tree-body" bind:this={treeBodyEl}>
    {#if loading}
      <div class="state-msg">
        <RefreshCw size={14} class="spin" />
        <span>Loading…</span>
      </div>

    {:else if error}
      <div class="state-msg err">{error}</div>

    {:else if rawPaths.length === 0}
      <div class="state-msg muted">No tracked files</div>

    {:else if searchResults !== null}
      {#if searchResults.total === 0}
        <div class="state-msg muted">No matches</div>
      {:else}
        {#if searchResults.total > MAX_RESULTS}
          <div class="search-count">Showing {MAX_RESULTS} of {searchResults.total}</div>
        {/if}
        {#each searchResults.items as path (path)}
          {@const name = path.split('/').pop() ?? path}
          {@const meta = fileMeta[path]}
          {@const fileIcon = getFileIcon(name)}
          <button
            class="file-row"
            class:file-active={activeFilter === path}
            style="--depth:0"
            data-path={path}
            use:tooltip={meta?.last_commit_summary ? { content: path, description: meta.last_commit_summary } : path}
            onclick={() => selectFile(path)}
            oncontextmenu={(e) => openContextMenu(e, path)}
          >
            <span class="node-icon">
              <Icon icon={fileIcon} width={16} height={16} />
            </span>
            <span class="node-name truncate">{path}</span>
            {#if meta?.last_commit_date}
              <span class="node-date">{formatDate(meta.last_commit_date)}</span>
            {/if}
          </button>
        {/each}
      {/if}

    {:else}
      <Tree
        nodes={tree}
        getId={(n: TreeNode) => n.path}
        getChildren={(n: TreeNode) => n.kind === 'dir' ? n.children : undefined}
        expandedIds={expanded}
        onExpandToggle={(id) => {
          const next = new Set(expanded);
          next.has(id) ? next.delete(id) : next.add(id);
          expanded = next;
        }}
        selectedId={activeFilter}
        selectable={(n: TreeNode) => n.kind === 'file'}
        indentSize={14}
        basePadding={4}
        ariaLabel="Repository file tree"
        rowClass={(ctx) => ctx.node.kind === 'file' && activeFilter === (ctx.node as any).path ? 'file-active' : ''}
        rowTitle={(n: TreeNode) => {
          if (n.kind !== 'file') return n.path || '(root)';
          const m = fileMeta[n.path];
          return m?.last_commit_summary ? `${n.path}\n${m.last_commit_summary}` : n.path;
        }}
        onSelect={(n: TreeNode) => { if (n.kind === 'file') selectFile(n.path); }}
        onContextMenu={(n: TreeNode, e: MouseEvent) => {
          if (n.kind === 'file') openContextMenu(e, n.path);
        }}
      >
        {#snippet row({ node, expanded: isOpen }: { node: TreeNode; expanded: boolean })}
          {#if node.kind === 'dir'}
            {@const folderIcon = getFolderIcon(node.name, isOpen)}
            <span class="node-icon">
              <Icon icon={folderIcon} width={16} height={16} />
            </span>
            <span class="node-name truncate">{node.name}</span>
          {:else}
            {@const meta = fileMeta[node.path]}
            {@const fileIcon = getFileIcon(node.name)}
            <span class="node-icon" data-path={node.path}>
              <Icon icon={fileIcon} width={16} height={16} />
            </span>
            <span class="node-name truncate">{node.name}</span>
            {#if meta?.last_commit_date}
              <span class="node-date">{formatDate(meta.last_commit_date)}</span>
            {/if}
          {/if}
        {/snippet}
      </Tree>
    {/if}
  </div>

</aside>

{#if ctxMenu}
  <ContextMenu
    items={ctxMenuItems}
    x={ctxMenu.x}
    y={ctxMenu.y}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

{#if blameTarget && tab}
  <GitBlameModal
    tabId={tab.id}
    path={blameTarget}
    onClose={() => blameTarget = null}
  />
{/if}

<style>
  .file-tree-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-base);
    overflow: hidden;
  }

  /* ── Header ── */
  .panel-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px 6px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    height: 34px;
  }
  .header-icon { color: var(--accent); display: flex; }
  .header-title {
    flex: 1;
    font-weight: 600;
    font-size: 11px;
    letter-spacing: 0.3px;
    color: var(--text-secondary);
    text-transform: uppercase;
  }
  .refresh-btn {
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 2px;
    border-radius: var(--radius-sm);
    transition: color var(--transition-fast);
  }
  .refresh-btn:hover { color: var(--text-primary); }
  .refresh-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .clear-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    background: var(--accent-subtle);
    border: 1px solid rgba(77,120,204,0.3);
    color: var(--accent);
    border-radius: 999px;
    padding: 1px 7px 1px 5px;
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 500;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .clear-btn:hover {
    background: rgba(199,84,80,0.15);
    border-color: rgba(199,84,80,0.3);
    color: var(--error, #c75450);
  }

  /* ── Filter banner ── */
  .filter-banner {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: rgba(77,120,204,0.08);
    border-bottom: 1px solid rgba(77,120,204,0.18);
    color: var(--accent);
    flex-shrink: 0;
    animation: fadeIn var(--anim-dur-fast, 80ms) ease;
  }
  .filter-path {
    font-family: var(--font-code);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  /* ── Search ── */
  .search-row {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 8px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  :global(.search-icon) { color: var(--text-muted); flex-shrink: 0; }
  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 0;
  }
  .search-input::placeholder { color: var(--text-disabled); }
  .search-clear {
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
    transition: color var(--transition-fast);
  }
  .search-clear:hover { color: var(--text-primary); }

  /* ── Body ── */
  .tree-body {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 2px 0;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }
  .tree-body::-webkit-scrollbar { width: 4px; }
  .tree-body::-webkit-scrollbar-track { background: transparent; }
  .tree-body::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }

  /* ── Search count ── */
  .search-count {
    padding: 3px 10px;
    font-size: 10px;
    color: var(--text-disabled);
    font-family: var(--font-ui-sans);
    border-bottom: 1px solid var(--border-subtle);
  }

  /* ── State messages ── */
  .state-msg {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 16px 14px;
    color: var(--text-muted);
    font-size: 12px;
    font-family: var(--font-ui-sans);
  }
  .state-msg.err   { color: var(--error, #c75450); }
  .state-msg.muted { color: var(--text-disabled); }

  /* The tree wrapper / row / chevron / indentation styling is owned by the
     shared <Tree> widget. Only the per-row glyphs + the active-file
     accent stripe still live here. */

  .meta-loading-badge {
    display: flex;
    align-items: center;
    color: var(--text-disabled);
    flex-shrink: 0;
    animation: spin 0.9s linear infinite;
  }

  /* Active file row — keep the IntelliJ-style left-edge accent stripe.
     The Tree widget already gives the row a soft accent fill via
     `.tree-row-selected`; here we add the stripe and bump the foreground
     to the accent text colour to keep parity with the legacy look. */
  :global(.tree .tree-row.file-active) {
    color: var(--accent);
    position: relative;
  }
  :global(.tree .tree-row.file-active::before) {
    content: '';
    position: absolute;
    left: 0; top: 0; bottom: 0;
    width: 2px;
    background: var(--accent);
    border-radius: 0 2px 2px 0;
    pointer-events: none;
  }

  /* ── Shared node parts ── */
  .node-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    /* Keep the icon at the intended size; vscode-icons are colorful SVGs */
    line-height: 0;
  }
  .node-name {
    flex: 1;
    min-width: 0;
  }

  .node-date {
    font-size: 10px;
    color: var(--text-disabled);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    margin-left: 2px;
  }
  :global(.tree .tree-row:hover .node-date) { color: var(--text-muted); }
  :global(.tree .tree-row.file-active .node-date) { color: var(--accent); opacity: 0.7; }

  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
</style>
