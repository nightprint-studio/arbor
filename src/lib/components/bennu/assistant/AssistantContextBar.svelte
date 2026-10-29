<script lang="ts">
  /**
   * How full the model's window is, and with what.
   *
   * One line carries the answer to "can I keep going": a bar split into the three things that fill
   * it, and `used / max` beside it. It stays calm until it matters — the tone only changes past 75%
   * and 90% — and then offers the one remedy there is, *Compact*, which folds the conversation into a
   * summary. Opening the line shows where the tokens went.
   *
   * Every figure here is an estimate (four characters to a token): the bar steers, and the count the
   * API returns after a request is the one to trust. It says so.
   */
  import { ChevronRight, Minimize2 } from 'lucide-svelte';
  import StackedBar from '$lib/components/shared/ui/StackedBar.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { contextTone, formatTokens, percentUsed } from './assistant-format';

  const store = bennuAssistantStore;
  const context = $derived(store.active?.context ?? { system: 0, project: 0, conversation: 0 });
  const used = $derived(store.used);
  const max = $derived(store.maxContext);
  const percent = $derived(percentUsed(used, max));
  const tone = $derived(contextTone(percent));
  const over = $derived(used > max);

  const parts = $derived([
    { id: 'system', label: 'Instructions & tools', value: context.system, tone: 'muted' as const },
    { id: 'project', label: 'Files & search results', value: context.project, tone: 'info' as const },
    { id: 'conversation', label: 'Conversation', value: context.conversation, tone: 'accent' as const },
  ]);
  // The bar takes on the warning colour as a whole once it matters, rather than one part of it.
  const barParts = $derived(
    tone === 'ok'
      ? parts
      : parts.map((p) => ({ ...p, tone: (tone === 'full' ? 'danger' : 'warning') as 'danger' | 'warning' })),
  );
</script>

<div class="ctx">
<Collapsible>
  {#snippet header({ open })}
    <div class="line" class:warn={tone === 'warn'} class:full={tone === 'full'}>
      <ChevronRight size={11} class="chev {open ? 'open' : ''}" />
      <span class="label">Context</span>
      <span class="figure"><b>~{formatTokens(used)}</b> / {formatTokens(max)}</span>
      <span class="pct">{over ? '>100' : percent}%</span>
    </div>
    <div class="bar"><StackedBar segments={barParts} {max} height={5} ariaLabel="Context used" /></div>
  {/snippet}

  <div class="detail">
    <ul class="legend">
      {#each parts as p (p.id)}
        <li>
          <i class="dot t-{p.tone}"></i>
          <span class="l">{p.label}</span>
          <span class="v">{formatTokens(p.value)}</span>
          <span class="p">{percentUsed(p.value, max)}%</span>
        </li>
      {/each}
    </ul>
    <p class="note">An estimate, at about four characters a token. The exact count comes back with each answer.</p>
    <div class="actions">
      <Button
        variant={tone === 'ok' ? 'ghost' : 'secondary'}
        size="sm"
        tooltip="Summarise the earlier part of the conversation to free room"
        onclick={() => store.openCompact()}
      >
        <Minimize2 size={12} /> Compact conversation…
      </Button>
    </div>
  </div>
</Collapsible>
</div>

<style>
  /* `Collapsible` sizes its header content to the content; this one is a bar and has to take the whole
     width, so the one wrapper it puts around the header is told to. */
  .ctx :global(.collapsible-header-content) { flex: 1; min-width: 0; }
  .line {
    display: flex; align-items: baseline; gap: 7px;
    padding: 8px 12px 5px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    cursor: pointer;
  }
  .line :global(.chev) { align-self: center; transition: transform var(--transition-fast); }
  .line :global(.chev.open) { transform: rotate(90deg); }
  .label { font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; font-size: var(--font-size-2xs); }
  .figure { margin-left: auto; font-family: var(--font-code); font-variant-numeric: tabular-nums; }
  .figure b { color: var(--text-primary); font-weight: 600; }
  .pct { min-width: 34px; text-align: right; font-family: var(--font-code); font-variant-numeric: tabular-nums; }
  .line.warn .pct, .line.warn .figure b { color: var(--warning); }
  .line.full .pct, .line.full .figure b { color: var(--error); }

  .bar { padding: 0 12px 9px; }

  .detail { padding: 2px 12px 10px; display: flex; flex-direction: column; gap: 8px; }
  .legend { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; }
  .legend li { display: flex; align-items: center; gap: 8px; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .l { flex: 1; min-width: 0; }
  .v { font-family: var(--font-code); font-variant-numeric: tabular-nums; color: var(--text-primary); }
  .p { width: 32px; text-align: right; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .t-muted { background: var(--text-muted); }
  .t-info { background: var(--info); }
  .t-accent { background: var(--accent); }
  .note { margin: 0; font-size: var(--font-size-2xs); line-height: 1.45; color: var(--text-muted); }
  .actions { display: flex; }
</style>
