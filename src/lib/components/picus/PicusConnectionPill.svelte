<script lang="ts" module>
  /**
   * Where the pill is rendered. Each density drops information rather than
   * shrinking it: the titlebar shows name + alias, the toolbar just the name,
   * the status bar name + schema in the footer's own type scale.
   */
  export type PillDensity = 'titlebar' | 'toolbar' | 'status';
</script>

<script lang="ts">
  /**
   * PicusConnectionPill — the connection identity chip, at three densities.
   *
   * Colour is the mechanism: every connection owns a slot in the shared
   * workspace palette, and that colour appears here, on the tab of every
   * document bound to the connection, and in the status bar. Two sessions on two
   * databases must never be confusable at a glance.
   *
   * Read-only connections carry a lock: the backend refuses writes on them, and
   * the pill is where that fact is always visible.
   *
   * Product-local for now (only Picus has database connections). If a second
   * product ever grows them, this moves to `shared/internal/` unchanged.
   */
  import { Database, Lock, ChevronDown } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import PicusConnectionStateDot from './PicusConnectionStateDot.svelte';
  import { DIALECTS, type Connection } from '$lib/types/picus';

  interface Props {
    connection: Connection | null;
    density?: PillDensity;
    /** Renders as a button with a chevron; omit for a static badge. */
    onclick?: () => void;
    /** Menu-open state, for the trigger styling. */
    open?: boolean;
    ariaLabel?: string;
  }

  let { connection, density = 'toolbar', onclick, open = false, ariaLabel }: Props = $props();

  const interactive = $derived(typeof onclick === 'function');
  const color = $derived(connectionColorVar(connection));

  const hint = $derived(
    connection
      ? `${connection.name} · ${DIALECTS[connection.dialect].label}\n${connection.schema}@${connection.host}` +
        (connection.readOnly ? '\nRead-only: write statements are refused' : '')
      : 'No connection selected',
  );
</script>

{#snippet body()}
  {#if connection}
    <span class="cp-swatch" style:background={color}></span>
    <!-- Which connection AND whether it is open, in the same glance. The pill is
         where a tab's binding is stated, and "can I run this" is half of that
         binding — reading it used to mean going to the sidebar and hovering. -->
    <PicusConnectionStateDot state={connection.state} />
    {#if density === 'titlebar'}
      <Database size={12} />
    {/if}
    <span class="cp-name">{connection.name}</span>
    {#if density === 'titlebar'}
      <span class="cp-alias">{connection.alias}</span>
    {:else if density === 'status'}
      <span class="cp-alias">{connection.schema}</span>
    {/if}
    {#if connection.readOnly}
      <Lock size={10} class="cp-lock" />
    {/if}
  {:else}
    <span class="cp-swatch cp-swatch-empty"></span>
    <span class="cp-name cp-muted">No connection</span>
  {/if}
  {#if interactive}
    <ChevronDown size={density === 'status' ? 9 : 11} />
  {/if}
{/snippet}

{#if interactive}
  <button
    type="button"
    class="cp cp-{density}"
    class:cp-open={open}
    class:cp-ro={connection?.readOnly}
    onclick={onclick}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={ariaLabel ?? (connection ? `Connection: ${connection.name}` : 'Select a connection')}
    use:tooltip={hint}
  >
    {@render body()}
  </button>
{:else}
  <span class="cp cp-{density}" class:cp-ro={connection?.readOnly} use:tooltip={hint}>
    {@render body()}
  </span>
{/if}

<style>
  .cp {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 23px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-overlay);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    white-space: nowrap;
    flex-shrink: 0;
    /* Titlebar hosts are drag regions — the pill must stay clickable. */
    -webkit-app-region: no-drag;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  button.cp { cursor: pointer; }
  button.cp:hover,
  button.cp.cp-open {
    background: var(--bg-hover);
    border-color: var(--border-focus);
  }

  /* Read-only is a standing warning, not a transient one. */
  .cp.cp-ro {
    border-color: color-mix(in srgb, var(--warning) 42%, transparent);
    background: color-mix(in srgb, var(--warning) 10%, var(--bg-overlay));
  }
  button.cp.cp-ro:hover {
    background: color-mix(in srgb, var(--warning) 18%, var(--bg-overlay));
  }

  /* A BAR, not a dot, and the shape is the point.
     This marker is the connection's identity — its slot in the shared workspace
     palette — and one of the twelve palette colours is the same green as
     `--success`. Picus reserves the round green dot for "the session is open"
     (`PicusConnectionStateDot`), so identity is drawn as a colour swatch instead:
     a stripe, like the accent bar a selected sidebar row already wears. Two
     vocabularies, two shapes, no green that has to be interpreted. */
  .cp-swatch {
    width: 3px;
    height: 11px;
    border-radius: 2px;
    flex-shrink: 0;
  }
  .cp-swatch-empty {
    background: transparent;
    border: 1px dashed var(--border);
  }

  .cp-name {
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 500;
  }
  .cp-alias {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cp-muted { color: var(--text-muted); font-weight: 400; }

  .cp :global(svg) { color: var(--text-secondary); flex-shrink: 0; }
  .cp :global(.cp-lock) { color: var(--warning); }

  /* ── Densities ──────────────────────────────────────────────────────────── */
  .cp-titlebar { max-width: 300px; }
  .cp-toolbar  { max-width: 220px; height: 22px; }

  /* The footer runs on its own smaller scale and has no chrome of its own. */
  .cp-status {
    height: 18px;
    padding: 0 5px;
    gap: 5px;
    border: none;
    background: transparent;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .cp-status .cp-name { font-weight: 400; color: var(--text-secondary); }
  .cp-status .cp-swatch { height: 9px; }
  button.cp-status:hover { background: var(--bg-hover); border-color: transparent; }
  .cp-status.cp-ro { background: transparent; border: none; }
  .cp-status.cp-ro .cp-name { color: var(--warning); }
</style>
