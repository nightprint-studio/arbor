<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Settings, BookOpen, LayoutDashboard, Palette, Check } from 'lucide-svelte';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import MenuBar from './MenuBar.svelte';
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon   from '$lib/components/plugins/PluginIcon.svelte';
  import ArborLogo    from '$lib/components/shared/ui/ArborLogo.svelte';
  import Kbd          from '$lib/components/shared/ui/Kbd.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  // Title bar buttons sit at the very top — tooltips fly downward away from
  // the bar, never above (they'd be clipped by the window edge).
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import CustomizeActivityBarModal from './CustomizeActivityBarModal.svelte';
  import WorkspaceDropdown from '../workspace/WorkspaceDropdown.svelte';

  interface Props {
    onOpen: () => void;
    onClone: () => void;
    onInit:  () => void;
    onOpenThemeEditor: () => void;
    onManageWorkspaces: () => void;
    onCreateWorkspace:  () => void;
  }

  let {
    onOpen, onClone, onInit, onOpenThemeEditor,
    onManageWorkspaces, onCreateWorkspace,
  }: Props = $props();

  // ── Settings dropdown menu ──────────────────────────────────────────────
  let settingsMenuOpen         = $state(false);
  let settingsMenuAnchor       = $state<{ x: number; y: number } | null>(null);
  let customizeActivityBarOpen = $state(false);

  // ── Theme switcher dropdown ─────────────────────────────────────────────
  let themeMenuOpen   = $state(false);
  let themeMenuAnchor = $state<{ x: number; y: number } | null>(null);

  function openThemeMenu(e: MouseEvent) {
    const btn  = e.currentTarget as HTMLElement;
    const rect = btn.getBoundingClientRect();
    themeMenuAnchor = { x: window.innerWidth - rect.right, y: rect.bottom + 6 };
    themeMenuOpen   = true;
  }
  function closeThemeMenu() {
    themeMenuOpen   = false;
    themeMenuAnchor = null;
  }
  async function selectTheme(id: string) {
    closeThemeMenu();
    await themeStore.setActive(id);
  }

  function openSettingsMenu(e: MouseEvent) {
    const btn  = e.currentTarget as HTMLElement;
    const rect = btn.getBoundingClientRect();
    settingsMenuAnchor = { x: window.innerWidth - rect.right, y: rect.bottom + 6 };
    settingsMenuOpen   = true;
  }
  function closeSettingsMenu() {
    settingsMenuOpen   = false;
    settingsMenuAnchor = null;
  }
  function handleSettingsMenuSelect(id: string) {
    closeSettingsMenu();
    if (id === 'settings') {
      uiStore.setPanel(uiStore.activePanel === 'settings' ? 'graph' : 'settings');
    } else if (id === 'customize-activity-bar') {
      customizeActivityBarOpen = true;
    }
  }

  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);

  $effect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    appWindow.isMaximized().then(m => { if (active) isMaximized = m; });
    appWindow.onResized(async () => {
      const m = await appWindow.isMaximized();
      if (active) isMaximized = m;
    }).then(fn => { if (active) unlisten = fn; else fn(); });
    return () => { active = false; unlisten?.(); };
  });

</script>

