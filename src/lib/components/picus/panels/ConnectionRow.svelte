<script lang="ts">
  /**
   * One connection in the sidebar: its colour, its name, its dialect, and the
   * controls that act on it.
   *
   * Only the two everyday verbs are on the row — open/close the session, and
   * start a query on it. Everything you do to the connection *itself* (edit,
   * details, delete) is behind the ⋯ button and the right-click menu, so the row
   * stays a name rather than becoming a toolbar. The menu is built and handled by
   * the panel; this component only says where it should appear.
   */
  import { ChevronRight, Lock, Play, Plug, PlugZap, MoreHorizontal } from 'lucide-svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusConnectionStateDot from '../PicusConnectionStateDot.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { connectionsStore, connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import type { Connection } from '$lib/types/picus';

  interface Props {
    connection: Connection;
    /** Whether the schema tree below the row is showing. */
    open: boolean;
    onToggle: () => void;
    /** Open the row menu at these viewport coordinates. */
    onMenu: (x: number, y: number) => void;
    /** Connecting can fail in ordinary ways, so the panel owns reporting it. */
    onConnect: () => void;
  }

  let { connection: conn, open, onToggle, onMenu, onConnect }: Props = $props();

  function menuFromPointer(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    onMenu(e.clientX, e.clientY);
  }

  /** From the ⋯ button: anchored under it rather than at the pointer. */
  function menuFromButton(e: MouseEvent) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    onMenu(r.left, r.bottom + 2);
  }
</script>

<SidebarItem
  selected={conn.id === connectionsStore.activeId}
  current={conn.id === connectionsStore.activeId}
  currentColor={connectionColorVar(conn)}
  onclick={() => { connectionsStore.setActive(conn.id); onToggle(); }}
  oncontextmenu={menuFromPointer}
>
  {#snippet icon()}
    <!-- The twisty and the session state, in one fixed-width lead so the dots line
         up in a column down the list: "which of these is open" is a question about
         the whole panel, and it should be answerable without reading a single name. -->
    <span class="cr-lead">
      <span class="cr-twist" class:cr-open={open}><ChevronRight size={12} /></span>
      <PicusConnectionStateDot state={conn.state} />
    </span>
  {/snippet}
  <span class="cr-name">{conn.name}</span>
  {#snippet badges()}
    {#if conn.readOnly}
      <span class="cr-ro" use:tooltip={'Read-only: the backend refuses write statements'}><Lock size={11} /></span>
    {/if}
    <PicusDialectChip engine={conn.dialect} terse />
  {/snippet}
  {#snippet actions()}
    <!-- Connect / disconnect. A connection can be configured, listed and edited
         with no server reachable — opening it is a separate act, so it gets its
         own control rather than happening on selection. -->
    {#if conn.state === 'disconnected'}
      <button
        class="cr-act"
        aria-label={`Connect to ${conn.name}`}
        use:tooltip={`Connect to ${conn.name}`}
        onclick={(e) => { e.stopPropagation(); onConnect(); }}
      >
        <Plug size={11} />
      </button>
    {:else if conn.state === 'connecting'}
      <span class="cr-act"><Spinner size={11} /></span>
    {:else}
      <button
        class="cr-act"
        aria-label={`Disconnect ${conn.name}`}
        use:tooltip={`Disconnect ${conn.name}`}
        onclick={(e) => { e.stopPropagation(); void connectionsStore.disconnect(conn.id); }}
      >
        <PlugZap size={11} />
      </button>
    {/if}
    <!-- The fastest path from "this database" to "a query on it". -->
    <button
      class="cr-act"
      aria-label={`New query on ${conn.name}`}
      use:tooltip={`New query on ${conn.name}`}
      onclick={(e) => { e.stopPropagation(); picusTabsStore.openQuery(conn.id); }}
    >
      <Play size={11} />
    </button>
    <button
      class="cr-act"
      aria-label={`Actions for ${conn.name}`}
      aria-haspopup="menu"
      use:tooltip={'Edit, details, delete'}
      onclick={menuFromButton}
    >
      <MoreHorizontal size={12} />
    </button>
  {/snippet}
</SidebarItem>

<style>
  .cr-lead {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .cr-twist {
    display: inline-flex;
    color: var(--text-disabled);
    transition: transform var(--transition-fast);
  }
  .cr-twist.cr-open { transform: rotate(90deg); }

  .cr-name { overflow: hidden; text-overflow: ellipsis; }

  .cr-ro { display: inline-flex; color: var(--warning); }

  /* Hover-revealed row action (SidebarItem shows `actions` on hover). */
  .cr-act {
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
  /* A neutral hover, like every other icon button in the app. It used to go green,
     which turned Disconnect and ⋯ green as well — and green in this panel is a
     claim about the session, not a way of saying "you are pointing at me". */
  .cr-act:hover { background: var(--bg-overlay); color: var(--text-primary); }
</style>
