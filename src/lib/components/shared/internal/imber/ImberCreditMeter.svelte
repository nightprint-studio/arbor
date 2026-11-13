<script lang="ts">
  /**
   * ImberCreditMeter — in a title bar, beside the documentation button: how much the providers Imber can use have
   * left, as a ring around Imber's spark; pressed, a menu with every provider.
   *
   * The ring fills with the **average** left over the providers that know a limit; its colour comes from the
   * **worst** one — amber when one is close to running out, red when one has — because an average hides a provider
   * about to run out. The menu has a row per provider (the active one first and open), each with its figure and,
   * opened, where the figure comes from. A provider with nothing to show still has its row, saying what is missing.
   *
   * Anthropic's "left" is an estimate: the balance you entered (from the Claude Console), less what was spent after
   * it — the organization's real spending with an Admin key, which is close, or only Imber's without one, which is
   * rough. Its row says which, and its figure carries a `~`.
   *
   * The figures come from `imberCreditsStore`, shared by every title bar; this only draws them. `onSettings` opens
   * the settings, `onAction` the page for one row's action — absent where the window has no such page.
   */
  import { onMount } from 'svelte';
  import { ChevronRight, ChevronDown, RefreshCw, Sparkles } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';
  import { imberCreditsStore } from '$lib/stores/imber-credits.svelte';
  import type { ProviderCredit } from '$lib/ipc/imber-credits';

  let { onSettings, onAction }: {
    onSettings?: () => void;
    /** Something a provider's row offers to do (`set_balance`, `add_admin_key`); absent, the buttons are not shown. */
    onAction?: (action: string) => void;
  } = $props();

  const ACTION_LABEL: Record<string, string> = {
    set_balance: 'Enter the balance…',
    add_admin_key: 'Add an Admin key…',
  };

  const store = imberCreditsStore;
  onMount(() => store.attach());

  const summary = $derived(store.summary);
  const average = $derived(summary?.average_left ?? null);
  const tone = $derived(
    !summary || average === null ? 'var(--text-muted)'
      : summary.level === 'out' ? 'var(--danger)'
      : summary.level === 'warn' ? 'var(--warning)'
      : 'var(--accent)',
  );
  const lowest = $derived(summary?.lowest ? summary.providers.find((p) => p.id === summary.lowest![0]) ?? null : null);

  // Ring geometry: radius 11 in a 26 box.
  const R = 11;
  const C = 2 * Math.PI * R;

  /** Opened rows; the active provider starts open. */
  let opened = $state<Set<string>>(new Set());
  let seeded = false;
  $effect(() => {
    const active = summary?.providers.find((p) => p.active);
    if (!seeded && active) {
      opened = new Set([active.id]);
      seeded = true;
    }
  });
  function toggleRow(id: string) {
    const next = new Set(opened);
    if (next.has(id)) next.delete(id); else next.add(id);
    opened = next;
  }

  const pct = (share: number) => `${Math.round(share * 100)}%`;
  /** Some provider's figure is an estimate (Anthropic's balance less what was spent since). */
  const estimated = $derived(!!summary?.providers.some((p) => p.figure.startsWith('~')));
  const tip = $derived(
    average === null
      ? 'Imber credits — no provider with a known limit'
      : `Imber credits: ${estimated ? '~' : ''}${pct(average)} left on average${lowest && summary!.providers.length > 1 ? ` · lowest ${lowest.name}` : ''}`,
  );
  const barColour = (p: ProviderCredit) =>
    p.left === null ? undefined : p.left <= 0 ? 'var(--danger)' : p.left <= 0.2 ? 'var(--warning)' : 'var(--accent)';
</script>

