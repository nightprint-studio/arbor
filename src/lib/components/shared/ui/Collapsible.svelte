<script lang="ts">
  import type { Snippet } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ChevronRight } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  interface Props {
    open?: boolean;
    /** Header snippet. Receives `{ open }` so it can swap icons / styles. */
    header: Snippet<[{ open: boolean }]>;
    children: Snippet;
    /** Animation duration override; defaults to the global panel animation. */
    duration?: number;
    /** Show a leading rotating chevron next to the header (useful for sections without their own indicator). */
    chevron?: boolean;
    /** Disable interaction. */
    disabled?: boolean;
    onopen?: () => void;
    onclose?: () => void;
  }

  let {
    open = $bindable(false),
    header,
    children,
    duration,
    chevron = false,
    disabled = false,
    onopen,
    onclose,
  }: Props = $props();

  const dur = $derived(duration ?? animStore.dPanel);

  function toggle() {
    if (disabled) return;
    open = !open;
    if (open) onopen?.(); else onclose?.();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      toggle();
    }
  }
</script>

<div class="collapsible" class:open class:disabled>
  <div
    class="collapsible-header"
    class:has-chevron={chevron}
    onclick={toggle}
    onkeydown={onKeydown}
    role="button"
    tabindex={disabled ? -1 : 0}
    aria-expanded={open}
    aria-disabled={disabled || undefined}
  >
    {#if chevron}
      <ChevronRight class="collapsible-chevron" size={12} />
    {/if}
    <div class="collapsible-header-content">
      {@render header({ open })}
    </div>
  </div>
  {#if open}
    <div class="collapsible-body" transition:slide={{ duration: dur, easing: cubicOut }}>
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .collapsible-header {
    cursor: pointer;
    user-select: none;
    display: flex;
    align-items: center;
    gap: 6px;
    border-radius: var(--radius-sm);
  }
  .collapsible-header:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .collapsible.disabled .collapsible-header {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .collapsible-header-content { flex: 1; min-width: 0; }

  .collapsible-body { overflow: hidden; }

  /* Chevron rotation when open. */
  :global(.collapsible-chevron) {
    transition: transform var(--transition-fast);
    flex-shrink: 0;
    color: var(--text-muted);
  }
  .collapsible.open :global(.collapsible-chevron) { transform: rotate(90deg); }
</style>
