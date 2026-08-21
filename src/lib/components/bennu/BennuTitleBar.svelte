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
    Play, Bug, Unplug, MoreVertical, Palette, SlidersHorizontal, Info, Hammer, Square, TriangleAlert,
    UserCog, Bot,
    ListChecks, ChevronDown, RotateCw, ListRestart, LayoutDashboard,
    Store, Package, ScrollText,
  } from 'lucide-svelte';
  import type { BuildType } from '$lib/stores/bennu/run.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import TitleBar from '$lib/components/shared/ui/TitleBar.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import WindowControls from '$lib/components/shared/WindowControls.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import BennuWorkspaceSwitcher from './BennuWorkspaceSwitcher.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { createNativeMenuPublisher } from '$lib/utils/native-menu';
  import { windowMenuItems } from '$lib/utils/window-menu';
  import WorkspaceTabs from '$lib/components/shared/internal/WorkspaceTabs.svelte';
  import BennuRunConfigSelect from './BennuRunConfigSelect.svelte';
  import { surfaceStore } from '$lib/stores/surfaces.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import McpProjectRuleModal from '$lib/components/shared/McpProjectRuleModal.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { activeTestStore } from '$lib/stores/bennu/test-runner.svelte';

  /** The runner for the open project — the toolbar's ▷ / ⟳ / ■ mean the same thing either way. */
  const testStore = $derived(activeTestStore());
  import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import ThemeEditorModal from '$lib/components/shared/ThemeEditorModal.svelte';
  import ProfileManagerModal from '$lib/components/shared/ProfileManagerModal.svelte';
  import { profileStore } from '$lib/stores/profiles.svelte';
  import { profileMenuItems } from '$lib/utils/profile-menu';

  let pickerOpen = $state(false);
  // Whether the folder picker opens a NEW workspace (replace) or ADDS a project to the current one.
  let pickerMode = $state<'open' | 'add'>('open');
  let themeEditorOpen = $state(false);
  let profileManagerOpen = $state(false);
  let mcpProjectOpen = $state(false);

  // Run/Build are wired to bennu-be (build.rs); Debug isn't implemented yet (a
  // later wave — it toasts). All disabled when no project is open.
  const hasProject = $derived(!!projectStore.project);
  const busy = $derived(bennuRunStore.active);
  /** A Cargo project has no Java model: `cargo check` is its only build, and "Validate
   *  (no compile)" / the JDK warning are statements about a Java stack that isn't there. */
  const isCargo = $derived(projectStore.isCargo);

  /** ▶ Run — build then launch the ACTIVE run configuration; if the project has no
   *  active config yet, open the run-config editor to create/pick one. */
  function runProject() {
    const root = projectStore.project?.root;
    if (!root) return;
    void bennuRunStore.runActive(root).then((ran) => {
      if (!ran) bennuUiStore.openRunConfig();
    });
  }
  /**
   * 🐞 Debug — the same launch as ▶, with the JDWP agent attached.
   *
   * The Debug panel opens with it, because a debug launch you cannot see the state of is just
   * a slower run: whether it attached, whether the breakpoints took, and where it stopped all
   * live there.
   */
  function debugProject() {
    const root = projectStore.project?.root;
    if (!root) return;
    bennuUiStore.showBottom('run');
    void bennuRunStore.runActive(root, true).then((ran) => {
      if (!ran) bennuUiStore.openRunConfig();
    });
  }
  /** Run every unit test in the project (the run menu + Ctrl+Shift+F5). Opens the Tests
   *  panel; everything after that streams into it. */
  function runAllTests() {
    const root = projectStore.project?.root;
    if (root) void testStore.runAll(root);
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

  // The active build type drives the main button's icon + label. A Cargo project has one
  // build (`cargo check`) and no second choice, so the split dropdown collapses to a
  // plain button rather than offering a Java-only alternative next to it.
  const buildType = $derived(bennuRunStore.preferredBuildType);
  const buildLabel = $derived(
    isCargo
      ? 'Check project (cargo check)'
      : buildType === 'validate'
        ? 'Validate project (no compile)'
        : 'Build project (Maven)',
  );
  // The split dropdown: choose (and run) a build type. Empty for Cargo → no split.
  const buildMenu = $derived<DropdownItem[]>(
    isCargo
      ? []
      : [
          { kind: 'separator', label: 'Build with' },
          { kind: 'item', id: 'mvn',      label: 'Maven build',           icon: Hammer,     disabled: busy, onclick: () => selectBuild('mvn') },
          { kind: 'item', id: 'validate', label: 'Validate (no compile)', icon: ListChecks, disabled: busy, onclick: () => selectBuild('validate') },
        ],
  );

  const runMenu = $derived<DropdownItem[]>([
    // `Run` launches the active run configuration, whatever kind it is: a Java main class, or a
    // cargo subcommand. Both stream into the same console, so Rerun means the same thing on either.
    { kind: 'item', id: 'run', label: 'Run', icon: Play, shortcut: 'Shift+F10', disabled: busy, onclick: runProject },
    { kind: 'item', id: 'rerun', label: 'Rerun', icon: RotateCw, disabled: busy || !bennuRunStore.canRerun,
      onclick: () => void bennuRunStore.rerunApp() },
    { kind: 'item', id: 'build',   label: buildLabel,        icon: isCargo || buildType !== 'validate' ? Hammer : ListChecks, shortcut: 'Ctrl+F9', disabled: busy, onclick: buildProject },
    ...(isCargo
      ? []
      : [{ kind: 'item', id: 'validate', label: 'Validate project', icon: ListChecks, disabled: busy, onclick: () => selectBuild('validate') } as DropdownItem]),
    { kind: 'item', id: 'stop',    label: 'Stop',            icon: Square, disabled: !bennuRunStore.running, onclick: () => void bennuRunStore.stop() },
    // Tests. A Cargo project's tests are `cargo test`, which this runner does not speak, so
    // the entries are out rather than failing when pressed.
    ...(isCargo
      ? []
      : [
          { kind: 'separator', label: 'Tests' } as DropdownItem,
          { kind: 'item', id: 'testall', label: 'Run all tests', icon: FlaskConical, shortcut: 'Ctrl+Shift+F5',
            disabled: busy || testStore.running || !hasProject, onclick: runAllTests } as DropdownItem,
          { kind: 'item', id: 'testrerun', label: 'Rerun tests', icon: RotateCw, shortcut: 'Ctrl+F5',
            disabled: busy || testStore.running || !testStore.hasResults, onclick: () => void testStore.rerun() } as DropdownItem,
          { kind: 'item', id: 'testrerunfailed', label: 'Rerun failed tests', icon: ListRestart,
            disabled: busy || testStore.running || !testStore.hasFailures, onclick: () => void testStore.rerunFailed() } as DropdownItem,
          { kind: 'item', id: 'teststop', label: 'Stop the test run', icon: Square,
            disabled: !testStore.running, onclick: () => void testStore.stop() } as DropdownItem,
        ]),
    { kind: 'separator' },
    // Both ecosystems: JDWP attaches to the JVM `bennu_run` spawned, a Cargo target is built and then
    // launched under a debug adapter. Detach means the same thing on both — the program keeps running
    // with nothing attached, which is not Stop.
    { kind: 'item', id: 'debug', label: 'Debug', icon: Bug, shortcut: 'Shift+F9',
      disabled: busy, onclick: debugProject },
    { kind: 'item', id: 'detach', label: 'Detach the debugger', icon: Unplug,
      disabled: !bennuDebugStore.live,
      onclick: () => void bennuDebugStore.detachSession() },
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
    // The same Tools section Corvus has. Bennu hosts plugins, so it needs the three doors a
    // host needs: install one, see which are loaded, and read why one did not start.
    { kind: 'separator', label: 'Tools' },
    { kind: 'item', id: 'marketplace', label: 'Plugin Marketplace', icon: Store,      action: 'open_marketplace', onclick: () => uiStore.openMarketplace() },
    { kind: 'item', id: 'plugins',     label: 'Plugin Manager',     icon: Package,    action: 'plugins',          onclick: () => bennuUiStore.togglePlugins() },
    { kind: 'item', id: 'plogs',       label: 'Plugin Logs',        icon: ScrollText, action: 'plugin_logs',      onclick: () => bennuUiStore.togglePluginLogs() },
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

  // ── Profile ───────────────────────────────────────────────────────────────────
  // The same submenu Corvus's titlebar has, from the same builder. It belongs here and not only
  // there: which profile is active decides where Bennu reads and writes its own settings and its
  // `workspace.toml`, and a window that cannot say — let alone change — which one it is on is a
  // window you cannot trust with a second profile. The active row carries the tick.
  const profileItems = $derived(profileMenuItems(() => { profileManagerOpen = true; }));

  // ── AI access, for the project that is open ───────────────────────────────────
  // The endpoint, the products and the defaults are Arbor-wide and stay on the home
  // surface. What belongs HERE is the one thing that is about this project: whether an
  // AI client may reach it and what it may do. Reaching that setting by going back to
  // the Welcome page and finding this project in a list would be the long way round to
  // a question you are already looking at the answer to.
  //
  // The row deliberately carries no "custom / inherited" hint: the store is per-window
  // and Bennu's copy is unloaded until this modal opens it, so such a hint would either
  // be wrong or force every Bennu window to fetch AI settings at startup for a feature
  // that is off by default. The modal states it on its own header instead.
  const mcpRoot = $derived(projectStore.project?.root ?? null);

  const settingsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'settings',  label: 'Settings…',           icon: Settings,  shortcut: 'Ctrl+,',   onclick: () => bennuUiStore.openSettings() },
    { kind: 'item', id: 'shortcuts', label: 'Keyboard shortcuts…', icon: Keyboard,  shortcut: 'F1',       onclick: () => bennuUiStore.toggleDocs() },
    // Same place Corvus keeps it — the gear rather than the hamburger, because it is about the
    // window's chrome and not about the project.
    { kind: 'item', id: 'customize-rails', label: 'Customize Activity Bar…', icon: LayoutDashboard,
      onclick: () => bennuUiStore.openCustomizeRails() },
    ...(mcpRoot
      ? [
          { kind: 'separator' as const },
          {
            kind: 'item' as const, id: 'mcp-project', icon: Bot,
            label: 'AI access by project…',
            onclick: () => { mcpProjectOpen = true; },
          },
        ]
      : []),
    { kind: 'separator' },
    { kind: 'submenu', id: 'profile', label: `Profile — ${profileStore.active}`, icon: UserCog, items: profileItems },
    { kind: 'submenu', id: 'theme', label: 'Theme', icon: Palette, items: themeItems },
  ]);

