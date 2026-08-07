<script lang="ts">
  /**
   * One crate of the workspace, in the Cargo tool window.
   *
   * Three groups, in the order the questions come up: what you can **run** on it, what it
   * **builds**, and what **features** it has. Its own file because the panel would otherwise be one
   * component holding three nested loops, and because a crate row is the thing most likely to gain
   * something (a size, a rebuild time, a lint count).
   *
   * ## The rows are actions, not decoration
   *
   * Every command row launches into the Run console, and every target row either runs (a binary) or
   * opens its source. That is the whole point of the panel: `cargo clippy -p bennu-cargo` is two
   * clicks instead of a remembered command line.
   */
  import { Boxes, Cog, FileCode2, Play, ToggleLeft } from 'lucide-svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import {
    hasComponent,
    type CargoCommandDef, type CargoCrate, type CargoToolchain,
  } from '$lib/ipc/bennu/cargo';

  let {
    crate,
    commands,
    toolchain,
    /** Runs an invocation. The panel owns the root, so the section only says what to run. */
    onRun,
    /** Opens a file — a manifest, or a target's source. */
    onOpen,
    expanded,
    onToggle,
  }: {
    crate: CargoCrate;
    commands: CargoCommandDef[];
    toolchain: CargoToolchain | null;
    onRun: (command: string, opts: { package: string; targetKind?: string; target?: string }) => void;
    onOpen: (path: string) => void;
    expanded: boolean;
    onToggle: () => void;
  } = $props();

  /** What the crate builds, for the header badge. */
  const kind = $derived.by(() => {
    const has = (k: string) => crate.targets.some((t) => t.kind === k);
    if (crate.targets.some((t) => t.proc_macro)) return 'proc-macro';
    if (has('lib') && has('bin')) return 'lib+bin';
    if (has('lib')) return 'lib';
    if (has('bin')) return 'bin';
    return '';
  });

  /** The runnable and testable targets, grouped — the library and the auto-discovered tests are
   *  rarely what you click, so they come after the binaries. */
  const grouped = $derived(
    (['bin', 'example', 'test', 'bench', 'lib'] as const)
      .map((k) => ({ kind: k, targets: crate.targets.filter((t) => t.kind === k) }))
      .filter((g) => g.targets.length > 0),
  );

  const total = $derived(crate.deps + crate.dev_deps + crate.build_deps);

  let commandsOpen = $state(true);
  let targetsOpen = $state(false);
  let featuresOpen = $state(false);

  /** The label a target row shows for its action. A binary and an example run; everything else is
   *  built or tested, and clicking it opens its source instead. */
  function runsDirectly(kind: string): boolean {
    return kind === 'bin' || kind === 'example';
  }

  /** A target's source path, absolute — its manifest's directory plus the relative path. */
  function sourceOf(target: { path: string }): string {
    const dir = crate.manifest.replace(/\/Cargo\.toml$/i, '');
    return target.path ? `${dir}/${target.path}` : crate.manifest;
  }

  const KIND_LABEL: Record<string, string> = {
    bin: 'Binaries',
    example: 'Examples',
    test: 'Tests',
    bench: 'Benchmarks',
    lib: 'Library',
  };
</script>

<SidebarSection
  label={crate.name}
  {expanded}
  {onToggle}
  badge={total || null}
  badgeTitle="Declared dependencies, across all three kinds"
