<script lang="ts">
  /**
   * Maven (right tool window) — the reactor, and what can be run in it.
   *
   * The panel used to be a sketch: one hard-coded lifecycle, a made-up plugin list, and a toast
   * saying every goal was unimplemented. What it shows now is the project's own poms
   * (`bennu_maven_model`) and every row runs (`bennu_maven_goal`), streaming into the same Run
   * console a `java` launch and a cargo command use.
   *
   * ## One section per module
   *
   * A Maven project is a reactor, and a single flat lifecycle was a lie about which pom a press
   * would apply to. Each module gets its own section (`BennuMavenModuleSection`), root first, and
   * a phase runs in **that module's directory**. See that file for what a row does.
   *
   * ## What the toolbar owns
   *
   * The three things that used to live in the Dependencies panel's header are here, because all
   * three are about the **build** rather than about the dependency list: the module graph,
   * re-reading the project, and the actions that change what is in `~/.m2`. Dependencies keeps
   * what is its own — the filter over the rows it draws.
   *
   * Profiles and `-DskipTests` belong to the **window**, not to a press: you set them once and
   * then run several goals. They travel with each run, so a tab's ⟳ repeats the build that
   * happened rather than the one the panel is set up for by the time you press it.
   */
  import {
    Download, FileCode2, FlaskConical, HardDriveDownload, Layers, Network, RefreshCw, RotateCw,
    Square, SquareCheckBig, WifiOff,
  } from 'lucide-svelte';
  import MavenIcon from './MavenIcon.svelte';
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
  import BennuMavenModuleSection from './BennuMavenModuleSection.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuMavenStore } from '$lib/stores/bennu/maven.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { dependenciesStore } from '$lib/stores/bennu/dependencies.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { mavenDownload, mavenDownloadSources, mavenReload } from '$lib/ipc/bennu/deps';

  let filter = $state('');

  const root = $derived(projectStore.project?.root ?? null);
  const isCargo = $derived(projectStore.isCargo);
  const model = $derived(bennuMavenStore.model);

  // Read on open and whenever the project changes. Poms only — no Maven, no network — so this
  // costs a few file reads and needs no watcher: the ⟳ is there for the pom you just edited.
  $effect(() => {
    const path = root;
    if (!path || isCargo) {
      bennuMavenStore.reset();
      return;
    }
    void bennuMavenStore.load(path);
  });

  /** The modules the filter leaves. Matched on everything the row shows, so typing `war` finds
   *  the module packaged as one. */
  const modules = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return bennuMavenStore.modules;
    return bennuMavenStore.modules.filter((m) =>
      `${m.name} ${m.artifact_id} ${m.dir} ${m.packaging}`.toLowerCase().includes(q),
    );
  });

  // Every module open on a small reactor, all closed on a large one: forty expanded sections is a
  // panel you have to scroll past rather than read. The root stays open either way — it is the
  // one whose lifecycle most presses come from.
  const collapseByDefault = $derived(bennuMavenStore.modules.length > 6);
  const open = $state<Record<string, boolean>>({});
  function isOpen(id: string, fallback: boolean): boolean {
    return open[id] ?? fallback;
  }
  function toggle(id: string, fallback: boolean) {
    open[id] = !isOpen(id, fallback);
  }

  let profilesOpen = $state(false);

  function openFile(path: string, line?: number) {
    void projectStore.openFile(path).then(() => {
      if (line) bennuUiStore.requestGoto(line);
    });
  }

  /** Run goals in a module, with the window's current profiles and toggles. */
  function run(moduleDir: string, goals: string[]) {
    if (!root) return;
    void bennuRunStore.runMavenGoals(root, goals, {
      module: moduleDir,
      profiles: bennuMavenStore.activeProfiles,
      skipTests: bennuMavenStore.skipTests,
      offline: bennuMavenStore.offline,
    });
  }

  /**
   * Re-read the build — the poms this panel draws from, and the dependency report drawn from the
   * same files.
   *
   * Both, because they are one question: a pom edited to add a module changed what BOTH panels
   * should say, and leaving the other stale is how two views of one project start disagreeing.
   */
  function refresh() {
    if (!root) return;
    void bennuMavenStore.load(root, true);
    void dependenciesStore.load(root, true);
  }

  /**
   * Start one of the three things that change what is on disk, and say so either way.
   *
   * All three are invisible by nature — none opens a window, none touches a file you have open,
   * and the work happens on a backend thread — so what we say about the press is the only evidence
   * it did anything.
   */
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
      subtitle: 'Drops the cached classpath, re-reads the repository, reindexes',
      icon: RotateCw,
      onclick: () => void runAction('Re-resolve', mavenReload),
    },
    {
      kind: 'item',
      id: 'download',
      label: 'Download missing dependencies',
      subtitle: 'mvn dependency:go-offline — the only thing here that uses the network',
      icon: Download,
      shortcut: 'Alt+Shift+U',
      onclick: () => void runAction('Download', mavenDownload),
    },
    {
      kind: 'item',
      id: 'sources',
      label: 'Download sources',
      subtitle: 'So Ctrl+B into a library lands on real source, not a decompiled stub',
      icon: FileCode2,
      onclick: () => void runAction('Download sources', mavenDownloadSources),
    },
    { kind: 'separator' },
    {
      kind: 'item',
      id: 'offline',
      label: bennuMavenStore.offline ? 'Offline: on' : 'Offline: off',
      subtitle: 'Adds -o to every goal run from this panel',
      icon: WifiOff,
      onclick: () => bennuMavenStore.setOffline(!bennuMavenStore.offline),
    },
  ]);
