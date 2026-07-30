<script lang="ts">
  /**
   * Version picker in the detail footer — opens UPWARD (fixed-positioned, so it
   * escapes the footer instead of being clipped) without expanding the footer.
   * Built on the shared `Dropdown`; the `--dd-*` overrides give it the launcher's
   * theme-independent "earth" palette that matches the rock footer.
   */
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';

  interface VerItem { v: string; active: boolean; }
  interface Props { versions: VerItem[]; current: string; onpick: (v: string) => void; }
  let { versions, current, onpick }: Props = $props();

  const items = $derived<DropdownItem[]>(versions.map(v => ({
    kind: 'item', id: v.v, label: v.v, active: v.active, onclick: () => onpick(v.v),
  })));
</script>

<div class="cv-earth">
  <Dropdown {items} position="fixed" direction="up" width="170px">
    {#snippet trigger({ open, toggle })}
      <button class="trigger" class:open onclick={toggle}
              aria-haspopup="menu" aria-expanded={open}>
        <span>{current}</span>
        <svg class="chev" class:flip={open} width="11" height="11" viewBox="0 0 24 24" fill="none">
          <path d="M6 9 L12 15 L18 9" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
    {/snippet}
  </Dropdown>
</div>

<style>
  /* Theme-independent "earth" palette (matches the rock footer) for the menu. */
  .cv-earth {
    display: inline-flex;
    --dd-bg: rgba(14, 9, 7, 0.97);
    --dd-border: rgba(255, 200, 150, 0.16);
    --dd-shadow: 0 -16px 40px -14px rgba(0, 0, 0, 0.85);
    --dd-text: #d8cab8;
    --dd-text-muted: #b39d88;
    --dd-hover-bg: rgba(255, 255, 255, 0.07);
    --dd-active-bg: rgba(255, 255, 255, 0.09);
    --dd-check: #8fce6a;
  }

  .trigger {
    display: flex; align-items: center; gap: 7px; padding: 9px 13px; border-radius: 9px;
    background: rgba(255, 255, 255, 0.07); border: 1px solid rgba(255, 200, 150, 0.16);
    color: #d8cab8; font-family: var(--canopy-mono); font-size: var(--font-size-xs); cursor: pointer;
    transition: background 0.15s, border-color 0.15s; white-space: nowrap;
  }
  .trigger:hover, .trigger.open { background: rgba(255, 255, 255, 0.12); border-color: rgba(255, 200, 150, 0.28); }
  .chev { color: #b39d88; transition: transform 0.15s; flex: none; }
  .chev.flip { transform: rotate(180deg); }
</style>