>
  {#snippet icon()}<Boxes size={13} />{/snippet}
  {#snippet actions()}
    <span class="cc-meta">
      {#if kind}<Badge variant="tone" tone="neutral" size="sm" label={kind} />{/if}
      {#if crate.version}
        <span class="cc-version" use:tooltip={crate.version === 'inherited'
          ? 'The version comes from [workspace.package]'
          : 'The version this crate declares'}>{crate.version}</span>
      {/if}
    </span>
  {/snippet}

  <div class="cc-body">
    <!-- The manifest, first: it is what you open when the answer is not in the panel. -->
    <SidebarItem onclick={() => onOpen(crate.manifest)}>
      {#snippet icon()}<span class="cc-ic"><FileCode2 size={11} /></span>{/snippet}
      Cargo.toml
      {#snippet badges()}
        {#if crate.rel_path}<span class="cc-hint">{crate.rel_path}</span>{/if}
      {/snippet}
    </SidebarItem>

    <SidebarSection
      label="Commands"
      expanded={commandsOpen}
      onToggle={() => (commandsOpen = !commandsOpen)}
      badge={commands.length}
    >
      {#snippet icon()}<Cog size={13} />{/snippet}
      {#each commands as c (c.id)}
        {@const available = hasComponent(toolchain, c.component)}
        <SidebarItem onclick={() => onRun(c.id, { package: crate.name })}>
          {#snippet icon()}<span class="cc-run"><Play size={11} /></span>{/snippet}
          cargo {c.label}
          {#snippet badges()}
            <span class="cc-hint" class:cc-missing={!available}>
              {available ? c.doc : `needs the ${c.component} component`}
            </span>
          {/snippet}
        </SidebarItem>
      {/each}
    </SidebarSection>

    {#if grouped.length > 0}
      <SidebarSection
        label="Targets"
        expanded={targetsOpen}
        onToggle={() => (targetsOpen = !targetsOpen)}
        badge={crate.targets.length}
      >
        {#snippet icon()}<FileCode2 size={13} />{/snippet}
        {#each grouped as group (group.kind)}
          <p class="cc-group">{KIND_LABEL[group.kind] ?? group.kind}</p>
          {#each group.targets as t (group.kind + t.name)}
            <SidebarItem
              onclick={() =>
                runsDirectly(group.kind)
                  ? onRun('run', { package: crate.name, targetKind: group.kind, target: t.name })
                  : onOpen(sourceOf(t))}
            >
              {#snippet icon()}
                <span class={runsDirectly(group.kind) ? 'cc-run' : 'cc-ic'}>
                  {#if runsDirectly(group.kind)}<Play size={11} />{:else}<FileCode2 size={11} />{/if}
                </span>
              {/snippet}
              {t.name}
              {#snippet badges()}
                <span class="cc-hint">
                  {#if t.proc_macro}proc-macro{:else if !t.declared}auto{:else}declared{/if}
                  {#if t.required_features.length}
                    · needs {t.required_features.join(', ')}
                  {/if}
                </span>
              {/snippet}
            </SidebarItem>
          {/each}
        {/each}
      </SidebarSection>
    {/if}

    {#if crate.features.length > 0}
      <SidebarSection
        label="Features"
        expanded={featuresOpen}
        onToggle={() => (featuresOpen = !featuresOpen)}
        badge={crate.features.length}
      >
        {#snippet icon()}<ToggleLeft size={13} />{/snippet}
        {#each crate.features as f (f.name)}
          <!-- Not a button: a feature is not something the panel can turn on. What it CAN do is say
               which are on by default and what each pulls in, which is the question you open a
               manifest to answer. -->
          <div class="cc-feature" use:tooltip={f.enables.length
            ? `Enables ${f.enables.join(', ')}`
            : 'Enables nothing on its own'}>
            <span class="cc-feature-name">{f.name}</span>
            {#if f.default}<Badge variant="tone" tone="success" size="sm" label="default" />{/if}
            {#if f.enables.length}
              <span class="cc-hint">→ {f.enables.join(', ')}</span>
            {/if}
          </div>
        {/each}
      </SidebarSection>
    {/if}
  </div>
</SidebarSection>

<style>
  .cc-body { display: flex; flex-direction: column; }
  .cc-meta { display: inline-flex; align-items: center; gap: 5px; }
  .cc-version {
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .cc-ic { color: var(--text-muted); display: flex; }
  .cc-run { color: var(--success); display: flex; }
  .cc-hint {
    font-size: var(--font-size-3xs); color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 22ch;
  }
  .cc-missing { color: var(--warning); }
  /* A label inside a section, for the target kinds. Not a nested SidebarSection: four collapsible
     groups inside a collapsible group inside a collapsible group is a tree nobody can navigate. */
  .cc-group {
    margin: 4px 0 1px; padding: 0 8px;
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.04em;
    color: var(--text-disabled);
  }
  .cc-feature {
    display: flex; align-items: center; gap: 6px;
    padding: 3px 8px; min-width: 0;
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .cc-feature-name { font-family: var(--font-code); color: var(--text-primary); flex-shrink: 0; }
</style>
