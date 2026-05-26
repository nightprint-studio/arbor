<script lang="ts">
  import {
    X, Search, ChevronRight, ChevronDown, Folder, FolderOpen,
    File, FileText, Image, Lock, GitFork, Archive,
    Star, Download, ExternalLink,
    GitBranch, Loader, AlertCircle, Copy, RefreshCw,
    ChevronLeft, Home, Package,
    Palette
  } from 'lucide-svelte';
  import Icon from '@iconify/svelte';
  import { getFileIcon, getFolderIcon } from '$lib/utils/file-icons';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { langColour, languageEntries } from '$lib/utils/language-colours';
  import { highlight, getLanguage } from '$lib/utils/diff-formatter';
  import { repoBrowserStore } from '$lib/stores/repoBrowser.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { cloneRepo, openRepo, closeRepo } from '$lib/ipc/graph';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import FilePickerModal from './FilePickerModal.svelte';
  import type { RemoteRepo, RemoteTreeEntry, NamespaceTreeNode } from '$lib/types/repoBrowser';
  import { animStore } from '$lib/stores/animations.svelte';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { matchesBinding } from '$lib/utils/keybindings';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import Dropdown from './ui/Dropdown.svelte';
  import ModalSidebarToggle from './ui/ModalSidebarToggle.svelte';
  import { fade } from 'svelte/transition';

  let { onClose }: { onClose: () => void } = $props();

  const store = repoBrowserStore;

  // Load accounts on first open
  let initialized = $state(false);
  $effect(() => {
    if (!initialized) {
      initialized = true;
      store.loadAccounts();
    }
  });

  // ── Account selector ───────────────────────────────────────────────────────
  function selectAccount(provider: string) {
    store.selectProvider(provider);
  }

  const activeAccount = $derived(
    store.accounts.find(a => a.provider === store.selectedProvider) ?? null
  );

  // ── Repo selection ─────────────────────────────────────────────────────────
  async function handleRepoClick(repo: RemoteRepo) {
    await store.selectRepo(repo);
  }

  // ── Local tab check ────────────────────────────────────────────────────────
  function findLocalTab(repo: RemoteRepo): string | null {
    const cloneUrl = repo.clone_url_https.replace(/\.git$/, '').toLowerCase();
    for (const tab of tabsStore.tabs) {
      const name = tab.path.replace(/\\/g, '/').split('/').pop()?.toLowerCase() ?? '';
      if (name === repo.name.toLowerCase()) return tab.id;
    }
    return null;
  }

  function openLocalTab(tabId: string) {
    tabsStore.setActive(tabId);
    onClose();
  }

  // ── Clone ──────────────────────────────────────────────────────────────────
  let cloning          = $state(false);
  let cloneError       = $state<string | null>(null);
  let showClonePicker  = $state(false);
  let cloneTargetRepo  = $state<RemoteRepo | null>(null);

  function handleClone(repo: RemoteRepo) {
    cloneTargetRepo = repo;
    showClonePicker = true;
  }

  async function onClonePickerConfirm(folder: string) {
    if (!cloneTargetRepo) return;
    const repo = cloneTargetRepo;
    showClonePicker = false;
    const destPath = `${folder.replace(/[\\/]+$/, '')}/${repo.name}`;
    cloning    = true;
    cloneError = null;

    // Phase 1: the actual clone. Only failures here keep the spinner +
    // surface an inline error — once `cloneRepo` resolves the user-facing
    // operation is "done" as far as the modal is concerned.
    const tempTabId = crypto.randomUUID();
    let cloned;
    try {
      cloned = await cloneRepo(
        { url: repo.clone_url_https, dest_path: destPath },
        tempTabId,
      );
    } catch (err) {
      cloneError = String(err).replace(/^.*error:/i, '').trim();
      cloning    = false;
      return;
    }

    // Phase 2: tell the user it's done and dismiss the browser. Doing this
    // BEFORE the post-clone setup means a slow/failing follow-up step
    // (registry, openRepo, plugin on_repo_open hook, …) can't strand the
    // modal in a perpetual "Cloning…" state.
    cloning = false;
    uiStore.showToast(`Cloned ${repo.name}`, 'success');
    onClose();

    // Phase 3: best-effort post-clone setup — swap the temp tab for one
    // keyed by the canonical workspace-registry id so the new repo is a
    // first-class member of the active workspace. If any step here fails,
    // log it and surface a follow-up toast; the clone itself already
    // succeeded so we don't undo anything.
    try { await closeRepo(tempTabId); } catch { /* best effort */ }
    try {
      const repoId = await workspacesStore.ensureRepoRegistered(cloned.path);
      const info   = await openRepo(cloned.path, repoId);
      tabsStore.addTab(info);
      uiStore.addRecentRepo(info.path);
    } catch (err) {
      console.warn('[RepoBrowserModal] post-clone setup failed', err);
      uiStore.showToast(`Couldn't open ${repo.name}: ${String(err)}`, 'error');
    }
  }

  // ── File tree entry click ──────────────────────────────────────────────────
  async function handleEntryClick(entry: RemoteTreeEntry) {
    if (entry.entry_type === 'dir') {
      await store.navigateToDir(entry);
    } else if (entry.entry_type === 'file') {
      await store.selectFile(entry);
    }
  }

  // ── Download file ─────────────────────────────────────────────────────────
  let downloadingFile     = $state(false);
  let showDownloadPicker  = $state(false);

  function handleDownloadFile() {
    if (!store.selectedFile) return;
    showDownloadPicker = true;
  }

  async function onDownloadPickerConfirm(folder: string) {
    if (!store.selectedFile) return;
    showDownloadPicker = false;
    const destPath = `${folder.replace(/[\\/]+$/, '')}/${store.selectedFile.name}`;
    downloadingFile = true;
    try {
      await store.downloadFile(store.selectedFile, destPath);
      uiStore.showToast(`Saved ${store.selectedFile.name}`, 'success');
    } catch (err) {
      uiStore.showToast(`Download failed: ${err}`, 'error');
    } finally {
      downloadingFile = false;
    }
  }

  // ── Open in browser ───────────────────────────────────────────────────────
  function openWebUrl(url: string) {
    window.open(url, '_blank');
  }

  // ── Copy file content ─────────────────────────────────────────────────────
  async function copyFileContent() {
    if (!store.fileContent?.content) return;
    await copyToClipboard(store.fileContent.content, { successToast: 'Copied to clipboard' });
  }

  // ── Format helpers ────────────────────────────────────────────────────────
  function formatSize(kb: number | null): string {
    if (kb == null) return '';
    if (kb < 1024) return `${kb} KB`;
    return `${(kb / 1024).toFixed(1)} MB`;
  }

  function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  function timeAgo(iso: string): string {
    if (!iso) return '';
    return timeAgoMs(new Date(iso).getTime());
  }

  function timeAgoMs(ts: number): string {
    if (!ts) return '';
    const ms   = Date.now() - ts;
    const secs = Math.floor(ms / 1000);
    if (secs < 5)    return 'just now';
    if (secs < 60)   return `${secs}s ago`;
    const mins = Math.floor(secs / 60);
    if (mins < 60)   return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24)  return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 30)   return `${days}d ago`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months}mo ago`;
    return `${Math.floor(months / 12)}y ago`;
  }

  // Short variant for sidebar — "12d" vs "12d ago"
  function timeAgoShort(iso: string): string {
    if (!iso) return '';
    const ms   = Date.now() - new Date(iso).getTime();
    const secs = Math.floor(ms / 1000);
    if (secs < 60)   return `${secs}s`;
    const mins = Math.floor(secs / 60);
    if (mins < 60)   return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 24)  return `${hours}h`;
    const days = Math.floor(hours / 24);
    if (days < 30)   return `${days}d`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months}mo`;
    return `${Math.floor(months / 12)}y`;
  }

  function formatStars(n: number): string {
    if (n < 1000) return String(n);
    if (n < 10000) return `${(n / 1000).toFixed(1)}k`;
    return `${Math.round(n / 1000)}k`;
  }

  // ── Syntax-highlight language from mime ───────────────────────────────────
  function langLabel(mime: string | null): string {
    if (!mime) return '';
    const map: Record<string, string> = {
      'text/x-rust': 'Rust', 'text/typescript': 'TypeScript',
      'text/javascript': 'JavaScript', 'text/x-python': 'Python',
      'text/x-go': 'Go', 'text/x-java': 'Java', 'text/x-c': 'C',
      'text/x-c++': 'C++', 'text/x-csharp': 'C#', 'text/x-ruby': 'Ruby',
      'text/x-php': 'PHP', 'text/x-sh': 'Shell', 'text/html': 'HTML',
      'text/css': 'CSS', 'application/json': 'JSON', 'text/xml': 'XML',
      'text/x-sql': 'SQL', 'text/markdown': 'Markdown',
      'application/typescript': 'TypeScript',
    };
    return map[mime] ?? '';
  }

  // ── Namespace tree helpers ────────────────────────────────────────────────
  function countRepos(node: NamespaceTreeNode): number {
    return node.repos.length + node.children.reduce((s, c) => s + countRepos(c), 0);
  }

  // ── Sidebar collapse ──────────────────────────────────────────────────────
  let sidebarCollapsed = $state(false);

  // ── Language legend popover ───────────────────────────────────────────────
  let legendOpen = $state(false);
  const legendEntries = languageEntries();

  // ── Syntax highlighting ───────────────────────────────────────────────────
  const highlightedContent = $derived.by(() => {
    if (!store.fileContent?.content || !store.selectedFile) return '';
    return highlight(store.fileContent.content, store.selectedFile.path);
  });

  // Modal-scoped Ctrl+B (toggle_sidebar) — hijack in capture phase so the
  // main layout's window listener doesn't also fire and toggle the app
  // sidebar underneath the modal.
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
</script>

