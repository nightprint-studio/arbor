<script lang="ts">
  import { GitBranch, AlertCircle, ArrowUp, ArrowDown, RefreshCw, Download, Upload, GitPullRequest, FolderOpen, Loader } from 'lucide-svelte';
  import type { SubmoduleInfo } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { submoduleFetch, submodulePull, submodulePush, listSubmodules } from '$lib/ipc/submodule';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import SubmoduleBranchModal from './SubmoduleBranchModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    submodules,
    onRefresh,
  }: { submodules: SubmoduleInfo[]; onRefresh: () => void } = $props();

  const tab = $derived(tabsStore.activeTab);

  // Track which submodule paths are currently being operated on.
  let busy = $state<Set<string>>(new Set());

  function isBusy(path: string) { return busy.has(path); }
  function setBusy(path: string, on: boolean) {
    const next = new Set(busy);
    if (on) next.add(path); else next.delete(path);
    busy = next;
  }

  // ── Context menu ──────────────────────────────────────────────────────────

  type CtxState = { x: number; y: number; sub: SubmoduleInfo };
  let ctxMenu = $state<CtxState | null>(null);
  let branchModal = $state<SubmoduleInfo | null>(null);

  function buildMenuItems(sub: SubmoduleInfo): MenuItem[] {
    const initialized = sub.is_initialized;
    return [
      { id: '__sync__', label: 'Sync', header: true },
      { id: 'fetch',    label: 'Fetch',  icon: Download,       iconColor: 'var(--success)', disabled: !initialized },
      { id: 'pull',     label: 'Pull',   icon: GitPullRequest, iconColor: 'var(--success)', disabled: !initialized },
      { id: 'push',     label: 'Push',   icon: Upload,         iconColor: 'var(--accent)',  disabled: !initialized },
      { id: '__nav__',  label: 'Navigate', header: true },
      { id: 'checkout', label: 'Checkout Branch…', icon: GitBranch, iconColor: 'var(--accent)', disabled: !initialized },
      { id: '__app__',  label: 'App', header: true },
      { id: 'open_tab', label: 'Open as Tab', icon: FolderOpen, iconColor: '#ffc66d', disabled: !initialized },
    ];
  }

  function handleContextMenu(e: MouseEvent, sub: SubmoduleInfo) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenu = { x: e.clientX, y: e.clientY, sub };
  }

  async function handleCtxSelect(id: string) {
    if (!ctxMenu) return;
    const sub = ctxMenu.sub;
    ctxMenu = null;

    if      (id === 'fetch')    await doFetch(sub);
    else if (id === 'pull')     await doPull(sub);
    else if (id === 'push')     await doPush(sub);
    else if (id === 'checkout') branchModal = sub;
    else if (id === 'open_tab') openAsTab(sub);
  }

  // ── Double-click opens as tab ──────────────────────────────────────────────

  function openAsTab(sub: SubmoduleInfo) {
    document.dispatchEvent(new CustomEvent('open-recent', { detail: sub.abs_path }));
  }

  // ── Operations ────────────────────────────────────────────────────────────

  async function doFetch(sub: SubmoduleInfo) {
    if (!tab || isBusy(sub.path)) return;
    setBusy(sub.path, true);
    try {
      await submoduleFetch(tab.id, sub.path);
      uiStore.showToast(`Fetched "${sub.name}"`, 'success');
      onRefresh();
    } catch (err) {
      uiStore.showToast(`Fetch failed: ${err}`, 'error');
    } finally { setBusy(sub.path, false); }
  }

  async function doPull(sub: SubmoduleInfo) {
    if (!tab || isBusy(sub.path)) return;
    setBusy(sub.path, true);
    try {
      const out = await submodulePull(tab.id, sub.path);
      uiStore.showToast(`Pulled "${sub.name}"${out.trim() ? ': ' + out.trim().split('\n')[0] : ''}`, 'success');
      onRefresh();
    } catch (err) {
      uiStore.showToast(`Pull failed: ${err}`, 'error');
    } finally { setBusy(sub.path, false); }
  }

  async function doPush(sub: SubmoduleInfo) {
    if (!tab || isBusy(sub.path)) return;
    setBusy(sub.path, true);
    try {
      const out = await submodulePush(tab.id, sub.path);
      uiStore.showToast(`Pushed "${sub.name}"${out.trim() ? ': ' + out.trim().split('\n')[0] : ''}`, 'success');
      onRefresh();
    } catch (err) {
      uiStore.showToast(`Push failed: ${err}`, 'error');
    } finally { setBusy(sub.path, false); }
  }
</script>

