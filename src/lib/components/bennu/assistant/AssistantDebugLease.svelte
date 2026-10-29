<script lang="ts">
  /**
   * "Started by the assistant (conversation) — ends in N min if unused", with a Stop button.
   *
   * A debug session the assistant opened outlives the message that opened it; this is where the user sees
   * that it is still there, how long it will wait, and ends it by hand. Shown beside the debugger's
   * transport controls.
   */
  import { onMount } from 'svelte';
  import { Bot } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { assistantDebugStore } from '$lib/stores/bennu/assistant-debug.svelte';

  onMount(() => { void assistantDebugStore.start(); });
</script>

{#if assistantDebugStore.actions.length > 0}
  {@const latest = assistantDebugStore.actions[assistantDebugStore.actions.length - 1]}
  <span class="lease" title={assistantDebugStore.actions.map((a) => a.text).join('\n')}>
    <Badge variant="tone" tone="warning" size="sm"><Bot size={10} /> Imber action</Badge>
    <span class="text">{latest.text}{assistantDebugStore.actions.length > 1 ? ` (+${assistantDebugStore.actions.length - 1} earlier)` : ''}</span>
  </span>
{/if}
{#each assistantDebugStore.sessions as s (s.conversation)}
  <span class="lease">
    <Badge variant="tone" tone="info" size="sm"><Bot size={10} /> Imber</Badge>
    <span class="text">“{s.title}” — ends in {s.endsInMin} min if unused</span>
    <Button variant="ghost" size="sm" onclick={() => void assistantDebugStore.stop(s.conversation)}>Stop</Button>
  </span>
{/each}

<style>
  .lease { display: inline-flex; align-items: center; gap: 6px; margin-left: 8px; }
  .text { font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 320px; }
</style>
