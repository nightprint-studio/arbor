<script lang="ts">
  /**
   * Dependencies (left tool window) — what the open project actually depends on.
   *
   * One group per module of the reactor, each row carrying the four things a dependency list is
   * opened to find out:
   *
   * - **the coordinate and the version** — with `${…}` expanded and `<dependencyManagement>`
   *   applied, because the version written in the pom in front of you is usually not the one you
   *   get;
   * - **where that version came from** — declared here, pinned by a parent's management, or
   *   inherited whole from a parent's own `<dependencies>`. This is the question the panel exists
   *   for, and clicking the row opens the pom that answers it;
   * - **the scope**, coloured, because `test` and `provided` change what a dependency means;
   * - **whether it resolved** — a declared dependency with no jar in the local repository is
   *   exactly the shape of "cannot find symbol" in a file that looks fine.
   *
   * Plus one group for the jars nobody declared: what the declared dependencies dragged in. Kept
   * separate rather than merged, because the two answer different questions and mixing them is how
   * a dependency panel becomes unreadable.
   *
   * All of it comes from `bennu_dependencies`, which reads poms and the classpath the index
   * already resolved. Nothing here runs Maven, so refreshing is cheap and the panel opens
   * instantly.
   */
  import { Library, Package, GitFork, Layers, RefreshCw, CircleSlash } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { dependenciesStore } from '$lib/stores/bennu/dependencies.svelte';
  import { coordOf, type Dependency, type DependencyModule } from '$lib/ipc/bennu/deps';

  let filter = $state('');

  const root = $derived(projectStore.project?.root ?? null);
  const report = $derived(dependenciesStore.report);

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
    if (!path || projectStore.isCargo) {
      dependenciesStore.reset();
      return;
    }
    // While the index runs the classpath is in flux; read once when it settles.
    if (busyIndexing) return;
    void dependenciesStore.load(path, true);
  });

  function matches(d: Dependency, q: string): boolean {
    if (!q) return true;
    const origin = d.origin.kind === 'declared' ? '' : d.origin.from;
    return `${coordOf(d)} ${d.version} ${d.scope} ${d.profile} ${origin}`.toLowerCase().includes(q);
  }

  // Modules with their filtered lists. A module that matched nothing drops out while a filter is
  // active, so the panel shows the answer rather than a page of empty groups.
  const modules = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return (report?.modules ?? [])
      .map((m) => ({ module: m, deps: m.dependencies.filter((d) => matches(d, q)) }))
      .filter((m) => m.deps.length > 0 || !q);
  });

  const transitive = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return (report?.transitive ?? []).filter(
      (t) => !q || `${coordOf(t)} ${t.version}`.toLowerCase().includes(q),
    );
  });

  const total = $derived(
    modules.reduce((n, m) => n + m.deps.length, 0) + transitive.length,
  );
  /** Declared, and not in the local repository. The number worth surfacing without being asked. */
  const missing = $derived(
    report?.classpath_known
      ? (report?.modules ?? []).flatMap((m) => m.dependencies).filter((d) => !d.jar).length
      : 0,
  );

  // All groups open by default; the transitive one closed, because it is the long tail and
  // opening the panel should show what the project asked for, not what came in behind it.
  const open = $state<Record<string, boolean>>({});
  function isOpen(id: string, fallback = true): boolean {
    return open[id] ?? fallback;
  }
  function toggle(id: string, fallback = true) {
    open[id] = !isOpen(id, fallback);
  }

  function scopeTone(scope: string): 'success' | 'info' | 'warning' | 'neutral' {
    if (scope === 'compile') return 'success';
    if (scope === 'provided' || scope === 'system') return 'info';
    if (scope === 'test') return 'warning';
    return 'neutral';
  }

  /** The pom that decides this dependency — which is the one the row opens. */
  function openDeclaration(d: Dependency) {
    void projectStore.openFile(d.declared_in.file).then(() => {
      if (d.declared_in.line) bennuUiStore.requestGoto(d.declared_in.line);
    });
  }

  function openPom(m: DependencyModule) {
    void projectStore.openFile(m.pom);
  }

  /** What the origin tag says, in the fewest words that are still true. */
  function originLabel(d: Dependency): string {
    switch (d.origin.kind) {
      case 'managed':
        return `pinned by ${d.origin.from}`;
      case 'inherited':
        return `from ${d.origin.from}`;
      default:
        return '';
    }
  }

  function originTooltip(d: Dependency): string {
    switch (d.origin.kind) {
      case 'managed':
        return `Declared here without a version — ${d.origin.from}'s <dependencyManagement> pins ${
          d.version || 'it'
        }. Opens that pom.`;
      case 'inherited':
        return `Not declared in this module: inherited from ${d.origin.from}'s own <dependencies>. Opens that pom.`;
      default:
        return `Declared in this module (line ${d.declared_in.line}).`;
    }
  }
