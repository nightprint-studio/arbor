<script lang="ts">
  import type { Snippet } from 'svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Optional title — rendered uppercase, small-caps, like PanelShell. */
    title?: string;
    /**
     * Optional count badge after the title — same look as PanelShell.
     * Hidden when null/undefined or 0.
     */
    count?: number | null;
    /** Optional icon before the title (lucide etc.). */
    icon?: Snippet;
    /**
     * Free content placed AFTER the title, BEFORE the spacer.
     * Use for inline status, breadcrumbs, tab strips, etc.
     */
    children?: Snippet;
    /**
     * Action buttons placed at the right, just before the close button.
     * Use the global `.ps-btn` / `.ps-btn-accent` / `.ps-btn-danger` /
     * `.ps-btn-success` / `.ps-btn-active` classes for consistent styling.
     */
    actions?: Snippet;
    /** Custom close handler. Defaults to `uiStore.setActiveBottomSection(null)`. */
    onClose?: () => void;
    /** Hide the close button — rare, only when the caller manages close itself. */
    hideClose?: boolean;
  }

  let {
    title,
    count = null,
    icon,
    children,
    actions,
    onClose,
    hideClose = false,
  }: Props = $props();

  function handleClose() {
    if (onClose) onClose();
    else uiStore.setActiveBottomSection(null);
  }
</script>

<div class="bp-header">
  {#if icon}
    <span class="bp-icon">{@render icon()}</span>
  {/if}
  {#if title}
    <span class="bp-title">{title}</span>
  {/if}
  {#if count != null && count > 0}
    <span class="bp-count">{count}</span>
  {/if}
  {#if children}
    {@render children()}
  {/if}
  <span class="bp-spacer"></span>
  {#if actions}
    <div class="bp-actions">{@render actions()}</div>
  {/if}
  {#if !hideClose}
    <!-- Same red-dot affordance used by `<ModalHeader>` so the close
         control reads consistently across modals and bottom panels. -->
    <button
      class="mac-close-btn bp-close"
      onclick={handleClose}
      use:tooltip={'Close panel'}
      aria-label="Close panel"
    ></button>
  {/if}
</div>

<style>
  /* Header bar mirrors PanelShell `.ps-header` so sidebar and bottom panels
     share the exact same chrome (height, padding, border, typography). */
  .bp-header {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    min-height: 34px;
    padding: 0 8px 0 12px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  .bp-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--accent);
  }

  .bp-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    color: var(--text-secondary);
    text-transform: uppercase;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .bp-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 14px;
    padding: 0 4px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .bp-spacer {
    flex: 1;
    min-width: 0;
  }

  .bp-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  .bp-close {
    margin-left: 6px;
  }
</style>
