<script lang="ts">
  /**
   * What you were doing: pinned projects as pills, then the rest grouped by when, as cards. Every change of the list —
   * a filter, a pin, a removal, "show more" — morphs rather than cuts (`utils/flip.ts`).
   *
   * The cards are a grid for the keyboard: ←→↑↓ move between them, Enter opens, P pins, Delete removes (with Undo — the
   * removal is only carried out once the toast is gone). `openNth` lets the page open the n-th project from a number key.
   */
  import { tick, untrack } from 'svelte';
  import { Star, X } from 'lucide-svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { morph } from '$lib/utils/flip';
  import type { RecentProject } from '$lib/ipc/recents';
  import { recentKey, recentsView, whenLabel, type RecentsView } from './welcome-logic';

  let { recents, productNames, onopen, onpin, onforget }: {
    recents: RecentProject[];
    productNames: Record<string, string>;
    onopen: (r: RecentProject) => void;
    onpin: (r: RecentProject, pinned: boolean) => void;
    onforget: (r: RecentProject) => void;
  } = $props();

  const LIMIT = 6;
  const now = () => Date.now() / 1000;

  let filter = $state('all');
  let showAll = $state(false);
  /** Keys taken off the page and not yet forgotten: the Undo toast is still up. */
  let hidden = $state<Set<string>>(new Set());
  let box = $state<HTMLElement>();

  // The products that have something to filter by, in the order they first appear.
  const filters = $derived([...new Set(recents.map((r) => r.product))]);

  const compute = (): RecentsView =>
    recentsView(recents.filter((r) => !hidden.has(recentKey(r))), filter, now(), { all: showAll, limit: LIMIT });

  // `view` is what is drawn. It is recomputed by `morph` after what leaves has faded out, and by the effect below for
  // changes that come from outside (the list loading, another window pinning something).
  let view = $state<RecentsView>(compute());
  let morphing = 0;
  $effect(() => {
    void recents; void hidden;
    untrack(() => { if (!morphing) view = compute(); });
  });

  const cards = () => [...(box?.querySelectorAll<HTMLElement>('.rc, .pin')) ?? []];
  let version = 0;
  async function change(apply: () => void) {
    const mine = ++version;
    morphing++;
    try {
      await morph({
        box: box!,
        items: cards,
        keyOf: (el) => el.dataset.k ?? '',
        change: apply,
        survives: () => new Set(compute().order.map(recentKey)),
        render: async () => { view = compute(); await tick(); },
        superseded: () => mine !== version,
      });
    } finally { morphing--; }
  }

  function setFilter(f: string) { if (f !== filter) void change(() => { filter = f; }); }
  const more = () => change(() => { showAll = true; });

  function pin(r: RecentProject) { void change(() => onpin(r, !r.pinned)); }

  function forget(r: RecentProject, then?: () => void) {
    const key = recentKey(r);
    let undone = false;
    void change(() => { hidden = new Set(hidden).add(key); });
    toastStore.show(`${r.name} removed from the list`, 'info', 5000, {
      label: 'Undo',
      onClick: () => { undone = true; void change(() => { const h = new Set(hidden); h.delete(key); hidden = h; }); },
    });
    // The toast's own timer is the deadline: once it is gone the entry is really forgotten.
    setTimeout(() => {
      if (undone) return;
      onforget(r);
      const h = new Set(hidden); h.delete(key); hidden = h;
    }, 5200);
    then?.();
  }

  export function openNth(n: number): boolean {
    const r = view.order[n - 1];
    if (!r) return false;
    onopen(r);
    return true;
  }

  // ── keyboard: the cards are a grid ───────────────────────────────────────────────────────────────
  const hitOf = (el: Element) => el.closest<HTMLElement>('.rc-hit, .pin');
  const hits = () => [...(box?.querySelectorAll<HTMLElement>('.rc-hit, .pin') ?? [])];
  const recentOf = (k: string | undefined) => recents.find((r) => recentKey(r) === k);

  function focusHit(el: HTMLElement | undefined) {
    if (!el) return;
    for (const h of hits()) h.tabIndex = -1;
    el.tabIndex = 0;
    el.focus();
  }

  function onKey(e: KeyboardEvent) {
    const hit = hitOf(e.target as Element);
    if (!hit) return;
    const all = hits(), i = all.indexOf(hit), at = hit.getBoundingClientRect();
    let next: HTMLElement | undefined;
    if (e.key === 'ArrowRight') next = all[i + 1];
    else if (e.key === 'ArrowLeft') next = all[i - 1];
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const dir = e.key === 'ArrowDown' ? 1 : -1;
      const beyond = all.filter((h) => (dir > 0 ? h.getBoundingClientRect().top > at.top + 4 : h.getBoundingClientRect().top < at.top - 4));
      if (beyond.length) {
        const tops = beyond.map((h) => h.getBoundingClientRect().top);
        const row = dir > 0 ? Math.min(...tops) : Math.max(...tops);
        next = beyond.filter((h) => Math.abs(h.getBoundingClientRect().top - row) < 4)
          .sort((a, b) => Math.abs(a.getBoundingClientRect().left - at.left) - Math.abs(b.getBoundingClientRect().left - at.left))[0];
      }
    } else if (e.key === 'p' || e.key === 'P') {
      const r = recentOf(hit.dataset.k); if (r) pin(r);
      e.preventDefault(); return;
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      const r = recentOf(hit.dataset.k);
      if (r) forget(r, () => queueMicrotask(() => focusHit(all[i + 1] ?? all[i - 1])));
      e.preventDefault(); return;
    } else return;
    e.preventDefault();
    focusHit(next);
  }
  const tab = (i: number) => (i === 0 ? 0 : -1);
