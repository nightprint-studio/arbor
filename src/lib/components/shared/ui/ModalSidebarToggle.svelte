<script lang="ts">
  /**
   * ModalSidebarToggle — standardised sidebar collapse/expand button for modals.
   *
   * Drop into `<ModalHeader>` (typically as the first child, so the icon sits
   * adjacent to the side it controls) or into the `actions` snippet. The icon
   * pair makes the action explicit: `PanelLeftClose` when expanded (click to
   * close), `PanelLeftOpen` when collapsed (click to open). Mirror for `right`.
   *
   *   <ModalHeader {onClose}>
   *     <ModalSidebarToggle bind:collapsed onToggle={() => collapsed = !collapsed} />
   *     <Package size={16} />
   *     <span class="modal-title">…</span>
   *   </ModalHeader>
   */
  import {
    PanelLeftClose, PanelLeftOpen,
    PanelRightClose, PanelRightOpen,
  } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    collapsed: boolean;
    onToggle:  () => void;
    side?:     'left' | 'right';
    /** Optional override for the tooltip / aria-label. Defaults adapt to side+state. */
    label?:    string;
    size?:     number;
  }

  let {
    collapsed,
    onToggle,
    side  = 'left',
    label,
    size  = 15,
  }: Props = $props();

  const defaultLabel = $derived(
    side === 'left'
      ? (collapsed ? 'Show sidebar' : 'Hide sidebar')
      : (collapsed ? 'Show panel'   : 'Hide panel')
  );
  const lbl = $derived(label ?? defaultLabel);
</script>

<button
  type="button"
  class="modal-sidebar-toggle"
  onclick={onToggle}
  use:tooltip={lbl}
  aria-label={lbl}
  aria-pressed={!collapsed}
>
  {#if side === 'left'}
    {#if collapsed}<PanelLeftOpen {size} />{:else}<PanelLeftClose {size} />{/if}
  {:else}
    {#if collapsed}<PanelRightOpen {size} />{:else}<PanelRightClose {size} />{/if}
  {/if}
</button>

<style>
  .modal-sidebar-toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--anim-fast), color var(--anim-fast), border-color var(--anim-fast);
    flex-shrink: 0;
  }
  .modal-sidebar-toggle:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .modal-sidebar-toggle:focus-visible {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-subtle);
  }
</style>
