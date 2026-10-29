<script lang="ts">
  /**
   * The project's scenarios — ordered steps with assertions (`.arbor/bennu/live/scenarios/*.toml`),
   * run without the assistant. The left list is the scenarios with their last result; the right pane
   * is the per-step result of the selected one, with the failed assertions and the request's
   * correlation id (which opens the timeline filtered to everything that request touched).
   *
   * ↑ ↓ choose a scenario, Enter runs it.
   */
  import { FilePlus, FileText, Play, Route } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ColumnPanes from '$lib/components/shared/ui/ColumnPanes.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { bennuLiveStore as live } from '$lib/stores/bennu/live.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import type { LiveScenario } from '$lib/ipc/bennu/live';
  import LiveLogView from './LiveLogView.svelte';
  import { durationText, eventClass, eventText, stepIndex } from './live-format';

  const scenarios = $derived(live.state?.scenarios ?? []);
  const selected = $derived(scenarios.find((s) => s.file === live.selectedScenario) ?? null);
  const recipes = $derived(live.state?.recipes ?? null);
  const result = $derived(live.scenarioResult);
  const busy = $derived(live.state?.scenario_running ?? null);
  let approving = $state<LiveScenario | null>(null);

  /** A scenario that starts a recipe needs the recipes file approved first. */
  const needsApproval = (s: LiveScenario) => s.recipes_needed.length > 0 && !recipes?.approved;

  function run(s: LiveScenario) {
    if (s.errors.length || busy) return;
    if (needsApproval(s)) approving = s;
    else void live.runScenario(s.file, false);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && selected) {
      e.preventDefault();
      run(selected);
      return;
    }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
    e.preventDefault();
    const next = stepIndex(scenarios.findIndex((s) => s.file === live.selectedScenario), scenarios.length, e.key);
    if (next >= 0) void live.selectScenario(scenarios[next].file);
  }

  async function createScenario() {
    const path = await live.createTemplate('scenario');
    if (path) await projectStore.openFile(path);
  }

  const openFile = (s: LiveScenario) => projectStore.openFile(`${live.state?.scenarios_dir ?? ''}/${s.file}.toml`);
  const tone = (o: string) => (o === 'passed' ? 'success' : o === 'failed' ? 'error' : 'neutral');
  const commands = (s: LiveScenario) =>
    (recipes?.recipes ?? []).filter((r) => s.recipes_needed.includes(r.name)).flatMap((r) => r.services.map((x) => `${x.name}: $ ${x.command}`)).join('\n');
</script>

