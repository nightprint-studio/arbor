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
  import { activityLabel, formatElapsed } from './assistant-activity';

  const store = bennuAssistantStore;

  let now = $state(Date.now());
  $effect(() => {
    if (!store.running) return;
    now = Date.now();
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  const activity = $derived(store.activity);
  const elapsed = $derived(store.runStartedAt ? formatElapsed(now - store.runStartedAt) : '');
</script>

{#if store.running}
  <div class="activity" role="status" aria-live="polite">
    <span class="asst-orb" aria-hidden="true"></span>
    <span class="asst-shimmer label">{activity ? activityLabel(activity) : 'Starting'}…</span>
    <span class="meta">{elapsed}{#if activity && activity.iteration > 1} · step {activity.iteration}{/if}</span>
  </div>
{/if}

<style>
  .activity { display: flex; align-items: center; gap: 10px; padding: 2px 4px 2px 25px; }
  .label { font-size: var(--font-size-sm); font-weight: 500; }
  .meta { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
