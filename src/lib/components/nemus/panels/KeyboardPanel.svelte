<script lang="ts">
  /**
   * Keyboard — a piano that lights the notes sounding at the playhead. Read-only
   * monitoring (it never plays): the active notes are derived from the evaluated
   * arrangement (`arrangementStore.lanes`) at the looped transport position, and
   * each lit key takes its track's lane colour. The span auto-fits the
   * arrangement's pitch range (whole octaves), falling back to a default range
   * when nothing pitched has been evaluated yet.
   */
  import { Piano } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { nemusStore } from '../nemus-store.svelte';
  import { transportStore } from '../stores/engine.svelte';
  import { arrangementStore, noteName, type VizLane } from '../viz/arrangement.svelte';
  import { laneColor } from '../palette';

  // Pitch classes that are black keys (C#, D#, F#, G#, A#).
  const BLACK = new Set([1, 3, 6, 8, 10]);
  const isBlack = (m: number) => BLACK.has(((m % 12) + 12) % 12);

  const DEFAULT_LO = 36; // C2
  const DEFAULT_HI = 84; // C6
  const MIN_SPAN = 24;   // at least two octaves visible
  const MAX_SPAN = 60;   // cap so keys stay wide enough to read

  const lanes      = $derived(arrangementStore.lanes);
  const loopCycles = $derived(arrangementStore.loopCycles);
  // Looped playhead position, matching the ruler (the song repeats every period).
  const pos = $derived(
    transportStore.playing
      ? (loopCycles > 0 ? transportStore.cycle % loopCycles : transportStore.cycle)
      : -1, // stopped: nothing sounds
  );

  const hasPitched = $derived(lanes.some((l) => l.noteLo != null));
  const range  = $derived(computeRange(lanes));
  const keys   = $derived(buildKeys(range.lo, range.hi));
  // midi → lane colour of the (lowest-index) track currently sounding it.
  const active = $derived(computeActive(lanes, pos));

  /** Whole-octave pitch span fitted to the arrangement, clamped to a readable width. */
  function computeRange(ls: VizLane[]): { lo: number; hi: number } {
    let lo = Infinity;
    let hi = -Infinity;
    for (const l of ls) {
      if (l.noteLo != null) lo = Math.min(lo, l.noteLo);
      if (l.noteHi != null) hi = Math.max(hi, l.noteHi);
    }
    if (lo === Infinity) { lo = DEFAULT_LO; hi = DEFAULT_HI; }
    lo = Math.floor(lo / 12) * 12;            // down to a C
    hi = Math.ceil((hi + 1) / 12) * 12 - 1;   // up to a B
    if (hi - lo < MIN_SPAN) hi = lo + MIN_SPAN;
    if (hi - lo > MAX_SPAN) hi = lo + MAX_SPAN;
    return { lo, hi };
  }

  /** White + black key descriptors, with black keys positioned (in %) over the
   *  white-key row so the layout is responsive to the panel width. */
  function buildKeys(lo: number, hi: number) {
    const whites: { midi: number; label: string | null }[] = [];
    const whiteIndex = new Map<number, number>();
    for (let m = lo; m <= hi; m++) {
      if (!isBlack(m)) {
        whiteIndex.set(m, whites.length);
        whites.push({ midi: m, label: m % 12 === 0 ? noteName(m) : null });
      }
    }
    const w = whites.length ? 100 / whites.length : 100;
    const blacks: { midi: number; leftPct: number }[] = [];
    for (let m = lo; m <= hi; m++) {
      if (!isBlack(m)) continue;
      const left = whiteIndex.get(m - 1); // the white key just below (C# sits after C)
      if (left == null) continue;
      blacks.push({ midi: m, leftPct: (left + 1) * w });
    }
    return { whites, blacks, w };
  }

  /** The notes sounding at `pos`, mapped to their owning track's colour. Lowest
   *  track index wins a shared note (stable colour). `pos < 0` (stopped) = none. */
  function computeActive(ls: VizLane[], at: number): Map<number, string> {
    const out = new Map<number, string>();
    if (at < 0) return out;
    for (const l of ls) {
      const c = laneColor(l.track);
      for (const h of l.haps) {
        if (h.note == null) continue;
        if (h.start <= at && at < h.end && !out.has(h.note)) out.set(h.note, c);
      }
    }
    return out;
  }
</script>

<div class="kbp">
  <BottomPanelHeader title="Keyboard" onClose={() => nemusStore.toggleBottom('keyboard')}>
    {#snippet icon()}<Piano size={13} />{/snippet}
    {#snippet children()}
      <span class="kbp-meta">{active.size} sounding</span>
    {/snippet}
  </BottomPanelHeader>

  <div class="kbp-body">
    {#if !hasPitched}
      <EmptyState message="Run an arrangement with pitched voices — the keys light up as it plays." />
    {:else}
      <div class="piano" role="img" aria-label="Live keyboard">
        <div class="whites">
          {#each keys.whites as k (k.midi)}
            <div class="wk" class:on={active.has(k.midi)} style:--c={active.get(k.midi) ?? 'transparent'}>
              {#if k.label}<span class="oct">{k.label}</span>{/if}
            </div>
          {/each}
        </div>
        {#each keys.blacks as b (b.midi)}
          <div
            class="bk"
            class:on={active.has(b.midi)}
            style="left: {b.leftPct}%; width: calc({keys.w}% * 0.62); --c: {active.get(b.midi) ?? 'transparent'};"
          ></div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .kbp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .kbp-meta { color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .kbp-body { flex: 1; min-height: 0; display: flex; padding: 10px 12px; }

  /* Piano: white keys fill the row; black keys overlay at computed offsets. */
  .piano {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
    min-height: 60px;
  }
  .whites { display: flex; height: 100%; gap: 1px; }
  .wk {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding-bottom: 4px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-top: none;
    border-radius: 0 0 var(--radius-sm) var(--radius-sm);
    transition: background var(--transition-fast), box-shadow var(--transition-fast);
  }
  .wk.on {
    background: var(--c);
    box-shadow: 0 0 10px color-mix(in srgb, var(--c) 60%, transparent);
  }
  .wk .oct { font-size: 9px; font-weight: 600; color: var(--text-disabled); pointer-events: none; }
  .wk.on .oct { color: color-mix(in srgb, #000 55%, var(--c)); }

  .bk {
    position: absolute;
    top: 0;
    height: 62%;
    transform: translateX(-50%);
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-top: none;
    border-radius: 0 0 var(--radius-sm) var(--radius-sm);
    box-shadow: 0 2px 3px rgba(0, 0, 0, 0.35);
    transition: background var(--transition-fast), box-shadow var(--transition-fast);
  }
  .bk.on {
    background: var(--c);
    box-shadow: 0 0 10px color-mix(in srgb, var(--c) 70%, transparent);
  }
</style>
