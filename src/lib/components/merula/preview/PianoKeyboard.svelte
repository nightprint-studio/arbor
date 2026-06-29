<script lang="ts">
  /**
   * On-screen keyboard for instrument preview. In **chromatic** mode it's a piano
   * (white + black keys) emitting a MIDI note; in **scale** mode it's a row of
   * scale-degree keys (white keys = scale steps) emitting a degree integer. Either
   * way `onnote` fires the value the host bakes into the preview snippet. Computer
   * keys play only while the keyboard is **focused**, so it never hijacks editor
   * typing. `role="application"` because it owns its own key handling.
   */
  import { onMount } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    from = 48,
    octaves = 2,
    mode = 'chromatic',
    pcKeys = true,
    onnote,
  }: {
    /** Chromatic: MIDI note of the leftmost key (should be a C). Scale: the degree
     *  of the leftmost key. Default 48 (C3 / degree 48 — the host sets it). */
    from?: number;
    /** Octaves to draw (7 keys each). Default 2. */
    octaves?: number;
    /** `chromatic` = piano (emits MIDI); `scale` = degree row (emits a degree). */
    mode?: 'chromatic' | 'scale';
    /** Map the computer keyboard while focused. Default true. */
    pcKeys?: boolean;
    /** Fired with the MIDI note (chromatic) or degree (scale) when a key triggers. */
    onnote: (value: number) => void;
  } = $props();

  // White-key semitone offsets within an octave, and the black key in the gap
  // *after* a given white index (C,D,F,G,A → C#,D#,F#,G#,A#).
  const WHITE_OFFS = [0, 2, 4, 5, 7, 9, 11];
  const BLACK_AFTER: Record<number, number> = { 0: 1, 1: 3, 3: 6, 4: 8, 5: 10 };

  type WhiteKey = { value: number; index: number; root: boolean };
  type BlackKey = { value: number; gap: number };

  const totalWhite = $derived(octaves * 7);

  // White keys: chromatic uses the piano offsets, scale uses contiguous degrees.
  const whites = $derived.by<WhiteKey[]>(() => {
    const out: WhiteKey[] = [];
    for (let i = 0; i < totalWhite; i++) {
      const o = Math.floor(i / 7), j = i % 7;
      const value = mode === 'scale' ? from + i : from + o * 12 + WHITE_OFFS[j];
      out.push({ value, index: i, root: j === 0 });
    }
    return out;
  });

  // Black keys only exist on the chromatic piano.
  const blacks = $derived.by<BlackKey[]>(() => {
    if (mode === 'scale') return [];
    const out: BlackKey[] = [];
    for (let o = 0; o < octaves; o++) {
      for (let j = 0; j < 7; j++) {
        const semi = BLACK_AFTER[j];
        if (semi === undefined) continue;
        out.push({ value: from + o * 12 + semi, gap: o * 7 + j + 1 });
      }
    }
    return out;
  });

  // Visual press-flash: a key stays lit briefly after a hit (the preview note is
  // fixed-length and self-releases, so there's no real note-off to track).
  let lit = $state<Set<number>>(new Set());
  const flashTimers = new Map<number, ReturnType<typeof setTimeout>>();
  function flash(v: number) {
    const next = new Set(lit);
    next.add(v);
    lit = next;
    const prev = flashTimers.get(v);
    if (prev) clearTimeout(prev);
    flashTimers.set(v, setTimeout(() => {
      const n = new Set(lit);
      n.delete(v);
      lit = n;
      flashTimers.delete(v);
    }, 160));
  }

  function hit(v: number) { flash(v); onnote(v); }

  // Drag glissando: hold a key down and slide across the others.
  let dragging = $state(false);
  let rootEl = $state<HTMLElement | null>(null);
  function down(v: number) { dragging = true; rootEl?.focus(); hit(v); }
  function enter(v: number) { if (dragging) hit(v); }
  function stopDrag() { dragging = false; }

  // ── Computer-keyboard mapping (active only while focused) ────────────────────
  // Two-octave layout (Ableton/Bitwig/FL-style): the lower QWERTY rows are the low
  // octave, the upper rows the octave above — so two octaves play with no modifier
  // (Ctrl/Alt would collide with copy/paste/select-all, and Alt+letter is dropped
  // on IT/DE/FR/ES layouts to preserve AltGr). `from` is the low-octave anchor;
  // the ◀/▶ octave buttons slide the whole window.
  let focused = $state(false);
  const PC_CHROMATIC: Record<string, number> = {
    // low octave: Z S X D C V G B H N J M ,
    z: 0, s: 1, x: 2, d: 3, c: 4, v: 5, g: 6, b: 7, h: 8, n: 9, j: 10, m: 11, ',': 12,
    // octave up: Q 2 W 3 E R 5 T 6 Y 7 U I
    q: 12, '2': 13, w: 14, '3': 15, e: 16, r: 17, '5': 18, t: 19, '6': 20, y: 21, '7': 22, u: 23, i: 24,
  };
  const PC_SCALE: Record<string, number> = {
    // low octave degrees: Z X C V B N M ,
    z: 0, x: 1, c: 2, v: 3, b: 4, n: 5, m: 6, ',': 7,
    // octave-up degrees: Q W E R T Y U I
    q: 7, w: 8, e: 9, r: 10, t: 11, y: 12, u: 13, i: 14,
  };
  const pcDown = new Set<string>();
  function onKeyDown(e: KeyboardEvent) {
    if (!pcKeys || e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
    const map = mode === 'scale' ? PC_SCALE : PC_CHROMATIC;
    const step = map[e.key.toLowerCase()];
    if (step === undefined || pcDown.has(e.key)) return;
    pcDown.add(e.key);
    e.preventDefault();
    hit(from + step);
  }
  function onKeyUp(e: KeyboardEvent) { pcDown.delete(e.key); }
  function onBlur() { focused = false; pcDown.clear(); }

  // ── Labels ───────────────────────────────────────────────────────────────────
  const NOTE_DISP = ['C', 'C♯', 'D', 'D♯', 'E', 'F', 'F♯', 'G', 'G♯', 'A', 'A♯', 'B'];
  const WHITE_LETTER: Record<number, string> = { 0: 'C', 2: 'D', 4: 'E', 5: 'F', 7: 'G', 9: 'A', 11: 'B' };
  const oct = (midi: number) => Math.floor(midi / 12) - 1; // MIDI 60 = C4
  const pc = (midi: number) => ((midi % 12) + 12) % 12;

  /** Full note name for tooltips / black keys (e.g. `C♯4`). */
  function noteDisplay(midi: number): string {
    return `${NOTE_DISP[pc(midi)]}${oct(midi)}`;
  }
  /** White-key caption: the letter, with the octave on each C (the anchor). */
  function whiteLabel(midi: number): string {
    return pc(midi) === 0 ? `C${oct(midi)}` : WHITE_LETTER[pc(midi)] ?? '';
  }
  /** Hover title for any key (chromatic: note name; scale: degree). */
  function keyTitle(value: number): string {
    return mode === 'scale' ? `Degree ${value}` : noteDisplay(value);
  }

  onMount(() => () => { for (const t of flashTimers.values()) clearTimeout(t); });
</script>

<svelte:window onpointerup={stopDrag} />

<!-- A playable instrument: the root captures keydown/keyup to sound notes, so it
     must be focusable (tabindex=0) and role="application" hands keyboard control to
     the widget. Both a11y rules below are false positives for this interactive case. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  bind:this={rootEl}
  class="kbd"
  class:focused
  role="application"
  aria-label="Preview keyboard"
  tabindex="0"
  style="--whites: {totalWhite};"
  onkeydown={onKeyDown}
  onkeyup={onKeyUp}
  onfocus={() => (focused = true)}
  onblur={onBlur}
>
  <div class="whites">
    {#each whites as w (w.value)}
      <button
        type="button"
        class="wkey"
        class:lit={lit.has(w.value)}
        class:cnote={mode === 'chromatic' && pc(w.value) === 0}
        class:root={mode === 'scale' && w.root}
        tabindex="-1"
        aria-label={keyTitle(w.value)}
        use:tooltip={keyTitle(w.value)}
        onpointerdown={() => down(w.value)}
        onpointerenter={() => enter(w.value)}
      >
        <span class="wlabel">{mode === 'scale' ? w.value : whiteLabel(w.value)}</span>
      </button>
    {/each}
  </div>
  <div class="blacks">
    {#each blacks as b (b.value)}
      <button
        type="button"
        class="bkey"
        class:lit={lit.has(b.value)}
        tabindex="-1"
        style="left: calc({b.gap} / var(--whites) * 100%);"
        aria-label={noteDisplay(b.value)}
        use:tooltip={noteDisplay(b.value)}
        onpointerdown={() => down(b.value)}
        onpointerenter={() => enter(b.value)}
      ></button>
    {/each}
  </div>
</div>

<style>
  .kbd {
    position: relative;
    width: 100%;
    height: 120px;
    user-select: none;
    touch-action: none;
    border-radius: var(--radius-sm);
    outline: none;
  }
  .kbd.focused { box-shadow: 0 0 0 2px var(--accent); }

  /* No flex gap: black keys are positioned as an exact percentage of the row, so
     the white keys must tile edge-to-edge (the shared 1px borders draw the seams). */
  .whites { display: flex; height: 100%; }
  .wkey {
    flex: 1; min-width: 0;
    display: flex; align-items: flex-end; justify-content: center;
    padding-bottom: 6px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-top: none; border-left: none;
    border-radius: 0 0 var(--radius-sm) var(--radius-sm);
    cursor: pointer; color: var(--text-disabled);
    transition: background var(--transition-fast);
  }
  .wkey:first-child { border-left: 1px solid var(--border); }
  .wkey:hover { background: var(--bg-hover); }
  .wkey.lit { background: var(--accent); color: var(--text-on-accent); }
  /* The C of each octave (and scale-mode roots) anchor the eye. */
  .wkey.cnote .wlabel { color: var(--text-secondary); font-weight: 600; }
  .wkey.root { background: var(--bg-overlay); color: var(--text-muted); }
  .wkey.root.lit { background: var(--accent); color: var(--text-on-accent); }
  .wlabel { font-size: 9px; font-family: var(--font-code); pointer-events: none; }

  .blacks { position: absolute; inset: 0; pointer-events: none; }
  .bkey {
    position: absolute; top: 0;
    width: 7%; height: 62%;
    transform: translateX(-50%);
    background: var(--bg-base);
    border: 1px solid var(--border-strong, var(--border));
    border-top: none;
    border-radius: 0 0 var(--radius-sm) var(--radius-sm);
    cursor: pointer; pointer-events: auto;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.4);
  }
  .bkey:hover { background: var(--bg-hover); }
  .bkey.lit { background: var(--accent); }
</style>
