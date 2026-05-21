<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { AlignJustify, FolderOpen, Clock, LogOut, Info, Package, ChevronRight, Zap, Download, FolderPlus, ScrollText } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { pluginStore }        from '$lib/stores/plugin.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import Kbd from '$lib/components/shared/ui/Kbd.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  let { onOpen, onClone, onInit }: { onOpen: () => void; onClone: () => void; onInit: () => void } = $props();

  const pluginMenuItems = $derived(
    contributionStore.forPoint('arbor:menu')
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
  );

  let open = $state(false);
  let recentHovered = $state(false);

  const recentRepos = $derived(uiStore.recentRepos);

  function close() { open = false; recentHovered = false; }

  function handleOpenRepo()  { close(); onOpen();  }
  function handleCloneRepo() { close(); onClone(); }
  function handleInitRepo()  { close(); onInit();  }

  async function handleOpenRecent(path: string) {
    close();
    // Dispatch via the parent's onOpen equivalent — we emit a custom event
    const event = new CustomEvent('open-recent', { detail: path, bubbles: true });
    document.dispatchEvent(event);
  }

  function handlePlugins() { close(); uiStore.setPanel('plugins'); }

  function handlePluginLogs() {
    close();
    uiStore.setActiveBottomSection('plugin-logs');
  }

  function handleAbout() {
    close();
    uiStore.setPanel('about');
  }

  async function handleExit() {
    close();
    await getCurrentWindow().close();
  }
</script>

