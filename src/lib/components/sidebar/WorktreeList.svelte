<script lang="ts">
  import { Layers, Home, CircleDot, Lock, Plus, Trash2, ExternalLink, Info, ChevronRight, Link2 } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import type { WorktreeInfo } from '$lib/types/git';
  import { worktreeStore } from '$lib/stores/worktree.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { removeWorktree, openInIde } from '$lib/ipc/worktree';
  import { switchToWorktree } from '$lib/utils/worktree-switch';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import WorktreeInfoModal from './WorktreeInfoModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import AddWorktreeModal from './AddWorktreeModal.svelte';

  let {
    expanded = $bindable(false),
  }: {
    expanded?: boolean;
  } = $props();

  const tab          = $derived(tabsStore.activeTab);
  const worktrees    = $derived(worktreeStore.worktrees);
  const ideConfig    = $derived(worktreeStore.ideConfig);
  const detectedIdes = $derived(worktreeStore.detectedIdes);

  // ── Context menu ───────────────────────────────────────────────────────────
  type CtxState = { x: number; y: number; worktree: WorktreeInfo };
  let ctxMenu = $state<CtxState | null>(null);
  let infoModal = $state<WorktreeInfo | null>(null);
  let addOpen   = $state(false);

  function buildMenuItems(wt: WorktreeInfo): MenuItem[] {
    const items: MenuItem[] = [];

    if (!wt.is_current) {
      items.push({ id: 'switch', label: 'Switch to this workspace', icon: ChevronRight, iconColor: 'var(--accent)' });
    }
    items.push({ id: 'info', label: 'Workspace info', icon: Info, iconColor: 'var(--text-muted)' });

    // Resolve the effective default IDE for this project type (language > global default).
    const effectiveDefaultId = ideConfig
      ? (ideConfig.language_defaults[wt.project_type] ?? ideConfig.default_ide)
      : undefined;

    const available = detectedIdes.filter(d => d.available);
    const customs   = ideConfig?.custom_ides ?? [];

    if (available.length > 0 || customs.length > 0) {
      items.push({ id: '__sep_ide__', label: '', separator: true });
      items.push({ id: '__hdr_ide__', label: 'Open in IDE', header: true });

      for (const ide of available) {
        const isDefault = effectiveDefaultId === ide.id;
        items.push({
          id:          `ide:${ide.id}`,
          label:       ide.name,
          icon:        ExternalLink,
          iconColor:   '#20b2aa',
          badge:       isDefault ? 'Default' : undefined,
          badgeAccent: isDefault,
        });
      }
      for (const custom of customs) {
        const isDefault = effectiveDefaultId === custom.id;
        items.push({
          id:          `ide:${custom.id}`,
          label:       custom.name,
          icon:        ExternalLink,
          iconColor:   '#20b2aa',
          badge:       isDefault ? 'Default' : undefined,
          badgeAccent: isDefault,
        });
      }
    } else {
      // Detection hasn't completed yet — show a single generic fallback.
      items.push({ id: '__sep_ide__', label: '', separator: true });
      items.push({ id: 'ide:default', label: 'Open in IDE', icon: ExternalLink, iconColor: '#20b2aa' });
    }

    if (wt.branch) {
      items.push({ id: '__sep_dl__', label: '', separator: true });
      items.push({ id: 'copy-deep-link', label: 'Copy arbor:// worktree link', icon: Link2, iconColor: '#20b2aa' });
    }

    if (!wt.is_main) {
      items.push({ id: '__sep_del__', label: '', separator: true });
      items.push({ id: 'remove', label: 'Remove workspace', icon: Trash2, danger: true });
    }

    return items;
  }

  // ── Handlers ──────────────────────────────────────────────────────────────

  function handleContextMenu(e: MouseEvent, wt: WorktreeInfo) {
    e.preventDefault();
    ctxMenu = { x: e.clientX, y: e.clientY, worktree: wt };
  }

  function handleDblClick(wt: WorktreeInfo) {
    if (wt.is_current) return;
    switchTo(wt);
  }

  const switchTo = switchToWorktree;

  async function handleCtxSelect(id: string) {
    if (!ctxMenu) return;
    const wt = ctxMenu.worktree;
    ctxMenu = null;

    if (id === 'switch') {
      switchTo(wt);
    } else if (id === 'info') {
      infoModal = wt;
    } else if (id === 'ide:default') {
      await doOpenInIde(wt.path); // uses backend default
    } else if (id.startsWith('ide:')) {
      await doOpenInIde(wt.path, id.slice(4));
    } else if (id === 'remove') {
      await handleRemove(wt);
    } else if (id === 'copy-deep-link') {
      if (tab && wt.branch) {
        void copyDeepLink({ kind: 'branch_worktree', branch: wt.branch }, tab.id);
      }
    }
  }

  async function doOpenInIde(path: string, ideId?: string) {
    try {
      await openInIde(path, ideId);
    } catch (err) {
      uiStore.showToast(`Failed to open IDE: ${err}`, 'error');
    }
  }

  async function handleRemove(wt: WorktreeInfo) {
    if (!tab) return;
    if (wt.is_main) {
      uiStore.showToast('Cannot remove the main workspace.', 'error');
      return;
    }
    try {
      await removeWorktree(tab.id, wt.path);
      uiStore.showToast(`Removed workspace "${wt.branch ?? wt.path}"`, 'success');
      await worktreeStore.load(tab.id);
    } catch (err) {
      uiStore.showToast(`Remove failed: ${err}`, 'error');
    }
  }

  function handleInfoSwitch() {
    if (infoModal) {
      switchTo(infoModal);
      infoModal = null;
    }
  }

  async function handleInfoIde(ideId?: string) {
    if (infoModal) await doOpenInIde(infoModal.path, ideId);
  }

  const PROJECT_ICON: Record<string, string> = {
    rust:         '🦀',
    node_js:      '🟩',
    java_maven:   '☕',
    java_gradle:  '☕',
    go:           '🐹',
    python:       '🐍',
    dot_net:      '🔷',
    cpp:          '⚙️',
    ruby:         '💎',
    php:          '🐘',
    unknown:      '',
  };

  /** Show just the folder name (last path segment). */
  function folderName(p: string) {
    return p.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? p;
  }
