<script lang="ts">
  /**
   * Bennu titlebar — composes the shared `TitleBar` chrome:
   *   logo · hamburger (open/demo/close) · project fast-swap · … ·
   *   run controls (▷ Run · 🐛 Debug · ⋮) · [gap] · palette · docs · settings ·
   *   window controls
   *
   * Layout matches Corvus/IntelliJ New UI: the project switcher sits on the LEFT
   * (leading), while the run controls and the app buttons (palette/docs/settings)
   * all live on the RIGHT of the bar next to the window controls. A small spacer
   * separates the run cluster from the app buttons so they read as two groups.
   *
   * The project switcher shows the current project name with a dropdown of recent
   * projects + "Open project…" (shared folder picker, never a native dialog). Its
   * trigger matches Corvus's `WorkspaceDropdown` look (transparent → bg-hover on
   * hover/open, Monogram + name + chevron). Run/Debug/More are UI stubs (toast).
   */
  import {
    FolderOpen, FolderPlus, LogOut, Settings, Keyboard, FlaskConical,
    Play, Bug, MoreVertical, Palette, SlidersHorizontal, Info, Hammer, Square, TriangleAlert,
    ListChecks, ChevronDown,
  } from 'lucide-svelte';
  import type { BuildType } from '$lib/stores/bennu/run.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ArborLogo from '$lib/components/shared/internal/ArborLogo.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import BennuWorkspaceSwitcher from './BennuWorkspaceSwitcher.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';
  import { windowMenuItems } from '$lib/utils/window-menu';
  import WorkspaceTabs from '$lib/components/shared/internal/WorkspaceTabs.svelte';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import ThemeEditorModal from '$lib/components/shared/ThemeEditorModal.svelte';

  let pickerOpen = $state(false);
  // Whether the folder picker opens a NEW workspace (replace) or ADDS a project to the current one.
  let pickerMode = $state<'open' | 'add'>('open');
  let themeEditorOpen = $state(false);

  // Run/Build are wired to bennu-be (build.rs); Debug isn't implemented yet (a
  // later wave — it toasts). All disabled when no project is open.
  const hasProject = $derived(!!projectStore.project);
  const busy = $derived(bennuRunStore.active);
  function notImplemented(what: string) {
    toastStore.show(`${what} isn't implemented yet.`, 'info');
  }

  /** ▶ Run — build then launch the ACTIVE run configuration; if the project has no
   *  active config yet, open the run-config editor to create/pick one. */
  function runProject() {
    const root = projectStore.project?.root;
    if (!root) return;
    void bennuRunStore.runActive(root).then((ran) => {
      if (!ran) bennuUiStore.openRunConfig();
    });
  }
  /** Run the preferred build type (Maven compile or whole-project validation) — the split-button
   *  main action + Ctrl+F9. */
  function buildProject() {
    const root = projectStore.project?.root;
    if (root) void bennuRunStore.runPreferred(root);
  }
  /** Pick a build type from the split dropdown: make it the default AND run it now. */
  function selectBuild(type: BuildType) {
    const root = projectStore.project?.root;
    void bennuRunStore.setPreferredBuildType(type);
    if (!root) return;
    if (type === 'validate') void bennuRunStore.validateProject(root);
    else void bennuRunStore.build(root);
  }

  // The active build type drives the main button's icon + label.
  const buildType = $derived(bennuRunStore.preferredBuildType);
  const buildLabel = $derived(
    buildType === 'validate' ? 'Validate project (no compile)' : 'Build project (Maven)',
  );
  // The split dropdown: choose (and run) a build type.
  const buildMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Build with' },
    { kind: 'item', id: 'mvn',      label: 'Maven build',           icon: Hammer,     disabled: busy, onclick: () => selectBuild('mvn') },
    { kind: 'item', id: 'validate', label: 'Validate (no compile)', icon: ListChecks, disabled: busy, onclick: () => selectBuild('validate') },
  ]);

  const runMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'run',     label: 'Run',             icon: Play,   shortcut: 'Shift+F10', disabled: busy, onclick: runProject },
    { kind: 'item', id: 'build',   label: buildLabel,        icon: buildType === 'validate' ? ListChecks : Hammer, shortcut: 'Ctrl+F9', disabled: busy, onclick: buildProject },
    { kind: 'item', id: 'validate', label: 'Validate project', icon: ListChecks, disabled: busy, onclick: () => selectBuild('validate') },
    { kind: 'item', id: 'stop',    label: 'Stop',            icon: Square, disabled: !bennuRunStore.running, onclick: () => void bennuRunStore.stop() },
    { kind: 'separator' },
    { kind: 'item', id: 'debug',   label: 'Debug…',          icon: Bug,  onclick: () => notImplemented('Debug') },
    { kind: 'separator' },
    { kind: 'item', id: 'editcfg', label: 'Edit configurations…', icon: SlidersHorizontal, disabled: !hasProject, onclick: () => bennuUiStore.openRunConfig() },
  ]);

  // Ctrl+O (window keybinding) → open the folder picker hosted here.
  $effect(() => {
    function open() { openPicker('open'); }
    window.addEventListener('bennu:open-project', open);
    return () => window.removeEventListener('bennu:open-project', open);
  });

  /** Open `dir` as a new single-project workspace (replaces the current one). */
  async function openProjectDirect(dir: string) {
    try { await projectStore.openProject(dir); } catch { /* mock fallback already applied */ }
  }

  /** Folder-picker confirm — either replace the workspace (`open`) or add a project to it (`add`). */
  async function confirmPicker(dir: string) {
    pickerOpen = false;
    if (pickerMode === 'add') { await projectStore.addProject(dir); return; }
    await openProjectDirect(dir);
  }

  function openPicker(mode: 'open' | 'add') { pickerMode = mode; pickerOpen = true; }

  // ── Hamburger (file / project actions) ────────────────────────────────────────
  // macOS: the hamburger becomes the real menu bar (Project, from the menu's own
  // labelled separator). No-op elsewhere.
  const publishNativeMenu = createNativeMenuPublisher('Bennu');

  const hamburgerMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Project' },
    { kind: 'item', id: 'open',  label: 'Open project…', icon: FolderOpen, shortcut: 'Ctrl+O', onclick: () => openPicker('open') },
    { kind: 'item', id: 'addproj', label: 'Add project to workspace…', icon: FolderPlus, disabled: !hasProject, onclick: () => openPicker('add') },
    { kind: 'item', id: 'projectcfg', label: 'Project Configuration…', icon: SlidersHorizontal, disabled: !hasProject, onclick: () => bennuUiStore.openProjectConfig() },
    // MOCK — remove the "Load demo project" entry when bennu-be serves real data.
    { kind: 'item', id: 'demo',  label: 'Load demo project', icon: FlaskConical, onclick: () => projectStore.loadDemo() },
    ...windowMenuItems(),
    { kind: 'separator' },
    { kind: 'item', id: 'about', label: 'About Bennu', icon: Info, onclick: () => bennuUiStore.openAbout() },
    { kind: 'item', id: 'close', label: 'Close Window', icon: LogOut, danger: true, onclick: () => { void getCurrentWindow().close(); } },
  ]);

  // ── Settings (gear) — Corvus-style: quick items + an expandable Theme submenu ──
  // Theme is a `submenu` flyout on the gear (built-in + custom themes toggle the
  // global themeStore live; "Edit themes…" opens the shared editor). NOT a settings
  // modal section — the gear expands it inline, exactly like Corvus's titlebar.
  const themeItems = $derived<DropdownItem[]>([
    ...themeStore.builtIn.map((t) => ({
      kind: 'item' as const, id: `theme:${t.id}`, label: t.name, icon: Palette,
      active: themeStore.activeId === t.id, onclick: () => void themeStore.setActive(t.id),
    })),
    ...(themeStore.custom.length
      ? [
          { kind: 'separator' as const, label: 'Custom' },
          ...themeStore.custom.map((t) => ({
            kind: 'item' as const, id: `theme:${t.id}`, label: t.name, icon: Palette,
            active: themeStore.activeId === t.id, onclick: () => void themeStore.setActive(t.id),
          })),
        ]
      : []),
    { kind: 'separator' as const },
    { kind: 'item' as const, id: 'edit-themes', label: 'Edit themes…', icon: Settings, onclick: () => { themeEditorOpen = true; } },
  ]);

  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'settings',  label: 'Settings…',           icon: Settings,  shortcut: 'Ctrl+,',   onclick: () => bennuUiStore.openSettings() },
    { kind: 'item', id: 'shortcuts', label: 'Keyboard shortcuts…', icon: Keyboard,  shortcut: 'F1',       onclick: () => bennuUiStore.toggleDocs() },
    { kind: 'separator' },
    { kind: 'submenu', id: 'theme', label: 'Theme', icon: Palette, items: themeItems },
  ]);

