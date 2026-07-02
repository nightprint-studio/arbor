<script lang="ts">
  /**
   * Maven (right tool window) — a MOCK Maven tool panel: the standard lifecycle
   * goals + a static plugins list. No backend wiring yet (bennu-be doesn't drive
   * Maven), so every goal shows a "not implemented yet" toast on activation. The
   * panel is clearly marked as a sketch so it isn't mistaken for a live tool.
   *
   * Reuses shared/ui only (PanelShell, SidebarSection, SidebarItem, EmptyState).
   */
  import { Hammer, Play, Puzzle, FolderGit2 } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  // MOCK — the standard Maven default-lifecycle phases.
  const LIFECYCLE = [
    { id: 'clean',    label: 'clean',    hint: 'Delete target/' },
    { id: 'validate', label: 'validate', hint: 'Validate the project' },
    { id: 'compile',  label: 'compile',  hint: 'Compile the source' },
    { id: 'test',     label: 'test',     hint: 'Run unit tests' },
    { id: 'package',  label: 'package',  hint: 'Build the JAR/WAR' },
    { id: 'verify',   label: 'verify',   hint: 'Run checks on the package' },
    { id: 'install',  label: 'install',  hint: 'Install to ~/.m2' },
  ] as const;

  // MOCK — a representative static plugin list. Replaced by the real reactor once
  // bennu-be resolves the effective POM.
  const PLUGINS = [
    'maven-compiler-plugin',
    'maven-surefire-plugin',
    'maven-war-plugin',
    'maven-resources-plugin',
  ];

  let lifecycleOpen = $state(true);
  let pluginsOpen = $state(false);

  function runGoal(goal: string) {
    toastStore.show(`Maven “${goal}” isn't implemented yet.`, 'info');
  }
</script>

<PanelShell title="Maven">
  {#snippet icon()}<Hammer size={13} />{/snippet}

  {#if !projectStore.project}
    <EmptyState message="Open a project to see its Maven goals." />
  {:else}
    <div class="mv-mock">Sketch — goals aren't wired to a build yet.</div>
    <div class="mv">
      <SidebarSection
        label="Lifecycle"
        expanded={lifecycleOpen}
        onToggle={() => (lifecycleOpen = !lifecycleOpen)}
        badge={LIFECYCLE.length}
      >
        {#snippet icon()}<Play size={13} />{/snippet}
        {#each LIFECYCLE as g (g.id)}
          <SidebarItem onclick={() => runGoal(g.label)}>
            {#snippet icon()}<span class="mv-goal-ic"><Play size={11} /></span>{/snippet}
            {g.label}
            {#snippet badges()}<span class="mv-hint">{g.hint}</span>{/snippet}
          </SidebarItem>
        {/each}
      </SidebarSection>

      <SidebarSection
        label="Plugins"
        expanded={pluginsOpen}
        onToggle={() => (pluginsOpen = !pluginsOpen)}
        badge={PLUGINS.length}
      >
        {#snippet icon()}<Puzzle size={13} />{/snippet}
        {#each PLUGINS as p (p)}
          <SidebarItem onclick={() => runGoal(p)}>
            {#snippet icon()}<span class="mv-plugin-ic"><Puzzle size={11} /></span>{/snippet}
            {p}
          </SidebarItem>
        {/each}
      </SidebarSection>
    </div>
  {/if}
</PanelShell>

<style>
  .mv-mock {
    padding: 6px 12px;
    font-size: 10.5px; font-style: italic;
    color: var(--text-disabled);
    border-bottom: 1px solid var(--border-subtle);
  }
  .mv { padding: 4px 0; }
  .mv-goal-ic { color: var(--success); display: flex; }
  .mv-plugin-ic { color: var(--text-muted); display: flex; }
  .mv-hint { font-size: 10px; color: var(--text-disabled); }
</style>
