<script lang="ts">
  /**
   * EnvironmentsPanel — the project's environments grouped by their type, with the active one marked.
   * Click switches; "Manage…" opens the editor for environments, types and this machine's own values.
   */
  import { Check } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands } from '../hirundo-commands';

  const groups = $derived(
    hirundoStore.envs.types.map((t) => ({ type: t, envs: hirundoStore.envs.environments.filter((e) => e.type_id === t.id) }))
      .concat([{ type: { id: '', name: 'No type', color: 'var(--text-muted)', confirm_mutating: false, vars: [] }, envs: hirundoStore.envs.environments.filter((e) => !hirundoStore.envs.types.some((t) => t.id === e.type_id)) }])
      .filter((g) => g.envs.length),
  );
</script>

<div class="ep">
  {#if !hirundoStore.project}
    <EmptyState message="No project open" />
  {:else}
    <div class="ep-top"><Button variant="outline" size="sm" onclick={hirundoCommands.environments}>Manage…</Button></div>
    {#if groups.length === 0}
      <EmptyState message="No environments yet" description="Create one for each place the API runs: your machine, staging, production." compact />
    {/if}
    {#each groups as g (g.type.id)}
      <div class="ep-group">
        <span class="ep-dot" style:background={g.type.color}></span>{g.type.name}
        {#if g.type.confirm_mutating}<Badge variant="tone" tone="warning" size="sm" label="protected" />{/if}
      </div>
      {#each g.envs as e (e.id)}
        <SidebarItem selected={e.id === hirundoStore.envs.local.active_env} onclick={() => void hirundoStore.selectEnvironment(e.id)} ondblclick={hirundoCommands.environments}>
          {#snippet icon()}{#if e.id === hirundoStore.envs.local.active_env}<Check size={13} />{:else}<span class="ep-gap"></span>{/if}{/snippet}
          {e.name}
        </SidebarItem>
      {/each}
    {/each}
  {/if}
</div>

<style>
  .ep { overflow: auto; padding: 0 4px 8px; flex: 1; min-height: 0; }
  .ep-top { padding: 0 8px 8px; }
  .ep-group { display: flex; align-items: center; gap: 6px; padding: 8px 8px 2px; font-size: var(--font-size-xs); text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .ep-dot { width: 8px; height: 8px; border-radius: 50%; }
  .ep-gap { width: 13px; display: inline-block; }
</style>
