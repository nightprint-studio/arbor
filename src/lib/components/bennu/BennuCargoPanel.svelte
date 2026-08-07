<script lang="ts">
  /**
   * Cargo (right tool window) — the workspace, and what you can run on it.
   *
   * The Rust counterpart of the Maven tool window, and unlike it these rows actually run: every
   * command launches into the Run console through the same path a JVM launch takes, so Stop, ⟳ and
   * the tab strip work on it unchanged.
   *
   * Three layers, outermost first:
   *
   * 1. **the workspace** — its commands, aimed at every crate (`--workspace`). What you press when
   *    the question is "does this still compile";
   * 2. **each crate** — its own commands, targets and features ({@link BennuCargoCrateSection});
   * 3. **what is wrong with it** — a crate under the root that `members` does not cover, and a
   *    toolchain missing the component a command needs. Both are silent failures otherwise.
   *
   * Everything comes from `bennu_cargo_workspace`, which reads manifests and the filesystem rather
   * than running `cargo metadata` — so the panel opens instantly, and opens on a workspace that has
   * never been built.
   */
  import { Cog, PackagePlus, Play, RefreshCw, TriangleAlert } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import BennuCargoCrateSection from './BennuCargoCrateSection.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuCargoStore } from '$lib/stores/bennu/cargo.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { emptyInvocation, hasComponent } from '$lib/ipc/bennu/cargo';

  let filter = $state('');

  const root = $derived(projectStore.project?.root ?? null);
  const isCargo = $derived(projectStore.isCargo);
  const workspace = $derived(bennuCargoStore.workspace);
  const toolchain = $derived(bennuCargoStore.toolchain);

  // No load effect here on purpose: `BennuWindow` reads the workspace when a Cargo project opens,
  // because the run-configuration editor and ▶ want it before this panel is ever opened. Two owners
  // would mean two effects racing to fill the same slot. ⟳ below is the way to re-read it.

  /** Crates matching the filter. A filter that matches nothing leaves an empty list rather than a
   *  page of empty groups. */
  const crates = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    const all = workspace?.crates ?? [];
    if (!q) return all;
    return all.filter((c) =>
      `${c.name} ${c.rel_path} ${c.description} ${c.targets.map((t) => t.name).join(' ')}`
        .toLowerCase()
        .includes(q),
    );
  });

  /** The commands offered on a crate row — the front row only, so a crate does not unfold into
   *  eleven rows. The rest are on the workspace section, where the list has room. */
  const crateCommands = $derived(bennuCargoStore.commonCommands);

  // Sections are open by default; the crates start collapsed on a big workspace, where a hundred
  // expanded crates is a scroll rather than an overview.
  const open = $state<Record<string, boolean>>({});
  const collapseByDefault = $derived((workspace?.crates.length ?? 0) > 6);
  function isOpen(id: string, fallback: boolean): boolean {
    return open[id] ?? fallback;
  }
  function toggle(id: string, fallback: boolean) {
    open[id] = !isOpen(id, fallback);
  }

  /** Run a command, aimed at whatever the caller says. */
  function run(
    command: string,
    opts: { package?: string; targetKind?: string; target?: string } = {},
  ) {
    if (!root) return;
    const invocation = {
      ...emptyInvocation(command),
      package: opts.package ?? '',
      // The whole workspace when no crate is named: at a virtual root a bare command builds
      // nothing at all, which is the worst possible answer to pressing Build.
      workspace: !opts.package,
      target: { kind: opts.targetKind ?? '', name: opts.target ?? '' },
    };
    const label = [command, opts.package, opts.target].filter(Boolean).join(' ');
    void bennuRunStore.runCargoCommand(root, invocation, label || command);
  }

  function openFile(path: string) {
    void projectStore.openFile(path);
  }

  /** The `[workspace] members` list, so an orphan is one click from being fixed. */
  function openRootManifest() {
    if (workspace) openFile(`${workspace.root}/Cargo.toml`);
  }

</script>

