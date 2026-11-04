<script lang="ts">
  /**
   * Node (right tool window) — the packages of a `package.json` project, and the scripts you run on them.
   *
   * The counterpart of the Maven and Cargo windows for npm, pnpm and Yarn. Same split of work as
   * those: this window is what you *run*; what the packages *depend on* is the Dependencies window
   * (the button in the toolbar goes there, already aimed at this project).
   *
   * - **one group per `package.json`** — the root, then each workspace member (`workspaces` /
   *   `pnpm-workspace.yaml`) — each script a row that runs in the Run console through the project's
   *   own package manager, read off its lockfile;
   * - **Install** runs `<pm> install` into the same console. Offered first when nothing is installed.
   *
   * ## Which Node project
   *
   * The Node modules of the whole workspace, as the other tool windows: with more than one a module
   * tree on top picks which, with one there is no selector.
   */
  import { Download, Library, Play, RefreshCw, ShieldAlert } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import BennuModuleTree from './BennuModuleTree.svelte';
  import NodeIcon from './NodeIcon.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { dependenciesStore } from '$lib/stores/bennu/dependencies.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuNodeStore } from '$lib/stores/bennu/node.svelte';
  import { moduleFocusStore } from '$lib/stores/bennu/module-focus.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { npmInstall, npmRunScript } from '$lib/ipc/bennu/npm';

  let filter = $state('');

  const target = $derived(moduleFocusStore.targetFor('Node'));
  const root = $derived(target?.toolRoot ?? null);
  const project = $derived(bennuNodeStore.project);
  const manager = $derived(project?.package_manager ?? 'npm');

  $effect(() => {
    const path = root;
    if (!path) {
      bennuNodeStore.reset();
      return;
    }
    void bennuNodeStore.load(path);
  });

  // The dependencies half of the same project, so this window shows them beside the scripts: the
  // Dependencies store is the one owner of the report, the advisories and the newer-version lookups,
  // and this only asks it to be loaded for this folder (a repeat for the same folder is cheap).
  $effect(() => {
    const path = root;
    if (!path) return;
    void dependenciesStore.load(path, false, true).then(() => {
      void dependenciesStore.loadAdvisories(path, true);
      void dependenciesStore.loadOutdated(path);
    });
  });

  const depReport = $derived(dependenciesStore.target?.node && dependenciesStore.target.root === root ? dependenciesStore.report : null);
  const declared = $derived((depReport?.modules ?? []).flatMap((m) => m.dependencies));
  const outdatedCount = $derived(declared.filter((d) => dependenciesStore.outdated.has(d.name)).length);
  const vulnerableCount = $derived(
    [...dependenciesStore.advisories.values()].reduce((n, versions) => n + versions.length, 0),
  );

  /** Packages with the scripts that match the filter; one that matches nothing drops out while filtering. */
  const packages = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return (project?.packages ?? [])
      .map((p) => ({
        pkg: p,
        scripts: q ? p.scripts.filter((s) => `${s.name} ${s.command} ${p.id}`.toLowerCase().includes(q)) : p.scripts,
      }))
      .filter((p) => p.scripts.length > 0 || !q);
  });

  // Every package starts open when there are few of them; a big monorepo opens to the list.
  const open = $state<Record<string, boolean>>({});
  const collapseByDefault = $derived((project?.packages.length ?? 0) > 4);
  const isOpen = (id: string) => open[id] ?? !collapseByDefault;
  const toggle = (id: string) => { open[id] = !isOpen(id); };

  function started(label: string) {
    return (e: unknown) => toastStore.show(`${manager} ${label}: ${e}`, 'error');
  }

  function run(manifest: string, script: string) {
    if (!root) return;
    void npmRunScript(root, manifest, script)
      .then(() => bennuUiStore.showBottom('run'))
      .catch(started(script));
  }

  function install() {
    const manifest = project?.packages[0]?.manifest;
    if (!root || !manifest) return;
    void npmInstall(root, manifest)
      .then(() => bennuUiStore.showBottom('run'))
      .catch(started('install'));
  }

  /** The Dependencies window, already aimed at this project: pinning the module is what it follows. */
  function openDependencies() {
    if (target) moduleFocusStore.select(target.id);
    bennuUiStore.showLeft('dependencies');
  }

  function refresh() {
    if (root) void bennuNodeStore.load(root, true);
  }
</script>