</script>

<PanelShell title="Maven" count={model ? model.modules.length : null}>
  {#snippet icon()}<MavenIcon size={13} />{/snippet}
  <!-- Declared unconditionally and gated inside: a snippet is a prop, and a prop wrapped in an
       `{#if}` is a prop the component may never be handed. -->
  {#snippet toolbar()}
    {#if root && !isCargo}
      <div class="mv-toolbar">
        <BennuFilterBar bind:query={filter} placeholder="Filter modules…" />
        <!-- The toggle that is flipped several times an hour, and the only one worth a place in
             the header rather than a menu. -->
        <IconButton
          tooltip={bennuMavenStore.skipTests
            ? 'Skip tests: on — every goal runs with -DskipTests'
            : 'Skip tests: off'}
          size={22}
          active={bennuMavenStore.skipTests}
          onclick={() => bennuMavenStore.setSkipTests(!bennuMavenStore.skipTests)}
        >
          <FlaskConical size={12} />
        </IconButton>
        <!-- This panel lists the modules; the graph shows how they are wired to each other. -->
        <IconButton
          tooltip="Module graph (Alt+Shift+D)"
          size={22}
          onclick={() => bennuUiStore.openModuleGraph()}
        >
          <Network size={12} />
        </IconButton>
        <IconButton
          tooltip="Re-read the poms — this panel and the Dependencies one"
          size={22}
          disabled={bennuMavenStore.loading}
          onclick={refresh}
        >
          <RefreshCw size={12} />
        </IconButton>
        <!-- The things that change what is ON DISK, as opposed to the refresh above which only
             re-reads it. Behind one trigger because they are the same errand — "the editor and my
             repository disagree" — and four more icons in a panel header is a toolbar nobody
             reads. -->
        <Dropdown items={actions} position="fixed" direction="down" width="300px">
          {#snippet trigger({ toggle: openMenu })}
            <IconButton tooltip="Maven actions" size={22} onclick={openMenu}>
              <HardDriveDownload size={12} />
            </IconButton>
          {/snippet}
        </Dropdown>
      </div>
    {/if}
  {/snippet}

  {#if !root}
    <EmptyState message="Open a project to see its Maven goals." />
  {:else if isCargo}
    <EmptyState message="This is a Cargo project — the Cargo tool window has its commands." />
  {:else if bennuMavenStore.error}
    <div class="mv-notice">
      <Alert variant="error" compact text={bennuMavenStore.error} />
    </div>
  {:else if !model && bennuMavenStore.loading}
    <div class="mv-loading"><Spinner size={16} /><span>Reading the project's poms…</span></div>
  {:else if model}
    <div class="mv-body">
      {#if !bennuMavenStore.hasLauncher}
        <div class="mv-notice">
          <Alert variant="warning" compact>
            No <code>mvn</code> could be found on the PATH this app inherits, and the project has no
            <code>mvnw</code>. Nothing below will run until one of those is true.
          </Alert>
        </div>
      {/if}

      {#if model.profiles.length > 0}
        <!-- Above the modules, because a profile changes what every press below does. -->
        <SidebarSection
          label="Profiles"
          expanded={profilesOpen}
          onToggle={() => (profilesOpen = !profilesOpen)}
          badge={bennuMavenStore.activeProfiles.length || null}
          badgeTitle="Active — passed as -P on every goal run from here"
        >
          {#snippet icon()}<Layers size={13} />{/snippet}
          {#each model.profiles as profile (profile.id)}
            {@const active = bennuMavenStore.isProfileActive(profile.id)}
            <SidebarItem onclick={() => bennuMavenStore.toggleProfile(profile.id)}>
              {#snippet icon()}
                <span class="mv-check" class:mv-on={active}>
                  {#if active}<SquareCheckBig size={11} />{:else}<Square size={11} />{/if}
                </span>
              {/snippet}
              {profile.id}
              {#snippet badges()}
                <span class="mv-hint">
                  {profile.active_by_default ? 'active by default' : profile.module || 'root'}
                </span>
              {/snippet}
            </SidebarItem>
          {/each}
        </SidebarSection>
      {/if}

      {#if modules.length === 0}
        <EmptyState
          message={filter
            ? 'No module matches the filter.'
            : 'This project declares no modules.'}
          compact
        />
      {:else}
        {#each modules as module (module.pom)}
          <BennuMavenModuleSection
            {module}
            phases={model.lifecycle}
            expanded={isOpen(module.pom, module.dir === '' || !collapseByDefault)}
            onToggle={() => toggle(module.pom, module.dir === '' || !collapseByDefault)}
            onRun={(goals) => run(module.dir, goals)}
            onOpen={openFile}
          />
        {/each}
      {/if}

      <!-- Not everything about a build is a goal. A goal you run with the same properties every
           time belongs in a run configuration, and this is where you would go looking. -->
      <button type="button" class="mv-foot" onclick={() => bennuUiStore.openRunConfig()}>
        Save a build as a run configuration…
      </button>
    </div>
  {:else}
    <div class="mv-loading"><Spinner size={16} /><span>Reading the project's poms…</span></div>
  {/if}
</PanelShell>

<style>
  /* Body rhythm mirrors the Cargo and Dependencies tools: SidebarSection owns the group header and
     the indent guideline, so the body keeps no horizontal padding. */
  .mv-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0 8px; }
  .mv-toolbar { display: flex; align-items: center; gap: 4px; padding-right: 4px; }
  .mv-toolbar > :global(:first-child) { flex: 1; min-width: 0; }
  .mv-notice { padding: 6px 8px 2px; }
  .mv-loading {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .mv-check { color: var(--text-disabled); display: flex; }
  .mv-on { color: var(--accent-primary); }
  .mv-hint {
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    max-width: 150px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mv-foot {
    display: block;
    width: calc(100% - 16px);
    margin: 6px 8px 0;
    padding: 5px 8px;
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--font-size-2xs);
    text-align: left;
    cursor: pointer;
  }
  .mv-foot:hover { color: var(--text-primary); border-color: var(--border-default); }
</style>