<div class="titlebar" data-tauri-drag-region role="banner">
  <!-- App mark — sits to the left of the hamburger so plugin branding
       overrides are the first thing the user sees. Click is a no-op for
       now; reserve the slot for a future "About / What's New" affordance. -->
  <div class="no-drag brand-slot" use:tooltip={'Arbor'}>
    <ArborLogo size={22} />
  </div>

  <!-- Hamburger menu -->
  <div class="no-drag">
    <MenuBar {onOpen} {onClone} {onInit} />
  </div>

  <!-- Separator -->
  <!-- <div class="ctrl-sep" data-tauri-drag-region></div> -->

  <!-- Workspace dropdown (replaces the tab bar that used to live here;
       repo tabs now sit above the main content area, IntelliJ-style). -->
  <div class="no-drag ws-slot">
    <WorkspaceDropdown
      onManage={onManageWorkspaces}
      onCreate={onCreateWorkspace}
    />
  </div>

  <!-- Plugin-contributed items (left segment) -->
  <div class="no-drag plugin-slot">
    <Contribution point="arbor:title-bar:left">
      {#snippet item({ payload, fire })}
        {@const p = payload as { label?: string; icon?: string; action?: string; tooltip?: string; color?: string }}
        {#if p.action}
          <button
            type="button"
            class="plugin-status-item plugin-status-clickable"
            class:plugin-color-info={p.color === 'info'}
            class:plugin-color-success={p.color === 'success'}
            class:plugin-color-warning={p.color === 'warning'}
            class:plugin-color-error={p.color === 'error'}
            class:plugin-color-muted={p.color === 'muted'}
            class:plugin-color-accent={p.color === 'accent'}
            use:tooltip={p.tooltip ?? p.label ?? ''}
            onclick={() => fire()}
          >
            {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
            {#if p.label}<span>{p.label}</span>{/if}
          </button>
        {:else}
          <span
            class="plugin-status-item"
            class:plugin-color-info={p.color === 'info'}
            class:plugin-color-success={p.color === 'success'}
            class:plugin-color-warning={p.color === 'warning'}
            class:plugin-color-error={p.color === 'error'}
            class:plugin-color-muted={p.color === 'muted'}
            class:plugin-color-accent={p.color === 'accent'}
            use:tooltip={p.tooltip ?? p.label ?? ''}
          >
            {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
            {#if p.label}<span>{p.label}</span>{/if}
          </span>
        {/if}
      {/snippet}
    </Contribution>
  </div>

  <!-- Draggable region so the user can grab the empty middle. -->
  <div class="spacer" data-tauri-drag-region></div>

  <!-- Right controls -->
  <div class="right-controls no-drag">
    <Contribution point="arbor:title-bar:right">
      {#snippet item({ payload, fire })}
        {@const p = payload as { label?: string; icon?: string; action?: string; tooltip?: string; color?: string }}
        {#if p.action}
          <button
            type="button"
            class="plugin-status-item plugin-status-clickable"
            class:plugin-color-info={p.color === 'info'}
            class:plugin-color-success={p.color === 'success'}
            class:plugin-color-warning={p.color === 'warning'}
            class:plugin-color-error={p.color === 'error'}
            class:plugin-color-muted={p.color === 'muted'}
            class:plugin-color-accent={p.color === 'accent'}
            use:tooltip={p.tooltip ?? p.label ?? ''}
            onclick={() => fire()}
          >
            {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
            {#if p.label}<span>{p.label}</span>{/if}
          </button>
        {:else}
          <span
            class="plugin-status-item"
            class:plugin-color-info={p.color === 'info'}
            class:plugin-color-success={p.color === 'success'}
            class:plugin-color-warning={p.color === 'warning'}
            class:plugin-color-error={p.color === 'error'}
            class:plugin-color-muted={p.color === 'muted'}
            class:plugin-color-accent={p.color === 'accent'}
            use:tooltip={p.tooltip ?? p.label ?? ''}
          >
            {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
            {#if p.label}<span>{p.label}</span>{/if}
          </span>
        {/if}
      {/snippet}
    </Contribution>

    <button
      class="icon-btn"
      class:active={uiStore.activePanel === 'docs'}
      use:tooltip={tooltipForAction('Documentation', 'toggle_docs')}
      aria-pressed={uiStore.activePanel === 'docs'}
      onclick={() => uiStore.setPanel(uiStore.activePanel === 'docs' ? 'graph' : 'docs')}
    >
      <BookOpen size={18} />
    </button>

    <button
      class="icon-btn"
      class:active={themeMenuOpen}
      use:tooltip={'Switch theme'}
      aria-haspopup="menu"
      aria-expanded={themeMenuOpen}
      onclick={openThemeMenu}
    >
      <Palette size={16} />
    </button>

    <button
      class="icon-btn settings-btn"
      class:active={uiStore.activePanel === 'settings' || settingsMenuOpen}
      onclick={openSettingsMenu}
      use:tooltip={tooltipForAction('Settings', 'settings')}
      aria-haspopup="menu"
      aria-expanded={settingsMenuOpen}
    >
      <Settings size={18} />
    </button>

    <div class="ctrl-sep"></div>

    <!-- Mac-style window controls (leftmost) -->
    <div class="window-controls no-drag">
      <button class="wc-btn wc-close"    onclick={() => appWindow.close()}          use:tooltip={'Close'}    aria-label="Close window">
        <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
          <path d="M1 1l5 5M6 1L1 6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
        </svg>
      </button>
      <button class="wc-btn wc-minimize" onclick={() => appWindow.minimize()}       use:tooltip={'Minimize'} aria-label="Minimize">
        <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
          <path d="M1 3.5h5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
        </svg>
      </button>
      <button class="wc-btn wc-maximize" onclick={() => appWindow.toggleMaximize()} use:tooltip={isMaximized ? 'Restore' : 'Maximize'} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
        {#if isMaximized}
          <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
            <path d="M2.5 1H6v3.5M1 2.5V6h3.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        {:else}
          <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
            <path d="M1 3.5h5M3.5 1v5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
          </svg>
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    height: 42px;
    background: var(--bg-elevated);
    padding: 0;
    flex-shrink: 0;
    overflow: visible;
    position: relative;
    z-index: 100;
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.04);
  }

  .no-drag { -webkit-app-region: no-drag; display: contents; }

  .ws-slot {
    display: flex;
    align-items: center;
    padding: 0 6px;
  }

  /* Brand slot — keeps the logo perfectly centred vertically and adds a
     little breathing room before the hamburger. The padding mirrors the
     ws-slot so visual rhythm stays consistent across the title bar. */
  .brand-slot {
    display: flex;
    align-items: center;
    padding: 0 8px 0 8px;
    flex-shrink: 0;
  }

  /* ── Mac-style window controls ──────────────────────────────── */
  .window-controls {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 14px;
    height: 100%;
    flex-shrink: 0;
    -webkit-app-region: no-drag;
  }

  .wc-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: none;
    cursor: pointer;
    color: transparent;
    transition: color var(--transition-fast), filter var(--transition-fast);
    flex-shrink: 0;
    padding: 0;
    -webkit-app-region: no-drag;
  }
  .wc-btn:hover .wc-icon { color: rgba(0,0,0,0.6); }
  .wc-close    { background: #ff5f57; }
  .wc-minimize { background: #ffbd2e; }
  .wc-maximize { background: #28ca41; }
  .wc-close:hover    { filter: brightness(0.82); }
  .wc-minimize:hover { filter: brightness(0.82); }
  .wc-maximize:hover { filter: brightness(0.82); }
  .wc-icon { display: block; pointer-events: none; }

  .ctrl-sep {
    width: 1px;
    height: 18px;
    background: var(--border);
    flex-shrink: 0;
    margin: 0 4px;
  }

  .spacer { flex: 1; min-width: 40px; height: 100%; }

  /* Plugin-contributed items rendered between the workspace dropdown and
     the draggable spacer. Keep flex-shrink:0 so the spacer absorbs slack. */
  .plugin-slot {
    display: flex;
    align-items: center;
    height: 100%;
    flex-shrink: 0;
    gap: 4px;
  }

  .right-controls {
    display: flex;
    align-items: center;
    height: 100%;
    flex-shrink: 0;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn.active { color: var(--accent); }

  .settings-btn { margin-right: 6px; }

  /* ── Settings dropdown menu ─────────────────────────────────────────── */
  .settings-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 490;
    background: transparent;
    border: none;
    padding: 0;
    cursor: default;
  }

  .settings-menu {
    position: fixed;
    z-index: 491;
    min-width: 220px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 5px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
    font-family: var(--font-ui-sans);
  }

  .settings-menu-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: 12px;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .settings-menu-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .settings-menu-item.active { color: var(--accent); }
  .settings-menu-item > span:first-of-type { flex: 1; white-space: nowrap; }

  .menu-shortcut { flex-shrink: 0; }

  .theme-menu { min-width: 180px; }

  .theme-menu-section-label {
    padding: 4px 10px 2px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }
  .theme-menu-section-divided {
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid var(--border-subtle);
  }

  /* Scrollable custom list — caps the dropdown height when many themes are
     imported. Item names are clipped with an ellipsis so a single very long
     name can never widen the menu either. */
  .theme-menu-custom-list {
    max-height: 260px;     /* ~9 items @ ~28px each */
    overflow-y: auto;
    overflow-x: hidden;
    padding-right: 2px;    /* breathing room beside the scrollbar */
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }
  .theme-menu-custom-list::-webkit-scrollbar          { width: var(--scrollbar-width); }
  .theme-menu-custom-list::-webkit-scrollbar-track    { background: transparent; }
  .theme-menu-custom-list::-webkit-scrollbar-thumb    {
    background: var(--scrollbar-thumb);
    border-radius: var(--scrollbar-radius);
  }
  .theme-menu-custom-list::-webkit-scrollbar-thumb:hover {
    background: var(--scrollbar-thumb-hover);
  }
  /* Theme-name span: take available width and clip overflow with an ellipsis
     instead of pushing the check icon out of the row. */
  .theme-menu-custom-list .settings-menu-item > span:first-of-type {
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .theme-menu-divider {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
  }

  :global(.theme-check) { color: var(--accent); margin-left: auto; }

  /* ── Plugin-contributed title-bar items (same shape as status bar pills) ── */
  .plugin-status-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    height: 100%;
    font-size: 11px;
    color: var(--text-secondary);
    user-select: none;
  }
  .plugin-status-clickable {
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
  }
  .plugin-status-clickable:hover { background: var(--bg-hover); color: var(--text-primary); }
  .plugin-color-info    { color: var(--accent); }
  .plugin-color-success { color: var(--diff-add-strong, #4ade80); }
  .plugin-color-warning { color: #f59e0b; }
  .plugin-color-error   { color: var(--diff-del-strong, #f87171); }
  .plugin-color-muted   { color: var(--text-muted); }
  .plugin-color-accent  { color: var(--accent); }
</style>

<!-- ── Settings dropdown menu ──────────────────────────────────────────────── -->
{#if settingsMenuOpen && settingsMenuAnchor}
  <button
    type="button"
    aria-label="Close menu"
    class="settings-menu-backdrop"
    onclick={closeSettingsMenu}
    transition:fade={{ duration: animStore.dFast }}
  ></button>
  <div
    class="settings-menu"
    style="right: {settingsMenuAnchor.x}px; top: {settingsMenuAnchor.y}px;"
    role="menu"
    aria-label="Settings menu"
    transition:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
  >
    <button
      class="settings-menu-item"
      class:active={uiStore.activePanel === 'settings'}
      role="menuitem"
      onclick={() => handleSettingsMenuSelect('settings')}
    >
      <Settings size={14} />
      <span>Settings…</span>
      <span class="menu-shortcut"><Kbd action="settings" variant="inline" /></span>
    </button>
    <button
      class="settings-menu-item"
      role="menuitem"
      onclick={() => handleSettingsMenuSelect('customize-activity-bar')}
    >
      <LayoutDashboard size={14} />
      <span>Customize Activity Bar…</span>
    </button>
  </div>
{/if}

{#if customizeActivityBarOpen}
  <CustomizeActivityBarModal onClose={() => customizeActivityBarOpen = false} />
{/if}

<!-- ── Theme switcher dropdown ──────────────────────────────────────────────── -->
{#if themeMenuOpen && themeMenuAnchor}
  <button
    type="button"
    aria-label="Close menu"
    class="settings-menu-backdrop"
    onclick={closeThemeMenu}
    transition:fade={{ duration: animStore.dFast }}
  ></button>
  <div
    class="settings-menu theme-menu"
    style="right: {themeMenuAnchor.x}px; top: {themeMenuAnchor.y}px;"
    role="menu"
    aria-label="Theme switcher"
    transition:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
  >
    <div class="theme-menu-section-label">Built-in</div>
    {#each themeStore.builtIn as theme}
      <button
        class="settings-menu-item"
        class:active={themeStore.activeId === theme.id}
        role="menuitem"
        onclick={() => selectTheme(theme.id)}
      >
        <Palette size={14} />
        <span>{theme.name}</span>
        {#if themeStore.activeId === theme.id}
          <Check size={12} class="theme-check" />
        {/if}
      </button>
    {/each}

    {#if themeStore.custom.length > 0}
      <div class="theme-menu-section-label theme-menu-section-divided">Custom</div>
      <!-- Custom themes can grow unbounded as the user imports presets and
           experiments. Cap the section's height and scroll it independently —
           Built-in + Edit-themes stay pinned in view at all times. -->
      <div class="theme-menu-custom-list">
        {#each themeStore.custom as theme}
          <button
            class="settings-menu-item"
            class:active={themeStore.activeId === theme.id}
            role="menuitem"
            onclick={() => selectTheme(theme.id)}
            use:tooltip={theme.name}
          >
            <Palette size={14} />
            <span>{theme.name}</span>
            {#if themeStore.activeId === theme.id}
              <Check size={12} class="theme-check" />
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <div class="theme-menu-divider"></div>
    <button
      class="settings-menu-item"
      role="menuitem"
      onclick={() => { closeThemeMenu(); onOpenThemeEditor(); }}
    >
      <Settings size={14} />
      <span>Edit themes…</span>
    </button>
  </div>
{/if}
