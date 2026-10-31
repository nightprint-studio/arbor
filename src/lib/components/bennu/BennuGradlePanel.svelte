<script lang="ts">
  /**
   * Gradle (right tool window) — the build's projects, and what can be run in them.
   *
   * The Maven window's counterpart, and deliberately the same shape: what it shows is the build's own
   * files (`bennu_gradle_model` — `settings.gradle` and every build script), and every task row runs
   * (`bennu_gradle_task`), streaming into the same Run console a `java` launch and a cargo command
   * use.
   *
   * ## One section per project
   *
   * A Gradle build is a root plus the projects `settings.gradle` includes. Each gets its own section
   * (`BennuGradleModuleSection`), root first, and a task runs in **that project**. See that file for
   * what a row does.
   *
   * ## Which Gradle build
   *
   * The window shows the Gradle builds of the whole workspace — one at a project's root, or one in a
   * subfolder of a project that is something else first. With more than one, a module tree on top
   * picks the one the tasks run in; with one there is no selector.
   *
   * ## What the toolbar owns
   *
   * The things that are about the **build** rather than the dependency list: the module graph,
   * re-reading the scripts, and the actions that change what the index is standing on. Dependencies
   * keeps what is its own — the filter over the rows it draws. `-x test` and `--offline` belong to
   * the **window**, not to a press: you set them once and then run several tasks, and they travel
   * with each run so a tab's ⟳ repeats the build that happened.
   */
  import { Download, FlaskConical, HardDriveDownload, Layers, Network, RefreshCw, RotateCw, WifiOff } from 'lucide-svelte';
  import GradleIcon from './GradleIcon.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import BennuModuleTree from './BennuModuleTree.svelte';
  import BennuGradleModuleSection from './BennuGradleModuleSection.svelte';
  import { gradleKeys } from './tree-expansion';
  import { treeExpansionStore } from '$lib/stores/bennu/tree-expansion.svelte';
  import type { GradleModule } from '$lib/ipc/bennu/gradle';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuGradleStore } from '$lib/stores/bennu/gradle.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { dependenciesStore } from '$lib/stores/bennu/dependencies.svelte';
  import { moduleFocusStore } from '$lib/stores/bennu/module-focus.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { gradleDownload, gradleReload } from '$lib/ipc/bennu/gradle';

  let filter = $state('');

  // The folder of the Gradle build the tasks run in — the project root for a plain Gradle project.
  const target = $derived(moduleFocusStore.targetFor('Gradle'));
  const root = $derived(target?.toolRoot ?? null);
  const model = $derived(bennuGradleStore.model);

  // Read on open and whenever the project changes. Build scripts only — no Gradle, no network — so
  // this costs a few file reads and needs no watcher: the ⟳ is there for the script you just edited.
  $effect(() => {
    const path = root;
    if (!path) {
      bennuGradleStore.reset();
      return;
    }
    void bennuGradleStore.load(path);
  });

  /** The projects the filter leaves. Matched on everything the row shows. */
  const modules = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return bennuGradleStore.modules;
    return bennuGradleStore.modules.filter((m) => `${m.name} ${m.path} ${m.dir} ${m.packaging}`.toLowerCase().includes(q));
  });

  // Every project open on a small build, all closed on a large one: forty expanded sections is a
  // panel you have to scroll past rather than read. The root stays open either way.
  const collapseByDefault = $derived(bennuGradleStore.modules.length > 6);
  function moduleOpenByDefault(module: GradleModule): boolean {
    return module.path === ':' || !collapseByDefault;
  }

  // Which sections are open, remembered per project in its session — not in this component, which is
  // unmounted every time the rail closes the window.
  const expansion = $derived(treeExpansionStore.view(root ?? '', 'gradle'));

  function openFile(path: string, line?: number) {
    void projectStore.openFile(path).then(() => {
      if (line) bennuUiStore.requestGoto(line);
    });
  }

  /** Where a project's directory is, for "focus in the Project tree". */
  function dirOf(module: GradleModule): string {
    if (!root) return '';
    return module.dir ? `${root}/${module.dir}` : root;
  }

  /** Run tasks in a project, with the window's current toggles. */
  function run(module: GradleModule, tasks: string[]) {
    if (!root) return;
    void bennuRunStore.runGradleTasks(root, tasks, {
      module: module.path === ':' ? '' : module.path,
      skipTests: bennuGradleStore.skipTests,
      offline: bennuGradleStore.offline,
    });
  }

  /**
   * Re-read the build — the scripts this panel draws from, and the dependency report drawn from the
   * same files. Both, because they are one question: a script edited to add a project changed what
   * BOTH panels should say.
   */
  function refresh() {
    if (!root) return;
    void bennuGradleStore.load(root, true);
    const depsRoot = moduleFocusStore.targetFor('Maven', 'Gradle', 'Cargo')?.toolRoot;
    if (depsRoot) void dependenciesStore.load(depsRoot, true);
  }

  /** Start one of the things that change what is on disk, and say so either way: none opens a window
   *  and the work happens on a backend thread, so what we say about the press is the only evidence
   *  it did anything. */
  async function runAction(what: string, start: (root: string) => Promise<string>) {
    if (!root) return;
    try {
      await start(root);
      toastStore.show(`${what} started — see the Jobs panel`, 'info');
    } catch (e) {
      toastStore.show(`${what} could not start: ${e}`, 'error');
    }
  }

  const actions = $derived<DropdownItem[]>([
    {
      kind: 'item',
      id: 'reload',
      label: 'Re-resolve dependencies & rebuild index',
      subtitle: 'Drops the cached classpath and reindexes',
      icon: RotateCw,
      // The index belongs to the PROJECT, not to the module a task runs in.
      onclick: () => void runAction('Re-resolve', () => gradleReload(target?.root ?? root ?? '')),
    },
    {
      kind: 'item',
      id: 'download',
      label: 'Download missing dependencies',
      subtitle: 'Resolves every classpath with --refresh-dependencies — the only thing here that uses the network',
      icon: Download,
      shortcut: 'Alt+Shift+U',
      onclick: () => void runAction('Download', gradleDownload),
    },
    { kind: 'separator' },
    {
      kind: 'item',
      id: 'offline',
      label: bennuGradleStore.offline ? 'Offline: on' : 'Offline: off',
      subtitle: 'Gradle may not use the network — for these tasks, the classpath and the build. Same as Settings → Java',
      icon: WifiOff,
      onclick: () => bennuGradleStore.setOffline(!bennuGradleStore.offline),
    },
  ]);
