<script lang="ts">
  /**
   * RunnerPanel — run a collection (or one folder of it) against the active environment and read which
   * requests passed. A protected environment is confirmed once, up front, for the whole run (that is
   * `hirundoCommands.runContainer`, also reached from the tree's menu and the palette).
   */
  import { Play, Square } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { hirundoRunner } from '$lib/stores/hirundo/runner.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands } from '../hirundo-commands';
  import { methodColor, methodShort } from '../request-model';
  import type { Node } from '$lib/types/hirundo';

  let target = $state('');

  /** Collections and their folders, as pickable targets. */
  const targets = $derived.by(() => {
    const out: { value: string; label: string }[] = [];
    const walk = (nodes: Node[], trail: string) => {
      for (const n of nodes) if (n.kind === 'folder') { out.push({ value: n.path, label: `${trail} / ${n.name}` }); walk(n.children, `${trail} / ${n.name}`); }
    };
    for (const c of hirundoStore.tree) { out.push({ value: c.path, label: c.name }); walk(c.children, c.name); }
    return out;
  });
  const chosen = $derived(target || targets[0]?.value || '');
</script>

<div class="rp">
  {#if !hirundoStore.project || targets.length === 0}
    <EmptyState message="Nothing to run" description="Create a collection with a few requests first." />
  {:else}
    <div class="rp-top">
      <div class="rp-select"><Select value={chosen} options={targets} size="sm" onchange={(v) => (target = v)} /></div>
      {#if hirundoRunner.running}
        <Button variant="danger" size="sm" onclick={() => hirundoRunner.stop()}>{#snippet iconStart()}<Square size={12} />{/snippet}Stop</Button>
      {:else}
        <Button variant="primary" size="sm" onclick={() => hirundoCommands.runContainer(chosen)}>{#snippet iconStart()}<Play size={12} />{/snippet}Run</Button>
      {/if}
      <span class="rp-muted">against <b>{hirundoStore.activeEnv?.name ?? 'no environment'}</b></span>
      {#if hirundoRunner.rows.length || hirundoRunner.running}
        <span class="rp-spacer"></span>
        <Badge variant="tone" tone="success" label={`${hirundoRunner.passed} ok`} />
        {#if hirundoRunner.failed}<Badge variant="tone" tone="error" label={`${hirundoRunner.failed} ko`} />{/if}
        <span class="rp-muted">{hirundoRunner.rows.length}/{hirundoRunner.total} · {(hirundoRunner.elapsedMs / 1000).toFixed(1)} s</span>
      {/if}
    </div>
    {#if hirundoRunner.running}<div class="rp-bar"><ProgressBar value={hirundoRunner.total ? (hirundoRunner.rows.length / hirundoRunner.total) * 100 : 0} /></div>{/if}

    {#if hirundoRunner.rows.length === 0 && !hirundoRunner.running}
      <EmptyState message="No run yet" description="Pick a collection or folder and press Run. Results stay in the History tab." compact />
    {:else}
      <ul class="rp-rows">
        {#each hirundoRunner.rows as r (r.path)}
          <li>
            <Badge variant="status" size="sm" tone={r.passed ? 'success' : 'error'} label={r.passed ? 'PASS' : 'FAIL'} />
            <span class="rp-m" style:color={methodColor(r.method)}>{methodShort(r.method)}</span>
            <Button variant="link" size="xs" onclick={() => void hirundoStore.openRequest(r.path, true)}>{r.name}</Button>
            <span class="rp-muted">{r.status ?? '—'}{r.time_ms != null ? ` · ${r.time_ms} ms` : ''}</span>
            {#if r.error}<span class="rp-err">{r.error}</span>{/if}
            {#each r.checks.filter((c) => !c.passed) as c, i (i)}<span class="rp-err">{c.line}{c.detail ? ` — ${c.detail}` : ''}</span>{/each}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .rp { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  .rp-top { display: flex; gap: 8px; padding: 8px 12px; align-items: center; flex-wrap: wrap; }
  .rp-select { flex: 1; min-width: 160px; }
  .rp-spacer { flex: 1; }
  .rp-bar { padding: 0 12px 6px; }
  .rp-muted { font-size: var(--font-size-xs); color: var(--text-muted); }
  .rp-rows { list-style: none; margin: 0; padding: 0 12px 8px; overflow: auto; flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 2px; }
  .rp-rows li { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 3px 4px; border-bottom: 1px solid var(--border-subtle); }
  .rp-m { font: 600 10px var(--font-mono); width: 28px; }
  .rp-err { flex-basis: 100%; font-size: var(--font-size-xs); color: var(--error); padding-left: 4px; }
</style>
