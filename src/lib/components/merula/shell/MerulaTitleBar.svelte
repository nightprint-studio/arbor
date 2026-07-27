<script lang="ts">
  /**
   * Merula titlebar — composes the shared `TitleBar` chrome and fills it with the
   * merula domain:
   *   logo · hamburger (file/project actions) · project fast-swap …
   *   … Run/Stop + log-level (IntelliJ-style) · layout toggles · settings · window controls
   *
   * The bar skeleton, hamburger menu and settings dropdown are all driven
   * declaratively through the shared widget; only merula-specific controls
   * (project switcher, transport, log threshold) are authored here as snippets.
   */
  import {
    Play, Square, SkipBack, SkipForward, Rewind, FastForward, ChevronDown, FolderGit2, Download, Settings, ScrollText, Keyboard,
    PanelLeft, PanelRight, Minimize2, Check, AlertTriangle,
    FolderOpen, FolderPlus, FilePlus2, Save, Clock, LogOut, FolderPen,
    FileAudio, FileMusic, Layers, PackageOpen, Crop, SlidersHorizontal, LayoutGrid,
    User, Plus, UserCog,
  } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import RecentProjectsModal from './RecentProjectsModal.svelte';
  // Profiles are a global Arbor concept (shared vault/settings across windows);
  // both the store and the manager modal are window-agnostic and reused as-is.
  import ProfileManagerModal from '$lib/components/shared/ProfileManagerModal.svelte';
  import { profileStore } from '$lib/stores/profiles.svelte';
  // Titlebar lives at the very top — tooltips fly downward so they don't get
  // clipped by the window edge.
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';
  import { merulaStore, LOG_LEVELS } from '../merula-store.svelte';
  import { merulaEngine } from '../stores/engine.svelte';
  import { configStore } from '../stores/config.svelte';
  import { workspaceStore } from '../stores/workspace.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { renderStore } from '../stores/render.svelte';
  import { transportUiStore } from '../stores/transport-ui.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';

  let recentOpen = $state(false);
  let profileManagerOpen = $state(false);

  // Skip-to-end targets the last cycle of the evaluated arrangement (its content
  // end); disabled while the arrangement is empty (nothing to skip to).
  const arrangementEnd = $derived(arrangementStore.contentEnd);
  const arrangementEmpty = $derived(arrangementStore.empty);

  // Export split button reflects the render job: spinner while bouncing, then a
  // brief ✓ / ⚠ so the user sees it finished (or why it didn't) instead of
  // nothing. Idle tip echoes the active format — the main action quick-exports
  // it; the chevron opens the format / "Edit export…" menu.
  const exportFmtLabel = $derived(projectActions.exportFormat.toUpperCase());
  const exportTip = $derived(
    renderStore.status === 'rendering' ? `Rendering ${renderStore.file ?? 'audio'}…`
    : renderStore.status === 'done'    ? `Exported ${renderStore.file ?? 'audio'}`
    : renderStore.status === 'failed'  ? `Render failed${renderStore.error ? `: ${renderStore.error}` : ''}`
    : `Export ${exportFmtLabel}`,
  );

  // Export menu (chevron): pick the default format (single-select check) or open
  // the full options dialog for loops + live duration/size estimate.
  const exportMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Format' },
    { kind: 'item', id: 'wav', label: 'WAV — lossless PCM', icon: FileAudio,
      active: projectActions.exportFormat === 'wav', onclick: () => projectActions.setExportFormat('wav') },
    { kind: 'item', id: 'ogg', label: 'OGG Vorbis — compressed', icon: FileAudio,
      active: projectActions.exportFormat === 'ogg', onclick: () => projectActions.setExportFormat('ogg') },
    { kind: 'separator' },
    { kind: 'item', id: 'region', label: 'Export loop region…', icon: Crop,
      disabled: !projectActions.canExportRegion, onclick: () => projectActions.exportRegion() },
    { kind: 'item', id: 'stems', label: 'Export stems…', icon: Layers,
      onclick: () => projectActions.exportStems() },
    { kind: 'item', id: 'export_all', label: 'Export all…', icon: PackageOpen,
      onclick: () => projectActions.exportAll() },
    { kind: 'item', id: 'midi', label: 'Export MIDI…', icon: FileMusic,
      onclick: () => projectActions.exportMidi() },
    { kind: 'item', id: 'edit', label: 'Edit export…', icon: SlidersHorizontal,
      shortcut: 'Ctrl+Shift+R', onclick: () => projectActions.exportWav() },
  ]);

  /** Last path segment (forward- or back-slash) for a recents label. */
  function basename(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  // macOS: the hamburger becomes the real menu bar (File · Project, from the
  // menu's own labelled separators). No-op elsewhere.
  const publishNativeMenu = createNativeMenuPublisher('merula');

  // ── Hamburger (file / project actions — absorbed from the old MerulaMenuBar) ──
  const hamburgerMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'File' },
    { kind: 'item', id: 'new',      label: 'New Project…',     icon: FolderPlus, shortcut: 'Ctrl+Shift+N', onclick: () => projectActions.newProject() },
    { kind: 'item', id: 'open',     label: 'Open Project…',    icon: FolderOpen, shortcut: 'Ctrl+O',       onclick: () => projectActions.openProject() },
    { kind: 'item', id: 'openfile', label: 'Open File…',       icon: FilePlus2,  shortcut: 'Ctrl+Shift+O', onclick: () => projectActions.openFile() },
    { kind: 'item', id: 'recent',   label: 'Recent Projects…', icon: Clock,                                  onclick: () => { recentOpen = true; } },
    { kind: 'separator', label: 'Project' },
    { kind: 'item', id: 'save',   label: 'Save',           icon: Save,     shortcut: 'Ctrl+S',       onclick: () => projectActions.save() },
    { kind: 'item', id: 'export', label: 'Export audio…', icon: Download, shortcut: 'Ctrl+Shift+R', onclick: () => projectActions.exportWav() },
    { kind: 'item', id: 'export_region', label: 'Export loop region…', icon: Crop, disabled: !projectActions.canExportRegion, onclick: () => projectActions.exportRegion() },
    { kind: 'item', id: 'export_stems', label: 'Export stems…', icon: Layers, onclick: () => projectActions.exportStems() },
    { kind: 'item', id: 'export_midi', label: 'Export MIDI…', icon: FileMusic, onclick: () => projectActions.exportMidi() },
    { kind: 'separator' },
    { kind: 'item', id: 'close', label: 'Close Window', icon: LogOut, danger: true, onclick: () => { void getCurrentWindow().close(); } },
  ]);

  // ── Profiles submenu (quick-switch + manage) ─────────────────────────────────
  // Profiles are global: switching here reloads every window. Mirrors the main
  // Arbor titlebar so the two windows expose the same surface.
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

  // ── Settings (gear) ─────────────────────────────────────────────────────────
  // The command palette has its own dedicated titlebar button (next to the gear,
  // Arbor-style), so it's no longer duplicated here.
  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'zen', label: 'Zen mode', icon: Minimize2, shortcut: 'Ctrl+Shift+Z', onclick: () => merulaStore.toggleZen() },
    { kind: 'separator' },
    { kind: 'item', id: 'settings',  label: 'Settings…',           icon: Settings,  shortcut: 'Ctrl+,', onclick: () => merulaStore.openSettings() },
    { kind: 'item', id: 'shortcuts', label: 'Keyboard Shortcuts…', icon: Keyboard,  shortcut: 'Shift+F1', onclick: () => merulaStore.openShortcuts() },
    { kind: 'separator' },
    { kind: 'submenu', id: 'profiles', label: 'Profile', icon: UserCog, items: profileItems },
  ]);

  // ── Project fast-swap (workspaces + their projects / recents + open) ──────────
  const projectName = $derived(projectStore.project?.name ?? 'No project');
  const activeWs = $derived(workspaceStore.activeWorkspaceObj);
  // When a workspace is active, its member projects are the swap list; otherwise
  // fall back to the global recents.
  const swapPaths = $derived(activeWs ? activeWs.project_paths : workspaceStore.recentProjects);
  const projectItems = $derived<DropdownItem[]>([
    // Workspaces section: switch the active group (or clear it), then Manage…
    ...(workspaceStore.workspaces.length
      ? [
          { kind: 'separator' as const, label: 'Workspaces' },
          ...workspaceStore.workspaces.map(w => ({
            kind: 'item' as const, id: `ws:${w.id}`, label: w.name, icon: Layers,
            active: w.id === workspaceStore.activeWorkspace,
            onclick: () => workspaceStore.setActiveWorkspace(w.id === workspaceStore.activeWorkspace ? null : w.id),
          })),
        ]
      : []),
    { kind: 'item' as const, id: '__manage_ws', label: 'Manage workspaces…', icon: LayoutGrid, onclick: () => merulaStore.openWorkspaces() },
    { kind: 'separator' as const, label: activeWs ? `${activeWs.name} · projects` : 'Recent' },
    ...swapPaths.map(path => ({
      kind: 'item' as const, id: path, label: basename(path), subtitle: path,
      icon: FolderGit2, active: path === projectStore.project?.path,
      onclick: () => void projectStore.open(path).catch(() => {}),
    })),
    ...(projectStore.project
      ? [{ kind: 'separator' as const },
         { kind: 'item' as const, id: '__rename', label: 'Rename project…', icon: FolderPen, onclick: () => merulaStore.openRenameProject() }]
      : []),
    { kind: 'item' as const, id: '__open', label: 'Open project…', icon: FolderOpen, shortcut: 'Ctrl+O', onclick: () => projectActions.openProject() },
  ]);

  // ── Log threshold ───────────────────────────────────────────────────────────
  const logItems = $derived<DropdownItem[]>(
    LOG_LEVELS.map(l => ({
      kind: 'item', id: l, label: l, active: configStore.logThreshold === l,
      onclick: () => configStore.setLogThreshold(l),
    })),
  );