<PanelShell title="Node" count={project ? project.packages.length : null}>
  {#snippet icon()}<NodeIcon size={13} />{/snippet}
  {#snippet toolbar()}
    {#if root}
      <div class="nd-toolbar">
        <BennuFilterBar bind:query={filter} placeholder="Filter scripts…" />
        <IconButton tooltip={`Install dependencies — runs ${manager} install`} size={22} onclick={install}>
          <Download size={12} />
        </IconButton>
        <IconButton tooltip="Dependencies of this project (Alt+N)" size={22} onclick={openDependencies}>
          <Library size={12} />
        </IconButton>
        <IconButton tooltip="Known vulnerabilities…" size={22} onclick={() => { if (target) moduleFocusStore.select(target.id); bennuUiStore.openVulnerabilities(); }}>
          <ShieldAlert size={12} />
        </IconButton>
        <IconButton tooltip="Re-read the package.json files" size={22} disabled={bennuNodeStore.loading} onclick={refresh}>
          <RefreshCw size={12} />
        </IconButton>
      </div>
    {/if}
  {/snippet}

  {#if !root}
    <EmptyState message={projectStore.project ? 'No package.json in this workspace.' : 'Open a project to see its packages.'} />
  {:else if bennuNodeStore.error}
    <div class="nd-notice"><Alert variant="error" compact text={bennuNodeStore.error} /></div>
  {:else if !project}
    <div class="nd-loading"><Spinner size={16} /><span>Reading package.json…</span></div>
  {:else}
    <div class="nd-body">
      <BennuModuleTree kinds={['Node']} ariaLabel="Node projects" />
      <p class="nd-manager" title={project.lockfile ? `Resolved by ${project.lockfile}` : 'No lockfile yet'}>
        {manager}{project.lockfile ? ` · ${project.lockfile}` : ''}
      </p>
      {#if !project.installed}
        <div class="nd-notice">
          <Alert variant="info" compact>
            There is no <code>node_modules</code> yet. Install the dependencies with the button above
            before running a script.
          </Alert>
        </div>
      {/if}

      {#if depReport}
        <SidebarItem onclick={openDependencies}>
          {#snippet icon()}<Library size={12} />{/snippet}
          Dependencies
          {#snippet badges()}
            <Badge variant="tone" tone="neutral" size="sm" label={`${declared.length} declared`} />
            {#if depReport.transitive.length > 0}<Badge variant="tone" tone="neutral" size="sm" label={`${depReport.transitive.length} transitive`} />{/if}
            {#if outdatedCount > 0}<Badge variant="tone" tone="info" size="sm" label={`↑ ${outdatedCount} newer`} />{/if}
            {#if vulnerableCount > 0}
              <Badge variant="tone" tone="error" size="sm" label={`${vulnerableCount} vulnerable`} />
            {/if}
          {/snippet}
        </SidebarItem>
      {/if}

      {#if packages.length === 0}
        <EmptyState message={filter ? 'No script matches the filter.' : 'This project has no package.json.'} compact />
      {:else}
        {#each packages as { pkg, scripts } (pkg.manifest)}
          <SidebarSection
            label={pkg.id}
            expanded={filter.trim().length > 0 || isOpen(pkg.manifest)}
            onToggle={() => toggle(pkg.manifest)}
            badge={scripts.length}
            badgeTitle="Scripts this package declares"
          >
            {#snippet icon()}<NodeIcon size={13} />{/snippet}
            {#if scripts.length === 0}
              <p class="nd-none">No scripts.</p>
            {:else}
              {#each scripts as s (s.name)}
                <SidebarItem onclick={() => run(pkg.manifest, s.name)}>
                  {#snippet icon()}<span class="nd-run"><Play size={11} /></span>{/snippet}
                  {s.name}
                  {#snippet badges()}<span class="nd-cmd" title={s.command}>{s.command}</span>{/snippet}
                </SidebarItem>
              {/each}
            {/if}
          </SidebarSection>
        {/each}
      {/if}
    </div>
  {/if}
</PanelShell>

<style>
  .nd-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0 8px; }
  .nd-toolbar { display: flex; align-items: center; gap: 4px; padding-right: 4px; }
  .nd-toolbar > :global(:first-child) { flex: 1; min-width: 0; }
  .nd-notice { padding: 6px 8px 2px; }
  .nd-loading {
    display: flex; align-items: center; gap: 8px;
    padding: 14px 12px; color: var(--text-muted); font-size: var(--font-size-xs);
  }
  .nd-manager {
    margin: 0; padding: 2px 10px 6px;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .nd-run { color: var(--success); display: flex; }
  .nd-cmd {
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 22ch;
  }
  .nd-none { margin: 0; padding: 4px 8px 6px; font-size: var(--font-size-2xs); color: var(--text-disabled); }
  code { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-primary); }
</style>
