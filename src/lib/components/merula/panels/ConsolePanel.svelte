<script lang="ts">
  /**
   * Console — engine + eval logs. Emission is gated to the titlebar threshold
   * (the design's "gating all'emissione"); on top of that the toolbar offers a
   * **text search** (Ctrl+F focuses it) and **per-level filter chips** (e.g.
   * isolate warn/error). Log lines carry no source span, so they are not clickable.
   */
  import { Terminal, ArrowDownToLine, Trash2 } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { merulaStore, levelsAtOrAbove, LOG_LEVELS } from '../merula-store.svelte';
  import { configStore } from '../stores/config.svelte';
  import { logStore } from '../stores/engine.svelte';

  let query = $state('');
  let searchEl = $state<HTMLInputElement | null>(null);
  let bodyEl = $state<HTMLElement | null>(null);
  // Per-level view filter (on top of the emission threshold). Default all on.
  let levelOn = $state<Record<string, boolean>>({ trace: true, debug: true, info: true, warn: true, error: true });

  // Ctrl+F → focus the search field.
  $effect(() => {
    if (merulaStore.findPending) { searchEl?.focus(); searchEl?.select(); merulaStore.clearFind(); }
  });

  const emitted = $derived(levelsAtOrAbove(configStore.logThreshold));
  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return logStore.lines.filter(l =>
      emitted.has(l.level) && (levelOn[l.level] ?? true) &&
      (!q || l.message.toLowerCase().includes(q)),
    );
  });

  function toggleLevel(l: string) { levelOn = { ...levelOn, [l]: !levelOn[l] }; }
  function scrollToBottom() { if (bodyEl) bodyEl.scrollTop = bodyEl.scrollHeight; }
</script>

<div class="con">
  <BottomPanelHeader title="Console" onClose={() => merulaStore.toggleBottom('console')}>
    {#snippet icon()}<Terminal size={13} />{/snippet}
    {#snippet children()}<span class="con-meta">{visible.length}/{logStore.count}</span>{/snippet}
    {#snippet actions()}
      <IconButton tooltip="Scroll to bottom" onclick={scrollToBottom}><ArrowDownToLine size={13} /></IconButton>
      <IconButton tooltip="Clear console" onclick={() => logStore.clear()}><Trash2 size={13} /></IconButton>
    {/snippet}
  </BottomPanelHeader>

  <div class="con-toolbar">
    <FilterInput bind:value={query} bind:element={searchEl} placeholder="Search… (Ctrl+F)" />
    <div class="con-levels">
      {#each LOG_LEVELS as l (l)}
        {@const gated = !emitted.has(l)}
        <button
          class="lvl-chip lvl-{l}"
          class:off={!levelOn[l]}
          class:gated
          disabled={gated}
          onclick={() => toggleLevel(l)}
          use:tooltip={gated ? `${l} — below the emission threshold` : l}
          aria-pressed={levelOn[l]}
        >{l[0].toUpperCase()}</button>
      {/each}
    </div>
  </div>

  <div class="con-body" bind:this={bodyEl}>
    {#if visible.length === 0}
      <EmptyState message={query ? `No log lines match “${query}”.` : 'No log lines for the current filter.'} />
    {:else}
      {#each visible as l (l.id)}
        <div class="con-line">
          <span class="con-level lvl-{l.level}">{l.level}</span>
          <span class="con-text">{l.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .con { display: flex; flex-direction: column; height: 100%; background: var(--bg-base); }
  .con-meta { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }

  /* ── Toolbar: search + level chips ── */
  .con-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 10px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  .con-levels { display: flex; gap: 3px; flex-shrink: 0; }
  .lvl-chip {
    width: 20px; height: 20px;
    display: flex; align-items: center; justify-content: center;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: var(--bg-input); cursor: pointer;
    font-size: var(--font-size-2xs); font-weight: 700; font-family: var(--font-code);
    transition: opacity var(--transition-fast), background var(--transition-fast);
  }
  .lvl-chip.off { opacity: 0.32; }
  .lvl-chip.gated { opacity: 0.18; cursor: not-allowed; }
  .lvl-chip.lvl-trace { color: var(--text-disabled); }
  .lvl-chip.lvl-debug { color: var(--text-muted); }
  .lvl-chip.lvl-info  { color: var(--info); border-color: color-mix(in srgb, var(--info) 40%, var(--border-subtle)); }
  .lvl-chip.lvl-warn  { color: var(--warning); border-color: color-mix(in srgb, var(--warning) 40%, var(--border-subtle)); }
  .lvl-chip.lvl-error { color: var(--error); border-color: color-mix(in srgb, var(--error) 40%, var(--border-subtle)); }

  /* ── Body ── */
  .con-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; font-family: var(--font-code); }
  .con-line {
    display: flex; align-items: baseline; gap: 8px;
    padding: 1px 12px; font-size: var(--font-size-xs); line-height: 1.6; white-space: nowrap;
  }
  .con-level { width: 42px; flex-shrink: 0; font-weight: 600; text-transform: uppercase; font-size: var(--font-size-3xs); }
  .con-text { color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; }

  .con-level.lvl-trace { color: var(--text-disabled); }
  .con-level.lvl-debug { color: var(--text-muted); }
  .con-level.lvl-info  { color: var(--info); }
  .con-level.lvl-warn  { color: var(--warning); }
  .con-level.lvl-error { color: var(--error); }
</style>
