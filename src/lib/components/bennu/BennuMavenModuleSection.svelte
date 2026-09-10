<script lang="ts">
  /**
   * One module of the reactor, in the Maven tool window.
   *
   * Two groups, in the order the questions come up: the **lifecycle** you drive the build from,
   * and the **plugins** the pom configures. Its own file for the reason `BennuCargoCrateSection`
   * is — the panel would otherwise be one component holding three nested loops — and so the two
   * build tools' windows stay recognisably the same shape.
   *
   * The header answers the question the groups cannot: **where the module is**, asked constantly
   * on a reactor of forty. A button, and the right-click menu shared with the Cargo and
   * Dependencies panels (`build-unit-menu`).
   *
   * ## What a row does
   *
   * A lifecycle row runs that phase **in this module's directory**, which is what makes the row
   * mean what it says: `install` on `orders` installs `orders`. The root row is the whole reactor,
   * because that is what the root pom's build is.
   *
   * A plugin row is not always a thing to run, and the panel does not pretend otherwise. A plugin
   * that binds goals lists them, and each one runs. A plugin that only carries `<configuration>`
   * has nothing to press — it runs because a lifecycle phase calls it — so its row opens the pom
   * where it is declared instead. That is most plugins, and calling it out is the difference
   * between a panel that reads a pom and a panel that guesses.
   */
  import { Boxes, FileCode2, LocateFixed, Play, Puzzle } from 'lucide-svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { buildUnitDir, openBuildUnitMenu } from './build-unit-menu';
  import type { MavenModule, MavenPhase, MavenPlugin } from '$lib/ipc/bennu/maven-build';

  let {
    module,
    phases,
    /** Runs goals in this module. The panel owns the root and the options; the section only says
     *  what to run. */
    onRun,
    /** Opens a file, optionally at a line — the pom, where a plugin is declared. */
    onOpen,
    expanded,
    onToggle,
  }: {
    module: MavenModule;
    phases: MavenPhase[];
    onRun: (goals: string[]) => void;
    onOpen: (path: string, line?: number) => void;
    expanded: boolean;
    onToggle: () => void;
  } = $props();

  /** The name a person uses: the pom's `<name>` when it gives one, else the artifactId. */
  const title = $derived(module.name.trim() || module.artifact_id);
  /** The root module carries no directory, and saying "." would be noise. */
  const isRoot = $derived(module.dir === '');
  /** `jar` is Maven's default and therefore says nothing; `war`, `pom` and `ear` do. */
  const packaging = $derived(
    module.packaging.trim() && module.packaging.trim() !== 'jar' ? module.packaging.trim() : '',
  );

  const unit = $derived({ name: title, manifest: module.pom });

  let lifecycleOpen = $state(true);
  let pluginsOpen = $state(false);
  const pluginOpen = $state<Record<string, boolean>>({});

  function pluginKey(p: MavenPlugin): string {
    return `${p.group_id}:${p.artifact_id}`;
  }

  /** What a plugin row says about itself on the right. */
  function pluginHint(p: MavenPlugin): string {
    if (p.managed) return 'managed — not bound here';
    if (p.goals.length === 0) return 'configuration only';
    return p.version || '';
  }
</script>

<SidebarSection
  label={title}
  {expanded}
  {onToggle}
  badge={module.plugins.length || null}
  badgeTitle="Plugins this pom configures"
  onContextMenu={(x, y) => openBuildUnitMenu(x, y, unit)}
>
  {#snippet icon()}<Boxes size={13} />{/snippet}
  {#snippet actions()}
    <span class="mm-meta">
      <IconButton
        tooltip="Focus this module in the Project tree"
        size={20}
        onclick={() => bennuUiStore.focusInTree(buildUnitDir(unit))}
      >
        <LocateFixed size={11} />
      </IconButton>
      <IconButton tooltip="Open its pom.xml" size={20} onclick={() => onOpen(module.pom)}>
        <FileCode2 size={11} />
      </IconButton>
      {#if packaging}<Badge variant="tone" tone="neutral" size="sm" label={packaging} />{/if}
      {#if !isRoot}
        <span class="mm-dir" use:tooltip={`Maven runs in ${module.dir}`}>{module.dir}</span>
      {/if}
    </span>
  {/snippet}

  <SidebarSection
    label="Lifecycle"
    expanded={lifecycleOpen}
    onToggle={() => (lifecycleOpen = !lifecycleOpen)}
    badge={phases.length}
  >
    {#snippet icon()}<Play size={13} />{/snippet}
    {#each phases as phase (phase.id)}
      <SidebarItem onclick={() => onRun([phase.id])}>
        {#snippet icon()}<span class="mm-run"><Play size={11} /></span>{/snippet}
        {phase.id}
        {#snippet badges()}<span class="mm-hint">{phase.hint}</span>{/snippet}
      </SidebarItem>
    {/each}
    <!-- The one combination nobody types out, because it is the one everybody runs. -->
    <SidebarItem onclick={() => onRun(['clean', 'install'])}>
      {#snippet icon()}<span class="mm-run"><Play size={11} /></span>{/snippet}
      clean install
      {#snippet badges()}<span class="mm-hint">Both lifecycles, in one run</span>{/snippet}
    </SidebarItem>
  </SidebarSection>

  <SidebarSection
    label="Plugins"
    expanded={pluginsOpen}
    onToggle={() => (pluginsOpen = !pluginsOpen)}
    badge={module.plugins.length}
  >
    {#snippet icon()}<Puzzle size={13} />{/snippet}
    {#if module.plugins.length === 0}
      <p class="mm-empty">This pom configures no plugins of its own.</p>
    {/if}
    {#each module.plugins as plugin (pluginKey(plugin))}
      {#if plugin.goals.length > 0}
        <SidebarSection
          label={plugin.artifact_id}
          expanded={pluginOpen[pluginKey(plugin)] ?? false}
          onToggle={() =>
            (pluginOpen[pluginKey(plugin)] = !(pluginOpen[pluginKey(plugin)] ?? false))}
          badge={plugin.goals.length}
        >
          {#snippet icon()}<span class="mm-plugin"><Puzzle size={12} /></span>{/snippet}
          {#each plugin.goals as goal (goal)}
            <SidebarItem
              onclick={() =>
                onRun([plugin.prefix ? `${plugin.prefix}:${goal}` : `${plugin.group_id}:${plugin.artifact_id}:${goal}`])}
            >
              {#snippet icon()}<span class="mm-run"><Play size={11} /></span>{/snippet}
              {goal}
              {#snippet badges()}
                <span class="mm-hint">
                  {plugin.prefix ? `${plugin.prefix}:${goal}` : 'fully qualified'}
                </span>
              {/snippet}
            </SidebarItem>
          {/each}
        </SidebarSection>
      {:else}
        <SidebarItem onclick={() => onOpen(module.pom, plugin.line)}>
          {#snippet icon()}<span class="mm-plugin"><Puzzle size={12} /></span>{/snippet}
          {plugin.artifact_id}
          {#snippet badges()}<span class="mm-hint">{pluginHint(plugin)}</span>{/snippet}
        </SidebarItem>
      {/if}
    {/each}
  </SidebarSection>
</SidebarSection>

<style>
  .mm-meta { display: flex; align-items: center; gap: 4px; }
  .mm-dir {
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mm-run { color: var(--success); display: flex; }
  .mm-plugin { color: var(--text-muted); display: flex; }
  .mm-hint {
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    max-width: 190px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mm-empty {
    margin: 0;
    padding: 4px 8px 6px 26px;
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
  }
</style>
