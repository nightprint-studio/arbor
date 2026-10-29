<script lang="ts">
  /**
   * The line that says the assistant is working, and on what.
   *
   * It sits at the end of the thread while a run is open: an orb with a ripple, the phrase for what is
   * happening now ("Thinking", "Reading", "Waiting for your answer") with a highlight travelling through it,
   * and the time and the step quietly beside it. It is not a progress bar, because the length of a run is not
   * known — it is a statement that the run is alive and what it is doing, which is what is worth knowing
   * while waiting.
   */
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { activityLabel, formatElapsed } from './assistant-activity';
  import { retryLine } from './assistant-retry';

  const store = bennuAssistantStore;

  let now = $state(Date.now());
  $effect(() => {
    if (!store.running) return;
    now = Date.now();
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const activity = $derived(store.activity);
  const retrying = $derived(store.retrying);
  const elapsed = $derived(store.runStartedAt ? formatElapsed(now - store.runStartedAt) : '');
</script>

{#if store.running}
  <div class="activity" role="status" aria-live="polite">
    <span class="asst-orb" aria-hidden="true"></span>
    {#if retrying}
      <!-- One calm line while the request is sent again: the wait is not an error and is not announced per attempt. -->
      <span class="label retry" title={retrying.detail}>{retryLine(retrying, now)}…</span>
      <Button variant="ghost" size="xs" tooltip="Skip the wait and send the request now" onclick={() => void store.retryNow()}>Retry now</Button>
      <Button variant="ghost" size="xs" tooltip="Stop the run" onclick={() => void store.cancel()}>Stop</Button>
    {:else}
      <span class="asst-shimmer label">{activity ? activityLabel(activity) : 'Starting'}…</span>
      <span class="meta">{elapsed}{#if activity && activity.iteration > 1} · step {activity.iteration}{/if}</span>
    {/if}
  </div>
{/if}

<style>
  .activity { display: flex; align-items: center; gap: 10px; padding: 2px 4px 2px 25px; }
  .label { font-size: var(--font-size-sm); font-weight: 500; }
  .retry { color: var(--warning); }
  .meta { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
