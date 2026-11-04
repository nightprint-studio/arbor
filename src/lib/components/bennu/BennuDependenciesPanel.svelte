<script lang="ts">
  /**
   * Dependencies (left tool window) — what the open project actually depends on.
   *
   * **Maven, Gradle, Cargo and Node, one panel.** The questions a dependency list is opened to answer
   * are the same either way, so the report shape is (see `bennu-deps`'s `model.rs`, whose module doc has the
   * field-by-field mapping). What differs is the words, and those come from
   * {@link DependencyReport.ecosystem} rather than from a second component.
   *
   * One group per module (Maven), crate (Cargo) or package (Node — each `package.json` of a
   * workspace), each row carrying the four things:
   *
   * - **the coordinate and the version** — with `${…}` expanded and `<dependencyManagement>`
   *   applied, or the requirement replaced by whatever `Cargo.lock` actually chose;
   * - **where that version came from** — declared here, pinned by a parent's management or by
   *   `[workspace.dependencies]`, or inherited whole. Clicking the row opens the manifest that
   *   answers it;
   * - **the scope** (Maven) or **kind** (Cargo), coloured, because `test` and `dev` change what a
   *   dependency means;
   * - **whether it resolved** — a declared dependency with nothing behind it in the local repository
   *   is exactly the shape of "cannot find symbol" in a file that looks fine.
   *
   * Plus one group for what nobody declared: what the declared dependencies dragged in.
   *
   * ## The resolved tree (Maven)
   *
   * Everything above is read from manifests and never runs the build tool, so the panel opens
   * instantly. Two questions need Maven itself, because mediation depends on the whole reactor:
   * **who brought this in** and **why is there more than one version of it**. The toolbar's
   * *Resolve tree* runs `dependency:tree` in the background (seconds to minutes, so on request and
   * never at open), and once it has answered the panel adds a *Version conflicts* group at the top,
   * a "via …" line to every transitive row, and an expandable detail on declared ones.
   */
  import {
    Library, Package, Layers, LocateFixed, ShieldAlert, Workflow, TriangleAlert, Eye, RotateCw,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import BennuModuleTree from './BennuModuleTree.svelte';
  import DependencyRow from './dependencies/DependencyRow.svelte';
  import DependencyConflicts from './dependencies/DependencyConflicts.svelte';
  import TransitiveRow from './dependencies/TransitiveRow.svelte';
  import { buildTreeIndex, childrenOf, nodeCoord, routes } from './dependencies/dependency-tree';
  import { afterPaint } from '$lib/utils/after-paint';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import type { ModuleRef } from './project-modules';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { buildUnitDir, openBuildUnitMenu } from './build-unit-menu';  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { moduleFocusStore } from '$lib/stores/bennu/module-focus.svelte';
  import { dependenciesStore, vulnFor } from '$lib/stores/bennu/dependencies.svelte';
  import {
    coordOf,
    type Dependency, type DependencyModule,
  } from '$lib/ipc/bennu/deps';

  let filter = $state('');

  // The module the report describes: a Maven, Gradle, Cargo or Node module anywhere in the workspace
  // (the project root for a plain one). A module tree picks it when there are several.
  const target = $derived(moduleFocusStore.targetFor('Maven', 'Gradle', 'Cargo', 'Node'));
  const root = $derived(target?.toolRoot ?? null);
  // A primitive on purpose: `target` is a fresh object whenever the active file changes, and an
  // effect that read it would re-read the report on every tab switch.
  const targetKind = $derived(target?.kind ?? null);
  const report = $derived(dependenciesStore.report);
  /** Which build tool this report describes — what decides the words, not the layout. */
  const ecosystem = $derived(report?.ecosystem ?? '');
  const isCargo = $derived(ecosystem === 'cargo');
  const isGradle = $derived(ecosystem === 'gradle');
  const isNode = $derived(ecosystem === 'node');
  /** Ecosystems whose tree has no "dropped" versions: both versions are built, so a conflict is a duplicate. */
  const duplicates = $derived(isCargo || isNode);
  /** What a module is called here. A Cargo "module" is a crate, a Gradle one a project and a Node
   *  one a package, and the panel should say so. */
  const unitWord = $derived(isCargo ? 'crate' : isGradle ? 'project' : isNode ? 'package' : 'module');
  const manifestWord = $derived(isCargo ? 'Cargo.toml' : isGradle ? 'build script' : isNode ? 'package.json' : 'pom');
  /** The tool the resolved tree is asked of. */
  const treeTool = $derived(isCargo ? 'cargo' : isGradle ? 'Gradle' : 'Maven');
  /** Where Node's tree comes from — the lockfile, which costs no package-manager run. */
  const nodeLockfile = 'the lockfile';

  // The resolved tree, once asked for (Maven only).
  const tree = $derived(dependenciesStore.tree);
  const treeIndex = $derived(buildTreeIndex(tree));

  // Re-read when the project changes, and again when the index **stops** — that is when a
  // classpath which was unresolved becomes resolved, and the jar column goes from "unknown" to an
  // answer.
  //
  // Gated on `indexing`, deliberately NOT on `buildRevision`: that counter ticks on every
  // index-progress event, including the per-file ones of the reference walk, so depending on it
  // means one whole-reactor pom walk per file indexed. On a real project that is thousands of
  // requests, each on its own backend thread, and what it produces is a backend that stops
  // answering anything at all.
  $effect(() => {
    const path = root;
    const busyIndexing = bennuIndexStore.indexing;
    const cargo = targetKind === 'Cargo';
    const node = targetKind === 'Node';
    if (!path) {
      dependenciesStore.reset();
      return;
    }
    // A project switch shows the tree already resolved for it (or none) — never starts Maven.
    dependenciesStore.adoptTree(path, node);
    // While the JAVA index runs the classpath is in flux; read once when it settles. A Cargo or Node
    // project builds no index, so waiting for one would mean waiting forever.
    if (!cargo && !node && busyIndexing) return;
    // After the swap has painted (the cached report is already on screen), and dropped if another
    // swap comes first.
    return afterPaint(() => {
      void dependenciesStore.load(path, true, node).then(() => {
        // After the report: the badges name artifacts the classpath (or the lockfile) now knows the
        // versions of.
        if (!cargo) void dependenciesStore.loadAdvisories(path, node);
        if (!cargo && !node) void dependenciesStore.checkStale(projectStore.workspaceRoots);
        if (node) void dependenciesStore.loadOutdated(path);
      });
    }, 200);
  });

  function resolveTree() {
    const path = root;
    if (!path) return;
    const node = targetKind === 'Node';
    // The report is read again behind the tree, so the two halves of this panel are drawn from the
    // repository as it is NOW. Left alone, the report kept the classpath of the last time it was read:
    // the tree said `1.0.7` and "Pulled in transitively" still said `1.0.6`.
    void dependenciesStore
      .loadTree(path, true, node)
      .then(() => dependenciesStore.load(path, true, node))
      .then(() => dependenciesStore.checkStale(projectStore.workspaceRoots));
  }

  /** Maven and Gradle keep a persisted classpath that a reload replaces; Cargo and Node do not. */
  const canReload = $derived(targetKind === 'Maven' || targetKind === 'Gradle');
  /** This project's classpath is older than the disk — see `bennu_dependencies_stale`. */
  const staleEntry = $derived(dependenciesStore.staleFor(root));

  /** Drop the cached classpath, the library jar pool and the index, and resolve again. */
  function reloadDependencies() {
    if (!root) return;
    void dependenciesStore.reload([staleEntry ?? { root, tool: isGradle ? 'gradle' : 'maven' }]);
  }

  /** A dependency matches by what it is, and — once the tree is resolved — by what it pulls in: searching
   *  for `freemarker` finds the library that brings it. */
  function matches(m: DependencyModule, d: Dependency, q: string): boolean {
    if (!q) return true;
    const origin = d.origin.kind === 'declared' ? '' : d.origin.from;
    if (`${coordOf(d)} ${d.name} ${d.version} ${d.scope} ${d.source} ${d.condition} ${origin}`.toLowerCase().includes(q)) {
      return true;
    }
    return pullsOf(m, d).some((n) => `${nodeCoord(n)} ${n.version}`.toLowerCase().includes(q));
  }

  const normDir = (p: string) => p.replace(/\\/g, '/').replace(/\/$/, '').toLowerCase();
  const solo = $derived(dependenciesStore.solo);
  const searching = $derived(filter.trim().length > 0);
  /** What the filter is, as the rows see it. */
  const query = $derived(filter.trim().toLowerCase());

  // Modules with their filtered lists. A module that matched nothing drops out while a filter is
  // active, so the panel shows the answer rather than a page of empty groups. A module the panel
  // is narrowed to is the only one drawn.
  const modules = $derived.by(() => {
    const q = query;
    return (report?.modules ?? [])
      .filter((m) => !solo || normDir(buildUnitDir(m)) === solo)
      .map((m) => ({ module: m, deps: m.dependencies.filter((d) => matches(m, d, q)) }))
      .filter((m) => m.deps.length > 0 || !q);
  });

  const transitive = $derived.by(() => {
    const q = query;
    // The report's transitive group is the whole project's, not one module's: narrowed to a module it
    // would show a long tail that is mostly not that module's, so it is left out.
    if (solo) return [];
    return (report?.transitive ?? []).filter((t) => {
      if (!q) return true;
      const via = routes(treeIndex.artifacts.get(coordOf(t))).flat().join(' ');
      return `${coordOf(t)} ${t.version} ${via}`.toLowerCase().includes(q);
    });
  });

  /** The menu of a module row of the selector tree: the same verb the section's own menu has. */
  function openTreeMenu(x: number, y: number, m: ModuleRef) {
    const dir = normDir(m.toolRoot);
    const only = solo === dir;
    bennuContextMenuStore.show(
      x, y,
      [
        { id: 'solo', label: only ? 'Show all modules' : 'Show only this module', icon: only ? Layers : Eye },
        { id: 'focus', label: 'Focus in Project', icon: LocateFixed },
      ],
      (id) => {
        if (id === 'solo') dependenciesStore.setSolo(m.toolRoot);
        else if (id === 'focus') bennuUiStore.focusInTree(m.toolRoot);
      },
    );
  }

  function openSectionMenu(x: number, y: number, m: DependencyModule) {
    const only = solo === normDir(buildUnitDir(m));
    openBuildUnitMenu(x, y, m, {
      label: only ? 'Show all modules' : 'Show only this module',
      icon: only ? Layers : Eye,
      run: () => dependenciesStore.setSolo(buildUnitDir(m)),
    });
  }

  const conflicts = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return treeIndex.conflicts.filter((c) => !q || `${c.coord} ${c.versions.join(' ')}`.toLowerCase().includes(q));
  });

  const total = $derived(
    modules.reduce((n, m) => n + m.deps.length, 0) + transitive.length,
  );
  /** Declared, and not in the local repository. The number worth surfacing without being asked. */
  const missing = $derived(
    report?.resolved_known
      ? (report?.modules ?? [])
          .flatMap((m) => m.dependencies)
          // A Node peer or optional dependency is legitimately absent (a platform binary, a package
          // the host app provides), so only what the project cannot run without counts as missing.
          .filter((d) => !d.resolved && (!isNode || d.scope === 'runtime' || d.scope === 'dev')).length
      : 0,
  );

  // Every group starts closed: opening the panel shows the list of modules, and what to read is
  // one click away. A filter opens the groups it found something in (see `searching`).
  const open = $state<Record<string, boolean>>({});
  function isOpen(id: string): boolean {
    return searching || (open[id] ?? false);
  }
  function toggle(id: string) {
    open[id] = !(open[id] ?? false);
  }

  /** The pom that decides this dependency — which is the one the row opens. */
  function openDeclaration(d: Dependency) {
    void projectStore.openFile(d.declared_in.file).then(() => {
      if (d.declared_in.line) bennuUiStore.requestGoto(d.declared_in.line);
    });
  }

  function openManifest(m: DependencyModule) {
    void projectStore.openFile(m.manifest);
  }

  /** What `m`'s dependency drags in, from the resolved tree. The tree names modules by full
   *  coordinate and the report by artifactId, so match on the tail. */
  function pullsOf(m: DependencyModule, d: Dependency) {
    const mod = tree?.modules.find((t) => t.module === m.id || t.module.endsWith(`:${m.id}`));
    return mod ? childrenOf(tree, mod.module, coordOf(d)) : [];
  }
