<script lang="ts">
  /**
   * Connections panel — every open database session, expandable into its schema.
   *
   * Each connection row carries its identity colour, the dialect, and a lock when
   * the session is read-only. Expanding one shows the schema it is pinned to and
   * four groups — tables, views, sequences, triggers — because "what is actually
   * in this database" is not answerable from tables alone: a missing sequence or
   * a disabled trigger breaks an installation just as thoroughly.
   *
   * Every connection row also carries the fastest way to use it: a query tab
   * bound to that session, one click (or Ctrl+T for the active one).
   */
  import {
    Database, ChevronRight, Table2, Eye, ListOrdered, Zap,
    Plus, RefreshCw, Lock, Play,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { connectionsStore, connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { SCHEMA_GROUP_LABELS, type SchemaGroup } from '$lib/types/picus';

  let query = $state('');
  /** Expansion is keyed `connId` and `connId/group` — one tree, flat keys. */
  let expanded = $state<Record<string, boolean>>({ dev: true, 'dev/tables': true });

  const needle = $derived(query.trim().toLowerCase());
  const matches = (name: string) => !needle || name.toLowerCase().includes(needle);

  const GROUP_ICONS: Record<SchemaGroup, any> = {
    tables: Table2,
    views: Eye,
    sequences: ListOrdered,
    triggers: Zap,
  };

  const GROUPS: SchemaGroup[] = ['tables', 'views', 'sequences', 'triggers'];

  /** Object names in a group, filtered — the tree only ever renders names. */
  function namesIn(group: SchemaGroup): string[] {
    const list =
      group === 'tables' ? schemaStore.tables.map((t) => t.name)
      : group === 'views' ? schemaStore.views.map((v) => v.name)
      : group === 'sequences' ? schemaStore.sequences.map((s) => s.name)
      : schemaStore.triggers.map((t) => t.name);
    return list.filter(matches);
  }

  /** The tab kind an object of this group opens as. */
  function objectKindOf(group: SchemaGroup) {
    return group === 'tables' ? 'table' as const
      : group === 'views' ? 'view' as const
      : group === 'sequences' ? 'sequence' as const
      : 'trigger' as const;
  }

  /** Secondary line for a row — what the object is, in its own terms. */
  function detailFor(group: SchemaGroup, name: string): string | null {
    if (group === 'tables' || group === 'views') {
      const rel = schemaStore.relation(name);
      if (!rel) return null;
      const parts = [`${rel.columns.length} columns`];
      if (rel.foreignKeys?.length) parts.push(`${rel.foreignKeys.length} FK`);
      if (rel.estimatedRows != null) parts.push(`~${rel.estimatedRows.toLocaleString()} rows`);
      return parts.join(' · ');
    }
    if (group === 'sequences') {
      const seq = schemaStore.sequence(name);
      return seq ? `last ${seq.lastValue.toLocaleString()} · step ${seq.incrementBy}` : null;
    }
    const trg = schemaStore.trigger(name);
    return trg ? `${trg.timing} ${trg.events.join('/')} on ${trg.table}${trg.enabled ? '' : ' · disabled'}` : null;
  }

  const visible = $derived(
    connectionsStore.connections.filter((c) => {
      if (!needle) return true;
      return (
        c.name.toLowerCase().includes(needle) ||
        c.alias.toLowerCase().includes(needle) ||
        c.schema.toLowerCase().includes(needle) ||
        GROUPS.some((g) => namesIn(g).length > 0)
      );
    }),
  );

  function toggle(key: string) {
    expanded = { ...expanded, [key]: !expanded[key] };
  }
</script>

<PanelShell title="Connections" count={connectionsStore.connections.length}>
  {#snippet icon()}<Database size={13} />{/snippet}

  {#snippet actions()}
    <Button
      variant="icon"
      size="xs"
      tooltip={{ content: 'New query on the active connection', shortcut: 'Ctrl+T' }}
      ariaLabel="New query"
      onclick={() => picusTabsStore.openQuery()}
    >
      {#snippet iconStart()}<Play size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      tooltip={{ content: 'Add a connection', shortcut: 'Ctrl+Shift+N' }}
      ariaLabel="Add a connection"
      onclick={() => picusUiStore.openConnectionEditor(null)}
    >
      {#snippet iconStart()}<Plus size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      title="Refresh the schema cache"
      ariaLabel="Refresh the schema cache"
      onclick={() => schemaStore.refresh()}
    >
      {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
    </Button>
  {/snippet}

  {#snippet toolbar()}
    <SearchBar bind:query showRegex={false} placeholder="Filter connections and objects" ariaLabel="Filter connections" />
  {/snippet}

  {#if !visible.length}
    <StateBlock tone="info" fill={false}>
      {#if connectionsStore.connections.length}
        <span>No connection or object matches “{query}”.</span>
      {:else}
        <div class="cp-empty">
          <p>No connection configured yet. Picus needs a session to read a schema, run queries and feed the generator.</p>
          <Button variant="primary" size="sm" onclick={() => picusUiStore.openConnectionEditor(null)}>
            {#snippet iconStart()}<Plus size={13} />{/snippet}
            Add a connection
          </Button>
        </div>
      {/if}
    </StateBlock>
  {:else}
    {#each visible as conn (conn.id)}
      <SidebarItem
        selected={conn.id === connectionsStore.activeId}
        current={conn.id === connectionsStore.activeId}
        currentColor={connectionColorVar(conn)}
        onclick={() => { connectionsStore.setActive(conn.id); toggle(conn.id); }}
      >
        {#snippet icon()}
          <span class="cp-twist" class:cp-open={expanded[conn.id]}><ChevronRight size={12} /></span>
        {/snippet}
        <span class="cp-name">{conn.name}</span>
        {#snippet badges()}
          {#if conn.readOnly}
            <span class="cp-ro" use:tooltip={'Read-only: the backend refuses write statements'}><Lock size={11} /></span>
          {/if}
          <PicusDialectChip dialect={conn.dialect} terse />
        {/snippet}
        {#snippet actions()}
          <!-- Hover action: the fastest path from "this database" to "a query on it". -->
          <button
            class="cp-act"
            aria-label={`New query on ${conn.name}`}
            use:tooltip={`New query on ${conn.name}`}
            onclick={(e) => { e.stopPropagation(); picusTabsStore.openQuery(conn.id); }}
          >
            <Play size={11} />
          </button>
        {/snippet}
      </SidebarItem>

      {#if expanded[conn.id]}
        <div class="cp-meta" style:--conn-color={connectionColorVar(conn)}>
          {conn.schema} · database version {conn.dbVersion}
          {#if schemaStore.loading}
            <span class="cp-loading"><Spinner size={10} /> reading schema…</span>
          {:else if schemaStore.loadedAt}
            <span class="cp-stamp">cached {schemaStore.loadedAt}</span>
          {/if}
        </div>

        {#each GROUPS as group (group)}
          {@const names = namesIn(group)}
          {@const key = `${conn.id}/${group}`}
          <SidebarItem indent={22} onclick={() => toggle(key)}>
            {#snippet icon()}
              <span class="cp-twist" class:cp-open={expanded[key]}><ChevronRight size={12} /></span>
            {/snippet}
            {@const Icon = GROUP_ICONS[group]}
            <Icon size={12} class="cp-group-icon" />
            <span class="cp-group">{SCHEMA_GROUP_LABELS[group]}</span>
            {#snippet badges()}
              <Badge variant="count" label={String(names.length)} />
            {/snippet}
          </SidebarItem>

          {#if expanded[key]}
            {#each names as name (name)}
              <SidebarItem
                indent={40}
                selected={picusTabsStore.active?.table === name}
                onclick={() => picusTabsStore.openObject(name, objectKindOf(group), conn.id)}
              >
                <span class="cp-object">
                  {conn.dialect === 'postgres' ? name.toLowerCase() : name}
                </span>
                {#snippet subtitle()}{detailFor(group, name) ?? ''}{/snippet}
              </SidebarItem>
            {/each}
            {#if !names.length}
              <p class="cp-none">Nothing in this group.</p>
            {/if}
          {/if}
        {/each}
      {/if}
    {/each}

    <p class="cp-hint">
      A connection's colour follows it everywhere — the tab of every document bound to
      it wears the same mark.
    </p>
  {/if}
</PanelShell>

<style>
  .cp-twist {
    display: inline-flex;
    color: var(--text-disabled);
    transition: transform var(--transition-fast);
  }
  .cp-twist.cp-open { transform: rotate(90deg); }

  .cp-name { overflow: hidden; text-overflow: ellipsis; }
  .cp-group {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-secondary);
  }
  .cp-object {
    font-family: var(--font-code);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .cp-ro { display: inline-flex; color: var(--warning); }

  /* Hover-revealed row action (SidebarItem shows `actions` on hover). */
  .cp-act {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
  }
  .cp-act:hover { background: var(--bg-overlay); color: var(--success); }

  .cp-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 3px 12px 5px 34px;
    margin-left: 10px;
    border-left: 2px solid var(--conn-color);
    font-size: 10.5px;
    color: var(--text-muted);
  }
  .cp-loading { display: inline-flex; align-items: center; gap: 4px; color: var(--accent); }
  .cp-stamp { color: var(--text-disabled); }

  .cp-none {
    padding: 4px 12px 4px 46px;
    font-size: 11px;
    color: var(--text-disabled);
    font-style: italic;
  }

  .cp-hint {
    padding: 10px 12px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-muted);
  }

  .cp-empty {
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: flex-start;
    padding: 4px 2px;
    text-align: left;
  }
  .cp-empty p { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); }

  :global(.cp-group-icon) { color: var(--text-muted); flex-shrink: 0; }
</style>
