<script lang="ts">
  /**
   * What the picture's marks mean — drawn, not described.
   *
   * ## Why this exists as a panel
   *
   * The footer used to carry three words (`dashed: dev`) and that was not a legend, it was a hint that
   * a legend was missing: the first question a reader asked was still "what is the difference between
   * the solid and the dashed lines". A convention nobody can decode is worse than no convention,
   * because the reader invents one — and guessing the arrow direction backwards inverts every
   * conclusion the window exists to support.
   *
   * ## Samples, not sentences
   *
   * Each row shows the actual mark beside its meaning. "Dashed" as a word next to "dotted" as a word
   * is two things you have to hold in your head and match against the screen; the mark itself is
   * recognised rather than recalled.
   *
   * The stroke values below deliberately mirror `GraphCanvas`'s. They are four numbers and Svelte
   * scopes styles per component, so sharing them would mean a `:global` or a stylesheet that owns
   * another component's internals — both worse than this. **If a line's appearance changes there,
   * change it here**: a legend that has drifted from the drawing is actively misleading.
   */
  import { X } from 'lucide-svelte';
  import { moduleWord } from '$lib/ipc/bennu/deps';

  let {
    /** `cargo` or `maven` — decides the vocabulary on both halves. */
    ecosystem,
    onClose,
  }: { ecosystem: string; onClose: () => void } = $props();

  const cargo = $derived(ecosystem === 'cargo');
  const words = $derived(moduleWord(ecosystem, true));
  /** The scope that does not order a build, in the ecosystem's own word. */
  const soft = $derived(cargo ? 'dev-dependency' : 'test scope');

  /** The box kinds, per ecosystem. Same colours as the drawing's kind bar. */
  const kinds = $derived(
    cargo
      ? [
          { cls: 'k-lib', label: 'library' },
          { cls: 'k-bin', label: 'program' },
          { cls: 'k-lib-bin', label: 'both' },
          { cls: 'k-proc-macro', label: 'proc-macro' },
        ]
      : [
          { cls: 'k-jar', label: 'jar' },
          { cls: 'k-war', label: 'war / ear' },
          { cls: 'k-pom', label: 'aggregator' },
        ],
  );
</script>