</script>

<PanelShell title="Dependencies" count={report ? total : null}>
  {#snippet icon()}<Library size={13} />{/snippet}
  <!-- Declared unconditionally and gated inside: a snippet is a prop, and a prop wrapped in an
       `{#if}` is a prop the component may never be handed. -->
  {#snippet toolbar()}
    {#if root}
      <div class="dep-toolbar">
        <!-- The filter, then the two things that act on the whole list. The module graph, the
             re-read and the repository actions live in the build tool's own window (Maven, Cargo);
             this panel keeps what acts on the rows it draws. -->
        <BennuFilterBar bind:query={filter} placeholder="Filter dependencies…" />
        <IconButton
          tooltip={dependenciesStore.treeLoading
            ? 'Resolving the dependency tree…'
            : tree ? 'Resolve the tree again' : `Resolve the tree — who brings what, and ${duplicates ? 'duplicate versions' : 'version conflicts'}`}
          size={24}
          onclick={resolveTree}
          disabled={dependenciesStore.treeLoading}
        >
          {#if dependenciesStore.treeLoading}<Spinner size={12} />{:else}<Workflow size={13} />{/if}
        </IconButton>
        {#if canReload}
          <IconButton
            tooltip={dependenciesStore.reloading
              ? 'Reloading the dependencies…'
              : 'Reload dependencies — drop the cached classpath, re-read the local repository and rebuild the index'}
            size={24}
            onclick={reloadDependencies}
            disabled={dependenciesStore.reloading}
          >
            {#if dependenciesStore.reloading}<Spinner size={12} />{:else}<RotateCw size={13} />{/if}
          </IconButton>
        {/if}
        {#if !isCargo}
          <IconButton tooltip="Known vulnerabilities…" size={24} onclick={() => bennuUiStore.openVulnerabilities()}>
            <ShieldAlert size={13} />
          </IconButton>
        {/if}
      </div>
    {/if}
  {/snippet}

  {#if !root}
    <EmptyState message="Open a project to see its dependencies." />
  {:else if dependenciesStore.error}
    <div class="dep-scroll"><BennuModuleTree kinds={['Maven', 'Gradle', 'Cargo', 'Node']} ariaLabel="Projects" onModuleContextMenu={openTreeMenu} /></div>
    <div class="dep-notice">
      <Alert variant="error" compact text={dependenciesStore.error} />
    </div>
  {:else if !report && dependenciesStore.loading}
    <div class="dep-loading"><Spinner size={16} /><span>Reading the project's manifests…</span></div>
  {:else if report}
    <div class="dep-body">
      <BennuModuleTree kinds={['Maven', 'Gradle', 'Cargo', 'Node']} ariaLabel="Projects" onModuleContextMenu={openTreeMenu} />
      {#if solo}
        <div class="dep-notice dep-solo">
          <span>Showing one {unitWord} only.</span>
          <Button variant="secondary" size="sm" onclick={() => dependenciesStore.setSolo(null)}>
            {#snippet iconStart()}<Layers size={12} />{/snippet}
            Show all
          </Button>
        </div>
      {/if}
      {#if staleEntry && canReload}
        <div class="dep-notice">
          <Alert variant="warning" compact>
            The libraries this project was resolved from changed on disk (a module was installed, or a
            pom edited), so the editor's classpath is older than what is listed here. Reload to bring
            completion, go-to and the library index in line.
          </Alert>
          <div class="dep-notice-act">
            <Button variant="secondary" size="sm" loading={dependenciesStore.reloading} onclick={reloadDependencies}>
              {#snippet iconStart()}<RotateCw size={12} />{/snippet}
              Reload dependencies
            </Button>
          </div>
        </div>
      {/if}
      {#if report.unreadable.length > 0}
        <div class="dep-notice">
          <Alert variant="warning" compact>
            {report.unreadable.length === 1
              ? 'A manifest'
              : `${report.unreadable.length} manifests`} could not be read, so
            {report.unreadable.length === 1 ? `its ${unitWord} is` : `those ${unitWord}s are`}
            missing here: {report.unreadable.join(', ')}
          </Alert>
        </div>
      {/if}
      {#if !report.resolved_known && report.modules.length > 0}
        <div class="dep-notice">
          <Alert variant="info" compact>
            {#if isCargo}
              There is no <code>Cargo.lock</code> yet, so the version each of these resolves to is
              unknown — what is shown is the requirement as written. Any cargo command creates one.
            {:else if isNode}
              There is no lockfile and no <code>node_modules</code> yet, so the version each of these
              resolves to is unknown — what is shown is the range as written. Install the dependencies
              once (<code>npm install</code>, <code>pnpm install</code> or <code>yarn</code>).
            {:else}
              The dependency classpath hasn't been resolved yet, so whether each of these is in your
              local repository is unknown. It resolves in the background as the project indexes.
            {/if}
          </Alert>
        </div>
      {:else if missing > 0}
        <div class="dep-notice">
          <Alert variant="warning" compact>
            {#if isNode}
              {missing} declared {missing === 1 ? 'package is' : 'packages are'} not installed
              (no <code>node_modules</code> folder for {missing === 1 ? 'it' : 'them'}) — imports of
              {missing === 1 ? 'it' : 'them'} won't resolve. Install the dependencies once.
            {:else}
              {missing} declared {missing === 1 ? 'dependency is' : 'dependencies are'} not in the local
              {isCargo ? 'registry' : 'repository'} — types from {missing === 1 ? 'it' : 'them'} won't
              resolve. {isCargo ? 'Run any cargo command' : 'Build the project'} once to download
              {missing === 1 ? 'it' : 'them'}.
            {/if}
          </Alert>
        </div>
      {/if}

      <!-- The tree is the one thing here that runs Maven, so it is offered rather than done. -->
      {#if !tree && report.modules.length > 0}
        <div class="dep-notice">
          {#if dependenciesStore.treeError}
            <Alert variant="warning" compact text={dependenciesStore.treeError} />
          {:else}
            <Alert variant="info" compact>
              To see <strong>who brings each dependency in</strong> and which ones come in
              <strong>{duplicates ? 'at more than one version' : 'with conflicting versions'}</strong>, resolve
              the tree.
              {#if isNode}It is read from {nodeLockfile}, in the background.{:else}It runs {treeTool} in the background.{/if}
            </Alert>
          {/if}
          <div class="dep-notice-act">
            <Button variant="secondary" size="sm" loading={dependenciesStore.treeLoading} onclick={resolveTree}>
              {#snippet iconStart()}<Workflow size={12} />{/snippet}
              Resolve tree
            </Button>
          </div>
        </div>
      {/if}

      <!-- Without the dropped nodes the tree can still say who brings what, but a conflict is
           exactly a dropped node — so an empty conflicts group would read as "none", which it is not. -->
      {#if tree && !tree.verbose && !isNode}
        <div class="dep-notice">
          <Alert variant="info" compact>
            This Maven did not report the versions it dropped, so <strong>version conflicts</strong>
            cannot be listed; who brings each dependency in is still shown.
          </Alert>
        </div>
      {/if}

      {#if modules.length === 0 && transitive.length === 0 && conflicts.length === 0}
        <EmptyState
          message={filter
            ? 'No dependencies match the filter.'
            : 'This project declares no dependencies.'}
          compact
        />
      {:else}
        {#if conflicts.length > 0}
          <SidebarSection
            label={duplicates ? 'Duplicate versions' : 'Version conflicts'}
            expanded={isOpen('__conflicts')}
            onToggle={() => toggle('__conflicts')}
            badge={conflicts.length}
            badgeTitle={`${isCargo ? 'Crates' : isNode ? 'Packages' : 'Artifacts'} the resolved tree sees at more than one version`}
          >
            {#snippet icon()}<TriangleAlert size={13} />{/snippet}
            <DependencyConflicts {conflicts} {ecosystem} />
          </SidebarSection>
        {/if}

        {#each modules as m (m.module.manifest)}
          <SidebarSection
            label={m.module.name}
            expanded={isOpen(m.module.manifest)}
            onToggle={() => toggle(m.module.manifest)}
            badge={m.deps.length}
            badgeTitle={`Dependencies this ${unitWord} declares`}
            onContextMenu={(x, y) => openSectionMenu(x, y, m.module)}
          >
            {#snippet icon()}<Package size={13} />{/snippet}
            {#snippet actions()}
              <IconButton
                tooltip={`Focus this ${unitWord} in the Project tree`}
                size={20}
                onclick={() => bennuUiStore.focusInTree(buildUnitDir(m.module))}
              >
                <LocateFixed size={11} />
              </IconButton>
            {/snippet}
            {#if m.deps.length === 0}
              <p class="dep-none">
                No dependencies.
                <button type="button" class="dep-link" onclick={() => openManifest(m.module)}>
                  Open {m.module.id}'s {manifestWord}
                </button>
              </p>
            {:else}
              <ul class="dep-list">
                {#each m.deps as d (coordOf(d) + '@' + d.variant + '@' + d.scope)}
                  <li>
                    <DependencyRow
                      dep={d}
                      {ecosystem}
                      resolvedKnown={report.resolved_known}
                      usage={treeIndex.artifacts.get(coordOf(d))}
                      vuln={vulnFor(dependenciesStore.advisories, coordOf(d), d.version)}
                      newer={isNode ? dependenciesStore.outdated.get(d.name) : undefined}
                      pulls={pullsOf(m.module, d)}
                      {query}
                      onOpen={openDeclaration}
                    />
                  </li>
                {/each}
              </ul>
            {/if}
          </SidebarSection>
        {/each}

        {#if transitive.length > 0}
          <SidebarSection
            label="Pulled in transitively"
            expanded={isOpen('__transitive')}
            onToggle={() => toggle('__transitive')}
            badge={transitive.length}
          >
            {#snippet icon()}<Layers size={13} />{/snippet}
            <ul class="dep-list">
              {#each transitive as t (t.name + '@' + t.version)}
                {@const coord = coordOf(t)}
                <li>
                  <TransitiveRow
                    dep={t}
                    usage={treeIndex.artifacts.get(coord)}
                    vuln={vulnFor(dependenciesStore.advisories, coord, t.version)}
                    {query}
                  />
                </li>
              {/each}
            </ul>
          </SidebarSection>
        {/if}
      {/if}
    </div>
  {:else}
    <!-- Before the first read lands. Not an empty state with an opinion: nothing is known yet. -->
    <div class="dep-loading"><Spinner size={16} /><span>Reading the project's manifests…</span></div>
  {/if}
</PanelShell>

<style>
  /* Body rhythm mirrors the Maven tool: SidebarSection owns the group header + indent guideline,
     so the body itself keeps no horizontal padding. */
  .dep-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0 8px; }
  /* The shell's toolbar slot is a bare row, so a toolbar with more than one control brings its
     own layout. */
  .dep-toolbar { display: flex; align-items: center; gap: 4px; padding-right: 4px; }
  .dep-toolbar > :global(:first-child) { flex: 1; min-width: 0; }
  .dep-notice { padding: 6px 8px 2px; }
  /* The selector alone, for the states that have no body to scroll in (an error). */
  .dep-scroll { max-height: 40%; overflow-y: auto; }
  .dep-notice-act { display: flex; justify-content: flex-end; padding-top: 6px; }
  .dep-loading {
    display: flex; align-items: center; gap: 8px;
    padding: 14px 12px; color: var(--text-muted); font-size: var(--font-size-xs);
  }

  .dep-list {
    list-style: none; margin: 0; padding: 0 6px 2px 0;
    display: flex; flex-direction: column; gap: 1px;
  }
  .dep-none {
    margin: 0; padding: 4px 8px 6px;
    font-size: var(--font-size-2xs); color: var(--text-disabled);
  }
  .dep-link {
    background: none; border: 0; padding: 0; cursor: pointer;
    font: inherit; color: var(--accent); text-decoration: underline;
  }

  .dep-solo {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    font-size: var(--font-size-xs); color: var(--text-muted);
  }
  code {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-primary);
  }
</style>