</script>

<PanelShell title="Gradle" count={model ? model.modules.length : null}>
  {#snippet icon()}<GradleIcon size={13} />{/snippet}
  <!-- Declared unconditionally and gated inside: a snippet is a prop, and a prop wrapped in an
       `{#if}` is a prop the component may never be handed. -->
  {#snippet toolbar()}
    {#if root}
      <div class="gp-toolbar">
        <BennuFilterBar bind:query={filter} placeholder="Filter projects…" />
        <!-- The toggle that is flipped several times an hour, and the only one worth a place in the
             header rather than a menu. -->
        <IconButton
          tooltip={bennuGradleStore.skipTests ? 'Skip tests: on — every task runs with -x test' : 'Skip tests: off'}
          size={22}
          active={bennuGradleStore.skipTests}
          onclick={() => bennuGradleStore.setSkipTests(!bennuGradleStore.skipTests)}
        >
          <FlaskConical size={12} />
        </IconButton>
        <!-- This panel lists the projects; the graph shows how they are wired to each other. -->
        <IconButton tooltip="Module graph (Alt+Shift+D)" size={22} onclick={() => bennuUiStore.openModuleGraph()}>
          <Network size={12} />
        </IconButton>
        <IconButton
          tooltip="Re-read the build scripts — this panel and the Dependencies one"
          size={22}
          disabled={bennuGradleStore.loading}
          onclick={refresh}
        >
          <RefreshCw size={12} />
        </IconButton>
        <!-- The things that change what is ON DISK, as opposed to the refresh above which only
             re-reads it. Behind one trigger because they are the same errand — "the editor and my
             Gradle cache disagree". -->
        <Dropdown items={actions} position="fixed" direction="down" width="300px">
          {#snippet trigger({ toggle: openMenu })}
            <IconButton tooltip="Gradle actions" size={22} onclick={openMenu}>
              <HardDriveDownload size={12} />
            </IconButton>
          {/snippet}
        </Dropdown>
      </div>
    {/if}
  {/snippet}

  {#if !root}
    <EmptyState message={projectStore.project ? 'No Gradle project in this workspace.' : 'Open a project to see its Gradle tasks.'} />
  {:else if bennuGradleStore.error}
    <div class="gp-scroll"><BennuModuleTree kinds={['Gradle']} ariaLabel="Gradle projects" /></div>
    <div class="gp-notice"><Alert variant="error" compact text={bennuGradleStore.error} /></div>
  {:else if !model && bennuGradleStore.loading}
    <div class="gp-loading"><Spinner size={16} /><span>Reading the build scripts…</span></div>
  {:else if model}
    <div class="gp-body">
      <BennuModuleTree kinds={['Gradle']} ariaLabel="Gradle projects" />
      {#if !bennuGradleStore.hasLauncher}
        <div class="gp-notice">
          <Alert variant="warning" compact>
            No <code>gradle</code> could be found on the PATH this app inherits, and the project has no
            <code>gradlew</code>. Nothing below will run until one of those is true.
          </Alert>
        </div>
      {/if}

      {#if model.catalogs.length > 0}
        <!-- Above the projects: where most versions of the build are decided. -->
        <SidebarSection
          label="Version catalogs"
          expanded={expansion.isOpen(gradleKeys.catalogs, true)}
          onToggle={() => expansion.toggle(gradleKeys.catalogs, true)}
          badge={model.catalogs.length}
        >
          {#snippet icon()}<Layers size={13} />{/snippet}
          {#each model.catalogs as catalog (catalog.name)}
            <SidebarItem onclick={() => openFile(catalog.file)}>
              {#snippet icon()}<span class="gp-catalog"><Layers size={12} /></span>{/snippet}
              {catalog.name}
              {#snippet badges()}<span class="gp-hint">{catalog.libraries} libraries</span>{/snippet}
            </SidebarItem>
          {/each}
        </SidebarSection>
      {/if}

      {#if modules.length === 0}
        <EmptyState message={filter ? 'No project matches the filter.' : 'This build includes no projects.'} compact />
      {:else}
        {#each modules as module (module.path)}
          <BennuGradleModuleSection
            {module}
            buildName={model.build_name}
            dirPath={dirOf(module)}
            expanded={expansion.isOpen(gradleKeys.module(module), moduleOpenByDefault(module))}
            onToggle={() => expansion.toggle(gradleKeys.module(module), moduleOpenByDefault(module))}
            {expansion}
            onRun={(tasks) => run(module, tasks)}
            onOpen={openFile}
          />
        {/each}
      {/if}
    </div>
  {:else}
    <div class="gp-loading"><Spinner size={16} /><span>Reading the build scripts…</span></div>
  {/if}
</PanelShell>

<style>
  /* Body rhythm mirrors the Maven, Cargo and Dependencies tools: SidebarSection owns the group header
     and the indent guideline, so the body keeps no horizontal padding. */
  .gp-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0 8px; }
  /* The selector alone, for the states that have no body to scroll in (an error). */
  .gp-scroll { max-height: 40%; overflow-y: auto; }
  .gp-toolbar { display: flex; align-items: center; gap: 4px; padding-right: 4px; }
  .gp-toolbar > :global(:first-child) { flex: 1; min-width: 0; }
  .gp-notice { padding: 6px 8px 2px; }
  .gp-loading {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .gp-catalog { color: var(--text-muted); display: flex; }
  .gp-hint { font-size: var(--font-size-2xs); color: var(--text-disabled); }
</style>
