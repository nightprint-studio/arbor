<script lang="ts">
  /**
   * A picture in the thread — a screenshot the assistant took, an image it showed, one you attached — as a small
   * preview that opens large in a lightbox (Enter on it, or a click). The thread carries only a path into the
   * pictures folder; the bytes are fetched when this appears (`assistant-images`). With a `gallery`, the
   * lightbox walks the other pictures of the message with the arrow keys.
   *
   * `small` is the attachment form: a square thumbnail with no caption, for a row of them.
   */
  import { Image as ImageIcon, Maximize2 } from 'lucide-svelte';
  import { imageLightbox } from '$lib/stores/imageLightbox.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { loadPicture, type Picture } from './assistant-images';

  let {
    path = '',
    caption,
    src: given = null,
    gallery = [],
    small = false,
  }: {
    /** Where the picture is kept; empty when `src` is given. */
    path?: string;
    caption: string;
    /** A picture that is not kept yet (one about to be sent): used as it is. */
    src?: string | null;
    gallery?: Picture[];
    small?: boolean;
  } = $props();

  let loaded = $state<string | null>(null);
  let failed = $state(false);

  const src = $derived(given ?? loaded);

  $effect(() => {
    const wanted = path;
    loaded = null;
    failed = false;
    if (given || !wanted) return;
    loadPicture(wanted)
      .then((url) => { if (wanted === path) loaded = url; })
      .catch(() => { failed = true; });
  });

  async function open() {
    if (!src) return;
    const set = gallery.length > 1 && gallery.some((g) => g.path === path) ? gallery : [{ path, caption }];
    const entries = await Promise.all(
      set.map(async (g) => {
        try {
          return { path: g.path, caption: g.caption, src: g.path === path ? src : await loadPicture(g.path) };
        } catch {
          return null; // a picture that is gone is left out of the walk
        }
      }),
    );
    const ok = entries.filter((e): e is NonNullable<typeof e> => e !== null);
    imageLightbox.show(ok.map(({ src, caption }) => ({ src, alt: caption })), Math.max(0, ok.findIndex((e) => e.path === path)));
  }
</script>

<div class="pic" class:small>
  {#if src}
    <button type="button" class="thumb" onclick={open} aria-label={`Enlarge ${caption}`} title={small ? caption : undefined}>
      <img {src} alt={caption} />
      {#if !small}<span class="zoom"><Maximize2 size={12} /></span>{/if}
    </button>
  {:else if failed}
    <div class="gone"><ImageIcon size={13} />{#if !small} The picture is no longer on disk.{/if}</div>
  {:else}
    <div class="gone"><Spinner size={12} /></div>
  {/if}
  {#if !small && caption}<span class="caption">{caption}</span>{/if}
</div>

<style>
  .pic { display: flex; flex-direction: column; gap: 4px; align-items: flex-start; }
  .thumb {
    position: relative; display: block; padding: 0; cursor: zoom-in;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    background: var(--bg-elevated); overflow: hidden; max-width: 100%;
  }
  .thumb:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .thumb img { display: block; max-width: 100%; max-height: 220px; min-width: 48px; min-height: 48px; object-fit: contain; }
  .small .thumb img { width: 56px; height: 56px; max-height: none; object-fit: cover; }
  .zoom {
    position: absolute; top: 6px; right: 6px; display: flex; padding: 3px; border-radius: 5px;
    background: var(--bg-overlay); color: var(--text-secondary); opacity: 0; transition: opacity var(--transition-fast);
  }
  .thumb:hover .zoom, .thumb:focus-visible .zoom { opacity: 1; }
  .gone { display: flex; align-items: center; gap: 6px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .caption { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
