<script lang="ts">
  /**
   * BennuIntentionsOverlay — the Alt+Enter intentions/quick-fix popup for the
   * Bennu editor, modeled on merula's caret-anchored intentions picker.
   *
   * A small floating list anchored at the caret (viewport coords from the store).
   * Fully keyboard-driven: ↑/↓ move the highlight (wrapping), Enter runs the
   * highlighted item, Esc closes; the first item is highlighted on open. The panel
   * is clamped into the viewport so it never spills off an edge. An outside-click
   * backdrop dismisses it (layered below the panel so item clicks still land).
   *
   * State comes from `bennuIntentionsStore`; each item owns its `run()`, so the
   * overlay just invokes it and closes — no id→handler indirection. The two
   * "Generate…" items call the `onGenerate(mode)` callback the collector was built
   * with, which the Wire phase points at the Generate modal.
   *
   * Imports only shared theming (CSS vars) + bennu-local store/types. Not yet
   * mounted in BennuWindow — that is the Wire phase.
   */
  import { tick } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { Lightbulb } from 'lucide-svelte';
  import { bennuIntentionsStore } from '$lib/stores/bennu/intentions.svelte';

  let {
    /** Called after the popup closes (running an item or dismissing) so the host
     *  can return focus to the editor. */
    onClose,
  }: {
    onClose?: () => void;
  } = $props();

  const open = $derived(bennuIntentionsStore.open);
  const items = $derived(bennuIntentionsStore.items);
  const anchor = $derived(bennuIntentionsStore.anchor);

  let panelEl = $state<HTMLElement | null>(null);
  let active = $state(0);

  // Re-highlight the first item every time the popup (re)opens.
  $effect(() => {
    if (open) {
      active = 0;
      // Park focus on the panel so arrow keys are live immediately (no mouse).
      tick().then(() => panelEl?.focus());
    }
  });

  // ── Viewport clamping ─────────────────────────────────────────────────────────
  // Anchor is the caret's bottom-left in viewport coords; drop the panel just
  // below it, then pull it back inside the viewport on both axes once measured.
  const PANEL_W = 260;
  let pos = $state<{ x: number; y: number }>({ x: 0, y: 0 });
  $effect(() => {
    if (!open || !panelEl) return;
    const a = anchor;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const rect = panelEl.getBoundingClientRect();
    let x = a ? a.x : vw / 2 - PANEL_W / 2;
    let y = a ? a.y + 6 : vh / 3;
    x = Math.min(Math.max(8, x), vw - rect.width - 8);
    // Flip above the caret when it would spill off the bottom.
    if (a && y + rect.height > vh - 8) y = Math.max(8, a.y - rect.height - 6);
    else y = Math.min(Math.max(8, y), vh - rect.height - 8);
    pos = { x, y };
  });

  function close() {
    bennuIntentionsStore.close();
    onClose?.();
  }

  function runItem(index: number) {
    const it = items[index];
    if (!it) return;
    // Close first so the action (which may open a modal / toast) lands on a clean
    // stack, then run it.
    close();
    it.run();
  }

  function onKeydown(e: KeyboardEvent) {
    // The popup owns the keyboard while open — never let a key leak to the editor
    // behind it (which would type the character or trigger its own bindings).
    e.stopPropagation();
    const n = items.length;
    if (!n) return;
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        active = (active + 1) % n;
        return;
      case 'ArrowUp':
        e.preventDefault();
        active = (active - 1 + n) % n;
        return;
      case 'Home':
        e.preventDefault();
        active = 0;
        return;
      case 'End':
        e.preventDefault();
        active = n - 1;
        return;
      case 'Enter':
        e.preventDefault();
        runItem(active);
        return;
      case 'Escape':
        e.preventDefault();
        close();
        return;
    }
  }
</script>

{#if open}
  <!-- Outside-click dismissal: a full-viewport catcher below the panel in z-order
       so clicks on items still hit the panel. -->
  <div
    class="bennu-intentions-backdrop"
    role="presentation"
    onpointerdown={close}
    oncontextmenu={(e) => { e.preventDefault(); close(); }}
  ></div>

  <div
    bind:this={panelEl}
    class="bennu-intentions"
    role="listbox"
    tabindex="-1"
    aria-label="Intentions"
    aria-activedescendant={items[active] ? `bennu-intention-${items[active].id}` : undefined}
    style="left: {pos.x}px; top: {pos.y}px; width: {PANEL_W}px;"
    onkeydown={onKeydown}
    in:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}
    out:fade={{ duration: animStore.dFast }}
  >
    <div class="bennu-intentions-head">
      <Lightbulb size={12} />
      <span>Intentions</span>
    </div>
    {#each items as item, i (item.id)}
      {@const ItemIcon = item.icon}
      <button
        id="bennu-intention-{item.id}"
        class="bennu-intention"
        class:active={i === active}
        role="option"
        aria-selected={i === active}
        type="button"
        onmousemove={() => (active = i)}
        onclick={() => runItem(i)}
      >
        <span class="bi-icon"><ItemIcon size={14} /></span>
        <span class="bi-label">{item.label}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .bennu-intentions-backdrop {
    position: fixed;
    inset: 0;
    z-index: calc(var(--z-menu) - 1);
    background: transparent;
  }

  .bennu-intentions {
    position: fixed;
    z-index: var(--z-menu);
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 4px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    outline: none;
  }

  .bennu-intentions-head {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px 5px;
    font-size: var(--font-size-2xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    user-select: none;
  }

  .bennu-intention {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    text-align: left;
  }
  .bennu-intention.active { background: var(--bg-selected); }

  .bi-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--accent);
  }
  .bi-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