</script>

<div class="rec">
  <div class="head">
    <h2>Recent projects</h2>
    {#if filters.length > 1}
      <div class="chips" role="group" aria-label="Filter recent projects">
        <Button variant={filter === 'all' ? 'tonal' : 'outline'} size="xs" onclick={() => setFilter('all')}>All</Button>
        {#each filters as f (f)}
          <Button variant={filter === f ? 'tonal' : 'outline'} size="xs" onclick={() => setFilter(f)}>{productNames[f] ?? f}</Button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" bind:this={box} onkeydown={onKey}>
    {#if view.order.length === 0}
      <EmptyState message={filter === 'all' ? 'Nothing opened yet' : 'Nothing here'}
        description={filter === 'all'
          ? 'Projects you open — repositories, Java projects, pieces — show up here, whichever product opened them.'
          : 'No recent project of this product.'} />
    {/if}

    {#if view.pinned.length}
      <div class="pins">
        {#each view.pinned as r, i (recentKey(r))}
          <button type="button" class="pin" data-k={recentKey(r)} tabindex={tab(i)} onclick={() => onopen(r)}>
            <ProductIcon id={r.product} size={22} />
            <b>{r.name}</b>
            <span class="star" aria-hidden="true"><Star size={12} fill="currentColor" /></span>
          </button>
        {/each}
      </div>
    {/if}

    {#each view.groups as g (g.label)}
      <div class="rg">
        <h3>{g.label}</h3>
        <div class="rgrid">
          {#each g.items as r (recentKey(r))}
            {@const n = view.order.indexOf(r)}
            <div class="rc" data-k={recentKey(r)} style="--c:var(--accent)">
              <button type="button" class="rc-hit" data-k={recentKey(r)} tabindex={view.pinned.length ? -1 : (n === 0 ? 0 : -1)}
                      aria-label="{r.name}, {productNames[r.product] ?? r.product}, {whenLabel(r.opened_at, now())}" onclick={() => onopen(r)}></button>
              <span class="ico"><ProductIcon id={r.product} size={40} /></span>
              <span class="tx">
                <b>{r.name}</b>
                <span class="meta">{productNames[r.product] ?? r.product}</span>
                <span class="path">{r.path}</span>
              </span>
              <span class="side">
                <span class="when">{whenLabel(r.opened_at, now())}{#if n < 9} <kbd>{n + 1}</kbd>{/if}</span>
                <span class="acts">
                  <IconButton tooltip="Pin to the top" shortcut="P" size={28} onclick={() => pin(r)}><Star size={14} /></IconButton>
                  <IconButton tooltip="Remove from the list" shortcut="Del" size={28} variant="danger" onclick={() => forget(r)}><X size={14} /></IconButton>
                </span>
              </span>
            </div>
          {/each}
        </div>
      </div>
    {/each}

    {#if view.hidden > 0}
      <Button variant="outline" size="sm" onclick={more}>Show {view.hidden} more</Button>
    {/if}
  </div>
</div>

<style>
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
  h2 { margin: 0; font-size: var(--font-size-sm); font-weight: 600; letter-spacing: 0.07em; text-transform: uppercase; color: var(--text-muted); }
  .chips { margin-left: auto; display: flex; gap: 6px; }

  .pins { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 18px; }
  .pin {
    display: inline-flex; align-items: center; gap: 9px; padding: 6px 12px 6px 7px; border-radius: 999px; cursor: pointer;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle, var(--border)); color: var(--text-primary); font: inherit; font-size: 13px;
    transition: border-color 400ms, background 500ms, transform 400ms;
  }
  .pin:hover, .pin:focus-visible { border-color: color-mix(in srgb, var(--accent) 60%, var(--border)); background: var(--bg-hover); transform: translateY(-2px); outline: 0; }
  .pin b { font-weight: 600; }
  .star { color: var(--warning); display: flex; }

  .rg { margin-top: 20px; }
  .rg:first-child { margin-top: 0; }
  h3 { display: flex; align-items: center; gap: 12px; margin: 0 0 10px; font-size: 12px; font-weight: 500; color: var(--text-muted); }
  h3::after { content: ''; flex: 1; height: 1px; background: var(--border-subtle, var(--border)); }
  .rgrid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
  @media (max-width: 760px) { .rgrid { grid-template-columns: 1fr; } }

  .rc {
    position: relative; display: flex; align-items: center; gap: 14px; padding: 14px 16px 14px 14px; overflow: hidden; border-radius: 14px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle, var(--border));
    transition: border-color 500ms, transform 500ms cubic-bezier(0.22, 0.8, 0.22, 1), box-shadow 600ms;
  }
  /* the card is washed with colour from the left, slowly */
  .rc::before {
    content: ''; position: absolute; inset: 0; pointer-events: none; opacity: 0; transform: translateX(-30%);
    background: linear-gradient(100deg, color-mix(in srgb, var(--accent) 20%, transparent), transparent 62%);
    transition: opacity 600ms, transform 700ms cubic-bezier(0.22, 0.8, 0.22, 1);
  }
  .rc:hover, .rc:focus-within { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); transform: translateY(-3px); box-shadow: 0 14px 30px color-mix(in srgb, var(--accent) 14%, transparent); }
  .rc:hover::before, .rc:focus-within::before { opacity: 1; transform: none; }
  .rc-hit { position: absolute; inset: 0; border: 0; background: none; cursor: pointer; border-radius: inherit; }
  .rc-hit:focus-visible { outline: 2px solid var(--accent); outline-offset: -3px; }

  .ico { position: relative; flex: none; display: flex; pointer-events: none; transition: transform 550ms cubic-bezier(0.3, 1.5, 0.5, 1); }
  .rc:hover .ico, .rc:focus-within .ico { transform: scale(1.1) rotate(-5deg); }
  .tx { position: relative; flex: 1; min-width: 0; pointer-events: none; display: flex; flex-direction: column; }
  .tx b { font-size: 15px; font-weight: 600; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { font-size: 12px; color: var(--text-secondary); }
  .path {
    font-family: var(--font-code); font-size: 11px; color: var(--text-disabled);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; direction: rtl; text-align: left;
  }
  .side { position: relative; flex: none; display: flex; align-items: center; justify-content: flex-end; min-width: 72px; }
  .when { font-size: 12px; color: var(--text-muted); pointer-events: none; transition: opacity 300ms; }
  .rc:hover .when, .rc:focus-within .when { opacity: 0; }
  kbd { font: 10px var(--font-code); padding: 0 5px; border: 1px solid var(--border); border-radius: 5px; color: var(--text-secondary); }
  .acts { position: absolute; right: 0; display: flex; gap: 2px; opacity: 0; transform: translateX(10px); pointer-events: none;
          transition: opacity 450ms 80ms, transform 500ms cubic-bezier(0.22, 0.8, 0.22, 1) 80ms; }
  .rc:hover .acts, .rc:focus-within .acts { opacity: 1; transform: none; pointer-events: auto; }
</style>
