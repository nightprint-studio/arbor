<script lang="ts">
  /** Hirundo footer: the project, its workspace and the environment — the answer's status and time are in the response panel, not repeated here. */
  import type { Snippet } from 'svelte';
  import StatusBarItem from '$lib/components/shared/ui/StatusBarItem.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';

  let { footerExtra }: { footerExtra?: Snippet } = $props();
</script>

<div class="hf">
  {#if hirundoStore.project}
    <StatusBarItem tooltip={hirundoStore.project}>{hirundoStore.project.split(/[\\/]/).filter(Boolean).pop()}</StatusBarItem>
    <StatusBarItem>{hirundoStore.tree.length} {hirundoStore.tree.length === 1 ? 'collection' : 'collections'}</StatusBarItem>
  {:else}
    <StatusBarItem>No project open</StatusBarItem>
  {/if}
  <span class="hf-spacer"></span>
  {#if hirundoStore.activeWorkspace}<StatusBarItem tooltip="Workspace">{hirundoStore.activeWorkspace.name}</StatusBarItem>{/if}
  {#if hirundoStore.project}<StatusBarItem tooltip="Environment the next request goes to">{hirundoStore.activeEnv?.name ?? 'No environment'}</StatusBarItem>{/if}
  {#if footerExtra}{@render footerExtra()}{/if}
</div>

<style>
  .hf {
    display: flex; align-items: center; gap: 4px; height: 26px; flex-shrink: 0; padding: 0 8px;
    background: var(--bg-elevated); font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .hf-spacer { flex: 1; min-width: 0; }
</style>
