<script lang="ts">
  /**
   * The run in progress: what it is doing now, the tool's own output as it streams, and — when it
   * fails — the reason, with a way back to the form that keeps everything typed.
   */
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import LogStream from '$lib/components/shared/ui/LogStream.svelte';
  import { newProjectStore as store } from '$lib/stores/bennu/new-project.svelte';

  let follow = $state(true);
</script>

<div class="run">
  {#if store.phase === 'running'}
    <div class="now">
      <Spinner size="sm" />
      <div>
        <strong>Creating {store.name.trim()}</strong>
        <span class="stage">{store.stage}</span>
      </div>
    </div>
    <ProgressBar indeterminate height={3} ariaLabel="Creating the project" />
  {:else if store.phase === 'error'}
    <Alert variant="error" title="The project could not be created" text={store.runError ?? 'Unknown error'}>
      {#snippet actions()}
        <Button variant="outline" size="sm" onclick={() => store.backToForm()}>Back to the form</Button>
      {/snippet}
    </Alert>
    <p class="note">Nothing was left behind: the working folder and any folders this run created have been removed.</p>
  {/if}

  <div class="log">
    <LogStream lines={store.log} bind:autoScroll={follow} emptyMessage="Waiting for output…" waiting={store.phase === 'running' && store.log.length === 0} />
  </div>
</div>

<style>
  .run { display: flex; flex-direction: column; gap: 10px; padding: 16px 18px; height: 100%; min-height: 0; box-sizing: border-box; }
  .now { display: flex; align-items: center; gap: 10px; font-size: var(--font-size-md); color: var(--text-primary); }
  .stage { display: block; font-size: var(--font-size-xs); color: var(--text-muted); }
  .note { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .log {
    flex: 1; min-height: 0; overflow: hidden;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
  }
</style>