<PanelShell title="Cargo" count={workspace ? workspace.crates.length : null}>
  {#snippet icon()}<Cog size={13} />{/snippet}
  <!-- Declared unconditionally and gated inside: a snippet is a prop, and a prop wrapped in an
       `{#if}` is a prop the component may never be handed. -->
  {#snippet toolbar()}
    {#if root && isCargo}
      <div class="cg-toolbar">
        <BennuFilterBar bind:query={filter} placeholder="Filter crates…" />
        <IconButton
          tooltip="Add a dependency — runs cargo add"
          size={22}
          onclick={() => bennuUiStore.openCargoAdd()}
        >
          <PackagePlus size={12} />
        </IconButton>
        <IconButton
          tooltip="Re-read the workspace manifests"
          size={22}
          disabled={bennuCargoStore.loading}
          onclick={() => root && void bennuCargoStore.load(root, true)}
        >
          <RefreshCw size={12} />
        </IconButton>
      </div>
    {/if}
  {/snippet}

  {#if !root}
    <EmptyState message="Open a project to see its crates." />
  {:else if !isCargo}
    <EmptyState message="This is not a Cargo project — the Maven tool window has its goals." />
  {:else if bennuCargoStore.error}
    <div class="cg-notice">
      <Alert variant="error" compact text={bennuCargoStore.error} />
    </div>
  {:else if !workspace && bennuCargoStore.loading}
    <div class="cg-loading"><Spinner size={16} /><span>Reading the workspace…</span></div>
  {:else if workspace}
    <div class="cg-body">
      {#if toolchain?.version}
        <p class="cg-toolchain" use:tooltip={toolchain.toolchain || 'The active toolchain'}>
          {toolchain.version}
        </p>
      {:else if toolchain}
        <div class="cg-notice">
          <Alert variant="warning" compact>
            <code>cargo</code> could not be run. Nothing here will work until it is on the PATH this
            app inherits.
          </Alert>
        </div>
      {/if}

      {#if !workspace.locked}
        <div class="cg-notice">
          <Alert variant="info" compact>
            No <code>Cargo.lock</code> yet, so the Dependencies panel cannot say which versions you
            are actually compiling against. Any command below creates one.
          </Alert>
        </div>
      {/if}

      {#if workspace.orphans.length > 0}
        <div class="cg-notice">
          <Alert variant="warning" compact>
            {workspace.orphans.length === 1 ? 'A crate is' : `${workspace.orphans.length} crates are`}
            not in <code>members</code>, so <code>--workspace</code> skips
            {workspace.orphans.length === 1 ? 'it' : 'them'}:
            {workspace.orphans.join(', ')}.
            <button type="button" class="cg-link" onclick={openRootManifest}>
              Open the workspace manifest
            </button>
          </Alert>
        </div>
      {/if}

      {#if workspace.unreadable.length > 0}
        <div class="cg-notice">
          <Alert variant="warning" compact>
            {workspace.unreadable.length === 1 ? 'A manifest' : `${workspace.unreadable.length} manifests`}
            could not be read, so
            {workspace.unreadable.length === 1 ? 'that crate is' : 'those crates are'} missing here:
            {workspace.unreadable.join(', ')}
          </Alert>
        </div>
      {/if}

      <!-- The workspace-wide commands. Every command, not just the common ones: this is the list
           you come here to read, and a crate row is where the short list belongs. -->
      <SidebarSection
        label={workspace.is_workspace ? 'Whole workspace' : workspace.name}
        expanded={isOpen('__workspace', true)}
        onToggle={() => toggle('__workspace', true)}
        badge={bennuCargoStore.commands.length}
      >
        {#snippet icon()}<Cog size={13} />{/snippet}
        {#each bennuCargoStore.commands as c (c.id)}
          {@const available = hasComponent(toolchain, c.component)}
          <SidebarItem onclick={() => run(c.id)}>
            {#snippet icon()}<span class="cg-run"><Play size={11} /></span>{/snippet}
            cargo {c.label}
            {#snippet badges()}
              <span class="cg-hint" class:cg-missing={!available}>
                {available ? c.doc : `needs the ${c.component} component`}
              </span>
            {/snippet}
          </SidebarItem>
        {/each}
        {#if bennuCargoStore.commands.some((c) => !hasComponent(toolchain, c.component))}
          <p class="cg-warn">
            <TriangleAlert size={11} />
            A greyed hint means this toolchain has no such component —
            <code>rustup component add …</code> installs it, then press ⟳.
          </p>
        {/if}
      </SidebarSection>

      {#if crates.length === 0}
        <EmptyState
          message={filter
            ? 'No crate matches the filter.'
            : 'This workspace declares no crates. Its `members` list may be empty.'}
          compact
        />
      {:else}
        {#each crates as crate (crate.manifest)}
          <BennuCargoCrateSection
            {crate}
            commands={crateCommands}
            {toolchain}
            expanded={isOpen(crate.manifest, !collapseByDefault)}
            onToggle={() => toggle(crate.manifest, !collapseByDefault)}
            onRun={(command, opts) => run(command, opts)}
            onOpen={openFile}
          />
        {/each}
      {/if}

      <!-- Not everything about a workspace is a command. The one other thing worth a click from
           here is where the run configurations live, because a command you run with particular
           features every time belongs in one. -->
      <button type="button" class="cg-foot" onclick={() => bennuUiStore.openRunConfig()}>
        Save a command as a run configuration…
      </button>
    </div>
  {:else}
    <div class="cg-loading"><Spinner size={16} /><span>Reading the workspace…</span></div>
  {/if}
</PanelShell>

<style>
  /* Body rhythm mirrors the Maven and Dependencies tools: SidebarSection owns the group header and
     the indent guideline, so the body keeps no horizontal padding. */
  .cg-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0 8px; }
  .cg-toolbar { display: flex; align-items: center; gap: 4px; padding-right: 4px; }
  .cg-toolbar > :global(:first-child) { flex: 1; min-width: 0; }
  .cg-notice { padding: 6px 8px 2px; }
  .cg-loading {
    display: flex; align-items: center; gap: 8px;
    padding: 14px 12px; color: var(--text-muted); font-size: var(--font-size-xs);
  }
  .cg-toolchain {
    margin: 0; padding: 2px 10px 6px;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .cg-run { color: var(--success); display: flex; }
  .cg-hint {
    font-size: var(--font-size-3xs); color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 24ch;
  }
  .cg-missing { color: var(--warning); }
  .cg-warn {
    display: flex; align-items: flex-start; gap: 5px;
    margin: 4px 0 2px; padding: 0 8px;
    font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .cg-warn :global(svg) { flex-shrink: 0; margin-top: 1px; color: var(--warning); }
  .cg-link {
    background: none; border: 0; padding: 0; cursor: pointer;
    font: inherit; color: var(--accent-primary); text-decoration: underline;
  }
  .cg-foot {
    display: block; width: 100%; margin-top: 6px; padding: 6px 10px;
    background: none; border: 0; border-top: 1px solid var(--border-subtle);
    text-align: left; cursor: pointer;
    font-size: var(--font-size-2xs); color: var(--text-muted);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .cg-foot:hover { color: var(--text-primary); background: var(--bg-hover); }
  .cg-foot:focus-visible { outline: 1px solid var(--accent-primary); outline-offset: -1px; }
  code {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-primary);
  }
</style>
