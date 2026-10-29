<script lang="ts">
  /**
   * What a run did with live processes, as a card at the end of the message: what it started and
   * stopped, what is still running, the requests it sent (each opens the timeline filtered to that
   * request), the scenarios and assertions, the files it changed, and what the run cost.
   */
  import { Activity, Route } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuLiveStore } from '$lib/stores/bennu/live.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { formatCost } from './assistant-format';
  import type { LiveSummary } from './assistant-types';

  let { summary }: { summary: LiveSummary } = $props();

  const failedAssertions = $derived(summary.assertions.failed);
  const bevyChanges = $derived(summary.bevy_changes ?? []);

  function trace(corr: string) {
    bennuUiStore.showBottom('live');
    void bennuLiveStore.followCorrelation(corr);
  }

  const statusTone = (s: number | null) => (s === null ? 'error' : s >= 500 ? 'error' : s >= 400 ? 'warning' : 'success');
</script>

<div class="ls">
  <div class="head">
    <Activity size={13} />
    <span class="title">Live run</span>
    {#if summary.cost !== null}<span class="cost">{formatCost(summary.cost)}</span>{/if}
  </div>

  <div class="chips">
    {#if summary.started.length}<Badge variant="tone" size="sm" tone="info" label={`started ${summary.started.join(', ')}`} />{/if}
    {#if summary.stopped.length}<Badge variant="tone" size="sm" tone="neutral" label={`stopped ${summary.stopped.join(', ')}`} />{/if}
    {#if summary.still_running.length}<Badge variant="tone" size="sm" dot tone="warning" label={`still running: ${summary.still_running.join(', ')}`} />{/if}
    {#if summary.request_count}
      <Badge variant="tone" size="sm" tone={summary.requests_ok === summary.request_count ? 'success' : 'warning'} label={`${summary.requests_ok} of ${summary.request_count} requests OK`} />
    {/if}
    {#each summary.scenarios as s (s.name)}
      <Badge variant="tone" size="sm" tone={s.passed ? 'success' : 'error'} label={`${s.name}: ${s.passed ? 'passed' : `${s.failed} of ${s.steps} steps failed`}`} />
    {/each}
    {#if summary.assertions.passed + failedAssertions > 0}
      <Badge variant="tone" size="sm" tone={failedAssertions ? 'error' : 'success'} label={`${summary.assertions.passed} assertions passed, ${failedAssertions} failed`} />
    {/if}
    {#if summary.files.length}<Badge variant="tone" size="sm" tone="accent" label={`${summary.files.length} file${summary.files.length === 1 ? '' : 's'} changed`} />{/if}
    {#if bevyChanges.length}<Badge variant="tone" size="sm" tone="warning" label={`${summary.bevy_change_count ?? bevyChanges.length} change${(summary.bevy_change_count ?? bevyChanges.length) === 1 ? '' : 's'} to the running game`} />{/if}
  </div>

  {#if summary.requests.length}
    <Collapsible chevron>
      {#snippet header()}<span class="sub">Requests ({summary.request_count})</span>{/snippet}
      <ul class="reqs">
        {#each summary.requests as r (r.corr)}
          <li>
            <Badge variant="tone" size="sm" tone={statusTone(r.status)} label={r.status === null ? 'failed' : String(r.status)} />
            <code>{r.method} {r.url}</code>
            {#if r.ms !== null}<span class="sub">{r.ms} ms</span>{/if}
            <IconButton tooltip="Show everything this request touched" size={18} onclick={() => trace(r.corr)}><Route size={11} /></IconButton>
          </li>
        {/each}
      </ul>
    </Collapsible>
  {/if}
  {#if bevyChanges.length}
    <Collapsible chevron>
      {#snippet header()}<span class="sub">Changes to the running game (no undo)</span>{/snippet}
      <ul class="files">{#each bevyChanges as c, i (i)}<li><code>{c}</code></li>{/each}</ul>
    </Collapsible>
  {/if}
  {#if summary.files.length}
    <Collapsible chevron>
      {#snippet header()}<span class="sub">Files changed</span>{/snippet}
      <ul class="files">{#each summary.files as f (f)}<li><code>{f}</code></li>{/each}</ul>
    </Collapsible>
  {/if}
</div>

<style>
  .ls { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-elevated); }
  .head { display: flex; align-items: center; gap: 7px; color: var(--accent); }
  .title { flex: 1; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .cost { font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; color: var(--text-muted); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .sub { font-size: var(--font-size-xs); color: var(--text-muted); }
  .reqs, .files { list-style: none; margin: 4px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .reqs li { display: flex; align-items: center; gap: 8px; min-width: 0; }
  code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
</style>