</script>

<PanelShell title="Dependencies" count={report ? total : null}>
  {#snippet icon()}<Library size={13} />{/snippet}
  <!-- Declared unconditionally and gated inside: a snippet is a prop, and a prop wrapped in an
       `{#if}` is a prop the component may never be handed. -->
  {#snippet toolbar()}
    {#if root && !projectStore.isCargo}
      <div class="dep-toolbar">
        <BennuFilterBar bind:query={filter} placeholder="Filter dependencies…" />
        <IconButton
          tooltip="Re-read the poms and the resolved classpath"
          size={22}
          disabled={dependenciesStore.loading}
          onclick={() => root && void dependenciesStore.load(root, true)}
        >
          <RefreshCw size={12} />
        </IconButton>
      </div>
    {/if}
  {/snippet}

  {#if !root}
    <EmptyState message="Open a project to see its dependencies." />
  {:else if projectStore.isCargo}
    <EmptyState message="Cargo projects don't have a Maven dependency graph." />
  {:else if dependenciesStore.error}
    <div class="dep-notice">
      <Alert variant="error" compact text={dependenciesStore.error} />
    </div>
  {:else if !report && dependenciesStore.loading}
    <div class="dep-loading"><Spinner size={16} /><span>Reading the project's poms…</span></div>
  {:else if report}
    <div class="dep-body">
      {#if report.unreadable.length > 0}
        <div class="dep-notice">
          <Alert variant="warning" compact>
            {report.unreadable.length === 1 ? 'A pom' : `${report.unreadable.length} poms`} could not
            be read, so {report.unreadable.length === 1 ? 'its module is' : 'those modules are'}
            missing here: {report.unreadable.join(', ')}
          </Alert>
        </div>
      {/if}
      {#if !report.classpath_known && report.modules.length > 0}
        <div class="dep-notice">
          <Alert variant="info" compact>
            The dependency classpath hasn't been resolved yet, so whether each of these is in your
            local repository is unknown. It resolves in the background as the project indexes.
          </Alert>
        </div>
      {:else if missing > 0}
        <div class="dep-notice">
          <Alert variant="warning" compact>
            {missing} declared {missing === 1 ? 'dependency is' : 'dependencies are'} not in the local
            repository — types from {missing === 1 ? 'it' : 'them'} won't resolve. Build the project
            once to download {missing === 1 ? 'it' : 'them'}.
          </Alert>
        </div>
      {/if}

      {#if modules.length === 0 && transitive.length === 0}
        <EmptyState
          message={filter
            ? 'No dependencies match the filter.'
            : 'This project declares no dependencies.'}
          compact
        />
      {:else}
        {#each modules as m (m.module.pom)}
          <SidebarSection
            label={m.module.name}
            expanded={isOpen(m.module.pom)}
            onToggle={() => toggle(m.module.pom)}
            badge={m.deps.length}
          >
            {#snippet icon()}<Package size={13} />{/snippet}
            {#if m.deps.length === 0}
              <p class="dep-none">
                No dependencies.
                <button type="button" class="dep-link" onclick={() => openPom(m.module)}>
                  Open {m.module.artifact_id}'s pom
                </button>
              </p>
            {:else}
              <ul class="dep-list">
                {#each m.deps as d (coordOf(d) + '@' + d.classifier)}
                  <li>
                    <button
                      type="button"
                      class="dep-row"
                      class:unresolved={report.classpath_known && !d.jar}
                      onclick={() => openDeclaration(d)}
                      use:tooltip={originTooltip(d)}
                    >
                      <span class="dep-main">
                        <span class="dep-coord mono">{coordOf(d)}</span>
                        <span class="dep-version mono" class:unknown={!d.version}>
                          {d.version || 'version unknown'}
                        </span>
                      </span>
                      <span class="dep-meta">
                        <Badge variant="tone" tone={scopeTone(d.scope)} size="sm" label={d.scope} />
                        {#if d.packaging}
                          <Badge variant="tone" tone="neutral" size="sm" label={d.packaging} />
                        {/if}
                        {#if d.classifier}
                          <Badge variant="tone" tone="neutral" size="sm" label={d.classifier} />
                        {/if}
                        {#if d.optional}
                          <span class="dep-tag">optional</span>
                        {/if}
                        {#if d.profile}
                          <span class="dep-tag dep-tag-profile">profile: {d.profile}</span>
                        {/if}
                        {#if d.origin.kind !== 'declared'}
                          <span class="dep-origin">
                            <GitFork size={10} />
                            <span class="dep-origin-txt">{originLabel(d)}</span>
                          </span>
                        {/if}
                        {#if report.classpath_known && !d.jar}
                          <span class="dep-missing"><CircleSlash size={10} /> not resolved</span>
                        {/if}
                      </span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </SidebarSection>
        {/each}

        {#if transitive.length > 0}
          <SidebarSection
            label="Pulled in transitively"
            expanded={isOpen('__transitive', false)}
            onToggle={() => toggle('__transitive', false)}
            badge={transitive.length}
          >
            {#snippet icon()}<Layers size={13} />{/snippet}
            <ul class="dep-list">
              {#each transitive as t (t.jar)}
                <li class="dep-row dep-row-static" use:tooltip={t.jar}>
                  <span class="dep-main">
                    <span class="dep-coord mono">{coordOf(t)}</span>
                    <span class="dep-version mono">{t.version}</span>
                  </span>
                </li>
              {/each}
            </ul>
          </SidebarSection>
        {/if}
      {/if}
    </div>
  {:else}
    <!-- Before the first read lands. Not an empty state with an opinion: nothing is known yet. -->
    <div class="dep-loading"><Spinner size={16} /><span>Reading the project's poms…</span></div>
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
    font: inherit; color: var(--accent-primary); text-decoration: underline;
  }

  .dep-row {
    width: 100%; display: flex; flex-direction: column; gap: 3px; align-items: stretch;
    padding: 5px 8px; border: 0; background: none; text-align: left; cursor: pointer;
    border-radius: var(--radius-sm); color: inherit; font: inherit;
  }
  .dep-row:hover { background: var(--bg-hover, rgba(255, 255, 255, 0.04)); }
  .dep-row:focus-visible { outline: 1px solid var(--accent-primary); outline-offset: -1px; }
  /* The transitive rows are not a link anywhere: there is no declaration to open. */
  .dep-row-static { cursor: default; }
  .dep-row-static:hover { background: none; }
  /* A declared dependency with no jar behind it — the row worth spotting from across the panel. */
  .dep-row.unresolved .dep-coord { color: var(--color-warning, #d6a640); }

  .dep-main { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .dep-coord {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--text-primary);
  }
  .dep-version { flex-shrink: 0; color: var(--text-muted); }
  .dep-version.unknown { font-style: italic; color: var(--text-disabled); }
  .mono { font-family: var(--font-code); font-size: var(--font-size-xs); }

  .dep-meta { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; }
  .dep-tag {
    font-size: var(--font-size-3xs); color: var(--text-disabled);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); padding: 0 4px;
  }
  .dep-tag-profile { color: var(--text-muted); }
  .dep-origin, .dep-missing {
    display: inline-flex; align-items: center; gap: 3px;
    font-size: var(--font-size-3xs); color: var(--text-disabled); min-width: 0;
  }
  .dep-missing { color: var(--color-warning, #d6a640); }
  .dep-origin :global(svg), .dep-missing :global(svg) { flex-shrink: 0; }
  .dep-origin-txt { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
