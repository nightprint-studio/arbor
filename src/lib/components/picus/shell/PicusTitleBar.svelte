<script lang="ts">
  /**
   * Picus titlebar — composes the shared `TitleBar` chrome:
   *   logo · hamburger · project breadcrumb · connection pill ·
   *   [gap] · palette · docs · settings · window controls
   *
   * The connection pill is the piece that makes this window Picus rather than
   * any other Arbor product: it names the database every new tab will bind to,
   * and its popover is the fastest path between sessions. Everything else is the
   * standard Arbor bar, on purpose.
   */
  import {
    FolderOpen, LogOut, Settings, Keyboard, Info, Palette, Plus,
    Database, RefreshCw, GitCompare,
  } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import ThemeEditorModal from '$lib/components/shared/ThemeEditorModal.svelte';
  import WorkspaceTabs from '$lib/components/shared/internal/WorkspaceTabs.svelte';
  import PicusConnectionPill from '../PicusConnectionPill.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';
  import { windowMenuItems } from '$lib/utils/window-menu';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionsStore, connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { DIALECTS } from '$lib/types/picus';

  let themeEditorOpen = $state(false);

  const project = $derived(picusProjectStore.project);

  // ── Connection popover ──────────────────────────────────────────────────────
  // Every session, with host + version on the right so "which one is behind?" is
  // answerable without opening anything.
  const connectionMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Connections' },
    ...connectionsStore.connections.map<DropdownItem>((c) => ({
      kind: 'item',
      id: c.id,
      label: c.name,
      subtitle: `${c.alias} · ${c.schema}@${c.host}`,
      meta: `${DIALECTS[c.dialect].short} · v${c.dbVersion}${c.readOnly ? ' · read-only' : ''}`,
      icon: Database,
      iconColor: connectionColorVar(c),
      active: c.id === connectionsStore.activeId,
      onclick: () => connectionsStore.setActive(c.id),
    })),
    { kind: 'separator' },
    {
      kind: 'item', id: 'new-conn', label: 'Add a connection…', icon: Plus, shortcut: 'Ctrl+Shift+N',
      onclick: () => picusUiStore.openConnectionEditor(null),
    },
    {
      kind: 'item', id: 'compare', label: 'Compare two connections…', icon: GitCompare,
      onclick: () => toastStore.show('Schema comparison arrives after the consistency milestone.', 'info'),
    },
    {
      kind: 'item', id: 'refresh', label: 'Refresh schema cache', icon: RefreshCw,
      onclick: () => toastStore.show('Schema cache refreshed.', 'success'),
    },
  ]);

  // ── Hamburger ───────────────────────────────────────────────────────────────
  // macOS: the hamburger becomes the real menu bar. No-op elsewhere.
  const publishNativeMenu = createNativeMenuPublisher('Picus');

  const hamburgerMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Project' },
    {
      // A repository belongs to a connection, so "open" is "attach a folder to the
      // connection I am on" — there is nothing to open without one, and saying so
      // is better than a picker that would have nowhere to put the answer.
      kind: 'item',
      id: 'open',
      label: picusProjectStore.attached ? 'Change the script folder…' : 'Attach a script folder…',
      icon: FolderOpen,
      disabled: !connectionsStore.activeId,
      onclick: () => {
        if (connectionsStore.activeId) picusUiStore.openScriptRootPicker(connectionsStore.activeId);
        else toastStore.show('Select a connection first — a repository belongs to a database.', 'info');
      },
    },
    {
      kind: 'item', id: 'rescan', label: 'Re-read the scripts from disk', icon: RefreshCw, shortcut: 'F5',
      disabled: !picusProjectStore.attached,
      onclick: () => void picusProjectStore.refresh(),
    },
    {
      kind: 'item', id: 'newconn', label: 'Add a connection…', icon: Database, shortcut: 'Ctrl+Shift+N',
      onclick: () => picusUiStore.openConnectionEditor(null),
    },
    ...windowMenuItems(),
    { kind: 'separator' },
    { kind: 'item', id: 'about', label: 'About Picus', icon: Info, onclick: () => picusUiStore.openAbout() },
    {
      kind: 'item', id: 'close', label: 'Close Window', icon: LogOut, danger: true,
      onclick: () => { void getCurrentWindow().close(); },
    },
  ]);

  // ── Settings (gear) ─────────────────────────────────────────────────────────
  const themeItems = $derived<DropdownItem[]>([
    ...themeStore.builtIn.map<DropdownItem>((t) => ({
      kind: 'item', id: `theme:${t.id}`, label: t.name, icon: Palette,
      active: themeStore.activeId === t.id, onclick: () => void themeStore.setActive(t.id),
    })),
    ...(themeStore.custom.length
      ? [
          { kind: 'separator' as const, label: 'Custom' },
          ...themeStore.custom.map<DropdownItem>((t) => ({
            kind: 'item' as const, id: `theme:${t.id}`, label: t.name, icon: Palette,
            active: themeStore.activeId === t.id, onclick: () => void themeStore.setActive(t.id),
          })),
        ]
      : []),
    { kind: 'separator' },
    { kind: 'item', id: 'edit-themes', label: 'Edit themes…', icon: Settings, onclick: () => { themeEditorOpen = true; } },
  ]);

  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'settings', label: 'Settings…', icon: Settings, shortcut: 'Ctrl+,', onclick: () => picusUiStore.openSettings() },
    { kind: 'item', id: 'shortcuts', label: 'Keyboard shortcuts…', icon: Keyboard, shortcut: 'Shift+F1', onclick: () => picusUiStore.openShortcuts() },
    { kind: 'separator' },
    { kind: 'submenu', id: 'theme', label: 'Theme', icon: Palette, items: themeItems },
  ]);
