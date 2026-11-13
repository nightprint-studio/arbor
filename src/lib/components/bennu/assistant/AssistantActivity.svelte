<script lang="ts">
  /**
   * The line that says the assistant is working, and on what: Bennu's activity (phase, tool, step, elapsed
   * time, a retry in progress) in the shared Imber activity line.
   */
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import ImberActivityLine from '$lib/components/shared/internal/imber/ImberActivityLine.svelte';
  import { activityLabel, formatElapsed, progressLine } from './assistant-activity';
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
  // Only a tool reports progress; the step clock runs for every step (a long think is worth timing too).
  const progress = $derived(store.stepStartedAt ? progressLine(activity?.phase === 'tool' ? store.toolProgress : null, now - store.stepStartedAt) : '');
  const meta = $derived(`${elapsed}${activity && activity.iteration > 1 ? ` · step ${activity.iteration}` : ''}`);
</script>

{#if store.running}
  <ImberActivityLine
    label={activity ? activityLabel(activity) : 'Starting'}
    {progress}
    {meta}
    retry={retrying ? retryLine(retrying, now) : null}
    detail={retrying?.detail ?? ''}
    onRetryNow={() => void store.retryNow()}
    onStop={() => void store.cancel()}
  />
{/if}