</script>

<TitleBar
  logoTooltip="Bennu — Java &amp; Rust editor"
  menu={hamburgerMenu}
  onNativeMenu={publishNativeMenu}
  nativeMenuEnabled={surfaceStore.hasFocus('bennu')}
  menuWidth="240px"
  docs={{ active: bennuUiStore.docsOpen, tooltip: 'Documentation (F1)', onclick: () => bennuUiStore.toggleDocs() }}
  commandPalette={{ active: bennuUiStore.paletteOpen, tooltip: 'Command palette (Ctrl+K)', onclick: () => bennuUiStore.togglePalette() }}
  settings={{ menu: settingsMenu, menuWidth: '260px', tooltip: 'Settings' }}
>
  {#snippet logo()}
    <ProductIcon id="bennu" size={22} />
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
    {#if bennuDiagnosticsStore.jdkMissing && !isCargo}
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
      <!-- What ▶ will launch, named next to the button that launches it. Both ecosystems now: a
           Cargo project has run configurations too, and which command ▶ runs is exactly the thing
           worth naming. -->
      <BennuRunConfigSelect />
      <!-- `btb-build-main` squares off the right edge for the attached caret. Without the
           caret (Cargo: one build type, no split) the button must round on both sides. -->
      <button
        class="btb-run-btn"
        class:btb-build-main={!isCargo}
        onclick={buildProject}
        disabled={!hasProject || busy}
        use:tooltip={{ content: buildLabel, shortcut: 'Ctrl+F9' }}
        aria-label={buildLabel}
      >
        {#if !isCargo && buildType === 'validate'}
          <ListChecks size={16} />
        {:else}
          <Hammer size={16} />
        {/if}
      </button>
      {#if !isCargo}
        <Dropdown items={buildMenu} position="fixed" direction="down" width="220px">
          {#snippet trigger({ open, toggle })}
            <button class="btb-run-btn btb-build-caret" class:open onclick={toggle} disabled={!hasProject} use:tooltip={'Choose build type'} aria-label="Choose build type" aria-haspopup="menu" aria-expanded={open}>
              <ChevronDown size={12} />
            </button>
          {/snippet}
        </Dropdown>
      {/if}
      <!-- ▶ on both ecosystems: it launches the active run configuration, and a Cargo one is a
           cargo subcommand. With no configuration yet the store creates one for the workspace's
           only binary, or opens the editor when there is a real question to answer. -->
      <button
        class="btb-run-btn btb-run-primary"
        onclick={runProject}
        disabled={!hasProject || busy}
        use:tooltip={{ content: 'Run', shortcut: 'Shift+F10' }}
        aria-label="Run"
      >
        <Play size={16} />
      </button>
      <!-- 🐞 on both ecosystems: JDWP attaches to the JVM `bennu_run` spawned, and a Cargo target is
           built and then launched under a debug adapter. Which of the two is behind it is not a
           question this button — or the panel it opens — can ask. -->
      <button
        class="btb-run-btn"
        class:btb-debugging={bennuDebugStore.live}
        onclick={debugProject}
        disabled={!hasProject || busy}
        use:tooltip={{ content: 'Debug', shortcut: 'Shift+F9' }}
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
    title={pickerMode === 'add' ? 'Add project to workspace' : 'Open project (Maven or Cargo)'}
    onConfirm={confirmPicker}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}

{#if themeEditorOpen}
  <ThemeEditorModal onClose={() => (themeEditorOpen = false)} />
{/if}

{#if profileManagerOpen}
  <ProfileManagerModal onClose={() => (profileManagerOpen = false)} />
{/if}

{#if mcpProjectOpen && mcpRoot}
  <McpProjectRuleModal
    root={mcpRoot}
    name={projectStore.project?.name}
    product="bennu"
    onClose={() => (mcpProjectOpen = false)} />
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
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs); font-weight: 600;
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
  /* The selector is a field, the rest are buttons — the gap is what separates the two
     groups. Set by the consumer, not by the chip: spacing belongs to the layout. */
  .btb-run :global(.rcs) { margin-right: 7px; }
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
  /* A session is attached — the button says so, the way ▶ says a program is running. */
  .btb-debugging:not(:disabled) { color: var(--error); }
  /* Split build control: the caret reads as attached to the Build button (tight pair, then a little
     breathing room before Run). */
  .btb-build-main { border-top-right-radius: 0; border-bottom-right-radius: 0; padding-right: 0; }
  .btb-build-caret {
    width: 16px; border-top-left-radius: 0; border-bottom-left-radius: 0;
    margin-right: 3px; color: var(--text-tertiary);
  }
</style>
