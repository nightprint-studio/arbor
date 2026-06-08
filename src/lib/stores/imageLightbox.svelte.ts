// Singleton store backing the global <ImageLightbox> overlay. Inline images in
// issue/MR/PR bodies (rendered via the `previewImages` action) call `show()`
// with the set of sibling images so the lightbox can page through them.

export interface LightboxItem {
  src: string;   // resolved data: URL (already fetched through the proxy)
  alt: string;
}

function createImageLightboxStore() {
  let open  = $state(false);
  let items = $state<LightboxItem[]>([]);
  let index = $state(0);

  return {
    get open()    { return open; },
    get items()   { return items; },
    get index()   { return index; },
    get current() { return items[index] ?? null; },

    show(list: LightboxItem[], i = 0) {
      if (!list.length) return;
      items = list;
      index = Math.max(0, Math.min(i, list.length - 1));
      open  = true;
    },
    close() { open = false; },
    next() { if (items.length > 1) index = (index + 1) % items.length; },
    prev() { if (items.length > 1) index = (index - 1 + items.length) % items.length; },
  };
}

export const imageLightbox = createImageLightboxStore();