</script>

<TitleBar
  logoTooltip="merula — music live-coding"
  menu={hamburgerMenu}
  onNativeMenu={publishNativeMenu}
  menuWidth="240px"
  docs={{ active: merulaStore.docsOpen, tooltip: 'Documentation (F1)', onclick: () => merulaStore.toggleDocs() }}
  commandPalette={{ active: merulaStore.paletteOpen, tooltip: 'Command palette (Ctrl+K)', onclick: () => merulaStore.togglePalette() }}
  settings={{ menu: settingsMenu, menuWidth: '220px', tooltip: 'Settings' }}
>
  {#snippet logo()}
    <ArborLogo size={22} />
  {/snippet}

  <!-- Project fast-swap -->
  {#snippet leading()}
    <Dropdown items={projectItems} position="fixed" direction="down" width="240px">
      {#snippet trigger({ open, toggle })}
        <button class="gtb-project" class:open onclick={toggle} use:tooltip={'Switch project'} aria-haspopup="menu" aria-expanded={open}>
          <FolderGit2 size={14} />
          <span class="gtb-project-name">{projectName}</span>
          <ChevronDown size={12} />
        </button>
      {/snippet}
    </Dropdown>
  {/snippet}

  <!-- Log level + transport cluster (IntelliJ-style) -->
  {#snippet trailing()}
    <div class="gtb-run-cluster">
      <Dropdown items={logItems} position="fixed" direction="down" width="160px">
        {#snippet trigger({ open, toggle })}
          <button class="gtb-log" class:open onclick={toggle} use:tooltip={'Log threshold'} aria-haspopup="menu" aria-expanded={open}>
            <ScrollText size={13} />
            <span>{configStore.logThreshold}</span>
            <ChevronDown size={11} />
          </button>
        {/snippet}
      </Dropdown>

      <div class="gtb-sep"></div>

      <button
        class="gtb-run-icon"
        onclick={() => void merulaEngine.seekToStart()}
        use:tooltip={{ content: 'Skip to start', shortcut: 'Ctrl+Shift+[' }}
        aria-label="Skip to start"
      >
        <SkipBack size={14} fill="currentColor" />
      </button>
      <button
        class="gtb-run-icon"
        onclick={() => transportUiStore.stepBy(-configStore.skipStep, arrangementEnd)}
        use:tooltip={{ content: `Step back ${configStore.skipStepLabel}`, shortcut: 'Ctrl+[' }}
        aria-label="Step back"
      >
        <Rewind size={14} fill="currentColor" />
      </button>
      <button
        class="gtb-run"
        class:running={merulaEngine.running}
        onclick={() => void merulaEngine.toggleRun(projectStore.activeSource, projectStore.project?.path)}
        use:tooltip={{ content: merulaEngine.running ? 'Stop' : 'Run', shortcut: 'Shift+F9' }}
        aria-label={merulaEngine.running ? 'Stop' : 'Run'}
      >
        {#if merulaEngine.running}<Square size={14} fill="currentColor" />{:else}<Play size={14} fill="currentColor" />{/if}
      </button>
      <button
        class="gtb-run-icon"
        onclick={() => transportUiStore.stepBy(configStore.skipStep, arrangementEnd)}
        disabled={arrangementEmpty}
        use:tooltip={{ content: `Step forward ${configStore.skipStepLabel}`, shortcut: 'Ctrl+]' }}
        aria-label="Step forward"
      >
        <FastForward size={14} fill="currentColor" />
      </button>
      <button
        class="gtb-run-icon"
        onclick={() => void merulaEngine.seekToEnd(arrangementEnd)}
        disabled={arrangementEmpty}
        use:tooltip={{ content: 'Skip to end', shortcut: 'Ctrl+Shift+]' }}
        aria-label="Skip to end"
      >
        <SkipForward size={14} fill="currentColor" />
      </button>

      <div class="gtb-sep"></div>

      <!-- Export split: main = quick export (last format), chevron = format / Edit export… -->
      <Dropdown items={exportMenu} position="fixed" direction="down" width="230px">
        {#snippet trigger({ open, toggle })}
          <div class="gtb-export" class:open>
            <button
              class="gtb-run-icon gtb-export-main"
              class:rendering={renderStore.active}
              class:ok={renderStore.status === 'done'}
              class:err={renderStore.status === 'failed'}
              onclick={() => projectActions.quickExport()}
              disabled={renderStore.active}
              use:tooltip={exportTip}
              aria-label="Export audio"
            >
              {#if renderStore.status === 'rendering'}<Spinner size={13} />
              {:else if renderStore.status === 'done'}<Check size={14} />
              {:else if renderStore.status === 'failed'}<AlertTriangle size={14} />
              {:else}<Download size={14} />{/if}
              <span class="gtb-export-fmt">{exportFmtLabel}</span>
            </button>
            <button
              class="gtb-export-chevron"
              onclick={toggle}
              disabled={renderStore.active}
              use:tooltip={'Export options'}
              aria-haspopup="menu" aria-expanded={open} aria-label="Export options" tabindex="-1"
            >
              <ChevronDown size={11} />
            </button>
          </div>
        {/snippet}
      </Dropdown>
    </div>
  {/snippet}

  <!-- Layout toggles -->
  {#snippet actions()}
    <button
      class="gtb-icon" class:active={!merulaStore.collapseUi}
      onclick={() => merulaStore.toggleCollapseUi()}
      use:tooltip={merulaStore.collapseUi ? 'Show arrangement' : 'Hide arrangement'}
      aria-label="Toggle arrangement" aria-pressed={!merulaStore.collapseUi}
    ><PanelLeft size={17} /></button>
    <button
      class="gtb-icon" class:active={!merulaStore.collapseTabpane}
      onclick={() => merulaStore.toggleCollapseTabpane()}
      use:tooltip={merulaStore.collapseTabpane ? 'Show editor' : 'Hide editor'}
      aria-label="Toggle editor" aria-pressed={!merulaStore.collapseTabpane}
    ><PanelRight size={17} /></button>
    <div class="gtb-sep"></div>
  {/snippet}

  {#snippet windowControls()}
    <WindowControls />
  {/snippet}
</TitleBar>

{#if recentOpen}
  <RecentProjectsModal onClose={() => recentOpen = false} />
{/if}

{#if profileManagerOpen}
  <ProfileManagerModal onClose={() => profileManagerOpen = false} />
{/if}

<style>
  /* Merula-specific controls authored as snippets above. Svelte scopes these
     rules to the elements declared in this component, so they still apply
     even though the markup is rendered by the shared TitleBar. */
  .gtb-icon {
    display: flex; align-items: center; justify-content: center;
    width: 34px; height: 34px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .gtb-icon:hover { background: var(--bg-hover); color: var(--text-primary); }
  .gtb-icon.active { color: var(--accent); }

  .gtb-project {
    display: flex; align-items: center; gap: 7px;
    height: 28px; margin-left: 4px; padding: 0 9px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-primary);
    font-size: 12px; font-weight: 500; cursor: pointer;
    transition: border-color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .gtb-project:hover, .gtb-project.open { border-color: var(--border-focus); }
  .gtb-project-name { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gtb-project :global(svg:first-child) { color: var(--accent); }

  /* ── Run cluster ── */
  /* IntelliJ leaves a clear gap before the layout toggles / window controls —
     don't let the transport sit flush against the right edge. */
  .gtb-run-cluster { display: flex; align-items: center; gap: 2px; height: 100%; padding-right: 80px; }
  .gtb-run, .gtb-run-icon {
    display: flex; align-items: center; justify-content: center;
    width: 30px; height: 28px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .gtb-run { color: var(--success); }
  .gtb-run:hover { background: color-mix(in srgb, var(--success) 16%, transparent); }
  .gtb-run.running { color: var(--error); }
  .gtb-run.running:hover { background: color-mix(in srgb, var(--error) 16%, transparent); }
  .gtb-run-icon { color: var(--text-muted); }
  .gtb-run-icon:hover { background: var(--bg-hover); color: var(--text-secondary); }
  /* Render-job feedback: busy (accent), succeeded (success), failed (error). */
  .gtb-run-icon.rendering { color: var(--accent); }
  .gtb-run-icon.rendering:hover { background: transparent; }
  .gtb-run-icon.ok { color: var(--success); }
  .gtb-run-icon.err { color: var(--error); }
  .gtb-run-icon:disabled { cursor: default; }

  /* ── Export split (quick export + format / options menu) ──
     Main icon keeps the .gtb-run-icon status feedback; chevron is a slim
     attached toggle so the pair reads as one control. */
  .gtb-export { display: flex; align-items: center; height: 28px; -webkit-app-region: no-drag; }
  .gtb-export .gtb-export-main {
    width: auto;
    gap: 5px;
    padding: 0 8px;
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }
  /* Active export format — shown on the button so the target is legible at a
     glance (and tracks the format picked from the chevron menu). */
  .gtb-export-fmt {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    font-variant-numeric: tabular-nums;
  }
  .gtb-export-chevron {
    display: flex; align-items: center; justify-content: center;
    width: 15px; height: 28px;
    background: transparent; border: none;
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .gtb-export-chevron:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .gtb-export.open .gtb-export-chevron { background: var(--bg-hover); color: var(--accent); }
  .gtb-export-chevron:disabled { cursor: default; opacity: 0.5; }

  .gtb-log {
    display: flex; align-items: center; gap: 6px;
    height: 26px; padding: 0 9px;
    background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-secondary);
    font-size: 11.5px; cursor: pointer;
    transition: border-color var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .gtb-log span { text-transform: capitalize; }
  .gtb-log:hover, .gtb-log.open { border-color: var(--border-focus); color: var(--text-primary); }

  .gtb-sep { width: 1px; height: 18px; background: var(--border); margin: 0 6px; flex-shrink: 0; }
</style>
