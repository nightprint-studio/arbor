<script lang="ts">
  /**
   * One project of the build, in the Gradle tool window.
   *
   * Two groups, in the order the questions come up: the **tasks** you drive the build from, and the
   * **plugins** the script applies. Its own file for the reason `BennuMavenModuleSection` is — the
   * panel would otherwise be one component holding three nested loops — and so the build tools'
   * windows stay recognisably the same shape.
   *
   * ## What a row does
   *
   * A task row runs that task **in this project**: `test` on `:orders` runs `:orders:test`. The root
   * row runs the bare task name, which Gradle applies to every project that has it — the whole build,
   * which is what pressing `build` on the root means.
   *
   * A plugin row is not a thing to run: it opens the build script at the line that applies it.
   *
   * The task list is what the build files *say* (the plugins' known tasks and the ones the script
   * declares itself), not `gradle tasks --all`: a plugin the table has never heard of — a company
   * convention plugin — adds tasks this list cannot show.
   */
  import { Boxes, FileCode2, LocateFixed, Play, Puzzle } from 'lucide-svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { openBuildUnitMenu } from './build-unit-menu';
  import { gradleKeys } from './tree-expansion';
  import type { ExpansionView } from '$lib/stores/bennu/tree-expansion.svelte';
  import type { GradleModule, GradleTask } from '$lib/ipc/bennu/gradle';

  let {
    module,
    /** The build's name — what the root project is called, since its path is just `:`. */
    buildName,
    /** Absolute path of the project's directory, for "focus in the Project tree". */
    dirPath,
    /** Runs a task in this project. The panel owns the root and the options; the section only says
     *  what to run. */
    onRun,
    /** Opens a file, optionally at a line — the build script, where a plugin is applied. */
    onOpen,
    expanded,
    onToggle,
    /** The Gradle window's remembered rows for this project. */
    expansion,
  }: {
    module: GradleModule;
    buildName: string;
    dirPath: string;
    onRun: (tasks: string[]) => void;
    onOpen: (path: string, line?: number) => void;
    expanded: boolean;
    onToggle: () => void;
    expansion: ExpansionView;
  } = $props();

  const isRoot = $derived(module.path === ':');
  const title = $derived(isRoot ? buildName || module.name : module.name);
  /** `jar` is what a JVM project is unless it says otherwise, so it is not worth a word. */
  const packaging = $derived(module.packaging && module.packaging !== 'jar' ? module.packaging : '');
  const hasScript = $derived(module.build_file !== '');

  // What each group draws when nobody has touched it: tasks are what most presses come from, plugins
  // are a list you go looking in.
  const TASKS_OPEN_BY_DEFAULT = true;
  const PLUGINS_OPEN_BY_DEFAULT = false;

  /** The order the groups are read in: the build first, `help` last. */
  const GROUP_ORDER = ['build', 'verification', 'application', 'documentation', 'publishing', 'other', 'help'];

  const tasks = $derived.by<GradleTask[]>(() => {
    const rank = (g: string) => {
      const i = GROUP_ORDER.indexOf(g);
      return i < 0 ? GROUP_ORDER.length : i;
    };
    // Stable: tasks keep the order the backend gave them inside a group.
    return [...module.tasks].sort((a, b) => rank(a.group) - rank(b.group));
  });

  const unit = $derived({ name: title, manifest: module.build_file });
</script>

<SidebarSection
  label={title}
  {expanded}
  {onToggle}
  badge={module.tasks.length || null}
  badgeTitle="Tasks this project offers"
  onContextMenu={hasScript ? (x, y) => openBuildUnitMenu(x, y, unit) : undefined}
>
  {#snippet icon()}<Boxes size={13} />{/snippet}
  {#snippet actions()}
    <span class="gm-meta">
      <IconButton tooltip="Focus this project in the Project tree" size={20} onclick={() => bennuUiStore.focusInTree(dirPath)}>
        <LocateFixed size={11} />
      </IconButton>
      {#if hasScript}
        <IconButton tooltip="Open its build script" size={20} onclick={() => onOpen(module.build_file)}>
          <FileCode2 size={11} />
        </IconButton>
      {/if}
      {#if packaging}<Badge variant="tone" tone="neutral" size="sm" label={packaging} />{/if}
      {#if !isRoot}
        <span class="gm-dir" use:tooltip={`Gradle path ${module.path}`}>{module.path}</span>
      {/if}
    </span>
  {/snippet}

  <SidebarSection
    label="Tasks"
    expanded={expansion.isOpen(gradleKeys.tasks(module), TASKS_OPEN_BY_DEFAULT)}
    onToggle={() => expansion.toggle(gradleKeys.tasks(module), TASKS_OPEN_BY_DEFAULT)}
    badge={tasks.length}
  >
    {#snippet icon()}<Play size={13} />{/snippet}
    {#if tasks.length === 0}
      <p class="gm-empty">
        This project applies no plugin Bennu knows the tasks of. Its tasks are still there — run
        <code>gradle tasks</code> in a terminal.
      </p>
    {/if}
    {#each tasks as task (task.id)}
      <SidebarItem onclick={() => onRun([task.id])}>
        {#snippet icon()}<span class="gm-run"><Play size={11} /></span>{/snippet}
        {task.id}
        {#snippet badges()}<span class="gm-hint">{task.hint}</span>{/snippet}
      </SidebarItem>
    {/each}
    <!-- The one combination nobody types out, because it is the one everybody runs. -->
    {#if tasks.some((t) => t.id === 'clean') && tasks.some((t) => t.id === 'build')}
      <SidebarItem onclick={() => onRun(['clean', 'build'])}>
        {#snippet icon()}<span class="gm-run"><Play size={11} /></span>{/snippet}
        clean build
        {#snippet badges()}<span class="gm-hint">Both, in one run</span>{/snippet}
      </SidebarItem>
    {/if}
  </SidebarSection>

  <SidebarSection
    label="Plugins"
    expanded={expansion.isOpen(gradleKeys.plugins(module), PLUGINS_OPEN_BY_DEFAULT)}
    onToggle={() => expansion.toggle(gradleKeys.plugins(module), PLUGINS_OPEN_BY_DEFAULT)}
    badge={module.plugins.length}
  >
    {#snippet icon()}<Puzzle size={13} />{/snippet}
    {#if module.plugins.length === 0}
      <p class="gm-empty">This build script applies no plugins of its own.</p>
    {/if}
    {#each module.plugins as plugin (`${plugin.id}@${plugin.line}`)}
      <SidebarItem onclick={() => onOpen(module.build_file, plugin.line)}>
        {#snippet icon()}<span class="gm-plugin"><Puzzle size={12} /></span>{/snippet}
        {plugin.id}
        {#snippet badges()}
          <span class="gm-hint">{plugin.apply ? plugin.version : `${plugin.version ? `${plugin.version} · ` : ''}not applied here`}</span>
        {/snippet}
      </SidebarItem>
    {/each}
  </SidebarSection>
</SidebarSection>

<style>
  .gm-meta { display: flex; align-items: center; gap: 4px; }
  .gm-dir {
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gm-run { color: var(--success); display: flex; }
  .gm-plugin { color: var(--text-muted); display: flex; }
  .gm-hint {
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    max-width: 190px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gm-empty {
    margin: 0;
    padding: 4px 8px 6px 26px;
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
  }
</style>