<Dropdown position="fixed" direction="down" width="340px" maxHeight={520} onopen={() => void store.refresh()}>
  {#snippet trigger({ open, toggle })}
    <button
      type="button"
      class="meter"
      class:open
      onclick={toggle}
      use:tooltip={tip}
      aria-label={tip}
      aria-haspopup="dialog"
      aria-expanded={open}
    >
      <svg width="24" height="24" viewBox="0 0 26 26" aria-hidden="true">
        <circle cx="13" cy="13" r={R} fill="none" stroke="var(--border)" stroke-width="2" />
        {#if average !== null}
          <circle
            cx="13" cy="13" r={R} fill="none" stroke={tone} stroke-width="2" stroke-linecap="round"
            stroke-dasharray={C} stroke-dashoffset={C * (1 - average)} transform="rotate(-90 13 13)"
            class="arc"
          />
        {/if}
      </svg>
      <span class="spark" style:color={tone}><Sparkles size={11} /></span>
      <ChevronDown size={11} class="caret" />
    </button>
  {/snippet}

  {#snippet children({ close })}
    <div class="pop">
      <div class="head">
        {#if average !== null && summary}
          <div><b>{pct(average)}</b> left on average over {summary.providers.filter((p) => p.left !== null).length} provider{summary.providers.filter((p) => p.left !== null).length === 1 ? '' : 's'}</div>
          {#if estimated}<div class="sub">An estimate: the API does not report the credit left.</div>{/if}
          {#if lowest && summary.providers.filter((p) => p.left !== null).length > 1}
            <div class="sub">Lowest: <span style:color={tone}>{lowest.name} {pct(lowest.left ?? 0)}</span></div>
          {/if}
        {:else if summary}
          <div>No provider with a known limit.</div>
          <div class="sub">Set a spending limit to see what is left.</div>
        {:else if store.error}
          <div class="err">The figures could not be read.</div>
          <div class="sub">{store.error}</div>
        {:else}
          <div class="sub"><Spinner size="xs" /> Reading…</div>
        {/if}
      </div>

      {#each summary?.providers ?? [] as p (p.id)}
        {@const isOpen = opened.has(p.id)}
        <div class="row" class:isOpen>
          <button type="button" class="line" aria-expanded={isOpen} onclick={() => toggleRow(p.id)}>
            <ChevronRight size={11} class="chev" />
            <span class="name"><b>{p.name}</b><small>{p.subtitle}</small></span>
            {#if p.left !== null}
              <span class="bar"><ProgressBar pct={(1 - p.left) * 100} height={4} color={barColour(p)} ariaLabel={`${p.name}: used`} /></span>
            {/if}
            <span class="fig">{p.figure}</span>
          </button>
          {#if isOpen}
            <div class="more">
              {#each p.sections as s (s.title)}
                <div class="sect">{s.title}</div>
                {#each s.lines as l (l.label)}
                  <div class="dl"><span>{l.label}</span><span>{l.value}</span></div>
                {/each}
                {#if s.note}<p class="note">{s.note}</p>{/if}
                {#if s.action && onAction && ACTION_LABEL[s.action]}
                  <span class="act"><Button variant="secondary" size="xs" onclick={() => { close(); onAction?.(s.action!); }}>{ACTION_LABEL[s.action]}</Button></span>
                {/if}
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/snippet}

  {#snippet footer({ close })}
    <div class="foot">
      <Button variant="secondary" size="sm" disabled={store.loading} onclick={() => void store.refresh(true)}>
        {#if store.loading}<Spinner size="xs" />{:else}<RefreshCw size={12} />{/if} Refresh
      </Button>
      {#if onSettings}
        <Button variant="ghost" size="sm" onclick={() => { close(); onSettings?.(); }}>Settings…</Button>
      {/if}
    </div>
  {/snippet}
</Dropdown>

<style>
  /* The size and the hover of the title bar's own icon buttons, beside which it sits. */
  .meter {
    position: relative; display: inline-flex; align-items: center; gap: 1px;
    height: 34px; padding: 0 4px 0 5px;
    border: none; border-radius: var(--radius-sm); background: transparent;
    color: var(--text-secondary); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .meter:hover, .meter.open { background: color-mix(in srgb, var(--text-primary) 9%, transparent); color: var(--text-primary); }
  .meter:focus-visible { outline: none; box-shadow: inset 0 0 0 2px var(--accent); }
  .spark { position: absolute; left: 5px; top: 5px; width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; }
  .meter :global(.caret) { opacity: 0.7; }
  .arc { transition: stroke-dashoffset 400ms ease, stroke 200ms ease; }

  .pop { display: flex; flex-direction: column; padding: 2px 0; font-size: var(--font-size-xs); }
  .head { padding: 6px 10px 9px; margin-bottom: 4px; border-bottom: 1px solid var(--border-subtle); color: var(--text-secondary); }
  .head b { color: var(--text-primary); font-variant-numeric: tabular-nums; }
  .sub { margin-top: 2px; color: var(--text-muted); display: inline-flex; align-items: center; gap: 6px; }
  .err { color: var(--danger); }

  .line {
    display: flex; align-items: center; gap: 8px; width: 100%;
    padding: 7px 10px; border: none; border-radius: var(--radius-sm); background: transparent;
    color: var(--text-secondary); text-align: left; cursor: pointer;
  }
  .line:hover { background: var(--bg-hover); }
  .line:focus-visible { outline: none; box-shadow: inset 0 0 0 2px var(--accent); }
  .line :global(.chev) { flex-shrink: 0; transition: transform var(--transition-fast); }
  .isOpen .line :global(.chev) { transform: rotate(90deg); }
  .name { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .name b { font-weight: 500; color: var(--text-primary); }
  .name small { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .bar { width: 54px; flex-shrink: 0; }
  .fig { flex-shrink: 0; font-family: var(--font-code); font-variant-numeric: tabular-nums; color: var(--text-primary); }

  .more { padding: 2px 12px 10px 29px; display: flex; flex-direction: column; gap: 3px; animation: open-row 160ms ease-out; }
  @keyframes open-row { from { opacity: 0; transform: translateY(-3px); } to { opacity: 1; transform: none; } }
  @media (prefers-reduced-motion: reduce) { .more { animation: none; } .arc { transition: none; } }
  .sect { margin-top: 6px; font-size: var(--font-size-2xs); letter-spacing: 0.05em; text-transform: uppercase; color: var(--text-muted); }
  .dl { display: flex; justify-content: space-between; gap: 12px; color: var(--text-secondary); }
  .dl span:last-child { font-family: var(--font-code); font-variant-numeric: tabular-nums; color: var(--text-primary); }
  .note { margin: 3px 0 4px; line-height: 1.45; color: var(--text-muted); }
  .act { display: flex; margin-bottom: 2px; }

  .foot { display: flex; gap: 6px; padding: 6px 8px; }
</style>
