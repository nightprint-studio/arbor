<script lang="ts">
  /** Hirundo footer: the project and the open request's last answer. */
  import type { Snippet } from 'svelte';
  import StatusBarItem from '$lib/components/shared/ui/StatusBarItem.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';

  let { footerExtra }: { footerExtra?: Snippet } = $props();

  const response = $derived(hirundoStore.active?.response ?? null);
  const tone = $derived(!response ? 'default' : response.status >= 400 ? 'warning' : 'accent');
</script>

<div class="hf">
  {#if hirundoStore.project}
    <StatusBarItem tooltip={hirundoStore.project}>{hirundoStore.project.split(/[\\/]/).filter(Boolean).pop()}</StatusBarItem>
    <StatusBarItem>{hirundoStore.tree.length} {hirundoStore.tree.length === 1 ? 'collection' : 'collections'}</StatusBarItem>
  {:else}
    <StatusBarItem>No project open</StatusBarItem>
  {/if}
  <span class="hf-spacer"></span>
  {#if response}
    <StatusBarItem {tone}>{response.status} {response.status_text} · {response.time_ms} ms</StatusBarItem>
  {/if}
  {#if footerExtra}{@render footerExtra()}{/if}
</div>

<style>
  .hf {
    display: flex; align-items: center; gap: 4px; height: 26px; flex-shrink: 0; padding: 0 8px;
    background: var(--bg-elevated); font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .hf-spacer { flex: 1; min-width: 0; }
</style>