<Modal {onClose} size="full" padBody={false} ariaLabel="Repository Browser">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ModalSidebarToggle
        collapsed={sidebarCollapsed}
        onToggle={() => sidebarCollapsed = !sidebarCollapsed}
        label={sidebarCollapsed ? 'Show file list' : 'Hide file list'}
      />
      <Package size={16} />
      <span class="modal-title">Repository Browser</span>
    </ModalHeader>
  {/snippet}

  <!-- ── Body ──────────────────────────────────────────────────────────────── -->
  <div class="rb-body">

    <!-- ════════════════════════════════ LEFT PANEL ═══════════════════════════ -->
    <div class="rb-left" class:collapsed={sidebarCollapsed}>

      <!-- Account selector -->
      <div class="rb-account-bar">
        {#if store.accounts.length === 0}
          <span class="rb-no-accounts">No accounts connected</span>
        {:else}
          <Dropdown position="absolute" class="rb-account-dd" width="100%">
            {#snippet trigger({ open, toggle })}
              <button
                class="rb-account-btn"
                onclick={toggle}
                aria-expanded={open}
                aria-haspopup="menu"
              >
                {#if activeAccount?.avatar_url}
                  <img class="rb-avatar" src={activeAccount.avatar_url} alt="" />
                {:else}
                  <div class="rb-avatar-placeholder">
                    {activeAccount?.username[0]?.toUpperCase() ?? '?'}
                  </div>
                {/if}
                <span class="rb-account-info">
                  <span class="rb-account-name">{activeAccount?.username ?? '—'}</span>
                  <span class="rb-account-provider">{activeAccount?.provider ?? ''}</span>
                </span>
                <ChevronDown size={12} class="rb-account-chev" />
              </button>
            {/snippet}

            {#snippet children({ close })}
              {#each store.accounts as acc}
                <button
                  class="rb-account-menu-item"
                  class:active={acc.provider === store.selectedProvider}
                  role="menuitem"
                  onclick={() => { selectAccount(acc.provider); close(); }}
                >
                  {#if acc.avatar_url}
                    <img class="rb-avatar-sm" src={acc.avatar_url} alt="" />
                  {:else}
                    <div class="rb-avatar-sm rb-avatar-placeholder">
                      {acc.username[0]?.toUpperCase()}
                    </div>
                  {/if}
                  <div class="rb-account-menu-info">
                    <span class="rb-account-menu-name">{acc.username}</span>
                    <span class="rb-account-menu-prov">{acc.provider}</span>
                  </div>
                </button>
              {/each}
            {/snippet}
          </Dropdown>
        {/if}
      </div>

      <!-- Search -->
      <div class="rb-search-wrap">
        <Search size={13} class="rb-search-icon" />
        <input
          class="rb-search"
          type="text"
          placeholder="Search repositories…"
          bind:value={store.searchQuery}
          autocomplete="off"
          spellcheck="false"
        />
        {#if store.searchQuery}
          <button
            class="rb-search-clear"
            onclick={() => { store.searchQuery = ''; }}
            aria-label="Clear search"
          ><X size={12} /></button>
        {/if}
      </div>

      <!-- Cache info + refresh.  The repo list is cached in localStorage
           with a TTL (configurable in Settings → Cache) — this strip shows
           when the data was last fetched and lets the user force-refresh. -->
      {#if store.selectedProvider && store.repos.length > 0 && !store.reposError}
        <div class="rb-cache-strip">
          <span class="rb-cache-meta">
            {#if store.reposFetchedAt}
              {store.reposFromCache ? 'Cached' : 'Updated'} · {timeAgoMs(store.reposFetchedAt)}
            {/if}
          </span>
          <button
            class="rb-refresh-btn"
            onclick={() => store.refreshRepos()}
            disabled={store.reposLoading}
            use:tooltip={{ content: 'Force refresh', description: 'Bypass cache' }}
            aria-label="Refresh repository list"
          >
            <RefreshCw size={11} class={store.reposLoading ? 'spin' : ''} />
            <span>Refresh</span>
          </button>
        </div>
      {/if}

      <!-- Repo list / loading / error -->
      <div class="rb-repo-list">
        {#if store.reposLoading}
          <div class="rb-state">
            <Loader size={20} class="spin rb-state-icon" />
            <span class="rb-state-title">Loading repositories…</span>
          </div>
        {:else if store.reposError}
          <div class="rb-state rb-state-error">
            <AlertCircle size={20} class="rb-state-icon" />
            <span class="rb-state-title">Couldn't load repositories</span>
            <span class="rb-state-msg">{store.reposError}</span>
          </div>
        {:else if store.namespaceGroups.length === 0}
          <div class="rb-state">
            {#if store.searchQuery}
              <Search size={20} class="rb-state-icon" />
              <span class="rb-state-title">No matches</span>
              <span class="rb-state-msg">Try a different query</span>
            {:else}
              <Package size={20} class="rb-state-icon" />
              <span class="rb-state-title">No repositories</span>
              <span class="rb-state-msg">Connect an account in Settings → Accounts</span>
            {/if}
          </div>
        {:else}
          {#snippet nsNode(node: NamespaceTreeNode, depth: number)}
            <div class="rb-ns-group">
              <button
                class="rb-ns-header"
                class:expanded={node.expanded}
                style="padding-left: {6 + depth * 12}px"
                onclick={() => store.toggleNamespace(node.fullPath)}
              >
                <span class="rb-ns-chev">
                  {#if node.expanded}
                    <ChevronDown size={11} />
                  {:else}
                    <ChevronRight size={11} />
                  {/if}
                </span>
                <span class="rb-ns-folder">
                  {#if node.expanded}
                    <FolderOpen size={12} />
                  {:else}
                    <Folder size={12} />
                  {/if}
                </span>
                <span class="rb-ns-name">{node.segment}</span>
                <span class="rb-ns-count">{countRepos(node)}</span>
              </button>

              {#if node.expanded}
                {#each node.children as child}
                  {@render nsNode(child, depth + 1)}
                {/each}
                {#each node.repos as repo}
                  {@const localTabId = findLocalTab(repo)}
                  <button
                    class="rb-repo-item"
                    style="padding-left: {18 + depth * 12}px"
                    class:active={store.selectedRepo?.id === repo.id &&
                                  store.selectedRepo?.provider === repo.provider}
                    class:archived={repo.is_archived}
                    onclick={() => handleRepoClick(repo)}
                  >
                    <span class="rb-repo-name">
                      {#if repo.private}
                        <Lock size={10} class="rb-repo-lock" />
                      {/if}
                      {#if repo.is_fork}
                        <GitFork size={10} class="rb-repo-fork" />
                      {/if}
                      {#if repo.is_archived}
                        <Archive size={10} class="rb-repo-archived" />
                      {/if}
                      <span class="rb-repo-name-text">{repo.name}</span>
                    </span>
                    {#if localTabId}
                      <span class="rb-repo-open-dot" use:tooltip={'Open locally'}></span>
                    {/if}
                    {#if repo.stars > 0}
                      <span class="rb-repo-stars" use:tooltip={`${repo.stars} stars`}>
                        <Star size={9} />
                        {formatStars(repo.stars)}
                      </span>
                    {/if}
                    {#if repo.language}
                      <span
                        class="rb-lang-dot"
                        style="background: {langColour(repo.language)}"
                        use:tooltip={repo.language}
                      ></span>
                    {/if}
                    {#if repo.updated_at}
                      <span class="rb-repo-updated" use:tooltip={`Updated ${timeAgo(repo.updated_at)}`}>
                        {timeAgoShort(repo.updated_at)}
                      </span>
                    {/if}
                  </button>
                {/each}
              {/if}
            </div>
          {/snippet}

          {#each store.namespaceGroups as node}
            {@render nsNode(node, 0)}
          {/each}
        {/if}
      </div>

      <!-- Repo count footer + language legend toggle -->
      {#if !store.reposLoading && store.repos.length > 0}
        <div class="rb-list-footer">
          <span class="rb-list-count">
            {#if store.searchQuery}
              {store.filteredRepos.length} of {store.repos.length} repos
            {:else}
              {store.repos.length} repositories
            {/if}
          </span>
          <button
            class="rb-legend-btn"
            class:active={legendOpen}
            onclick={() => legendOpen = !legendOpen}
            use:tooltip={'Language colour legend'}
            aria-label="Toggle language colour legend"
            aria-expanded={legendOpen}
          >
            <Palette size={11} />
            <span>Legend</span>
          </button>
        </div>

        {#if legendOpen}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            class="rb-legend-pop"
            role="dialog"
            aria-label="Language colour legend"
            transition:fade={{ duration: animStore.dFast }}
          >
            <div class="rb-legend-head">
              <Palette size={12} />
              <span>Languages</span>
              <button
                class="rb-legend-close"
                onclick={() => legendOpen = false}
                aria-label="Close legend"
              ><X size={12} /></button>
            </div>
            <div class="rb-legend-grid">
              {#each legendEntries as [name, colour]}
                <div class="rb-legend-item">
                  <span class="rb-lang-dot" style="background: {colour}"></span>
                  <span class="rb-legend-name">{name}</span>
                </div>
              {/each}
            </div>
          </div>
          <!-- click-outside backdrop -->
          <div
            class="rb-legend-backdrop"
            role="presentation"
            onclick={() => legendOpen = false}
          ></div>
        {/if}
      {/if}
    </div>

    <!-- ════════════════════════════════ RIGHT PANEL ══════════════════════════ -->
    <div class="rb-right">

      {#if !store.selectedRepo}
        <!-- Empty state -->
        <div class="rb-right-empty">
          <Package size={40} />
          <p>Select a repository to browse its files</p>
        </div>

      {:else}
        <!-- ── Repo info bar ──────────────────────────────────────────────────── -->
        {@const repo = store.selectedRepo}
        {@const localTabId = findLocalTab(repo)}
        <div class="rb-repo-bar">
          <div class="rb-repo-meta">
            <div class="rb-repo-full-name">
              {#if repo.private}<Lock size={12} />{/if}
              <span>{repo.full_name}</span>
            </div>
            {#if repo.description}
              <span class="rb-repo-desc">{repo.description}</span>
            {/if}
            <div class="rb-repo-tags">
              {#if repo.language}
                <span class="rb-tag">{repo.language}</span>
              {/if}
              {#if repo.stars > 0}
                <span class="rb-tag"><Star size={10} /> {repo.stars}</span>
              {/if}
              {#if repo.size_kb}
                <span class="rb-tag">{formatSize(repo.size_kb)}</span>
              {/if}
              <span class="rb-tag">{timeAgo(repo.updated_at)}</span>
            </div>
          </div>

          <div class="rb-repo-actions">
            <!-- Branch badge -->
            <div class="rb-branch-badge">
              <GitBranch size={12} />
              <span>{store.currentBranch}</span>
            </div>

            <!-- Clone / Open Tab -->
            {#if localTabId}
              <button
                class="rb-action-btn rb-action-btn--primary"
                onclick={() => openLocalTab(localTabId)}
                use:tooltip={'Switch to open tab'}
              >
                <FolderOpen size={13} /> Open Tab
              </button>
            {:else}
              <button
                class="rb-action-btn rb-action-btn--primary"
                onclick={() => handleClone(repo)}
                disabled={cloning}
                use:tooltip={'Clone and open this repository'}
              >
                {#if cloning}
                  <Loader size={13} class="spin" /> Cloning…
                {:else}
                  <Download size={13} /> Clone
                {/if}
              </button>
            {/if}

            <button
              class="rb-action-btn"
              onclick={() => openWebUrl(repo.web_url)}
              use:tooltip={'Open in browser'}
            >
              <ExternalLink size={13} />
            </button>
          </div>
        </div>

        {#if cloneError}
          <div class="rb-clone-error">
            <AlertCircle size={13} />
            {cloneError}
          </div>
        {/if}

        <!-- ── File tree area ─────────────────────────────────────────────────── -->
        <div class="rb-file-area" class:has-preview={store.selectedFile !== null}>

          <!-- Breadcrumb -->
          <div class="rb-breadcrumb">
            <button
              class="rb-bc-seg"
              class:active={store.breadcrumbs.length === 0}
              onclick={() => store.navigateToBreadcrumb(-1)}
            >
              <Home size={12} />
              <span>{repo.name}</span>
            </button>
            {#each store.breadcrumbs as seg, i}
              <ChevronRight size={11} class="rb-bc-sep" />
              <button
                class="rb-bc-seg"
                class:active={i === store.breadcrumbs.length - 1}
                onclick={() => store.navigateToBreadcrumb(i)}
              >
                {seg.name}
              </button>
            {/each}
          </div>

          <!-- Tree entries -->
          <div class="rb-tree">
            {#if store.treeLoading}
              <div class="rb-tree-loading">
                <Loader size={14} class="spin" />
                <span>Loading…</span>
              </div>
            {:else if store.treeError}
              <div class="rb-tree-error">
                <AlertCircle size={13} />
                <span>{store.treeError}</span>
              </div>
            {:else if store.treeEntries.length === 0}
              <div class="rb-tree-empty">Empty directory</div>
            {:else}
              <!-- Back button when inside a subdirectory -->
              {#if store.breadcrumbs.length > 0}
                <button
                  class="rb-tree-entry rb-tree-entry--back"
                  onclick={() => store.navigateToBreadcrumb(store.breadcrumbs.length - 2)}
                >
                  <ChevronLeft size={14} />
                  <span>..</span>
                </button>
              {/if}

              {#each store.treeEntries as entry}
                <button
                  class="rb-tree-entry"
                  class:active={store.selectedFile?.path === entry.path}
                  class:is-dir={entry.entry_type === 'dir'}
                  onclick={() => handleEntryClick(entry)}
                >
                  <span class="rb-entry-icon">
                    {#if entry.entry_type === 'dir'}
                      <Icon icon={getFolderIcon(entry.name, false)} width={15} height={15} />
                    {:else if entry.entry_type === 'submodule'}
                      <Package size={14} />
                    {:else}
                      <Icon icon={getFileIcon(entry.name)} width={15} height={15} />
                    {/if}
                  </span>
                  <span class="rb-entry-name">{entry.name}</span>
                  {#if entry.size != null && entry.entry_type === 'file'}
                    <span class="rb-entry-size">{formatFileSize(entry.size)}</span>
                  {/if}
                </button>
              {/each}
            {/if}
          </div>
        </div>

        <!-- ── File preview ───────────────────────────────────────────────────── -->
        {#if store.selectedFile}
          <div class="rb-preview">
            <div class="rb-preview-header">
              <div class="rb-preview-file-info">
                <FileText size={13} />
                <span class="rb-preview-filename">{store.selectedFile.name}</span>
                {#if store.fileContent}
                  <span class="rb-preview-size">
                    {formatFileSize(store.fileContent.size)}
                  </span>
                  {#if langLabel(store.fileContent.mime_type)}
                    <span class="rb-preview-lang">
                      {langLabel(store.fileContent.mime_type)}
                    </span>
                  {/if}
                {/if}
              </div>
              <div class="rb-preview-actions">
                {#if store.fileContent?.content}
                  <button
                    class="rb-icon-btn"
                    onclick={copyFileContent}
                    use:tooltip={'Copy content'}
                  ><Copy size={13} /></button>
                {/if}
                <button
                  class="rb-icon-btn"
                  onclick={handleDownloadFile}
                  disabled={downloadingFile || store.fileLoading}
                  use:tooltip={'Download file'}
                >
                  {#if downloadingFile}
                    <Loader size={13} class="spin" />
                  {:else}
                    <Download size={13} />
                  {/if}
                </button>
                <div class="rb-preview-sep"></div>
                <button
                  class="mac-close-btn"
                  onclick={() => store.closeFilePreview()}
                  use:tooltip={'Close preview'}
                  aria-label="Close preview"
                ></button>
              </div>
            </div>

            <div class="rb-preview-body">
              {#if store.fileLoading}
                <div class="rb-preview-loading">
                  <Loader size={18} class="spin" />
                </div>

              {:else if store.fileError}
                <div class="rb-preview-message rb-preview-error">
                  <AlertCircle size={16} />
                  <span>{store.fileError}</span>
                </div>

              {:else if store.fileContent}
                {#if store.fileContent.is_image && store.fileContent.image_data}
                  <div class="rb-preview-image">
                    <img src={store.fileContent.image_data} alt={store.selectedFile.name} />
                  </div>

                {:else if store.fileContent.is_image && !store.fileContent.image_data}
                  <div class="rb-preview-message">
                    <Image size={24} />
                    <span>Image too large to preview</span>
                    <button class="rb-action-btn" onclick={handleDownloadFile}>
                      <Download size={13} /> Download
                    </button>
                  </div>

                {:else if store.fileContent.is_binary}
                  <div class="rb-preview-message">
                    <File size={24} />
                    <span>Binary file — {formatFileSize(store.fileContent.size)}</span>
                    <button class="rb-action-btn" onclick={handleDownloadFile}>
                      <Download size={13} /> Download
                    </button>
                  </div>

                {:else}
                  <pre class="rb-code language-{getLanguage(store.selectedFile.path)}"><code>{@html highlightedContent}</code></pre>
                {/if}
              {/if}
            </div>
          </div>
        {/if}

      {/if}
    </div>
  </div>
</Modal>

<!-- File pickers must come AFTER the main Modal so they paint above it.
     At equal z-index, the later DOM node wins the stacking contest — putting
     these before the Modal causes the picker dialog to render UNDER the
     repository browser. -->
{#if showClonePicker && cloneTargetRepo}
  <FilePickerModal
    mode="folder"
    title="Choose clone destination"
    onConfirm={onClonePickerConfirm}
    onCancel={() => showClonePicker = false}
  />
{/if}

{#if showDownloadPicker && store.selectedFile}
  <FilePickerModal
    mode="folder"
    title="Save to folder"
    onConfirm={onDownloadPickerConfirm}
    onCancel={() => showDownloadPicker = false}
  />
{/if}

<style>
  /* ── Body ────────────────────────────────────────────────────────────────── */
  /* Standard Arbor modal rhythm: chrome on --bg-elevated, two --bg-base
     cards with rounded corners floating on it, separated by a thin gap. */
  .rb-body {
    display: flex;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }

  /* ════════════════════════════════════ LEFT PANEL ═══════════════════════════ */
  .rb-left {
    width: 280px;
    min-width: 280px;
    max-width: 280px;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    overflow: hidden;
    flex-shrink: 0;
    position: relative;
    transition: width var(--anim-dur-panel) ease, min-width var(--anim-dur-panel) ease,
                margin-right var(--anim-dur-panel) ease;
  }
  .rb-left.collapsed {
    width: 0;
    min-width: 0;
    overflow: hidden;
    margin-right: -4px;
  }

  /* Account selector */
  .rb-account-bar {
    padding: 8px 8px 6px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .rb-no-accounts {
    font-size: 11px;
    color: var(--text-muted);
    padding: 8px 4px;
    text-align: center;
    display: block;
  }
  /* Dropdown root — full width to match the account bar */
  :global(.rb-account-dd) { width: 100%; }
  .rb-account-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    color: var(--text-primary);
    cursor: pointer;
    font-size: 12px;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .rb-account-btn:hover {
    border-color: var(--border);
    background: var(--bg-hover);
  }
  .rb-account-btn[aria-expanded="true"] {
    border-color: var(--accent);
  }
  .rb-avatar {
    width: 26px; height: 26px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  .rb-avatar-sm {
    width: 24px; height: 24px;
    border-radius: 50%;
    object-fit: cover;
    flex-shrink: 0;
  }
  .rb-avatar-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-subtle);
    color: var(--accent);
    font-size: 11px;
    font-weight: 700;
    flex-shrink: 0;
    width: 26px; height: 26px;
    border-radius: 50%;
  }
  .rb-account-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }
  .rb-account-name {
    width: 100%;
    text-align: left;
    font-weight: 500;
    font-size: 12px;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rb-account-provider {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: capitalize;
    line-height: 1.2;
  }
  .rb-account-chev {
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .rb-account-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 12px;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast);
  }
  .rb-account-menu-item:hover  { background: var(--bg-hover); }
  .rb-account-menu-item.active { background: var(--accent-subtle); }
  .rb-account-menu-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
  }
  .rb-account-menu-name { font-weight: 500; }
  .rb-account-menu-prov {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: capitalize;
  }

  /* Search */
  .rb-search-wrap {
    position: relative;
    padding: 8px 10px 6px;
    flex-shrink: 0;
  }
  :global(.rb-search-icon) {
    position: absolute;
    left: 18px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-muted);
    pointer-events: none;
  }
  .rb-search {
    width: 100%;
    padding: 5px 26px 5px 28px;
    border-radius: 5px;
    border: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 12px;
    outline: none;
    transition: border-color var(--transition-fast), background var(--transition-fast);
    box-sizing: border-box;
  }
  .rb-search:hover { border-color: var(--border); }
  .rb-search:focus { border-color: var(--accent); }
  .rb-search::placeholder { color: var(--text-muted); }
  .rb-search-clear {
    position: absolute;
    right: 16px;
    top: 50%;
    transform: translateY(-50%);
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px;
    border-radius: var(--radius-sm);
    display: flex;
  }
  .rb-search-clear:hover { color: var(--text-primary); }

  /* Cache strip — between the search input and the repo list. */
  .rb-cache-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 12px 6px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .rb-cache-meta {
    font-size: 10.5px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rb-refresh-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
  .rb-refresh-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
  .rb-refresh-btn:disabled { opacity: 0.4; cursor: default; }

  /* Repo list */
  .rb-repo-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }
  /* Generic state block (loading / error / empty) */
  .rb-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 28px 18px;
    text-align: center;
  }
  :global(.rb-state-icon) {
    color: var(--text-muted);
    margin-bottom: 4px;
  }
  .rb-state-title {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
  }
  .rb-state-msg {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.4;
    max-width: 220px;
  }
  .rb-state-error :global(.rb-state-icon) { color: var(--error, #f87171); }
  .rb-state-error .rb-state-title { color: var(--error, #f87171); }

  .rb-ns-group { margin-bottom: 1px; }
  .rb-ns-header {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 4px 10px 4px 6px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .rb-ns-header:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
  .rb-ns-chev {
    display: flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .rb-ns-folder {
    display: flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .rb-ns-header.expanded .rb-ns-folder { color: var(--accent); }
  .rb-ns-name {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-left: 2px;
  }
  .rb-ns-count {
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    padding: 0px 6px;
    border-radius: 9px;
    flex-shrink: 0;
    font-weight: 500;
    min-width: 20px;
    text-align: center;
  }

  /* ── Repo row — single dense line ───────────────────────────────────────── */
  .rb-repo-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 10px 5px 18px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
    border-left: 2px solid transparent;
  }
  .rb-repo-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .rb-repo-item.active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-left-color: var(--accent);
  }
  .rb-repo-item.archived .rb-repo-name-text { text-decoration: line-through; opacity: 0.7; }

  .rb-repo-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .rb-repo-name-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.rb-repo-lock) { color: var(--text-muted); flex-shrink: 0; }
  :global(.rb-repo-fork) { color: var(--text-muted); flex-shrink: 0; }
  :global(.rb-repo-archived) { color: var(--text-disabled); flex-shrink: 0; }

  .rb-repo-open-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--success);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--success) 25%, transparent);
    flex-shrink: 0;
  }
  .rb-repo-stars {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
  .rb-repo-item.active .rb-repo-stars,
  .rb-repo-item.active .rb-repo-updated { color: var(--accent); }

  .rb-lang-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
  }
  .rb-repo-updated {
    font-size: 10px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  .rb-list-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 5px 8px 5px 12px;
    font-size: 10px;
    color: var(--text-disabled);
    border-top: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .rb-list-count {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rb-legend-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    padding: 2px 7px;
    font-size: 10px;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
    flex-shrink: 0;
  }
  .rb-legend-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
  .rb-legend-btn.active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }

  /* Floating legend popover anchored above the footer */
  .rb-legend-pop {
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: 32px;
    max-height: 50%;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0,0,0,0.35);
    z-index: 11;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .rb-legend-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 600;
    flex-shrink: 0;
    background: var(--bg-elevated);
  }
  .rb-legend-head > span { flex: 1; }
  .rb-legend-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .rb-legend-close:hover { background: var(--bg-hover); color: var(--text-primary); }

  .rb-legend-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 10px;
    padding: 8px 10px;
    overflow-y: auto;
    min-height: 0;
  }
  .rb-legend-item {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
    line-height: 1.5;
  }
  .rb-legend-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rb-legend-backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
  }

  /* ════════════════════════════════════ RIGHT PANEL ═══════════════════════════ */
  .rb-right {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  /* When a file preview is open, the right pane mirrors the outer body's
     rhythm — it becomes a mini-frame on --bg-elevated holding two
     --bg-base sub-cards (tree on top, preview below) with a 4px gap. */
  .rb-right:has(.rb-preview) {
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }
  .rb-right:has(.rb-preview) > .rb-repo-bar {
    background: var(--bg-base);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    /* Eat the parent gap so repo-bar joins the file-area as one card. */
    margin-bottom: -4px;
  }
  .rb-right:has(.rb-preview) > .rb-file-area {
    background: var(--bg-base);
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    border-bottom: none;
  }
  .rb-right:has(.rb-preview) > .rb-preview {
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    border-top: none;
  }

  .rb-right-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 100%;
    color: var(--text-disabled);
    font-size: 13px;
  }
  .rb-right-empty p { margin: 0; }

  /* Repo bar */
  .rb-repo-bar {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    background: transparent;
  }
  .rb-repo-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .rb-repo-full-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .rb-repo-desc {
    font-size: 11px;
    color: var(--text-muted);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .rb-repo-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 2px;
  }
  .rb-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 500;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    padding: 2px 8px;
    border-radius: 999px;
    line-height: 1.4;
  }
  .rb-tag :global(svg) { color: var(--text-muted); }

  .rb-repo-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .rb-branch-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    padding: 3px 10px;
    border-radius: 999px;
    line-height: 1.4;
  }
  .rb-branch-badge :global(svg) { color: var(--text-muted); }
  .rb-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--bg-overlay);
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .rb-action-btn:hover { background: var(--bg-hover); border-color: var(--border); color: var(--text-primary); }
  .rb-action-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .rb-action-btn--primary {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 500;
  }
  .rb-action-btn--primary:hover {
    background: var(--accent);
    color: var(--text-on-accent);
  }
  .rb-clone-error {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 16px;
    font-size: 11px;
    color: var(--diff-del-fg, #f87171);
    background: var(--diff-del-bg);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  /* File area */
  .rb-file-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .rb-file-area.has-preview {
    flex: 0 0 auto;
    max-height: 45%;
    border-bottom: 1px solid var(--border-subtle);
  }

  /* Breadcrumb */
  .rb-breadcrumb {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    overflow-x: auto;
    scrollbar-width: none;
    background: transparent;
  }
  .rb-breadcrumb::-webkit-scrollbar { display: none; }
  .rb-bc-seg {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 12px;
    white-space: nowrap;
    transition: background var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .rb-bc-seg:hover { background: var(--bg-hover); color: var(--text-primary); }
  .rb-bc-seg.active { color: var(--text-primary); font-weight: 500; cursor: default; }
  :global(.rb-bc-sep) { color: var(--text-disabled); flex-shrink: 0; }

  /* Tree */
  .rb-tree {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }
  .rb-tree-loading,
  .rb-tree-error,
  .rb-tree-empty {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 16px;
    font-size: 12px;
    color: var(--text-muted);
  }
  .rb-tree-entry {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 4px 14px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 13px;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
    font-family: var(--font-code);
  }
  .rb-tree-entry:hover { background: var(--bg-hover); color: var(--text-primary); }
  .rb-tree-entry.active { background: var(--accent-subtle); color: var(--accent); }
  .rb-tree-entry.is-dir { color: var(--text-primary); font-weight: 500; }
  .rb-tree-entry--back { color: var(--text-muted); font-family: var(--font-ui-sans); font-size: 12px; }
  .rb-entry-icon { flex-shrink: 0; color: var(--text-muted); display: flex; }
  .rb-tree-entry.is-dir .rb-entry-icon { color: var(--accent); }
  .rb-entry-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rb-entry-size { font-size: 11px; color: var(--text-disabled); flex-shrink: 0; }

  /* Preview */
  .rb-preview {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border-top: 1px solid var(--border-subtle);
  }
  .rb-preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    background: transparent;
    gap: 8px;
  }
  .rb-preview-file-info {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    overflow: hidden;
  }
  .rb-preview-filename {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
    font-family: var(--font-code);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rb-preview-size {
    font-size: 11px;
    color: var(--text-disabled);
    flex-shrink: 0;
  }
  .rb-preview-lang {
    font-size: 10px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    padding: 1px 6px;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .rb-preview-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .rb-icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .rb-icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .rb-icon-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .rb-preview-sep {
    width: 1px;
    height: 16px;
    background: var(--border-subtle);
    margin: 0 2px;
    flex-shrink: 0;
  }

  .rb-preview-body {
    flex: 1;
    overflow: auto;
    min-height: 0;
  }
  .rb-preview-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
  }
  .rb-preview-message {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    height: 100%;
    color: var(--text-muted);
    font-size: 13px;
  }
  .rb-preview-error { color: var(--text-secondary); }
  .rb-preview-image {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    height: 100%;
    box-sizing: border-box;
  }
  .rb-preview-image img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-sm);
  }
  .rb-code {
    margin: 0;
    padding: 14px 16px;
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.65;
    color: var(--text-secondary);
    white-space: pre;
    tab-size: 2;
    overflow: visible;
  }

</style>