</script>

<TitleBar
  logoTooltip="Picus — SQL studio"
  menu={hamburgerMenu}
  onNativeMenu={publishNativeMenu}
  nativeMenuEnabled={surfaceStore.hasFocus('picus')}
  menuWidth="250px"
  docs={{ active: picusUiStore.docsOpen, tooltip: 'Documentation (F1)', onclick: () => picusUiStore.toggleDocs() }}
  commandPalette={{ active: picusUiStore.paletteOpen, tooltip: 'Command palette (Ctrl+K)', onclick: () => picusUiStore.togglePalette() }}
  settings={{ menu: settingsMenu, menuWidth: '220px', tooltip: 'Settings' }}
>
  {#snippet logo()}
    <ArborLogo size={22} />
  {/snippet}

  {#snippet center()}
    <!-- Product tabs, when this window is the tabbed container. Empty in a
         standalone Picus window. -->
    <WorkspaceTabs />
  {/snippet}

  {#snippet leading()}
    <!-- Project breadcrumb, then the connection every new tab binds to. -->
    <button
      class="ptb-crumb"
      onclick={() => picusUiStore.showSection('scripts')}
      use:tooltip={project ? `${project.name} · ${project.root}` : 'No project open'}
      aria-label="Script project"
    >
      <Monogram name={project?.name ?? 'Picus'} size={16} />
      <span class="ptb-crumb-name">{project?.name ?? 'No project'}</span>
    </button>

    <span class="ptb-sep" aria-hidden="true">/</span>

    <Dropdown items={connectionMenu} position="fixed" direction="down" width="330px">
      {#snippet trigger({ open, toggle })}
        <PicusConnectionPill
          connection={connectionsStore.active}
          density="titlebar"
          {open}
          onclick={toggle}
        />
      {/snippet}
    </Dropdown>
  {/snippet}

  {#snippet windowControls()}
    <WindowControls />
  {/snippet}
</TitleBar>

{#if themeEditorOpen}
  <ThemeEditorModal onClose={() => (themeEditorOpen = false)} />
{/if}

<style>
  .ptb-crumb {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 24px;
    padding: 0 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    white-space: nowrap;
    cursor: pointer;
    -webkit-app-region: no-drag;
    transition: background var(--transition-fast);
  }
  .ptb-crumb:hover { background: var(--bg-hover); }
  .ptb-crumb-name { max-width: 180px; overflow: hidden; text-overflow: ellipsis; }

  .ptb-sep {
    color: var(--text-disabled);
    font-size: 12px;
    padding: 0 1px;
    -webkit-app-region: no-drag;
  }
</style>
