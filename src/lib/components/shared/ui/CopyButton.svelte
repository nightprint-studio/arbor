<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Copy, Check } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';

  interface Props {
    /** Static string, OR sync/async function returning the string to copy.
     *  Use a function when the value needs to be computed at click time
     *  (e.g. an IPC call or a transformation of reactive state). */
    value: string | (() => string | Promise<string>);
    /** 'icon' = square icon-only button (22×22).
     *  'inline' = leading icon + label. */
    variant?: 'icon' | 'inline';
    /** Inline label (default 'Copy'). */
    label?: string;
    /** Inline label when in the copied state (default 'Copied'). */
    copiedLabel?: string;
    /** Tooltip + aria-label. Default: 'Copy to clipboard'. */
    title?: string;
    /** Show this toast on successful copy. Pass nothing to suppress. */
    toastSuccess?: string;
    /** Show a generic error toast on failure (default true). */
    showErrorToast?: boolean;
    /** Custom icon snippet — receives `{ copied }`. Default: Copy → Check swap. */
    icon?: Snippet<[{ copied: boolean }]>;
    /** Feedback duration in ms (default 1500). */
    feedbackMs?: number;
    /** Called with the copied text after a successful copy.
     *  Useful when you need a dynamic toast (e.g. including the value). */
    oncopied?: (text: string) => void;
  }

  let {
    value,
    variant        = 'icon',
    label          = 'Copy',
    copiedLabel    = 'Copied',
    title          = 'Copy to clipboard',
    toastSuccess,
    showErrorToast = true,
    icon,
    feedbackMs     = 1500,
    oncopied,
  }: Props = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function doCopy() {
    let text: string;
    try {
      text = typeof value === 'function' ? await value() : value;
    } catch (err) {
      if (showErrorToast) uiStore.showToast(`Copy failed: ${err}`, 'error');
      return;
    }
    const ok = await copyToClipboard(text, {
      successToast: toastSuccess,
      errorToast: showErrorToast,
    });
    if (ok) {
      copied = true;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { copied = false; }, feedbackMs);
      oncopied?.(text);
    }
  }

  const iconSize = $derived(variant === 'icon' ? 12 : 13);
</script>

<button
  type="button"
  class="copy-btn variant-{variant}"
  class:copied
  onclick={doCopy}
  use:tooltipAction={title}
  aria-label={title}
>
  {#if icon}
    {@render icon({ copied })}
  {:else if copied}
    <Check size={iconSize} />
  {:else}
    <Copy size={iconSize} />
  {/if}

  {#if variant === 'inline'}
    <span>{copied ? copiedLabel : label}</span>
  {/if}
</button>

<style>
  .copy-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast),
                color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .copy-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border-subtle);
  }

  /* ── Icon-only — square 22×22 ──────────────────────────────────────── */
  .variant-icon {
    width: 22px;
    height: 22px;
    border-radius: var(--radius-sm);
  }

  /* ── Inline — leading icon + label ────────────────────────────────── */
  .variant-inline {
    gap: 5px;
    padding: 4px 9px;
    font-size: 12px;
    font-weight: 500;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
  }

  /* ── Copied feedback state ────────────────────────────────────────── */
  .copy-btn.copied {
    color: var(--success);
    border-color: rgba(80, 200, 120, 0.35);
    background: rgba(80, 200, 120, 0.10);
  }
  .copy-btn.copied:hover { background: rgba(80, 200, 120, 0.15); }
</style>
