<script lang="ts">
  /**
   * The Picus tool window — Bennu's view of a database, through the connections Picus defines.
   *
   * Three jobs, none of them a SQL client (that is Picus): check the project's entities against
   * the schema, validate its native queries on the server, and generate an entity from a table.
   * The chosen connection is a Picus connection; creating or editing one happens in Picus.
   *
   * Only mounted when `bennuPicusStore.available` — the rail button that opens it does not exist
   * without the Picus backend.
   */
  import { Database, RefreshCw, Unlink, Wand2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { bennuPicusStore } from '$lib/stores/bennu/picus.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import PicusEntitiesTab from './PicusEntitiesTab.svelte';
  import PicusGenerateModal from './PicusGenerateModal.svelte';
  import PicusMybatisTab from './PicusMybatisTab.svelte';
  import PicusQueriesTab from './PicusQueriesTab.svelte';

  type PicusTab = 'entities' | 'queries' | 'mybatis';
  let wanted = $state<PicusTab>('entities');

  const root = $derived(projectStore.project?.root ?? '');
  const connected = $derived(bennuPicusStore.connected);
  const selected = $derived(bennuPicusStore.selected);

  // Only what the project has: a tab for JPA on a project without JPA, or for MyBatis on one without
  // mappers, is a tab that can only ever say "nothing here".
  const tabs = $derived([
    ...(bennuPicusStore.jpa
      ? [
          { id: 'entities', label: 'Entities', badge: bennuPicusStore.checks.length || undefined },
          { id: 'queries', label: 'Queries', badge: bennuPicusStore.model?.native_queries.length || undefined },
        ]
      : []),
    ...(bennuPicusStore.mybatis.length
      ? [{ id: 'mybatis', label: 'MyBatis', badge: bennuPicusStore.mybatis.length }]
      : []),
  ]);
  const tab = $derived((tabs.some((t) => t.id === wanted) ? wanted : tabs[0]?.id) as PicusTab | undefined);

  // Connections are read when the window opens, not at app start: the backend is only worth
  // bringing up for someone who looks.
  $effect(() => {
    void bennuPicusStore.loadConnections();
  });

  // A new project (or a fresh connection) invalidates what was read — re-read when connected.
  $effect(() => {
    if (connected && root) void bennuPicusStore.refresh(root);
  });

  function refresh() {
    if (root) void bennuPicusStore.refresh(root);
  }
</script>

<PanelShell title="Database">
  {#snippet icon()}<Database size={14} />{/snippet}
  {#snippet actions()}
    {#if connected && bennuPicusStore.jpa}
      <IconButton tooltip="Generate an entity from a table…" onclick={() => bennuPicusStore.openGenerate()}>
        <Wand2 size={13} />
      </IconButton>
    {/if}
    <IconButton tooltip="Refresh" disabled={!connected || bennuPicusStore.loading} onclick={refresh}>
      <RefreshCw size={13} />
    </IconButton>
  {/snippet}
  {#snippet toolbar()}
    <div class="bar">
      <Select fill size="sm" ariaLabel="Picus connection" placeholder="Choose a connection…"
        emptyMessage="No connections — define one in Picus"
        value={bennuPicusStore.selectedId}
        onchange={(v) => bennuPicusStore.select(v)}
        options={bennuPicusStore.connections.map((c) => ({ value: c.id, label: c.alias ? `${c.name} · ${c.alias}` : c.name }))} />
      {#if bennuPicusStore.loading}<Spinner size={13} />{/if}
      {#if bennuPicusStore.bound}
        <IconButton tooltip="Forget this project's connection" onclick={() => bennuPicusStore.unbind()}>
          <Unlink size={13} />
        </IconButton>
      {/if}
    </div>
    {#if connected && tabs.length}
      <Tabs items={tabs} value={tab ?? ''} size="sm" onSelect={(id) => (wanted = id as PicusTab)} />
    {/if}
  {/snippet}

  {#if bennuPicusStore.error}
    <div class="pad"><Callout variant="danger" title="Picus">{bennuPicusStore.error}</Callout></div>
  {/if}

  {#if !selected}
    <EmptyState message="No connection selected."
      description="Connections are defined in Picus; pick one above once you have added it there." />
  {:else if !connected}
    <div class="connect">
      <EmptyState message={`${selected.name} is not connected.`} />
      <Button variant="primary" size="sm" disabled={bennuPicusStore.loading} onclick={() => void bennuPicusStore.connect()}>
        Connect
      </Button>
    </div>
  {:else if tab === 'entities'}
    <PicusEntitiesTab />
  {:else if tab === 'queries'}
    <PicusQueriesTab />
  {:else if tab === 'mybatis'}
    <PicusMybatisTab />
  {:else}
    <EmptyState message="Nothing to check in this project."
      description="Entities and native queries need JPA or Hibernate; statements need MyBatis mappers." />
  {/if}
</PanelShell>

{#if bennuPicusStore.generating}
  <PicusGenerateModal onClose={() => bennuPicusStore.closeGenerate()} />
{/if}

<style>
  .bar { display: flex; align-items: center; gap: 8px; padding: 6px 12px; }
  .pad { padding: 8px 12px 0; }
  .connect { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 12px; }
</style>
