<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { CheckCircle2, AlertCircle, AlertTriangle, Info, X } from 'lucide-svelte';
  import type { Toast } from '$lib/stores/ui.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  let { toast }: { toast: Toast } = $props();

  const icons = { success: CheckCircle2, error: AlertCircle, warning: AlertTriangle, info: Info };
  const Icon = $derived(icons[toast.kind]);
</script>

<div
  class="toast toast-{toast.kind}"
  role="alert"
  aria-live="polite"
  in:fly|global={{ x: 360, duration: animStore.dPanel, easing: cubicOut }}
  out:fly|global={{ x: 360, duration: animStore.dPanel, easing: cubicOut, opacity: 0 }}
>
  <span class="stripe" aria-hidden="true"></span>
  <span class="icon"><Icon size={14} /></span>
  <span class="message">{toast.message}</span>
  {#if toast.action}
    <button
      class="action"
      onclick={() => { toast.action?.onClick(); uiStore.dismissToast(toast.id); }}
    >{toast.action.label}</button>
  {/if}
  <button class="dismiss" onclick={() => uiStore.dismissToast(toast.id)} aria-label="Dismiss">
    <X size={11} />
  </button>
</div>

<style>
  /* Modern flat toast: dark card, 3px coloured stripe on the left signals
     kind without dyeing the entire surface.  Backdrop blur + soft shadow
     give it depth without competing with the rest of the UI. */
  .toast {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 9px 11px 9px 14px;
    border-radius: var(--radius-lg);
    font-size: 12.5px;
    line-height: 1.35;
    color: var(--text-primary);
    /* 95% opaque already — bumped to 100% so the lost blur diffusion does
       not let cluttered chrome bleed through. `backdrop-filter: blur()`
       removed for the same reason as Modal.svelte (see comment there). */
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    box-shadow:
      0 1px 0 0 rgba(255, 255, 255, 0.04) inset,
      0 8px 24px rgba(0, 0, 0, 0.32),
      0 1px 3px rgba(0, 0, 0, 0.2);
    min-width: 240px;
    max-width: 480px;
    overflow: hidden;
  }

  .stripe {
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    border-radius: 2px;
  }

  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .toast-info    .stripe { background: var(--accent); }
  .toast-success .stripe { background: var(--success); }
  .toast-warning .stripe { background: var(--warning); }
  .toast-error   .stripe { background: var(--error); }

  .toast-info    .icon { color: var(--accent); }
  .toast-success .icon { color: var(--success); }
  .toast-warning .icon { color: var(--warning); }
  .toast-error   .icon { color: var(--error); }

  .message {
    flex: 1;
    word-break: break-word;
    color: var(--text-primary);
  }

  .dismiss {
    flex-shrink: 0;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-disabled);
    width: 20px;
    height: 20px;
    border-radius: 5px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .dismiss:hover { color: var(--text-primary); background: var(--bg-overlay); }

  /* Inline action button — same accent treatment as notification actions
     so the two surfaces stay visually consistent. */
  .action {
    flex-shrink: 0;
    background: transparent;
    border: 1px solid var(--border-subtle);
    color: var(--accent);
    cursor: pointer;
    padding: 3px 9px;
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    font-weight: 500;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .action:hover {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