<ColumnPanes leftWidth="340px">
  {#snippet left()}
    <PaneHeader title="Scenarios" count={scenarios.length}>
      {#snippet end()}
        <IconButton tooltip="New scenario" size={20} onclick={() => void createScenario()}><FilePlus size={12} /></IconButton>
      {/snippet}
    </PaneHeader>
    {#if scenarios.length === 0}
      <EmptyState compact message="No scenarios" description="A scenario is steps plus assertions — start a stack, call an endpoint, check a log line." />
    {:else}
      <div class="slist" role="listbox" tabindex="0" aria-label="Scenarios" onkeydown={onKeydown}>
        {#each scenarios as s (s.file)}
          <SidebarItem selected={s.file === live.selectedScenario} onclick={() => void live.selectScenario(s.file)} ondblclick={() => run(s)}>
            {s.name}
            {#snippet subtitle()}{s.errors.length ? 'invalid file' : `${s.steps} step${s.steps === 1 ? '' : 's'}${s.last ? ` · ${durationText(s.last.duration_ms)}` : ''}`}{/snippet}
            {#snippet badges()}
              {#if s.errors.length}<Badge variant="tone" size="sm" tone="error" label="invalid" />
              {:else if busy === s.name}<Badge variant="tone" size="sm" tone="info" dot label="running" />
              {:else if s.last}<Badge variant="tone" size="sm" tone={s.last.passed ? 'success' : 'error'} label={s.last.passed ? 'passed' : `${s.last.failed} failed`} />{/if}
            {/snippet}
          </SidebarItem>
        {/each}
      </div>
    {/if}
  {/snippet}
  {#snippet right()}
    {#if selected}
      <PaneHeader loading={busy === selected.name}>
        <span class="title">{selected.name}</span>
        {#if selected.description}<span class="desc">{selected.description}</span>{/if}
        {#snippet end()}
          <IconButton tooltip="Open the file" size={20} onclick={() => void openFile(selected)}><FileText size={12} /></IconButton>
          <Button variant="primary" size="xs" disabled={!!busy || selected.errors.length > 0} onclick={() => run(selected)}>
            {#snippet iconStart()}<Play size={11} />{/snippet}
            Run
          </Button>
        {/snippet}
      </PaneHeader>
      <div class="detail">
        {#if selected.errors.length}
          <Alert variant="error" title="This scenario file is not valid" compact>
            {#each selected.errors as e (e)}<div>{e}</div>{/each}
          </Alert>
        {:else if !result}
          <EmptyState compact message="Not run yet" description="Run it (Enter) to see each step's result and the timeline it produced." />
        {:else}
          <div class="summary">
            <Badge variant="tone" size="sm" tone={result.passed ? 'success' : 'error'} label={result.passed ? 'passed' : 'failed'} />
            <span class="desc">{durationText(result.duration_ms)}</span>
          </div>
          <ol class="steps">
            {#each result.steps as step (step.index)}
              <li>
                <div class="step-head">
                  <Badge variant="tone" size="sm" tone={tone(step.outcome)} label={step.outcome} />
                  <span class="step-name">{step.index + 1}. {step.name}</span>
                  <span class="desc">{step.duration_ms ? durationText(step.duration_ms) : ''}</span>
                  {#if step.corr}
                    <IconButton tooltip={`Show everything ${step.corr} touched`} size={18} onclick={() => void live.followCorrelation(step.corr!)}>
                      <Route size={11} />
                    </IconButton>
                  {/if}
                </div>
                {#if step.message}<div class="msg">{step.message}</div>{/if}
                {#each step.assertions.filter((a) => !a.passed) as a (a.description)}
                  <div class="fail">✗ {a.description} — {a.detail}</div>
                {/each}
                {#if step.outcome === 'failed' && step.slice.length}
                  <Collapsible open chevron>
                    {#snippet header()}<span class="desc">Timeline during this step ({step.slice.length})</span>{/snippet}
                    <div class="slice">
                      <LiveLogView lines={step.slice.map(eventText)} classes={step.slice.map(eventClass)} keys={step.slice.map((e) => e.seq)} />
                    </div>
                  </Collapsible>
                {/if}
              </li>
            {/each}
          </ol>
        {/if}
      </div>
    {:else}
      <EmptyState message="Select a scenario" />
    {/if}
  {/snippet}
</ColumnPanes>

{#if approving}
  {@const target = approving}
  <ConfirmModal
    title={`Run “${target.name}”?`}
    message="It starts these services on your machine, in the project:"
    detail={commands(target)}
    variant="warning"
    confirmLabel="Approve and run"
    onConfirm={() => { approving = null; void live.runScenario(target.file, true); }}
    onCancel={() => (approving = null)}
  />
{/if}

<style>
  .slist { overflow-y: auto; flex: 1; min-height: 0; outline: none; }
  .slist:focus-visible { box-shadow: inset 0 0 0 1px var(--accent); }
  .title { font-weight: 600; color: var(--text-primary); text-transform: none; letter-spacing: 0; }
  .desc { color: var(--text-muted); text-transform: none; letter-spacing: 0; font-size: var(--font-size-xs); }
  .detail { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 8px; }
  .summary { display: flex; align-items: center; gap: 8px; }
  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; }
  .step-head { display: flex; align-items: center; gap: 8px; }
  .step-name { color: var(--text-primary); font-weight: 500; }
  .msg { color: var(--text-secondary); font-size: var(--font-size-xs); margin: 2px 0 0 4px; }
  .fail { color: var(--danger); font-size: var(--font-size-xs); margin: 2px 0 0 4px; }
  .slice { height: 180px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); overflow: hidden; margin-top: 6px; }
</style>
