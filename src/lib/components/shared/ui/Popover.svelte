<!--
  Popover — a floating panel pinned above the status bar, dismissed by a click
  outside or Esc. Position comes from the global `.overlay-panel` rule
  (`position: fixed; bottom: 32px`), so every overlay of this family lands in
  the same place; `align` picks which side of the footer it grows from.

  What this adds over the bare backdrop + panel markup it replaces:
    - Esc closes it (while focus is inside, which it is: the panel takes focus
      on open), and the key does not also reach the shell's own Esc fallback;
    - focus goes back to whatever opened it once it unmounts;
    - `role="dialog"` + label on the panel, and the click-catcher is out of the
      Tab order.

  Content is a snippet, so the consumer's scoped styles keep applying to it.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Accessible name of the dialog. */
    label: string;
    /** Called for Esc and for a click outside. */
    onclose: () => void;
    /** Which footer edge the panel hangs from. */
    align?: 'start' | 'end';
    width?: number;
    maxHeight?: number;
    children: Snippet;
  }

  let { label, onclose, align = 'end', width = 320, maxHeight = 460, children }: Props = $props();

  let panelEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const opener = document.activeElement;
    panelEl?.focus({ preventScroll: true });
    return () => {
      if (opener instanceof HTMLElement && opener !== document.body && opener.isConnected) {
        opener.focus({ preventScroll: true });
      }
    };
  });

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    e.preventDefault();
    e.stopPropagation();
    onclose();
  }
</script>

<button type="button" aria-label="Close overlay" class="overlay-backdrop" tabindex="-1" onclick={onclose}></button>

<div
  bind:this={panelEl}
  class="overlay-panel popover-panel"
  class:start={align === 'start'}
  style:width="{width}px"
  style:max-height="{maxHeight}px"
  role="dialog"
  aria-label={label}
  tabindex="-1"
  onkeydown={onKey}
>
  {@render children()}
</div>

<style>
  .popover-panel {
    background: var(--bg-base);
    border-color: var(--border);
    box-shadow: 0 8px 32px rgba(0,0,0,0.7);
    outline: none;
  }
  .popover-panel.start { right: auto; left: 12px; }
</style>
