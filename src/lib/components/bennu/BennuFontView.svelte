<script lang="ts">
  /**
   * A font file, in an editor tab.
   *
   * ## Why a viewer and not an icon
   *
   * A `.ttf` in a project tree is there for a reason — it is the game's UI face, or the one a
   * PDF is generated with — and every question you have about it is visual: what does it look
   * like, does it have the accents this project needs, does the ligature you were promised
   * exist. Refusing to open it (which is what Bennu did, as a "binary file") sent you to a
   * system font previewer to answer questions about a file you were looking at.
   *
   * ## No parser, on purpose
   *
   * The browser already has one. `FontFace` takes the bytes, and from there the font is a font:
   * it renders, it measures, and what it cannot draw it draws as `.notdef`. So coverage is
   * measured rather than read out of a `cmap` table — a glyph is present when it measures
   * differently from the replacement box, which is the same test a reader's eye performs.
   *
   * That is a real trade: it cannot name a glyph, list OpenType features, or say which script a
   * font declares. Those need a table parser, and a table parser is a dependency. What it buys
   * is that this file works on any font the browser can load, today, with nothing added.
   *
   * ## Read-only, like every preview
   *
   * The file never enters the source cache (see `opensAsPreview`), so there is no buffer for a
   * stray Ctrl+S to write back over a binary.
   */
  import { Type, ExternalLink, Lock } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import { fsReadBytes } from '$lib/ipc/fs';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { baseName } from '$lib/utils/paths';
  import { formatBytes } from '$lib/utils/format';
  import { tooltip } from '$lib/actions/tooltip';

  let { path }: { path: string } = $props();

  /** The sizes a specimen is read at. A waterfall rather than one size, because what a face is
   *  FOR is decided by how it holds up small and how it opens up large. */
  const WATERFALL = [11, 13, 16, 20, 28, 40, 56, 72];

  /** The blocks whose coverage is worth reporting for the projects this opens in — Latin and
   *  its accents, the symbols a UI uses, and the arrows and box drawing a terminal font lives
   *  or dies by. Not the whole of Unicode: measuring 150k code points to render a summary is a
   *  second of work for an answer nobody reads. */
  const BLOCKS: { label: string; from: number; to: number }[] = [
    { label: 'Basic Latin', from: 0x0020, to: 0x007e },
    { label: 'Latin-1 accents', from: 0x00a0, to: 0x00ff },
    { label: 'Latin Extended-A', from: 0x0100, to: 0x017f },
    { label: 'Greek', from: 0x0370, to: 0x03ff },
    { label: 'Cyrillic', from: 0x0400, to: 0x04ff },
    { label: 'Punctuation', from: 0x2000, to: 0x206f },
    { label: 'Currency', from: 0x20a0, to: 0x20bf },
    { label: 'Arrows', from: 0x2190, to: 0x21ff },
    { label: 'Maths', from: 0x2200, to: 0x22ff },
    { label: 'Box drawing', from: 0x2500, to: 0x257f },
    { label: 'Powerline', from: 0xe0a0, to: 0xe0d4 },
  ];

  const SAMPLE = 'The quick brown fox jumps over the lazy dog — perché è così? 0123456789';

  /** A family name of our own. The file's own name may collide with an installed font, and then
   *  what you would be looking at is the one on your machine.
   *
   *  `$derived`, because the tab is reused when you open a second font: a family computed once
   *  would leave the new file registered under the old file's name, and the preview would show
   *  the previous font while claiming to be this one. */
  const family = $derived(`bennu-font-${Math.abs(hash(path))}`);

  function hash(s: string): number {
    let h = 0;
    for (let i = 0; i < s.length; i += 1) h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
    return h;
  }

  let loading = $state(true);
  let error = $state('');
  let bytes = $state(0);
  let ready = $state(false);
  let coverage = $state<{ label: string; have: number; total: number }[]>([]);

  let specimen = $state(SAMPLE);
  let size = $state(40);
  let weight = $state(400);
  let italic = $state(false);
  let tracking = $state(0);

  $effect(() => {
    const file = path;
    const fam = family;
    let cancelled = false;
    let loaded: FontFace | null = null;

    loading = true;
    error = '';
    ready = false;

    void (async () => {
      try {
        const data = toBytes(await fsReadBytes(file));
        if (cancelled) return;
        bytes = data.byteLength;
        const face = new FontFace(fam, data.buffer as ArrayBuffer);
        await face.load();
        if (cancelled) return;
        document.fonts.add(face);
        loaded = face;
        ready = true;
        loading = false;
        coverage = measureCoverage(fam);
      } catch (e) {
        if (cancelled) return;
        // A font the browser refuses is a font it cannot render either, so there is nothing to
        // fall back to — say which file and what it said.
        error = e instanceof Error ? e.message : String(e);
        loading = false;
      }
    })();

    return () => {
      cancelled = true;
      // Registered globally, so it has to be taken back: a project with forty fonts in it would
      // otherwise accumulate forty faces in the document for tabs that are long closed.
      if (loaded) document.fonts.delete(loaded);
    };
  });

  /** Decode the base64 the IPC seam hands back, in one pass. */
  function toBytes(b64: string): Uint8Array {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
    return out;
  }

  /**
   * How much of each block the font actually draws.
   *
   * Measured, not read: a code point is covered when its advance width differs from the one the
   * font gives a code point that certainly is not there (a private-use character). That is the
   * same judgement a reader makes — if it comes out as a box, it is not there — and it needs no
   * table parser.
   */
  function measureCoverage(fam: string): { label: string; have: number; total: number }[] {
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    if (!ctx) return [];
    ctx.font = `64px "${fam}"`;
    // The reference: a private-use code point no real font assigns, so whatever it measures is
    // this font's replacement glyph.
    const missing = ctx.measureText('').width;
    return BLOCKS.map((block) => {
      let have = 0;
      let total = 0;
      for (let cp = block.from; cp <= block.to; cp += 1) {
        const ch = String.fromCodePoint(cp);
        total += 1;
        const w = ctx.measureText(ch).width;
        // A zero-width result is a control or a combining mark rendered as nothing — not a
        // missing glyph, and counting it as one would report every font as gap-ridden.
        if (w > 0 && Math.abs(w - missing) > 0.01) have += 1;
      }
      return { label: block.label, have, total };
    });
  }

  const style = $derived(
    `font-family: "${family}", sans-serif; font-weight: ${weight}; ` +
      `font-style: ${italic ? 'italic' : 'normal'}; letter-spacing: ${tracking}px;`,
  );