<div class="submodule-list" role="list">
  {#each submodules as sub (sub.name)}
    {@const loading = isBusy(sub.path)}

    <div
      class="sub-item"
      class:uninit={!sub.is_initialized}
      role="button"
      tabindex="0"
      use:tooltip={{ content: sub.url, description: 'Double-click to open as tab · Right-click for options' }}
      ondblclick={() => sub.is_initialized && openAsTab(sub)}
      onkeydown={(e) => e.key === 'Enter' && sub.is_initialized && openAsTab(sub)}
      oncontextmenu={(e) => handleContextMenu(e, sub)}
    >
      <!-- Name + dirty dot -->
      <div class="sub-main">
        <span class="sub-name" class:uninit-text={!sub.is_initialized}>
          {sub.name}
          {#if sub.is_dirty}
            <span class="dirty-dot" use:tooltip={'Dirty working directory'}>•</span>
          {/if}
        </span>
        <span class="sub-path">{sub.path}</span>
      </div>

      <!-- Right side: busy spinner OR badges -->
      <div class="sub-badges">
        {#if loading}
          <span class="spinner-wrap"><Loader size={11} class="spin" /></span>
        {:else if !sub.is_initialized}
          <span class="badge-uninit" use:tooltip={'Submodule not initialised'}>
            <AlertCircle size={11} />
          </span>
        {:else}
          <!-- Branch badge -->
          {#if sub.branch}
            <span class="badge-branch" use:tooltip={`Current branch: ${sub.branch}`}>
              <GitBranch size={9} />
              <span class="badge-text">{sub.branch}</span>
            </span>
          {:else}
            <span class="badge-detached" use:tooltip={`Detached HEAD at ${sub.head_hash}`}>
              <AlertCircle size={9} />
              <span class="badge-text">{sub.head_hash}</span>
            </span>
          {/if}

          <!-- Ahead/behind -->
          {#if sub.ahead > 0}
            <span class="badge-ahead" use:tooltip={`${sub.ahead} commit${sub.ahead !== 1 ? 's' : ''} ahead of remote`}>
              <ArrowUp size={9} />
              {sub.ahead}
            </span>
          {/if}
          {#if sub.behind > 0}
            <span class="badge-behind" use:tooltip={`${sub.behind} commit${sub.behind !== 1 ? 's' : ''} behind remote`}>
              <ArrowDown size={9} />
              {sub.behind}
            </span>
          {/if}
          {#if sub.ahead === 0 && sub.behind === 0 && sub.branch}
            <span class="badge-synced" use:tooltip={'In sync with remote'}></span>
          {/if}
        {/if}
      </div>
    </div>
  {/each}
</div>

{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={buildMenuItems(ctxMenu.sub)}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

{#if branchModal}
  <SubmoduleBranchModal
    sub={branchModal}
    onClose={() => branchModal = null}
    onDone={() => { branchModal = null; onRefresh(); }}
  />
{/if}

<style>
  .submodule-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 2px 0;
  }

  /* ── Row ── */
  .sub-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px 5px 4px;
    border-radius: var(--radius-sm);
    cursor: default;
    transition: background var(--transition-fast);
    outline: none;
    min-height: 36px;
  }
  .sub-item:hover { background: var(--bg-hover); }
  .sub-item:focus-visible { outline: 1px solid var(--border-focus); outline-offset: -1px; }
  .sub-item.uninit { opacity: 0.6; }

  /* ── Text block ── */
  .sub-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .sub-name {
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub-name.uninit-text { color: var(--text-disabled); }

  .dirty-dot {
    color: var(--color-stash);
    font-size: 14px;
    line-height: 1;
    vertical-align: middle;
    margin-left: 3px;
  }

  .sub-path {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Badges container ── */
  .sub-badges {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .spinner-wrap {
    display: flex;
    align-items: center;
    color: var(--text-muted);
  }

  /* Branch badge */
  .badge-branch {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--text-secondary);
    font-size: 10px;
    font-family: var(--font-ui-sans);
    max-width: 90px;
    overflow: hidden;
  }
  .badge-branch :global(svg) { flex-shrink: 0; }

  /* Detached HEAD badge */
  .badge-detached {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px;
    background: color-mix(in srgb, var(--color-stash) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-stash) 25%, transparent);
    border-radius: 999px;
    color: var(--color-stash);
    font-size: 10px;
    font-family: var(--font-code);
    max-width: 90px;
    overflow: hidden;
  }

  .badge-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Ahead */
  .badge-ahead {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 10px;
    font-family: var(--font-ui-sans);
    font-weight: 600;
    color: var(--color-submodule);
    padding: 1px 4px;
    background: color-mix(in srgb, var(--color-submodule) 10%, transparent);
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--color-submodule) 20%, transparent);
  }

  /* Behind */
  .badge-behind {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 10px;
    font-family: var(--font-ui-sans);
    font-weight: 600;
    color: var(--color-stash);
    padding: 1px 4px;
    background: color-mix(in srgb, var(--color-stash) 10%, transparent);
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--color-stash) 22%, transparent);
  }

  /* Synced indicator */
  .badge-synced {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--color-submodule);
    opacity: 0.7;
    flex-shrink: 0;
  }

  /* Uninitialised warning icon */
  .badge-uninit {
    display: flex;
    align-items: center;
    color: var(--color-stash);
  }

</style>
