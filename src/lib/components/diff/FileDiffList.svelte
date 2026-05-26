<script lang="ts">
  import { List, FolderTree, ChevronRight, ChevronDown, Folder, FileSearch, Loader2 } from 'lucide-svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import type { DiffFile } from '$lib/types/git';
  import { tooltip } from '$lib/actions/tooltip';
  import { compactMiddleDirs } from '$lib/utils/file-tree/compact-middle-dirs';

  let { files }: { files: DiffFile[] } = $props();

  const viewMode = $derived(diffStore.fileListView);
  function setViewMode(v: 'list' | 'tree') { diffStore.setFileListView(v); }

  const STATUS_ICON: Record<string, string> = {
    added: 'A', modified: 'M', deleted: 'D',
    renamed: 'R', copied: 'C', untracked: 'U', binary: 'B',
  };
  const STATUS_COLOR: Record<string, string> = {
    added:     'var(--success)',
    modified:  'var(--warning)',
    deleted:   'var(--error)',
    renamed:   'var(--color-file-renamed)',
    copied:    'var(--color-file-renamed)',
    untracked: 'var(--color-file-untracked)',
    binary:    'var(--text-muted)',
  };

  // ---- Tree logic ----
  interface TreeNode {
    name: string;
    fullPath: string;
    children: Map<string, TreeNode>;
    /** Pre-sorted children (dirs first, then alphabetical) baked at build time
     *  so the template doesn't allocate + sort on every render. */
    sortedChildren: TreeNode[];
    file?: DiffFile;
  }

  function buildTree(files: DiffFile[]): TreeNode {
    const root: TreeNode = { name: '', fullPath: '', children: new Map(), sortedChildren: [] };
    for (const file of files) {
      const parts = file.path.split('/');
      let node = root;
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        if (!node.children.has(part)) {
          const fullPath = parts.slice(0, i + 1).join('/');
          node.children.set(part, { name: part, fullPath, children: new Map(), sortedChildren: [] });
        }
        node = node.children.get(part)!;
      }
      node.file = file;
    }
    function bakeSort(n: TreeNode) {
      n.sortedChildren = [...n.children.values()].sort((a, b) => {
        const aIsDir = a.children.size > 0;
        const bIsDir = b.children.size > 0;
        if (aIsDir !== bIsDir) return aIsDir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      for (const c of n.sortedChildren) bakeSort(c);
    }
    bakeSort(root);
    return root;
  }

  // IntelliJ-style "compact middle packages" — collapse single-child dir
  // chains so the tree stays vertically compact.
  const FILE_DIFF_ACCESSORS = {
    isDir:       (n: TreeNode) => n.children.size > 0,
    getName:     (n: TreeNode) => n.name,
    setName:     (n: TreeNode, name: string) => { n.name = name; },
    getChildren: (n: TreeNode) => n.sortedChildren,
    setChildren: (n: TreeNode, kids: TreeNode[]) => { n.sortedChildren = kids; },
  };
  function maybeCompactDiffTree(root: TreeNode): TreeNode {
    if (!appearanceStore.compactFileTreeDirs) return root;
    root.sortedChildren = compactMiddleDirs(root.sortedChildren, FILE_DIFF_ACCESSORS);
    const sortFn = (a: TreeNode, b: TreeNode) => {
      const aIsDir = a.children.size > 0, bIsDir = b.children.size > 0;
      if (aIsDir !== bIsDir) return aIsDir ? -1 : 1;
      return a.name.localeCompare(b.name);
    };
    const reSort = (n: TreeNode) => {
      n.sortedChildren.sort(sortFn);
      for (const c of n.sortedChildren) reSort(c);
    };
    reSort(root);
    return root;
  }
  const tree = $derived(maybeCompactDiffTree(buildTree(files)));

  let expandedPaths = $state<Set<string>>(new Set());

  function toggleDir(path: string) {
    const s = new Set(expandedPaths);
    if (s.has(path)) s.delete(path);
    else s.add(path);
    expandedPaths = s;
  }

  // Auto-expand all dirs when tree changes. Walk sortedChildren so the Set
  // matches the rendered structure when compact-middle-dirs is active.
  $effect(() => {
    const paths = new Set<string>();
    function collect(node: TreeNode) {
      if (node.sortedChildren.length > 0 && node.fullPath) paths.add(node.fullPath);
      for (const child of node.sortedChildren) collect(child);
    }
    collect(tree);
    expandedPaths = paths;
  });
</script>

<div class="file-list">
  <div class="list-header">
    <span>Files changed</span>
    <span class="count">{files.length}</span>
    {#if diffStore.isLoading && diffStore.totalExpected > 0}
      <span class="parse-progress" use:tooltip={'Parsing diffs…'}>
        <Loader2 size={11} class="spinner" />
        <span>{diffStore.parsedCount}/{diffStore.totalExpected}</span>
      </span>
    {/if}
    <div class="view-toggle">
      <button
        class="toggle-btn"
        class:active={viewMode === 'list'}
        use:tooltip={'List view'}
        onclick={() => setViewMode('list')}
      >
        <List size={11} />
      </button>
      <button
        class="toggle-btn"
        class:active={viewMode === 'tree'}
        use:tooltip={'Tree view'}
        onclick={() => setViewMode('tree')}
      >
        <FolderTree size={11} />
      </button>
    </div>
  </div>

  <div class="list-body">
    {#if viewMode === 'list'}
      {#each files as file (file.path)}
        <div
          class="file-item"
          class:selected={diffStore.selectedFile?.path === file.path}
          role="button"
          tabindex="0"
          onclick={() => diffStore.selectFile(file.path)}
          onkeydown={(e) => e.key === 'Enter' && diffStore.selectFile(file.path)}
          use:tooltip={file.path}
        >
          <span class="status-icon" style="color: {STATUS_COLOR[file.status] ?? 'var(--text-muted)'}">
            {STATUS_ICON[file.status] ?? '?'}
          </span>
          <span class="filename truncate">
            {file.old_path ? `${file.old_path} → ` : ''}{file.path.split('/').pop()}
          </span>
          {#if diffStore.pendingPaths.has(file.path)}
            <span class="parsing-badge" use:tooltip={'Parsing diff…'}>
              <Loader2 size={10} class="spinner" />
            </span>
          {:else if !file.is_binary}
            <span class="stats">
              {#if file.stats.additions > 0}<span class="add">+{file.stats.additions}</span>{/if}
              {#if file.stats.deletions > 0}<span class="del">-{file.stats.deletions}</span>{/if}
            </span>
          {:else}
            <span class="binary-badge">bin</span>
          {/if}
          <button
            class="filter-btn"
            use:tooltip={'Filter graph to commits touching this file'}
            onclick={(e) => { e.stopPropagation(); graphStore.filterByFile(file.path); }}
          >
            <FileSearch size={10} />
          </button>
        </div>
      {/each}

    {:else}
      <!-- Tree view -->
      {#snippet treeNode(node: TreeNode, depth: number)}
        {#each node.sortedChildren as child}
          {#if child.children.size > 0}
            <!-- Directory -->
            <button
              class="tree-dir"
              style="padding-left: {8 + depth * 12}px"
              onclick={() => toggleDir(child.fullPath)}
            >
              <span class="tree-chevron">
                {#if expandedPaths.has(child.fullPath)}<ChevronDown size={10} />{:else}<ChevronRight size={10} />{/if}
              </span>
              <Folder size={11} class="folder-icon" />
              <span class="dir-name">{child.name}</span>
            </button>
            {#if expandedPaths.has(child.fullPath)}
              {@render treeNode(child, depth + 1)}
            {/if}
          {:else if child.file}
            <!-- File -->
            {@const f = child.file}
            <div
              class="file-item tree-file"
              class:selected={diffStore.selectedFile?.path === f.path}
              style="padding-left: {8 + depth * 12 + 18}px"
              onclick={() => diffStore.selectFile(f.path)}
              onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); diffStore.selectFile(f.path); } }}
              use:tooltip={f.path}
              role="button"
              tabindex="0"
            >
              <span class="status-icon" style="color: {STATUS_COLOR[f.status] ?? 'var(--text-muted)'}">
                {STATUS_ICON[f.status] ?? '?'}
              </span>
              <span class="filename truncate">{child.name}</span>
              {#if diffStore.pendingPaths.has(f.path)}
                <span class="parsing-badge" use:tooltip={'Parsing diff…'}>
                  <Loader2 size={10} class="spinner" />
                </span>
              {:else if !f.is_binary}
                <span class="stats">
                  {#if f.stats.additions > 0}<span class="add">+{f.stats.additions}</span>{/if}
                  {#if f.stats.deletions > 0}<span class="del">-{f.stats.deletions}</span>{/if}
                </span>
              {:else}
                <span class="binary-badge">bin</span>
              {/if}
              <button
                class="filter-btn"
                use:tooltip={'Filter graph to commits touching this file'}
                onclick={(e) => { e.stopPropagation(); graphStore.filterByFile(f.path); }}
              >
                <FileSearch size={10} />
              </button>
            </div>
          {/if}
        {/each}
      {/snippet}
      {@render treeNode(tree, 0)}
    {/if}
  </div>
</div>

<style>
  .file-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-base);
    border-right: 1px solid var(--border);
    overflow: hidden;
  }

  .list-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .count {
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 999px;
    flex-shrink: 0;
  }

  .parse-progress {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--accent);
    font-variant-numeric: tabular-nums;
    font-size: 10px;
  }
  .parse-progress :global(svg.spinner),
  .parsing-badge :global(svg.spinner) {
    animation: arbor-spin 1s linear infinite;
  }

  .parsing-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
    flex-shrink: 0;
    min-width: 16px;
  }

  @keyframes arbor-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  .view-toggle {
    margin-left: auto;
    display: flex;
    gap: 2px;
  }

  .toggle-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px; height: 20px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .toggle-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .toggle-btn.active { background: var(--accent-subtle); color: var(--accent); }

  .list-body { flex: 1; overflow-y: auto; padding: 4px; }

  .file-item {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    padding: 3px 6px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    text-align: left;
    transition: background var(--transition-fast);
  }
  .file-item:hover   { background: var(--bg-hover); }
  .file-item.selected { background: var(--bg-selected); }

  .tree-dir {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    border: none;
    background: transparent;
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    padding-top: 2px;
    padding-bottom: 2px;
    transition: background var(--transition-fast), color var(--transition-fast);
    text-align: left;
  }
  .tree-dir:hover { background: var(--bg-hover); color: var(--text-primary); }

  .tree-chevron { display: flex; align-items: center; width: 12px; flex-shrink: 0; }

  :global(.folder-icon) { color: var(--warning); flex-shrink: 0; }

  .dir-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }

  .tree-file { gap: 5px; }

  .status-icon { font-weight: 700; flex-shrink: 0; font-size: 10px; width: 10px; text-align: center; }
  .filename { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .stats { display: flex; gap: 3px; flex-shrink: 0; font-family: var(--font-code); font-size: 10px; }
  .add { color: var(--success); }
  .del { color: var(--error); }

  .binary-badge {
    font-size: 9px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 3px;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }

  .filter-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    flex-shrink: 0;
    opacity: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
  }
  .file-item:hover .filter-btn,
  .tree-file:hover .filter-btn { opacity: 1; }
  .filter-btn:hover { color: var(--accent); background: var(--accent-subtle); }
</style>
