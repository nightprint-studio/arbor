<script lang="ts">
  /**
   * The inside of a note row: type dot, title, trailing meta.
   *
   * One component rather than three copies, because four surfaces draw this exact
   * row — the vault tree, the pinned list, the recents list and (with the dot
   * alone) the tab strip. A dot that is 6px in one of them and 5px in another is
   * the kind of drift nobody reports and everybody sees.
   *
   * It renders content only: the interactive wrapper is the caller's, because in
   * the tree that wrapper is a `Tree` row and in the lists it is a `<button>`.
   */
  import { Pin } from 'lucide-svelte';

  interface Props {
    title: string;
    /** The note type's colour, or `null` for an untyped note. */
    accent?: string | null;
    /** Right-aligned afterword — how long ago, usually. */
    meta?: string | null;
    /** Show the pin glyph ahead of the dot. */
    pinned?: boolean;
    /** The note has unsaved bytes. */
    dirty?: boolean;
    /** Dim the row: a note the vault lists but this window cannot open. */
    muted?: boolean;
  }

  let { title, accent = null, meta = null, pinned = false, dirty = false, muted = false }: Props =
    $props();
</script>

<span class="nrc" class:muted>
  {#if pinned}<span class="nrc-pin"><Pin size={11} /></span>{/if}
  <span class="nrc-dot" style:background={accent ?? 'var(--text-disabled)'}></span>
  <span class="nrc-name">{title}</span>
  {#if dirty}<span class="nrc-dirty" aria-label="Unsaved changes"></span>{/if}
  {#if meta}<span class="nrc-meta">{meta}</span>{/if}
</span>

<style>
  .nrc {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .nrc.muted { color: var(--text-disabled); }

  .nrc-pin {
    display: inline-flex;
    flex: none;
    color: var(--text-muted);
  }

  /* A rounded square rather than a circle: it reads as "kind", while the round
     dot beside it means "unsaved". Two shapes, two meanings. */
  .nrc-dot {
    width: 6px;
    height: 6px;
    border-radius: 2px;
    flex: none;
  }
  .nrc.muted .nrc-dot { opacity: 0.5; }

  .nrc-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .nrc-dirty {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex: none;
    background: var(--warning);
  }

  .nrc-meta {
    flex: none;
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
  }
</style>
