<script lang="ts">
  /**
   * BennuHttpPanel — the answer to the request run from a `.http` file. The same response viewer Hirundo uses
   * (Pretty · Raw · Headers · Tests, with status, time and size at the right of the tabs), the environment picked
   * here from the file's `http-client.env.json`, "run all", and what was sent. The sending is Hirundo's engine.
   */
  import { History, Play } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import ResponseViewer from '$lib/components/hirundo/response/ResponseViewer.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuHttp, isHttpFile } from '$lib/stores/bennu/http.svelte';
  import type { RequestTab } from '$lib/stores/hirundo/workspace.svelte';

  let historyOpen = $state(false);

  /** What the viewer reads, in the shape it expects of a Hirundo request tab. */
  const tab = $derived({
    sending: bennuHttp.running,
    response: bennuHttp.response,
    error: bennuHttp.error,
    unresolved: bennuHttp.unresolved,
    missingSecrets: [],
    report: bennuHttp.report,
  } as unknown as RequestTab);

  const file = $derived(projectStore.activeFilePath);
  const canRun = $derived(isHttpFile(file) && !bennuHttp.running);
  const shortName = (p: string | null) => p?.split(/[\\/]/).pop() ?? '';

  function runAll() {
    if (file) void bennuHttp.runAll(file, projectStore.sourceOf(file));
  }
</script>

<div class="bh">
  <PaneHeader title="HTTP" />
  <div class="bh-top">
    <div class="bh-env">
      <Select
        value={bennuHttp.env ?? ''}
        options={bennuHttp.envs.length ? bennuHttp.envs.map((e) => ({ value: e.name, label: e.name })) : [{ value: '', label: 'No environment' }]}
        size="sm"
        disabled={bennuHttp.envs.length === 0}
        onchange={(v) => bennuHttp.selectEnv(v || null)}
      />
    </div>
    {#if bennuHttp.protectedEnv}<Badge variant="tone" tone="warning" size="sm" tooltip="Asks before POST, PUT, PATCH and DELETE" label="protected" />{/if}
    <Button size="sm" variant="primary" disabled={!canRun} tooltip={{ content: 'Run every request of the file' }} onclick={runAll}>{#snippet iconStart()}<Play size={12} />{/snippet}Run all</Button>
  </div>
  {#if bennuHttp.name}<p class="bh-title">{bennuHttp.name} · {shortName(bennuHttp.file)}</p>{/if}

  <div class="bh-resp">
    {#if bennuHttp.response || bennuHttp.running || bennuHttp.error || bennuHttp.unresolved.length}
      <ResponseViewer {tab} />
    {:else}
      <EmptyState message="Nothing run yet" description="Open a .http file and press Ctrl+Enter on a request, or the ▶ beside it." compact />
    {/if}
  </div>

  <div class="bh-history">
    <Collapsible bind:open={historyOpen} chevron>
      {#snippet header()}<span class="bh-h"><History size={13} /> History · {bennuHttp.history.length}</span>{/snippet}
      {#each bennuHttp.history as h (h.at)}
        <SidebarItem>
          <span class="bh-m">{h.method}</span>{h.name}
          {#snippet badges()}<span class="bh-muted">{h.status ?? 'failed'}{h.time_ms != null ? ` · ${h.time_ms} ms` : ''}</span>{/snippet}
        </SidebarItem>
      {:else}
        <p class="bh-muted bh-pad">Requests sent from .http files appear here.</p>
      {/each}
    </Collapsible>
  </div>
</div>

{#if bennuHttp.pending}
  {@const p = bennuHttp.pending}
  <ConfirmModal
    title={`Send to ${p.env || 'a protected environment'}?`}
    message={p.name}
    detail="This environment asks for a confirmation before a request that changes things."
    variant="warning"
    confirmLabel="Send"
    onConfirm={() => void bennuHttp.confirmPending()}
    onCancel={() => bennuHttp.dismissPending()}
  />
{/if}

<style>
  .bh { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bh-top { display: flex; align-items: center; gap: 8px; padding: 6px 10px; }
  .bh-env { flex: 1; min-width: 0; }
  .bh-title { margin: 0; padding: 0 12px 4px; font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bh-resp { flex: 1; min-height: 0; display: flex; flex-direction: column; border-top: 1px solid var(--border-subtle); }
  .bh-resp > :global(*) { flex: 1; min-height: 0; }
  .bh-history { border-top: 1px solid var(--border-subtle); padding: 4px 8px; max-height: 40%; overflow: auto; }
  .bh-h { display: inline-flex; align-items: center; gap: 6px; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .bh-m { font: 600 10px var(--font-mono); margin-right: 6px; }
  .bh-muted { font-size: var(--font-size-xs); color: var(--text-muted); }
  .bh-pad { padding: 4px 8px; margin: 0; }
</style>
