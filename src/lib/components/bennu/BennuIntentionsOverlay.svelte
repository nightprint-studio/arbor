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
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { bennuIntentionsStore } from '$lib/stores/bennu/intentions.svelte';
  import { INTENTION_SECTIONS, type IntentionCategory } from './bennu-intentions';

  /** The section header to draw above row `i`, when it starts one. The rows arrive ordered
   *  (`orderIntentions`), so a section starts wherever the category changes. */
  function sectionStartingAt(i: number) {
    const category: IntentionCategory = items[i].category;
    if (i > 0 && items[i - 1].category === category) return null;
    return INTENTION_SECTIONS.find((s) => s.category === category) ?? null;
  }

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
  // Wide enough for an import's package to be read: the package is the whole choice.
  const PANEL_W = 340;
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
    if (!it || it.disabled) return;
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
    {#each items as item, i (item.id)}
      {@const ItemIcon = item.icon}
      {@const section = sectionStartingAt(i)}
      {#if section}
        {@const SectionIcon = section.icon}
        <div class="bennu-intentions-head cat-{section.category}" class:later={i > 0} role="presentation">
          <span class="bh-icon"><SectionIcon size={12} /></span>
          <span>{section.title}</span>
        </div>
      {/if}
      <button
        id="bennu-intention-{item.id}"
        class="bennu-intention cat-{item.category}"
        class:active={i === active}
        class:disabled={item.disabled}
        role="option"
        aria-selected={i === active}
        aria-disabled={item.disabled}
        type="button"
        onmousemove={() => (active = i)}
        onclick={() => runItem(i)}
      >
        <span class="bi-icon"><ItemIcon size={14} /></span>
        <span class="bi-label" title={item.label}>{item.label}</span>
        {#if item.preferred}
          <Badge variant="tone" tone="success" size="sm" label="suggested" />
        {/if}
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

  /* One colour per section, the IntelliJ reading: a red bulb repairs, a yellow one improves. The
     token is set once on the header and the row, and everything inside reads `--cat`. */
  .cat-fix { --cat: var(--error); }
  .cat-intention { --cat: var(--warning); }
  .cat-refactor { --cat: var(--accent); }
  .cat-generate { --cat: var(--success); }

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
  .bh-icon { display: inline-flex; color: var(--cat); }
  .bennu-intentions-head.later {
    margin-top: 3px;
    padding-top: 7px;
    border-top: 1px solid var(--border-subtle);
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
  .bennu-intention.disabled { color: var(--text-muted); cursor: default; }
  .bennu-intention.disabled .bi-icon { opacity: 0.5; }

  .bi-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--cat, var(--accent));
  }
  .bi-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
