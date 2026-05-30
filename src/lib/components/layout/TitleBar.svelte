<script lang="ts">
  import { Settings, BookOpen, LayoutDashboard, Palette, Check, Command, ChevronLeft } from 'lucide-svelte';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import MenuBar from './MenuBar.svelte';
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon   from '$lib/components/plugins/PluginIcon.svelte';
  import ArborLogo    from '$lib/components/shared/internal/ArborLogo.svelte';
  import Kbd          from '$lib/components/shared/internal/Kbd.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  // Title bar buttons sit at the very top — tooltips fly downward away from
  // the bar, never above (they'd be clipped by the window edge).
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import CustomizeActivityBarModal from './CustomizeActivityBarModal.svelte';
  import WindowControls from './WindowControls.svelte';
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

  // ── Theme hover submenu (opens off the Settings menu's "Theme" row) ─────
  // Submenu sits to the LEFT of the Settings menu, aligned with the Theme
  // row. A short close-timer gives the cursor time to bridge the gap
  // between the row and the submenu without it snapping shut.
  const SUBMENU_CLOSE_DELAY_MS = 150;
  let themeSubmenuOpen   = $state(false);
  let themeSubmenuAnchor = $state<{ right: number; top: number } | null>(null);
  let themeSubmenuTimer: ReturnType<typeof setTimeout> | null = null;

  function cancelThemeSubmenuClose() {
    if (themeSubmenuTimer !== null) {
      clearTimeout(themeSubmenuTimer);
      themeSubmenuTimer = null;
    }
  }
  function openThemeSubmenu(e: MouseEvent) {
    cancelThemeSubmenuClose();
    const row  = e.currentTarget as HTMLElement;
    const rect = row.getBoundingClientRect();
    themeSubmenuAnchor = {
      right: window.innerWidth - rect.left + 4,   // 4 px gap between menus
      top:   rect.top - 5,                         // line up with row's padding
    };
    themeSubmenuOpen = true;
  }
  function scheduleThemeSubmenuClose() {
    cancelThemeSubmenuClose();
    themeSubmenuTimer = setTimeout(() => {
      themeSubmenuOpen   = false;
      themeSubmenuAnchor = null;
      themeSubmenuTimer  = null;
    }, SUBMENU_CLOSE_DELAY_MS);
  }
  function closeThemeSubmenuNow() {
    cancelThemeSubmenuClose();
    themeSubmenuOpen   = false;
    themeSubmenuAnchor = null;
  }
  async function selectTheme(id: string) {
    closeThemeSubmenuNow();
    closeSettingsMenu();
    await themeStore.setActive(id);
  }
  function openThemeEditorFromSubmenu() {
    closeThemeSubmenuNow();
    closeSettingsMenu();
    onOpenThemeEditor();
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
    // Drop the theme submenu with its parent — otherwise it hangs in the
    // void after the user dismisses the Settings menu via the backdrop.
    closeThemeSubmenuNow();
  }
  function handleSettingsMenuSelect(id: string) {
    closeSettingsMenu();
    if (id === 'settings') {
      uiStore.setPanel(uiStore.activePanel === 'settings' ? 'graph' : 'settings');
    } else if (id === 'customize-activity-bar') {
      customizeActivityBarOpen = true;
    }
  }


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
      class:active={uiStore.commandPaletteOpen}
      use:tooltip={tooltipForAction('Command palette', 'command_palette')}
      aria-pressed={uiStore.commandPaletteOpen}
      onclick={() => uiStore.toggleCommandPalette()}
    >
      <Command size={18} />
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

    <!-- Window controls (close/minimize/maximize). Style is user-controlled
         from Appearance settings; dimensions and position stay constant. -->
    <WindowControls />
  </div>
</div>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    height: var(--titlebar-h, 42px);
    background: var(--bg-elevated);
    padding: 0;
    flex-shrink: 0;
    overflow: visible;
    position: relative;
    z-index: 100;
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.04);
    transition: height var(--anim-dur-base) ease;
  }
  /* Compact title bar — shrinks icon-buttons to match the smaller height,
     keeping the chrome visually proportional. */
  :global([data-compact-title-bar="true"]) .icon-btn {
    width: 26px;
    height: 26px;
  }
  :global([data-compact-title-bar="true"]) .ctrl-sep {
    height: 14px;
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

  /* Theme row inside the Settings menu — same shape as the surrounding
     menu items but hover-driven (the click target lives in the submenu).
     Chevron points LEFT because the submenu opens leftward (Settings menu
     hugs the right window edge, so there's no room to expand rightward). */
  .theme-row { cursor: default; }
  :global(.theme-row-arrow) {
    color: var(--text-muted);
    margin-left: auto;
    flex-shrink: 0;
  }
  .theme-row:hover :global(.theme-row-arrow),
  .theme-row.active :global(.theme-row-arrow) { color: var(--text-primary); }

  /* Theme submenu floats off the Settings menu — needs its own stacking
     context above the backdrop (z 490) and the parent menu (z 491). */
  .theme-submenu { z-index: 492; }

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
    <!-- Theme row — hover entry that opens the theme submenu to the left
         of this menu (built-in + custom themes for quick switching). Click
         is also wired so keyboard users can Tab here and press Enter/Space
         to pin the submenu open — otherwise the submenu would be reachable
         only with a mouse. -->
    <button
      type="button"
      class="settings-menu-item theme-row"
      class:active={themeSubmenuOpen}
      role="menuitem"
      aria-haspopup="menu"
      aria-expanded={themeSubmenuOpen}
      onmouseenter={openThemeSubmenu}
      onmouseleave={scheduleThemeSubmenuClose}
      onfocus={openThemeSubmenu}
      onclick={openThemeSubmenu}
    >
      <Palette size={14} />
      <span>Theme</span>
      <ChevronLeft size={12} class="theme-row-arrow" />
    </button>
  </div>
{/if}

{#if customizeActivityBarOpen}
  <CustomizeActivityBarModal onClose={() => customizeActivityBarOpen = false} />
{/if}

<!-- ── Theme hover submenu (anchored off the Settings menu's Theme row) ───── -->
{#if themeSubmenuOpen && themeSubmenuAnchor}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="settings-menu theme-menu theme-submenu"
    style="right: {themeSubmenuAnchor.right}px; top: {themeSubmenuAnchor.top}px;"
    role="menu"
    aria-label="Theme submenu"
    onmouseenter={cancelThemeSubmenuClose}
    onmouseleave={scheduleThemeSubmenuClose}
    transition:fly={{ x: 6, duration: animStore.dFast, easing: cubicOut }}
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
      onclick={openThemeEditorFromSubmenu}
    >
      <Settings size={14} />
      <span>Edit themes…</span>
    </button>
  </div>
{/if}