<div class="gl" role="group" aria-label="Legend">
  <div class="gl-head">
    <span>Legend</span>
    <button type="button" aria-label="Close the legend" onclick={onClose}><X size={12} /></button>
  </div>

  <p class="gl-lead">
    <strong>A → B means A depends on B.</strong> Layers read left to right — dependents first, the
    foundation last — so every arrow in a healthy project points rightwards, and
    <strong>a leftward arrow is a cycle</strong>.
  </p>

  <div class="gl-rows">
    <div class="gl-row">
      <svg viewBox="0 0 44 8" aria-hidden="true"><path class="s-normal" d="M 1 4 H 40" /></svg>
      <span class="gl-name">solid</span>
      <span class="gl-what">
        an ordinary dependency — {cargo
          ? 'a plain one, or a build-dependency'
          : 'compile, provided or runtime scope'}. It orders the build.
      </span>
    </div>
    <div class="gl-row">
      <svg viewBox="0 0 44 8" aria-hidden="true"><path class="s-soft" d="M 1 4 H 40" /></svg>
      <span class="gl-name">dashed</span>
      <span class="gl-what">
        a <strong>{soft}</strong>. Real, and it does <em>not</em> order the build
        {#if cargo}
          — cargo compiles a crate's tests as a separate unit, so it may legally close a cycle
        {:else}
          — but Maven still refuses a cycle through it, because it orders the whole reactor as one graph
        {/if}. It counts towards what rebuilds either way.
      </span>
    </div>
    <div class="gl-row">
      <svg viewBox="0 0 44 8" aria-hidden="true"><path class="s-optional" d="M 1 4 H 40" /></svg>
      <span class="gl-name">dotted</span>
      <span class="gl-what">
        {cargo ? 'optional — only on the graph when a feature turns it on' : 'optional — the consumer has to ask for it'}.
        Whether it is active is not a fact about the manifest, so it is shown and labelled rather than
        guessed at.
      </span>
    </div>
    <div class="gl-row">
      <svg viewBox="0 0 44 8" aria-hidden="true"><path class="s-cycle" d="M 1 4 H 40" /></svg>
      <span class="gl-name gl-bad">red</span>
      <span class="gl-what">part of a <strong>cycle</strong> — {cargo ? 'cargo' : 'Maven'} refuses to build this.</span>
    </div>
    <div class="gl-row">
      <svg viewBox="0 0 44 8" aria-hidden="true"><path class="s-hot" d="M 1 4 H 40" /></svg>
      <span class="gl-name gl-hot">blue</span>
      <span class="gl-what">touching the {moduleWord(ecosystem)} you have selected, or the one under the pointer.</span>
    </div>
  </div>

  <div class="gl-sep"></div>

  <div class="gl-kinds">
    <span class="gl-kinds-label">the bar on a box:</span>
    {#each kinds as k (k.cls)}
      <span class="gl-kind"><i class={k.cls}></i>{k.label}</span>
    {/each}
  </div>

  <ul class="gl-notes">
    <li><b>42</b> on the right of a box — how many {words} rebuild when it changes.</li>
    <li>A red border means the {moduleWord(ecosystem)} is in a cycle; a blue one, that it is selected.</li>
    <li>An edge crossing several layers is routed <em>between</em> the boxes, not through them.</li>
  </ul>
</div>

<style>
  .gl {
    position: absolute;
    right: 10px;
    bottom: 10px;
    z-index: 3;
    width: 372px;
    max-height: 82%;
    overflow: auto;
    display: flex; flex-direction: column; gap: 7px;
    padding: 9px 10px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg, 0 8px 28px rgb(0 0 0 / 45%));
  }
  .gl-head {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary);
  }
  .gl-head button {
    margin-left: auto;
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; padding: 0;
    background: none; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
  }
  .gl-head button:hover { background: var(--bg-hover); color: var(--text-primary); }

  .gl-lead {
    margin: 0;
    font-size: var(--font-size-2xs); line-height: 1.45; color: var(--text-secondary);
  }
  .gl-lead strong { color: var(--text-primary); }

  .gl-rows { display: flex; flex-direction: column; gap: 6px; }
  /* Sample, name, meaning — the sample first, because that is the column the eye arrives from the
     drawing with. */
  .gl-row {
    display: grid;
    grid-template-columns: 44px 46px 1fr;
    align-items: start;
    gap: 7px;
  }
  .gl-row svg { width: 44px; height: 8px; margin-top: 5px; overflow: visible; }
  .gl-name {
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-muted);
    padding-top: 1px;
  }
  .gl-bad { color: var(--error); }
  .gl-hot { color: var(--accent); }
  .gl-what {
    font-size: var(--font-size-3xs); line-height: 1.5; color: var(--text-secondary);
  }
  .gl-what strong { color: var(--text-primary); }

  /* Mirrors GraphCanvas's edge strokes — see the note in the module doc. */
  .gl-row path { fill: none; stroke-width: 1.4; }
  .s-normal { stroke: var(--border-strong); }
  .s-soft { stroke: var(--border-strong); stroke-dasharray: 5 4; }
  .s-optional { stroke: var(--border-strong); stroke-dasharray: 2 3; }
  .s-cycle { stroke: var(--error); stroke-width: 1.8; }
  .s-hot { stroke: var(--accent); stroke-width: 1.8; }

  .gl-sep { height: 1px; background: var(--border-subtle); }

  .gl-kinds {
    display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px;
    font-size: var(--font-size-3xs); color: var(--text-secondary);
  }
  .gl-kinds-label { color: var(--text-disabled); }
  .gl-kind { display: inline-flex; align-items: center; gap: 5px; }
  .gl-kind i {
    width: 3px; height: 12px; border-radius: 1.5px; background: var(--border-strong);
  }
  /* The same four colours the drawing uses. */
  .k-lib, .k-jar { background: var(--info); }
  .k-bin, .k-war { background: var(--success); }
  .k-lib-bin { background: var(--warning); }
  .k-proc-macro { background: var(--accent); }
  .k-pom { background: var(--border-strong); }

  .gl-notes {
    margin: 0; padding-left: 14px;
    display: flex; flex-direction: column; gap: 3px;
    font-size: var(--font-size-3xs); line-height: 1.45; color: var(--text-muted);
  }
  .gl-notes b { font-family: var(--font-code); color: var(--text-secondary); }
</style>
