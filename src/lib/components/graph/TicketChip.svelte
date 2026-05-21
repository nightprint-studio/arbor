<script lang="ts">
  import { Hash } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { TicketLink } from '$lib/types/git';

  let {
    link,
    onclick,
    onRemove,
  }: {
    link:      TicketLink;
    onclick?:  (link: TicketLink) => void;
    /** Present only for manual links — shows a × button on hover. */
    onRemove?: (link: TicketLink) => void;
  } = $props();

  // Derive a stable accent color per tracker so chips are easy to scan.
  const trackerColor: Record<string, string> = {
    linear: '#a78bfa',   // violet — distinct from branch blue
    jira:   '#38bdf8',   // sky/cyan — distinct from branch blue
    github: '#8b949e',
    gitlab: '#fc6d26',
  };

  const color = $derived(trackerColor[link.tracker] ?? '#6b7280');
</script>

<span class="ticket-chip-wrap" class:has-remove={!!onRemove}>
  <button
    class="ticket-chip"
    class:manual={link.source === 'manual'}
    use:tooltip={{
      content: `${link.tracker}: ${link.ticket_id}`,
      description: onRemove ? 'Right-click to unlink' : undefined,
    }}
    style="--chip-color: {color}"
    onclick={(e) => { e.stopPropagation(); onclick?.(link); }}
  >
    <Hash size={9} />{link.ticket_id}
  </button>
  {#if onRemove}
    <button
      class="chip-remove"
      use:tooltip={'Remove link'}
      style="--chip-color: {color}"
      onclick={(e) => { e.stopPropagation(); onRemove(link); }}
    >×</button>
  {/if}
</span>

<style>
  .ticket-chip-wrap {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    position: relative;
  }

  .ticket-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 16px;
    padding: 0 5px;
    border-radius: var(--radius-sm);
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.2px;
    white-space: nowrap;
    cursor: pointer;
    border: 1px solid color-mix(in srgb, var(--chip-color) 50%, transparent);
    background: color-mix(in srgb, var(--chip-color) 14%, transparent);
    color: var(--chip-color);
    transition: background var(--transition-fast);
    line-height: 1;
  }

  .ticket-chip:hover {
    background: color-mix(in srgb, var(--chip-color) 26%, transparent);
  }

  /* Manual links: slightly stronger border */
  .ticket-chip.manual {
    border-style: solid;
  }

  /* Remove (×) button — hidden by default, shown on wrap hover */
  .chip-remove {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 0;
    height: 14px;
    overflow: hidden;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--chip-color);
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    transition: width var(--transition-fast), opacity var(--transition-fast);
    margin-left: 1px;
  }

  .ticket-chip-wrap:hover .chip-remove {
    width: 12px;
    opacity: 0.7;
  }

  .chip-remove:hover {
    opacity: 1 !important;
  }
</style>
