<script lang="ts">
  /**
   * Arbor titlebar — composes the shared `TitleBar` chrome and fills it with the
   * Arbor domain: app mark · hamburger (file/tools/plugins) · workspace switcher
   * · plugin status pills · docs / command-palette / settings buttons · window
   * controls.
   *
   * The hamburger and settings menus are driven declaratively through the shared
   * widget's `DropdownItem[]` API. Theme is a `submenu` flyout (hover-intent +
   * keyboard, opens to the side); Recent stays an inline collapsible group. Only
   * the workspace dropdown and the plugin-contributed pills are authored here as
   * snippets.
   */
  import {
    Settings, Keyboard, LayoutDashboard, Palette,
    FolderOpen, Download, FolderPlus, Package, Clock, ScrollText, Info, LogOut, Zap,
    UserCog, User, Plus,
  } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { profileStore } from '$lib/stores/profiles.svelte';
  import { contributionStore } from '$lib/stores/corvus/contribution.svelte';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Contribution from '$lib/components/corvus/Contribution.svelte';
  import PluginIcon   from '$lib/components/plugins/PluginIcon.svelte';
  import ArborLogo    from '$lib/components/shared/internal/ArborLogo.svelte';
  import WindowControls from './WindowControls.svelte';
  import WorkspaceDropdown from '../corvus/workspace/WorkspaceDropdown.svelte';
  import CustomizeActivityBarModal from '../corvus/CustomizeActivityBarModal.svelte';
  import ProfileManagerModal from '$lib/components/shared/ProfileManagerModal.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  // Title bar buttons sit at the very top — tooltips fly downward away from the
  // bar, never above (they'd be clipped by the window edge).
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { getCurrentWindow } from '@tauri-apps/api/window';

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

  let customizeActivityBarOpen = $state(false);
  let profileManagerOpen = $state(false);

  /** Last path segment for a recent-repo label. */
  function basename(path: string): string {
    return path.split(/[/\\]/).filter(Boolean).pop() ?? path;
  }

  function openRecent(path: string) {
    // Routed through AppShell, which owns the open-repo flow.
    document.dispatchEvent(new CustomEvent('open-recent', { detail: path, bubbles: true }));
  }

  // ── Hamburger menu (absorbed from the old MenuBar) ─────────────────────────
  const recentItems = $derived<DropdownItem[]>(
    uiStore.recentRepos.length
      ? uiStore.recentRepos.map(path => ({
          kind: 'item' as const, id: path, label: basename(path), subtitle: path,
          icon: Clock, onclick: () => openRecent(path),
        }))
      : [{ kind: 'item' as const, id: '__none', label: 'No recent repositories', disabled: true, onclick: () => {} }],
  );

  const pluginMenuItems = $derived(
    contributionStore.forPoint('arbor:menu')
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name)),
  );

  const hamburgerMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'File' },
    { kind: 'item', id: 'open',   label: 'Open Repository…',            icon: FolderOpen, action: 'open_repo',     onclick: onOpen },
    { kind: 'item', id: 'clone',  label: 'Clone Repository…',           icon: Download,   action: 'clone_repo',    onclick: onClone },
    { kind: 'item', id: 'init',   label: 'Initialize Repository…',      icon: FolderPlus, action: 'init_repo',     onclick: onInit },
    { kind: 'item', id: 'browse', label: 'Browse Remote Repositories…', icon: Package,    action: 'repo_browser',  onclick: () => uiStore.openRepoBrowser() },
    { kind: 'group', id: 'recent', label: 'Recent', collapsible: true, defaultCollapsed: true, items: recentItems },
    { kind: 'separator', label: 'Tools' },
    { kind: 'item', id: 'plugins', label: 'Plugin Manager', icon: Package,    action: 'plugins',     onclick: () => uiStore.setPanel('plugins') },
    { kind: 'item', id: 'plogs',   label: 'Plugin Logs',    icon: ScrollText, action: 'plugin_logs', onclick: () => uiStore.setActiveBottomSection('plugin-logs') },
    ...(pluginMenuItems.length
      ? [
          { kind: 'separator' as const, label: 'Plugins' },
          ...pluginMenuItems.map(c => {
            const p = c.payload as { label?: string; action?: string };
            return {
              kind: 'item' as const,
              id: `${c.plugin_name}:${c.item_id}`,
              label: p.label ?? '',
              icon: Zap,
              meta: c.plugin_name,
              onclick: async () => {
                if (!p.action) return;
                try {
                  await firePluginAction(c.plugin_name, p.action, '{}');
                } catch (err) {
                  uiStore.showToast(`Plugin action failed: ${err}`, 'error');
                }
              },
            };
          }),
        ]
      : []),
    { kind: 'separator' },
    { kind: 'item', id: 'about', label: 'About Arbor', icon: Info,   onclick: () => uiStore.setPanel('about') },
    { kind: 'item', id: 'exit',  label: 'Exit',        icon: LogOut, danger: true, onclick: () => { void getCurrentWindow().close(); } },
  ]);

  // ── Settings menu (absorbed from the old settings dropdown + theme submenu) ──
  const themeItems = $derived<DropdownItem[]>([
    ...themeStore.builtIn.map(t => ({
      kind: 'item' as const, id: `theme:${t.id}`, label: t.name, icon: Palette,
      active: themeStore.activeId === t.id, onclick: () => void themeStore.setActive(t.id),
    })),
    ...(themeStore.custom.length
      ? [
          { kind: 'separator' as const, label: 'Custom' },
          ...themeStore.custom.map(t => ({
            kind: 'item' as const, id: `theme:${t.id}`, label: t.name, icon: Palette,
            active: themeStore.activeId === t.id, onclick: () => void themeStore.setActive(t.id),
          })),
        ]
      : []),
    { kind: 'separator' as const },
    { kind: 'item' as const, id: 'edit-themes', label: 'Edit themes…', icon: Settings, onclick: onOpenThemeEditor },
  ]);

  // ── Profiles submenu (quick-switch + manage) ─────────────────────────────────
  const profileItems = $derived<DropdownItem[]>([
    ...profileStore.list.map(name => ({
      kind: 'item' as const, id: `profile:${name}`, label: name, icon: User,
      active: profileStore.active === name,
      onclick: () => void profileStore.switchTo(name),
    })),
    { kind: 'separator' as const },
    { kind: 'item' as const, id: 'new-profile', label: 'New profile…', icon: Plus,
      onclick: () => { profileManagerOpen = true; } },
    { kind: 'item' as const, id: 'manage-profiles', label: 'Manage profiles…', icon: UserCog,
      onclick: () => { profileManagerOpen = true; } },
  ]);

  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'settings', label: 'Settings…', icon: Settings, action: 'settings',
      active: uiStore.activePanel === 'settings',
      onclick: () => uiStore.setPanel(uiStore.activePanel === 'settings' ? 'graph' : 'settings') },
    { kind: 'item', id: 'shortcuts', label: 'Keyboard Shortcuts…', icon: Keyboard, action: 'open_shortcuts',
      onclick: () => uiStore.openShortcutsHelp() },
    { kind: 'item', id: 'customize-ab', label: 'Customize Activity Bar…', icon: LayoutDashboard,
      onclick: () => { customizeActivityBarOpen = true; } },
    { kind: 'separator' },
    { kind: 'submenu', id: 'profiles', label: 'Profile', icon: UserCog, items: profileItems },
    { kind: 'submenu', id: 'theme', label: 'Theme', icon: Palette, items: themeItems },
  ]);
