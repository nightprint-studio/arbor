<script lang="ts">
  /**
   * What you wrote, as the thread shows it: your words as they were typed, and each fenced block as a
   * highlighted code block (the same one the assistant's answers use) instead of a run of backticks.
   */
  import AssistantCode from './AssistantCode.svelte';
  import { inlineSpans, splitFences } from './assistant-fences';

  let { text }: { text: string } = $props();
  const segments = $derived(splitFences(text));
</script>

<div class="user-text">
  {#each segments as s}
    {#if s.kind === 'code'}
      <AssistantCode language={s.language || 'text'} code={s.code} />
    {:else}
      <p class="prose">{#each inlineSpans(s.text) as p}{#if p.code}<code>{p.text}</code>{:else if p.bold}<strong>{p.text}</strong>{:else}{p.text}{/if}{/each}</p>
    {/if}
  {/each}
</div>

<style>
  .user-text { display: flex; flex-direction: column; gap: 8px; min-width: 0; width: 100%; }
  .prose { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .prose code {
    font-family: var(--font-code); font-size: 0.92em;
    padding: 0 4px; border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-base) 55%, transparent);
  }
</style>
