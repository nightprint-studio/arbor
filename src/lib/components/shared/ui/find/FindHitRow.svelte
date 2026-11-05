<script lang="ts">
  /** One hit line inside a find list: position, then the line with its match marked. */
  import type { FindHitItem, FindSegment } from './find-types';

  interface Props {
    hit: FindHitItem;
    segments: FindSegment[];
    selected: boolean;
    external?: boolean;
    onopen: () => void;
    onhover: () => void;
  }

  let { hit, segments, selected, external = false, onopen, onhover }: Props = $props();
</script>

<button
  type="button"
  class="fh"
  class:ext={external}
  class:sel={selected}
  onclick={onopen}
  onmousemove={onhover}
>
  {#if hit.location}<span class="fh-loc">{hit.location}</span>{/if}
  <span class="fh-text">{#each segments as s, i (i)}{#if s.hit}<mark class="fh-mark">{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
</button>

<style>
  .fh {
    display: flex; align-items: center; gap: 10px; height: 100%;
    width: 100%; text-align: left;
    padding: 0 14px 0 30px; background: transparent; border: none; cursor: pointer;
  }
  .fh.sel { background: var(--bg-selected); }
  .fh:hover { background: var(--bg-hover); }
  .fh.sel:hover { background: var(--bg-selected); }

  .fh-loc {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled);
    flex-shrink: 0; min-width: 44px;
  }
  .fh-text {
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
  }
  /* A hit has to be findable at a glance in a wall of monospace, which a faint tint is not —
     this is the one place in the list that earns a solid fill: it is the answer to the question. */
  .fh-mark {
    background: var(--accent);
    color: var(--bg-base);
    border-radius: 2px;
    padding: 0 2px;
    font-weight: 600;
  }
  /* On the selected row the fill would fight the selection band, so the hit inverts instead. */
  .fh.sel .fh-mark { background: var(--accent-hover); }

  /* Not the user's own — read-only content. Same hue as the editor's external tabs: not an
     error, but not one of your files either. The edge bar makes a run of them legible as a block. */
  .fh.ext {
    background: color-mix(in srgb, var(--warning) 7%, transparent);
    box-shadow: inset 2px 0 0 color-mix(in srgb, var(--warning) 45%, transparent);
  }
  .fh.ext:hover { background: color-mix(in srgb, var(--warning) 12%, transparent); }
  /* The selection has to win over the tint; equal specificity would leave it to source order. */
  .fh.ext.sel, .fh.ext.sel:hover {
    background: var(--bg-selected);
    box-shadow: inset 2px 0 0 var(--warning);
  }
</style>
