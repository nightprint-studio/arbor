<script lang="ts">
  /**
   * Connections panel — every configured database session, expandable into its
   * schema.
   *
   * Three files, three questions: this one owns *which* connections there are and
   * what can be done to one, `ConnectionRow` draws a connection, and
   * `ConnectionSchemaTree` draws what is inside it.
   *
   * A connection is a thing you keep, not only a thing you create: it can be
   * opened and closed, edited, inspected and deleted. The two everyday verbs sit
   * on the row; the rest live in the row menu — right-click, or the ⋯ button —
   * and every one of them is also in the command palette, so none of this needs a
   * mouse. Deleting asks first, and says what goes with the connection.
   */
  import {
    Database, Plus, RefreshCw, Play, Plug, PlugZap, Pencil, Info, Trash2,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import ConnectionRow from './ConnectionRow.svelte';
  import ConnectionSchemaTree from './ConnectionSchemaTree.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { openConnection } from '../open-connection';
  import type { Connection } from '$lib/types/picus';

  let query = $state('');
  let searchBar: SearchBar | undefined = $state();
  /** Which connections are expanded. The active one starts open; the rest closed. */
  let expanded = $state<Record<string, boolean>>({});

  const needle = $derived(query.trim().toLowerCase());

  /**
   * Whether one connection's catalogue has anything matching the filter.
   *
   * Answered with `some` rather than by building the four lists: on a schema with
   * hundreds of tables this runs on every keystroke, for every connection held, and
   * the panel only needs a yes or a no. An unread connection answers `false`
   * immediately — the cheap case is also the common one.
   */
  function schemaMatches(connectionId: string): boolean {
    if (!needle) return false;
    const catalogue = schemaStore.of(connectionId);
    if (!catalogue.loaded) return false;
    const has = (list: { name: string }[]) => list.some((o) => o.name.toLowerCase().includes(needle));
    return has(catalogue.tables) || has(catalogue.views)
      || has(catalogue.sequences) || has(catalogue.triggers);
  }

  const visible = $derived(
    connectionsStore.connections.filter((c) => {
      if (!needle) return true;
      return (
        c.name.toLowerCase().includes(needle) ||
        c.alias.toLowerCase().includes(needle) ||
        c.schema.toLowerCase().includes(needle) ||
        // Objects belong to the connection whose catalogue they came from — never
        // to every row on screen. Asked per connection now that several catalogues
        // are held, so a filter finds objects in any of them and not only in the
        // selected one.
        schemaMatches(c.id)
      );
    }),
  );

  /** A filter reveals what it found; otherwise only the active connection is open. */
  function isOpen(conn: Connection): boolean {
    if (needle) return true;
    return expanded[conn.id] ?? conn.id === connectionsStore.activeId;
  }

  function toggle(conn: Connection, open: boolean) {
    expanded = { ...expanded, [conn.id]: !open };
  }

  // ── Row menu ────────────────────────────────────────────────────────────────
  let menu = $state<{ x: number; y: number; conn: Connection } | null>(null);

  const menuItems = $derived.by<MenuItem[]>(() => {
    const conn = menu?.conn;
    if (!conn) return [];
    const isActive = conn.id === connectionsStore.activeId;
    const live = conn.state !== 'disconnected';
    return [
      { id: 'query', label: 'New query on this connection', icon: Play,
        shortcut: isActive ? 'Ctrl+T' : undefined },
      live
        ? { id: 'disconnect', label: 'Disconnect', icon: PlugZap, disabled: conn.state === 'connecting' }
        : { id: 'connect', label: 'Connect', icon: Plug },
      { id: 'refresh', label: 'Refresh the schema', icon: RefreshCw, disabled: !live },
      { id: 'sep1', label: '', separator: true },
      { id: 'edit', label: 'Edit…', icon: Pencil, shortcut: isActive ? 'F4' : undefined },
      { id: 'details', label: 'Details', icon: Info },
      { id: 'sep2', label: '', separator: true },
      { id: 'delete', label: 'Delete…', icon: Trash2, danger: true },
    ];
  });

  function onMenuSelect(id: string) {
    const conn = menu?.conn;
    menu = null;
    if (!conn) return;
    switch (id) {
      case 'query': picusTabsStore.openQuery(conn.id); break;
      case 'connect': void openConnection(conn.id); break;
      case 'disconnect': void connectionsStore.disconnect(conn.id); break;
      case 'refresh':
        connectionsStore.setActive(conn.id);
        // `refresh`, not `ensure`: this is the user saying the catalogue is stale,
        // so already having one is the reason to re-read rather than not to.
        void schemaStore.refresh(conn.id);
        break;
      case 'edit': picusUiStore.openConnectionEditor(conn.id); break;
      case 'details': picusUiStore.openConnectionDetails(conn.id); break;
      case 'delete': picusUiStore.requestConnectionDelete(conn.id); break;
    }
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
      onclick={() => void schemaStore.refresh()}
    >
      {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
    </Button>
  {/snippet}

  {#snippet toolbar()}
    <SearchBar
      bind:this={searchBar}
      bind:query
      showRegex={false}
      showCounter={false}
      placeholder="Filter connections and objects"
      ariaLabel="Filter connections"
    />
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
      {@const open = isOpen(conn)}
      <ConnectionRow
        connection={conn}
        {open}
        onToggle={() => toggle(conn, open)}
        onMenu={(x, y) => (menu = { x, y, conn })}
        onConnect={() => void openConnection(conn.id)}
      />

      {#if open}
        <ConnectionSchemaTree
          connection={conn}
          {needle}
          onNarrow={() => searchBar?.focus()}
        />
      {/if}
    {/each}

    <!-- The colour convention used to be explained here, permanently, under the
         list. It is a thing you learn in one glance the first time two tabs wear
         the same mark, and after that it is a paragraph taking up the panel for
         the rest of the session. The Docs say it once; the panel shows it. -->
  {/if}
</PanelShell>

{#if menu}
  <ContextMenu
    items={menuItems}
    x={menu.x}
    y={menu.y}
    onSelect={onMenuSelect}
    onClose={() => (menu = null)}
  />
{/if}

<style>
  .cp-empty {
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: flex-start;
    padding: 4px 2px;
    text-align: left;
  }
  .cp-empty p { font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }
</style>
