<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Info, AlertTriangle, XCircle, CheckCircle2, X } from 'lucide-svelte';

  type Variant = 'info' | 'warning' | 'error' | 'success';

  interface Props {
    variant?: Variant;
    /** Title — rendered bold above the body. Optional. */
    title?: string;
    /** Body text. Use `children` for rich content. */
    text?: string;
    /** Hide the leading icon. */
    noIcon?: boolean;
    /** Show a close button — fires `onclose`. */
    dismissible?: boolean;
    /** Compact (inline rows, e.g. inside dense forms). */
    compact?: boolean;
    onclose?: () => void;
    /** Right-side action snippet (button cluster). */
    actions?: Snippet;
    children?: Snippet;
  }

  let {
    variant = 'info',
    title,
    text,
    noIcon = false,
    dismissible = false,
    compact = false,
    onclose,
    actions,
    children,
  }: Props = $props();

  const Icon = $derived(
    variant === 'warning' ? AlertTriangle :
    variant === 'error'   ? XCircle :
    variant === 'success' ? CheckCircle2 :
                            Info,
  );
</script>

<div class="alert alert-{variant}" class:compact role="alert">
  {#if !noIcon}
    <span class="alert-icon" aria-hidden="true">
      <Icon size={compact ? 13 : 15} />
    </span>
  {/if}
  <div class="alert-body">
    {#if title}<div class="alert-title">{title}</div>{/if}
    {#if children}
      <div class="alert-text">{@render children()}</div>
    {:else if text}
      <div class="alert-text">{text}</div>
    {/if}
  </div>
  {#if actions}
    <div class="alert-actions">{@render actions()}</div>
  {/if}
  {#if dismissible}
    <button
      type="button"
      class="alert-close"
      aria-label="Dismiss"
      onclick={onclose}
    >
      <X size={13} />
    </button>
  {/if}
</div>

<style>
  .alert {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 9px 11px;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    line-height: 1.4;
  }
  .alert.compact {
    padding: 5px 8px;
    font-size: var(--font-size-xs);
    gap: 6px;
  }

  .alert-icon { flex-shrink: 0; margin-top: 1px; display: inline-flex; }
  .alert-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .alert-title { font-weight: 600; color: var(--text-primary); }
  .alert-text  { color: inherit; }

  .alert-actions { flex-shrink: 0; display: inline-flex; align-items: center; gap: 5px; }

  .alert-close {
    flex-shrink: 0;
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 2px;
    border-radius: var(--radius-sm);
    opacity: 0.65;
    transition: opacity var(--transition-fast), background var(--transition-fast);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .alert-close:hover { opacity: 1; background: rgba(255,255,255,0.06); }

  .alert-info {
    background: color-mix(in srgb, var(--info) 10%, transparent);
    border-color: color-mix(in srgb, var(--info) 30%, transparent);
    color: var(--info);
  }
  .alert-warning {
    background: color-mix(in srgb, var(--warning) 10%, transparent);
    border-color: color-mix(in srgb, var(--warning) 32%, transparent);
    color: var(--warning);
  }
  .alert-error {
    background: color-mix(in srgb, var(--error) 12%, transparent);
    border-color: color-mix(in srgb, var(--error) 35%, transparent);
    color: var(--error);
  }
  .alert-success {
    background: color-mix(in srgb, var(--success) 10%, transparent);
    border-color: color-mix(in srgb, var(--success) 30%, transparent);
    color: var(--success);
  }

  /* Make the body text legible on the tinted background. */
  .alert-info .alert-text,
  .alert-warning .alert-text,
  .alert-error .alert-text,
  .alert-success .alert-text { color: var(--text-primary); }
</style>