</script>

<SidebarSection
  label="Worktrees"
  iconColor="var(--accent)"
  badge={worktrees.length || null}
  badgeColor="var(--accent)"
  bind:expanded
>
  {#snippet icon()}<Layers size={13} />{/snippet}
  {#snippet actions()}
    <button class="add-btn" use:tooltip={'Add linked worktree'} onclick={() => addOpen = true}>
      <Plus size={11} />
    </button>
  {/snippet}

  {#if worktreeStore.loading}
    <div class="empty-msg">Loading…</div>
  {:else if worktreeStore.error}
    <div class="empty-msg error-msg">{worktreeStore.error}</div>
  {:else if worktrees.length === 0}
    <div class="empty-msg">No additional worktrees found.</div>
  {:else}
    {#each worktrees as wt (wt.path)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="wt-row"
          class:is-current={wt.is_current}
          role="button"
          tabindex="0"
          use:tooltip={wt.path}
          ondblclick={() => handleDblClick(wt)}
          oncontextmenu={(e) => handleContextMenu(e, wt)}
          onkeydown={(e) => {
            if (e.key === 'Enter') handleDblClick(wt);
            else if (e.key === 'F10' && e.shiftKey) handleContextMenu(e as unknown as MouseEvent, wt);
          }}
        >
          <!-- Project type emoji -->
          <span class="wt-project-icon" aria-hidden="true">
            {PROJECT_ICON[wt.project_type] ?? ''}
          </span>

          <!-- Name = branch name OR last path segment -->
          <span class="wt-name">
            {#if wt.branch}
              {wt.branch}
            {:else}
              <span class="wt-detached" use:tooltip={'Detached HEAD'}>{folderName(wt.path)}</span>
            {/if}
          </span>

          <!-- Badges row -->
          <span class="wt-badges">
            {#if wt.is_main}
              <span class="wt-badge wt-badge-main" use:tooltip={{ content: 'Main worktree', description: 'Cannot be removed' }}>
                <Home size={9} />
              </span>
            {/if}
            {#if wt.is_current}
              <span class="wt-badge wt-badge-current" use:tooltip={'Currently open'}>
                <CircleDot size={9} />
              </span>
            {/if}
            {#if wt.is_locked}
              <span class="wt-badge wt-badge-locked" use:tooltip={'Locked'}>
                <Lock size={9} />
              </span>
            {/if}
          </span>

          <!-- Always-visible info button -->
          <button
            class="wt-info-btn"
            use:tooltip={'Info'}
            onclick={(e) => { e.stopPropagation(); infoModal = wt; }}
          >
            <Info size={11} />
          </button>

          <!-- Switch button (hover only) -->
          {#if !wt.is_current}
            <span class="wt-actions">
              <button
                class="wt-action-btn"
                use:tooltip={{ content: 'Switch here', description: 'Double-click' }}
                onclick={(e) => { e.stopPropagation(); switchTo(wt); }}
              >
                <ChevronRight size={11} />
              </button>
            </span>
          {/if}
        </div>
    {/each}
  {/if}
</SidebarSection>

<!-- ── Context menu ── -->
{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={buildMenuItems(ctxMenu.worktree)}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

<!-- ── Info modal ── -->
{#if infoModal}
  <WorktreeInfoModal
    worktree={infoModal}
    onClose={() => infoModal = null}
    onSwitch={handleInfoSwitch}
    onOpenInIde={handleInfoIde}
  />
{/if}

<!-- ── Add worktree modal ── -->
{#if addOpen && tab}
  <AddWorktreeModal
    tabId={tab.id}
    onClose={() => addOpen = false}
    onAdded={() => { addOpen = false; tab && worktreeStore.load(tab.id); }}
  />
{/if}

<style>
  /* "Add linked worktree" — hover-reveal handled by SidebarSection's
     .section-actions wrapper. */
  .add-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .add-btn:hover { background: var(--bg-hover); color: var(--accent) !important; }

  .empty-msg {
    padding: 6px 16px;
    font-size: 11.5px;
    color: var(--text-muted);
    font-style: italic;
  }
  .error-msg { color: var(--diff-del-bg-strong, #ff5555); font-style: normal; }

  /* ── Worktree row ── */
  .wt-row {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px 3px 22px;
    cursor: pointer;
    border-radius: var(--radius-sm);
    min-height: 24px;
    transition: background 0.1s;
    position: relative;
  }
  .wt-row:hover { background: var(--bg-hover); }
  .wt-row:focus-visible { outline: 1px solid var(--accent); }

  .wt-row.is-current {
    background: var(--accent-subtle);
  }
  .wt-row.is-current .wt-name {
    color: var(--accent);
    font-weight: 600;
  }

  .wt-project-icon {
    font-size: 12px;
    line-height: 1;
    flex-shrink: 0;
    width: 16px;
    text-align: center;
  }

  .wt-name {
    font-size: 12px;
    color: var(--text-primary);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wt-detached {
    color: var(--text-secondary);
    font-style: italic;
  }

  /* Badges */
  .wt-badges {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
  }
  .wt-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1px 3px;
    border-radius: var(--radius-sm);
    font-size: 9px;
  }
  .wt-badge-main {
    color: var(--color-stash);
    background: color-mix(in srgb, var(--color-stash) 18%, transparent);
  }
  .wt-badge-current {
    color: var(--accent);
    background: var(--accent-subtle);
  }
  .wt-badge-locked {
    color: var(--text-muted);
    background: var(--bg-overlay);
  }

  /* Always-visible info button */
  .wt-info-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-disabled);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }
  .wt-row:hover .wt-info-btn,
  .wt-row.is-current .wt-info-btn { color: var(--text-muted); }
  .wt-info-btn:hover { background: var(--bg-overlay); color: var(--text-primary) !important; }

  /* Switch action (hover only) */
  .wt-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
    flex-shrink: 0;
  }
  .wt-row:hover .wt-actions { opacity: 1; }

  .wt-action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }
  .wt-action-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }
</style>
