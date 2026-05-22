import { uiStore } from '$lib/stores/ui.svelte';

export interface CopyOptions {
  /** Toast to show on success. Omit to stay silent. */
  successToast?: string;
  /** Toast to show on failure. `true` → generic "Copy failed: <err>". Omit / `false` → silent. */
  errorToast?: string | boolean;
}

/**
 * Write `text` to the system clipboard. Returns `true` on success, `false` on failure.
 * Silent by default; pass `successToast` / `errorToast` to surface feedback via `uiStore.showToast`.
 */
export async function copyToClipboard(text: string, opts: CopyOptions = {}): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    if (opts.successToast) uiStore.showToast(opts.successToast, 'success');
    return true;
  } catch (err) {
    if (opts.errorToast) {
      const msg = typeof opts.errorToast === 'string' ? opts.errorToast : `Copy failed: ${err}`;
      uiStore.showToast(msg, 'error');
    }
    return false;
  }
}