<!-- Click-away backdrop -->
{#if open}
  <div class="backdrop" onclick={close} role="presentation"></div>
{/if}

<div class="menubar-root">
  <button
    class="hamburger"
    class:active={open}
    onclick={(e) => { e.stopPropagation(); open = !open; }}
    use:tooltip={'Main menu'}
    aria-label="Open main menu"
    aria-expanded={open}
  >
    <AlignJustify size={20} strokeWidth={2} />
  </button>

  {#if open}
    <div class="menu-panel" role="menu"
         transition:fly={{ y: -8, duration: animStore.dBase, easing: cubicOut }}>

      <!-- File section -->
      <div class="menu-section-label">File</div>

      <button class="menu-item" role="menuitem" onclick={handleOpenRepo}>
        <FolderOpen size={13} />
        <span>Open Repository…</span>
        <span class="kb-slot"><Kbd action="open_repo" variant="inline" /></span>
      </button>

      <button class="menu-item" role="menuitem" onclick={handleCloneRepo}>
        <Download size={13} />
        <span>Clone Repository…</span>
        <span class="kb-slot"><Kbd action="clone_repo" variant="inline" /></span>
      </button>

      <button class="menu-item" role="menuitem" onclick={handleInitRepo}>
        <FolderPlus size={13} />
        <span>Initialize Repository…</span>
        <span class="kb-slot"><Kbd action="init_repo" variant="inline" /></span>
      </button>

      <button class="menu-item" role="menuitem" onclick={() => { close(); uiStore.openRepoBrowser(); }}>
        <Package size={13} />
        <span>Browse Remote Repositories…</span>
        <span class="kb-slot"><Kbd action="repo_browser" variant="inline" /></span>
      </button>

      <!-- Recent submenu -->
      <div
        class="menu-item has-sub"
        class:hovered={recentHovered}
        role="menuitem"
        tabindex="0"
        onmouseenter={() => recentHovered = true}
        onmouseleave={() => recentHovered = false}
        onkeydown={(e) => e.key === 'Enter' && (recentHovered = true)}
      >
        <Clock size={13} />
        <span>Recent</span>
        <ChevronRight size={11} class="sub-arrow" />

        {#if recentHovered}
          <div class="submenu">
            {#if recentRepos.length === 0}
              <div class="menu-item disabled">No recent repositories</div>
            {:else}
              {#each recentRepos as path (path)}
                <button class="menu-item recent-entry" onclick={() => handleOpenRecent(path)}>
                  <span class="recent-name">{path.split(/[/\\]/).pop()}</span>
                  <span class="recent-path">{path}</span>
                </button>
              {/each}
            {/if}
          </div>
        {/if}
      </div>

      <div class="menu-sep"></div>

      <!-- Plugins section -->
      <div class="menu-section-label">Tools</div>

      <button class="menu-item" role="menuitem" onclick={handlePlugins}>
        <Package size={13} />
        <span>Plugin Manager</span>
        <span class="kb-slot"><Kbd action="plugins" variant="inline" /></span>
      </button>

      <button class="menu-item" role="menuitem" onclick={handlePluginLogs}>
        <ScrollText size={13} />
        <span>Plugin Logs</span>
        <span class="kb-slot"><Kbd action="plugin_logs" variant="inline" /></span>
      </button>

      <div class="menu-sep"></div>

      <!-- Plugin-registered menu items -->
      {#if pluginMenuItems.length > 0}
        <div class="menu-section-label">Plugins</div>
        {#each pluginMenuItems as c (c.plugin_name + ':' + c.item_id)}
          {@const p = c.payload as { label?: string; action?: string }}
          <button
            class="menu-item"
            role="menuitem"
            onclick={async () => {
              close();
              if (!p.action) return;
              try {
                await firePluginAction(c.plugin_name, p.action, '{}');
              } catch (err) {
                uiStore.showToast(`Plugin action failed: ${err}`, 'error');
              }
            }}
          >
            <Zap size={13} />
            <span>{p.label ?? ''}</span>
            <span class="menu-plugin-tag">{c.plugin_name}</span>
          </button>
        {/each}
        <div class="menu-sep"></div>
      {/if}

      <!-- About / Exit -->
      <button class="menu-item" role="menuitem" onclick={handleAbout}>
        <Info size={13} />
        <span>About Arbor</span>
      </button>

      <button class="menu-item danger" role="menuitem" onclick={handleExit}>
        <LogOut size={13} />
        <span>Exit</span>
      </button>

    </div>
  {/if}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-menu);
  }

  .menubar-root {
    position: relative;
    z-index: calc(var(--z-menu) + 1);
    flex-shrink: 0;
  }

  .hamburger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 42px;
    height: 42px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hamburger:hover,
  .hamburger.active { background: var(--bg-overlay); color: var(--text-primary); }

  .menu-panel {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 280px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    padding: 4px 0;
    z-index: calc(var(--z-menu) + 1);
  }

  .menu-section-label {
    padding: 6px 12px 2px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--text-muted);
    pointer-events: none;
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 12px;
    background: transparent;
    border: none;
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
    position: relative;
  }
  .menu-item:hover:not(.disabled), .menu-item.hovered { background: var(--bg-hover); color: var(--text-primary); }
  .menu-item.disabled { color: var(--text-disabled); cursor: default; }
  .menu-item.danger:hover { background: var(--error-subtle); color: var(--error); }

  .kb-slot { margin-left: auto; }

  .has-sub { user-select: none; }

  :global(.sub-arrow) {
    margin-left: auto;
    color: var(--text-muted);
  }

  .submenu {
    position: absolute;
    left: 100%;
    top: -4px;
    min-width: 260px;
    max-width: 380px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    padding: 4px 0;
    z-index: calc(var(--z-menu) + 2);
  }

  .recent-entry { flex-direction: column; align-items: flex-start; gap: 1px; }
  .recent-name { font-size: var(--font-size-sm); color: var(--text-primary); }
  .recent-path {
    font-size: 10px;
    color: var(--text-muted);
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .menu-sep {
    height: 1px;
    background: var(--border);
    margin: 3px 0;
  }

  .menu-plugin-tag {
    margin-left: auto;
    font-size: 9px;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: var(--radius-sm);
    padding: 0 5px;
    line-height: 16px;
    white-space: nowrap;
    letter-spacing: 0.3px;
  }
</style>
