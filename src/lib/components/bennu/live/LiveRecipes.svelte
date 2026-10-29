<script lang="ts">
  /**
   * The project's recipes — named stacks of services from `.arbor/bennu/live/recipes.toml`.
   *
   * Running one starts its services in dependency order and waits for each to be ready. The first run
   * of a recipes file (and the first after it changes) shows the commands and asks: the approval is
   * bound to the file's content, so an edited command is asked about again.
   */
  import { FilePlus, FileText, Play, Square } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Card from '$lib/components/shared/ui/Card.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { bennuLiveStore as live } from '$lib/stores/bennu/live.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import type { LiveRecipe, LiveReadySpec } from '$lib/ipc/bennu/live';

  const recipes = $derived(live.state?.recipes ?? null);
  let confirming = $state<LiveRecipe | null>(null);

  const running = (r: LiveRecipe) =>
    (live.state?.processes ?? []).some((p) => p.group === r.name && (p.phase === 'running' || p.phase === 'ready'));

  function ready(spec: LiveReadySpec): string {
    const parts = [
      spec.port ? `port ${spec.port}` : '',
      spec.log ? `log /${spec.log}/` : '',
      spec.http ? `GET ${spec.http}` : '',
      spec.delay_ms ? `after ${spec.delay_ms} ms` : '',
    ].filter(Boolean);
    return parts.length ? parts.join(' + ') : 'immediately';
  }

  function run(r: LiveRecipe) {
    if (recipes?.approved) void live.runRecipe(r.name, false);
    else confirming = r;
  }

  async function createFile() {
    const path = await live.createTemplate('recipes');
    if (path) await projectStore.openFile(path);
  }

  const commands = (r: LiveRecipe) => r.services.map((s) => `${s.name}: $ ${s.command}  (in ${s.cwd})`).join('\n');
</script>

<div class="rc">
  {#if !recipes?.present}
    <EmptyState message="No recipes yet" description="A recipe starts a stack of services — ports, readiness, dependencies — in one go." />
    <div class="rc-center">
      <Button variant="primary" size="sm" onclick={() => void createFile()}>
        {#snippet iconStart()}<FilePlus size={13} />{/snippet}
        Create recipes file
      </Button>
    </div>
  {:else}
    {#if recipes.errors.length}
      <Alert variant="error" title="The recipes file has errors" compact>
        {#each recipes.errors as e (e)}<div>{e}</div>{/each}
      </Alert>
    {/if}
    <div class="rc-bar">
      <Badge variant="tone" size="sm" dot tone={recipes.approved ? 'success' : 'warning'} label={recipes.approved ? 'approved' : 'asks before running'} />
      <Button variant="ghost" size="sm" onclick={() => void projectStore.openFile(recipes.path)}>
        {#snippet iconStart()}<FileText size={13} />{/snippet}
        Open recipes file
      </Button>
    </div>
    <div class="rc-list">
      {#each recipes.recipes as r (r.name)}
        <Card>
          <div class="rc-head">
            <strong>{r.name}</strong>
            {#if r.description}<span class="muted">{r.description}</span>{/if}
            <span class="rc-actions">
              {#if running(r)}
                <Button variant="ghost" size="sm" onclick={() => void live.stopRecipe(r.name)}>
                  {#snippet iconStart()}<Square size={12} />{/snippet}
                  Stop
                </Button>
              {/if}
              <Button variant="primary" size="sm" disabled={recipes.errors.length > 0} onclick={() => run(r)}>
                {#snippet iconStart()}<Play size={12} />{/snippet}
                Run
              </Button>
            </span>
          </div>
          <ul class="svc">
            {#each r.services as s (s.name)}
              <li>
                <span class="svc-name">{s.name}</span>
                <code>{s.command}</code>
                <span class="muted">
                  {s.depends_on.length ? `after ${s.depends_on.join(', ')} · ` : ''}{s.ports.length ? `:${s.ports.join(', :')} · ` : ''}ready {ready(s.ready)}
                </span>
              </li>
            {/each}
          </ul>
        </Card>
      {:else}
        <EmptyState compact message="The file defines no recipe" />
      {/each}
    </div>
  {/if}
</div>

{#if confirming}
  {@const target = confirming}
  <ConfirmModal
    title={`Run recipe “${target.name}”?`}
    message="These commands will run on your machine, in the project:"
    detail={commands(target)}
    variant="warning"
    confirmLabel="Approve and run"
    onConfirm={() => { confirming = null; void live.runRecipe(target.name, true); }}
    onCancel={() => (confirming = null)}
  />
{/if}

<style>
  .rc { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow-y: auto; gap: 8px; padding: 8px 10px; }
  .rc-center { display: flex; justify-content: center; }
  .rc-bar { display: flex; align-items: center; gap: 8px; }
  .rc-list { display: flex; flex-direction: column; gap: 8px; }
  .rc-head { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .rc-actions { margin-left: auto; display: flex; gap: 6px; }
  .muted { color: var(--text-muted); font-size: var(--font-size-xs); }
  .svc { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .svc li { display: flex; align-items: baseline; gap: 10px; min-width: 0; flex-wrap: wrap; }
  .svc-name { font-weight: 600; min-width: 90px; }
  code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
</style>