</script>

<TitleBar
  logoTooltip="Bennu — Java editor"
  menu={hamburgerMenu}
  onNativeMenu={publishNativeMenu}
  nativeMenuEnabled={surfaceStore.hasFocus('bennu')}
  menuWidth="240px"
  docs={{ active: bennuUiStore.docsOpen, tooltip: 'Documentation (F1)', onclick: () => bennuUiStore.toggleDocs() }}
  commandPalette={{ active: bennuUiStore.paletteOpen, tooltip: 'Command palette (Ctrl+K)', onclick: () => bennuUiStore.togglePalette() }}
  settings={{ menu: settingsMenu, menuWidth: '220px', tooltip: 'Settings' }}
>
  {#snippet logo()}
    <ArborLogo size={22} />
  {/snippet}

  <!-- Project/workspace switcher — Corvus-tree: workspace headers + nested projects. -->
  {#snippet center()}
    <!-- Product tabs, when this window is the tabbed container: they belong to
         the window, not to Bennu (nothing in a standalone Bennu window). -->
    <WorkspaceTabs />
  {/snippet}

  {#snippet leading()}
    <BennuWorkspaceSwitcher onOpenPicker={openPicker} />
  {/snippet}

  <!-- Right cluster head: the Run / Debug / overflow run-controls, then a small
       gap before the app buttons (palette · docs · settings). -->
  {#snippet trailing()}
    {#if bennuDiagnosticsStore.jdkMissing}
      <button
        class="btb-jdk-warn"
        onclick={() => bennuUiStore.openSettings()}
        use:tooltip={'No JDK found — completion and navigation can’t resolve the standard library. Click to set a JDK path in Settings.'}
        aria-label="No JDK found — open Settings"
      >
        <TriangleAlert size={14} />
        <span class="btb-jdk-warn-label">No JDK</span>
      </button>
    {/if}
    <div class="btb-run" role="group" aria-label="Run controls">
      <button
        class="btb-run-btn btb-build-main"
        onclick={buildProject}
        disabled={!hasProject || busy}
        use:tooltip={{ content: buildLabel, shortcut: 'Ctrl+F9' }}
        aria-label={buildLabel}
      >
        {#if buildType === 'validate'}
          <ListChecks size={16} />
        {:else}
          <Hammer size={16} />
        {/if}
      </button>
      <Dropdown items={buildMenu} position="fixed" direction="down" width="220px">
        {#snippet trigger({ open, toggle })}
          <button class="btb-run-btn btb-build-caret" class:open onclick={toggle} disabled={!hasProject} use:tooltip={'Choose build type'} aria-label="Choose build type" aria-haspopup="menu" aria-expanded={open}>
            <ChevronDown size={12} />
          </button>
        {/snippet}
      </Dropdown>
      <button
        class="btb-run-btn btb-run-primary"
        onclick={runProject}
        disabled={!hasProject || busy}
        use:tooltip={{ content: 'Run', shortcut: 'Shift+F10' }}
        aria-label="Run"
      >
        <Play size={16} />
      </button>
      <button
        class="btb-run-btn"
        onclick={() => notImplemented('Debug')}
        disabled={!hasProject}
        use:tooltip={'Debug'}
        aria-label="Debug"
      >
        <Bug size={16} />
      </button>
      <Dropdown items={runMenu} position="fixed" direction="down" width="220px">
        {#snippet trigger({ open, toggle })}
          <button class="btb-run-btn" class:open onclick={toggle} disabled={!hasProject} use:tooltip={'More run actions'} aria-label="More run actions" aria-haspopup="menu" aria-expanded={open}>
            <MoreVertical size={14} />
          </button>
        {/snippet}
      </Dropdown>
    </div>
    <div class="btb-run-gap"></div>
  {/snippet}

  {#snippet windowControls()}
    <WindowControls />
  {/snippet}
</TitleBar>

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title={pickerMode === 'add' ? 'Add project to workspace' : 'Open Java project'}
    onConfirm={confirmPicker}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}

{#if themeEditorOpen}
  <ThemeEditorModal onClose={() => (themeEditorOpen = false)} />
{/if}

<style>
  /* JDK-missing warning badge (titlebar) — a click opens Settings to set a JDK path. */
  .btb-jdk-warn {
    display: inline-flex; align-items: center; gap: 5px;
    height: 26px; margin-right: 6px; padding: 0 9px;
    background: color-mix(in srgb, var(--warning) 16%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 40%, transparent);
    border-radius: var(--radius-sm);
    color: var(--warning); cursor: pointer;
    font-family: var(--font-ui-sans); font-size: 11px; font-weight: 600;
    -webkit-app-region: no-drag;
    transition: background var(--transition-fast);
  }
  .btb-jdk-warn:hover { background: color-mix(in srgb, var(--warning) 26%, transparent); }
  .btb-jdk-warn-label { white-space: nowrap; }

  /* ── Run controls (▷ Run · 🐛 Debug · ⋮) ─────────────────────────────────── */
  .btb-run {
    display: flex; align-items: center; gap: 1px;
    -webkit-app-region: no-drag;
  }
  /* Gap between the run cluster and the app buttons (palette · docs · settings),
     so the two groups on the right read as distinct (~72px). */
  .btb-run-gap { width: 72px; flex-shrink: 0; }
  .btb-run-btn {
    display: flex; align-items: center; justify-content: center;
    width: 30px; height: 28px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .btb-run-btn:hover:not(:disabled), .btb-run-btn.open { background: var(--bg-hover); color: var(--text-primary); }
  .btb-run-btn:disabled { opacity: 0.4; cursor: default; }
  .btb-run-primary:not(:disabled) { color: var(--success); }
  .btb-run-primary:hover:not(:disabled) { background: var(--success-subtle); color: var(--success); }
  /* Split build control: the caret reads as attached to the Build button (tight pair, then a little
     breathing room before Run). */
  .btb-build-main { border-top-right-radius: 0; border-bottom-right-radius: 0; padding-right: 0; }
  .btb-build-caret {
    width: 16px; border-top-left-radius: 0; border-bottom-left-radius: 0;
    margin-right: 3px; color: var(--text-tertiary);
  }
</style>