</script>

<div class="fv">
  <div class="fv-bar">
    <Type size={13} />
    <span class="fv-name">{baseName(path)}</span>
    {#if bytes}<span class="fv-meta">{formatBytes(bytes)}</span>{/if}
    <span class="fv-ro" use:tooltip={'Bennu renders this font; it never writes to it'}>
      <Lock size={10} /> Read-only
    </span>
    <button
      class="fv-open"
      type="button"
      use:tooltip={'Open in the system application'}
      aria-label="Open in the system application"
      onclick={() => void openPath(path).catch(() => {})}
    >
      <ExternalLink size={13} />
    </button>
  </div>

  {#if error}
    <div class="fv-state">
      <EmptyState message="This font could not be loaded." description={error} />
    </div>
  {:else if loading}
    <div class="fv-state"><Spinner size="lg" label="Loading the face…" /></div>
  {:else if ready}
    <div class="fv-body">
      <!-- Left: what you type. Right: what the file is. The split is the point of the view —
           a specimen you cannot change is a picture, and the question is always "how does MY
           text look in it". -->
      <div class="fv-live">
        <label class="fv-label" for="fv-input">Type here</label>
        <textarea
          id="fv-input"
          class="fv-input"
          bind:value={specimen}
          spellcheck="false"
          rows="2"
        ></textarea>

        <div class="fv-controls">
          <label class="fv-ctl">
            <span>Size</span>
            <input type="range" min="8" max="160" step="1" bind:value={size} />
            <span class="fv-val">{size}px</span>
          </label>
          <label class="fv-ctl">
            <span>Weight</span>
            <input type="range" min="100" max="900" step="100" bind:value={weight} />
            <span class="fv-val">{weight}</span>
          </label>
          <label class="fv-ctl">
            <span>Tracking</span>
            <input type="range" min="-4" max="12" step="0.5" bind:value={tracking} />
            <span class="fv-val">{tracking}px</span>
          </label>
          <label class="fv-check">
            <input type="checkbox" bind:checked={italic} />
            <span>Italic</span>
          </label>
        </div>

        <div class="fv-preview" style={`${style} font-size: ${size}px;`}>{specimen}</div>

        <div class="fv-label">Waterfall</div>
        <div class="fv-waterfall">
          {#each WATERFALL as px}
            <div class="fv-wf-row">
              <span class="fv-wf-size">{px}</span>
              <span class="fv-wf-text" style={`${style} font-size: ${px}px;`}>{SAMPLE}</span>
            </div>
          {/each}
        </div>
      </div>

      <ResizablePanel direction="horizontal" initialSize={280} minSize={200} maxSize={480} reverse>
        <div class="fv-side">
          <div class="fv-label">Coverage</div>
          <p class="fv-note">
            Measured by rendering, not read from the font's tables — a code point counts when it
            draws as something other than the replacement box.
          </p>
          <ul class="fv-blocks">
            {#each coverage as block (block.label)}
              {@const pct = Math.round((block.have / block.total) * 100)}
              <li>
                <span class="fv-block-name">{block.label}</span>
                <span class="fv-bar-track"><span class="fv-bar-fill" style={`width: ${pct}%`}></span></span>
                <span class="fv-block-pct" class:full={pct === 100} class:none={pct === 0}>{pct}%</span>
              </li>
            {/each}
          </ul>
        </div>
      </ResizablePanel>
    </div>
  {/if}
</div>

<style>
  .fv { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }

  .fv-bar {
    display: flex; align-items: center; gap: 8px; flex: none;
    height: 28px; padding: 0 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .fv-name { color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fv-meta { color: var(--text-faint); }
  .fv-ro {
    margin-left: auto;
    display: inline-flex; align-items: center; gap: 4px;
    height: 17px; padding: 0 7px;
    border: 1px solid color-mix(in srgb, var(--warning) 45%, transparent);
    border-radius: 9px;
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    color: var(--warning);
    font-size: var(--font-size-2xs); font-weight: 600;
    letter-spacing: 0.04em; white-space: nowrap;
  }
  .fv-open {
    display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 20px; padding: 0;
    background: none; border: 0; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
  }
  .fv-open:hover { background: var(--bg-hover); color: var(--text-primary); }

  .fv-state { flex: 1; display: flex; align-items: center; justify-content: center; min-height: 0; }

  .fv-body { flex: 1; min-height: 0; display: flex; }

  .fv-live {
    flex: 1; min-width: 0; min-height: 0;
    overflow: auto;
    padding: 14px 16px;
    display: flex; flex-direction: column; gap: 10px;
  }

  .fv-label {
    font-size: var(--font-size-2xs); font-weight: 600;
    letter-spacing: 0.06em; text-transform: uppercase;
    color: var(--text-muted);
  }

  .fv-input {
    width: 100%;
    padding: 7px 9px;
    background: var(--bg-input); border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
    resize: vertical;
  }
  .fv-input:focus { outline: none; border-color: var(--accent); }

  .fv-controls { display: flex; flex-wrap: wrap; gap: 14px; align-items: center; }
  .fv-ctl {
    display: inline-flex; align-items: center; gap: 7px;
    font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .fv-ctl input[type='range'] { width: 110px; }
  .fv-val { min-width: 38px; font-family: var(--font-code); color: var(--text-secondary); }
  .fv-check {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: var(--font-size-xs); color: var(--text-muted);
  }

  /* The specimen. `bg-elevated` rather than the page, so a light face on a dark theme still
     sits on a surface rather than floating. */
  .fv-preview {
    padding: 18px 16px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    line-height: 1.25;
    overflow-wrap: anywhere;
  }

  .fv-waterfall {
    display: flex; flex-direction: column; gap: 6px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .fv-wf-row { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
  .fv-wf-size {
    flex: none; width: 26px; text-align: right;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-faint);
  }
  .fv-wf-text {
    color: var(--text-primary); line-height: 1.2;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .fv-side {
    height: 100%; overflow-y: auto;
    padding: 14px 14px 14px 12px;
    border-left: 1px solid var(--border-subtle);
    display: flex; flex-direction: column; gap: 8px;
  }
  .fv-note { margin: 0; font-size: var(--font-size-2xs); color: var(--text-faint); line-height: 1.5; }

  .fv-blocks { list-style: none; margin: 4px 0 0; padding: 0; display: flex; flex-direction: column; gap: 7px; }
  .fv-blocks li { display: grid; grid-template-columns: 1fr 60px 34px; align-items: center; gap: 8px; }
  .fv-block-name {
    font-size: var(--font-size-xs); color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .fv-bar-track {
    height: 4px; border-radius: 2px;
    background: var(--bg-overlay); overflow: hidden;
  }
  .fv-bar-fill { display: block; height: 100%; background: var(--accent); }
  .fv-block-pct {
    text-align: right;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .fv-block-pct.full { color: var(--success); }
  .fv-block-pct.none { color: var(--text-disabled); }
</style>