</script>

<TitleBar
  logoTooltip="Arbor"
  menu={hamburgerMenu}
  docs={{
    active: uiStore.activePanel === 'docs',
    tooltip: tooltipForAction('Documentation', 'toggle_docs'),
    onclick: () => uiStore.setPanel(uiStore.activePanel === 'docs' ? 'graph' : 'docs'),
  }}
  commandPalette={{
    active: uiStore.commandPaletteOpen,
    tooltip: tooltipForAction('Command palette', 'command_palette'),
    onclick: () => uiStore.toggleCommandPalette(),
  }}
  settings={{
    active: uiStore.activePanel === 'settings',
    tooltip: tooltipForAction('Settings', 'settings'),
    menu: settingsMenu,
    menuWidth: '230px',
    menuMaxHeight: 460,
  }}
>
  {#snippet logo()}
    <ArborLogo size={22} />
  {/snippet}

  {#snippet leading()}
    <!-- Workspace dropdown (replaces the tab bar that used to live here;
         repo tabs now sit above the main content area, IntelliJ-style). -->
    <div class="ws-slot">
      <WorkspaceDropdown onManage={onManageWorkspaces} onCreate={onCreateWorkspace} />
    </div>
    <div class="plugin-slot">
      <Contribution point="arbor:title-bar:left">
        {#snippet item({ payload, fire })}
          {@render pluginPill(payload, fire)}
        {/snippet}
      </Contribution>
    </div>
  {/snippet}

  {#snippet trailing()}
    <Contribution point="arbor:title-bar:right">
      {#snippet item({ payload, fire })}
        {@render pluginPill(payload, fire)}
      {/snippet}
    </Contribution>
  {/snippet}

  {#snippet windowControls()}
    <WindowControls />
  {/snippet}
</TitleBar>

{#if customizeActivityBarOpen}
  <CustomizeActivityBarModal onClose={() => customizeActivityBarOpen = false} />
{/if}

{#if profileManagerOpen}
  <ProfileManagerModal onClose={() => profileManagerOpen = false} />
{/if}

<!-- Plugin-contributed title-bar item (same pill shape on the left + right). -->
{#snippet pluginPill(payload: unknown, fire: () => void)}
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

<style>
  .ws-slot {
    display: flex;
    align-items: center;
    padding: 0 6px;
  }

  /* Plugin-contributed items rendered between the workspace dropdown and the
     draggable spacer. */
  .plugin-slot {
    display: flex;
    align-items: center;
    height: 100%;
    flex-shrink: 0;
    gap: 4px;
  }

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
