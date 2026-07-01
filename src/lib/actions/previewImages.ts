import { fetchRemoteImage, type ImageProvider } from '$lib/ipc/corvus/images';
import { imageLightbox, type LightboxItem } from '$lib/stores/imageLightbox.svelte';

export interface PreviewImagesOptions {
  /** Provider whose credentials resolve auth-gated images. */
  provider: ImageProvider;
  /** GitLab instance origin (from the MR web URL) — resolves relative
   *  `/uploads/...` paths and decides whether the token is attached. */
  baseUrl?: string | null;
}

// Module-level cache so re-rendering a body (or reopening a modal) reuses an
// in-flight / settled fetch instead of hammering the proxy. Keyed by the full
// resolution context, since the same URL can need a different token per host.
// Each value is a (potentially multi-MB) base64 data URL, so the map is bounded
// LRU-style: oldest insertions are evicted once the cap is hit.
const cache = new Map<string, Promise<string>>();
const CACHE_CAP = 80;

function resolveImage(url: string, provider: ImageProvider, baseUrl?: string | null): Promise<string> {
  const key = `${provider}|${baseUrl ?? ''}|${url}`;
  let p = cache.get(key);
  if (!p) {
    p = fetchRemoteImage(url, provider, baseUrl).catch((e) => {
      cache.delete(key); // don't cache failures — a later retry may succeed
      throw e;
    });
    cache.set(key, p);
    if (cache.size > CACHE_CAP) {
      const oldest = cache.keys().next().value;
      if (oldest !== undefined) cache.delete(oldest);
    }
  }
  return p;
}

/**
 * Enhance markdown/HTML image tags (emitted by `prepareImagesForPreview` as
 * `<img class="md-img" data-img-src>`) inside `node`:
 *   • fetches each source through the provider proxy and swaps in the result,
 *   • wires click / Enter / Space to open the full-size <ImageLightbox>,
 *   • pages the lightbox across all loaded images in this container.
 *
 * A MutationObserver re-runs the pass when Svelte replaces the `{@html}` body
 * (e.g. the issue description updates), so freshly-injected images are caught.
 */
export function previewImages(node: HTMLElement, opts: PreviewImagesOptions) {
  let current = opts;
  let scheduled = false;

  function loadedItems(): LightboxItem[] {
    const imgs = node.querySelectorAll<HTMLImageElement>('img.md-img[data-img-state="loaded"]');
    return Array.from(imgs).map((img) => ({
      src: img.dataset.imgFull || img.src,
      alt: img.alt || '',
    }));
  }

  function openFor(img: HTMLImageElement) {
    if (img.dataset.imgState !== 'loaded') return;
    const items = loadedItems();
    const full = img.dataset.imgFull || img.src;
    const idx = Math.max(0, items.findIndex((it) => it.src === full));
    imageLightbox.show(items, idx);
  }

  function onClick(e: Event) { openFor(e.currentTarget as HTMLImageElement); }
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      openFor(e.currentTarget as HTMLImageElement);
    }
  }

  function wireInteractive(img: HTMLImageElement) {
    img.setAttribute('role', 'button');
    img.setAttribute('tabindex', '0');
    img.addEventListener('click', onClick);
    img.addEventListener('keydown', onKey);
  }

  function process() {
    scheduled = false;
    const imgs = node.querySelectorAll<HTMLImageElement>('img.md-img:not([data-img-state])');
    for (const img of imgs) {
      const deferred = img.getAttribute('data-img-src');
      if (!deferred) {
        // Direct `src` already present (e.g. a `data:` image) — just make it
        // interactive; it's available immediately.
        if (img.getAttribute('src')) {
          img.dataset.imgState = 'loaded';
          img.dataset.imgFull = img.src;
          wireInteractive(img);
        } else {
          img.dataset.imgState = 'error';
        }
        continue;
      }

      img.dataset.imgState = 'loading';
      img.removeAttribute('src'); // ensure the WebView never tries the raw URL
      resolveImage(deferred, current.provider, current.baseUrl)
        .then((dataUrl) => {
          if (img.dataset.imgState !== 'loading') return; // node recycled
          img.src = dataUrl;
          img.dataset.imgFull = dataUrl;
          img.dataset.imgState = 'loaded';
          wireInteractive(img);
        })
        .catch(() => {
          img.dataset.imgState = 'error';
        });
    }
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(process);
  }

  // `{@html}` swaps the whole subtree on body changes — observe so new images
  // get picked up. Our own attribute writes are filtered out by the
  // `:not([data-img-state])` selector, so the observer never loops on itself.
  const mo = new MutationObserver(schedule);
  mo.observe(node, { childList: true, subtree: true });
  schedule();

  return {
    update(next: PreviewImagesOptions) {
      current = next;
      schedule();
    },
    destroy() {
      mo.disconnect();
      // Per-image listeners are torn down with their nodes when the subtree is
      // replaced/removed, so there's nothing else to clean up here.
    },
  };
}
