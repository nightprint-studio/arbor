<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { CheckCircle2, AlertCircle, AlertTriangle, Info, X } from 'lucide-svelte';
  import type { Toast } from '$lib/feedback/stores/toasts.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';

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
  {#each toast.actions as action (action.label)}
    <Button
      variant="tonal"
      size="xs"
      onclick={() => { action.onClick(); uiStore.dismissToast(toast.id); }}
    >{action.label}</Button>
  {/each}
  <IconButton tooltip="Dismiss" size={20} onclick={() => uiStore.dismissToast(toast.id)}>
    <X size={11} />
  </IconButton>
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
    font-size: var(--font-size-sm);
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

</style>
