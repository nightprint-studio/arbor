<script module lang="ts">
  // ── Modal stack (module scope, SHARED across instances) ────────────────
  // Every mounted modal pushes its `stackToken` here on mount and pops it on
  // destroy, so we can answer "am I the topmost?" when ESC fires. This MUST
  // live in `<script module>` — a `const` inside the regular `<script>`
  // would be recreated per component instance and every modal would think
  // it's alone in the world, closing parents on dismiss.
  const modalStack: symbol[] = [];

  /**
   * True when at least one `Modal` instance is currently mounted. Lets
   * outer keydown handlers (notably AppShell's global ESC dispatcher)
   * defer to the inner Modal's own ESC handling and avoid closing the
   * parent panel when a child modal is open. Modals already close
   * themselves via `modalStack[top] === stackToken`.
   */
  export function hasOpenModal(): boolean {
    return modalStack.length > 0;
  }
</script>

<script lang="ts">
  /**
   * Modal — pure structural shell.
   *
   *   • Provides the backdrop, chrome, animation, escape handling, sizing.
   *   • Slots: `header` (optional), `children` (body, required), `footer` (optional).
   *   • Concrete modals (ConfirmModal, CreateBranchModal, …) compose this shell
   *     and fill the slots with `<ModalHeader>` / `<ModalFooter>` helpers (or
   *     custom content) so visual rhythm stays consistent across the app.
   */
  import { onMount, setContext, type Component, type Snippet } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltipState } from '$lib/stores/tooltip.svelte';
  import { parkedModalsStore } from '$lib/stores/parked-modals.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import ActivityBar from '$lib/components/layout/ActivityBar.svelte';

  type Size = 'sm' | 'md' | 'lg' | 'full';

  let {
    onClose,
    header,
    children,
    footer,
    leftRail,
    rightRail,
    size            = 'sm',
    width,
    height,
    closeOnBackdrop = true,
    padBody         = true,
    topGap          = false,
    zIndex,
    ariaLabel,
    minimizable     = false,
    parkId,
    parkTitle,
    parkIcon,
    onRestoreFromScratch,
  }: {
    onClose:          () => void;
    header?:          Snippet;
    children:         Snippet;
    footer?:          Snippet;
    /** Optional activity rails along the modal's left/right edge. When
     *  provided, the snippet contents are rendered INSIDE an
     *  `<ActivityBar>` so consumers only have to ship buttons
     *  (`<button class="ab-btn">…</button>`) — the bar chrome, sizing
     *  and accent stripe come from the shared widget and stay
     *  consistent with the main app's activity bar.
     *
     *  The rails sit at the modal's full height (header → body →
     *  footer all share the same row); the body card's left/right
     *  margins automatically adjust to make room. */
    leftRail?:        Snippet;
    rightRail?:       Snippet;
    size?:            Size;
    /** Optional CSS width override (e.g. `"700px"`, `"min(1480px, 97vw)"`).
     *  When set, takes precedence over `size`. */
    width?:           string;
    /** Optional CSS height override. When set, takes precedence over `size`. */
    height?:          string;
    closeOnBackdrop?: boolean;
    /** When false, the body card has no inner padding — useful for modals
     *  that render their own split panes / lists / diff viewer edge-to-edge. */
    padBody?:         boolean;
    /** When true, the body card gets a 4px top gap exposing a chrome strip
     *  between the header and the body — the body's rounded top corners then
     *  read clearly. Off by default to keep existing modals flush. */
    topGap?:          boolean;
    /** Override the backdrop z-index. Pass a CSS value (e.g.
     *  `"var(--z-modal-picker)"`) when a modal must float ABOVE other
     *  modals — file/folder pickers spawned from another dialog use this
     *  so they're never trapped behind their caller. */
    zIndex?:          string;
    ariaLabel?:       string;
    /** When true, the header shows a minimize button that parks the modal
     *  into the status-bar dock. Requires `onRestoreFromScratch` —
     *  minimize closes the modal AND records a re-open action; the chip
     *  doesn't preserve local state, it re-runs the open path on click. */
    minimizable?:     boolean;
    /** Stable id used to dedupe parked entries across re-parks. Defaults
     *  to a random id per Modal instance, so each mount gets its own slot. */
    parkId?:          string;
    /** Label shown on the parked-dock chip. Defaults to the dialog's
     *  aria-label, then to a generic "Parked dialog" fallback. */
    parkTitle?:       string;
    /** Optional Lucide icon component rendered on the chip. */
    parkIcon?:        Component<{ size?: number; class?: string }>;
    /** Required for `minimizable`. Re-opens the modal from scratch when
     *  the chip is clicked: typically `() => switchToTab(srcTab) then
     *  openDetail(payload)`. May be async; the chip shows a spinner
     *  until it resolves. */
    onRestoreFromScratch?: () => void | Promise<void>;
  } = $props();

  const sizeStyle = $derived([
    width  ? `width: ${width};`   : '',
    height ? `height: ${height};` : '',
  ].filter(Boolean).join(' '));

  // ── Minimize / park ────────────────────────────────────────────────────
  // Minimize records a re-open action in `parkedModalsStore` and immediately
  // closes the modal. The chip in the status-bar dock survives workspace /
  // tab switches because it's not a hidden DOM node — it's a callback that
  // knows how to re-run the open path. State local to the modal (scroll,
  // unsaved input) is intentionally lost: chasing self-contained state
  // preservation across remounts is much more complex than the workflow
  // continuity that the action-based approach already gives us.
  const resolvedParkId = parkId ?? `modal-${crypto.randomUUID()}`;

  function doMinimize() {
    if (!minimizable || !onRestoreFromScratch) return;
    const accepted = parkedModalsStore.park({
      id:      resolvedParkId,
      title:   parkTitle ?? ariaLabel ?? 'Parked dialog',
      icon:    parkIcon,
      execute: onRestoreFromScratch,
    });
    if (!accepted) {
      uiStore.showToast(
        'Minimize cap reached — close a parked dialog or raise the limit in Appearance settings',
        'warning',
      );
      return;
    }
    onClose();
  }

  // ModalHeader reads this context to decide whether to render the minimize
  // button. The minimize button is gated on BOTH `minimizable` and the
  // consumer providing `onRestoreFromScratch` — without the latter we'd
  // park a dead action.
  setContext('arbor-modal', {
    minimize: (minimizable && onRestoreFromScratch) ? doMinimize : undefined,
  });

  // Backdrop dismiss must require BOTH mousedown AND mouseup on the backdrop.
  // Otherwise a text-selection drag that starts inside an input and releases
  // outside the modal card (very common with long fields) ends up firing
  // `click` on the backdrop (its nearest common ancestor) and closing the
  // modal — losing whatever the user was typing.
  let mouseDownOnBackdrop = false;

  function handleBackdropMouseDown(e: MouseEvent) {
    mouseDownOnBackdrop = e.target === e.currentTarget;
  }

  function handleBackdrop(e: MouseEvent) {
    const dismiss = mouseDownOnBackdrop && e.target === e.currentTarget && closeOnBackdrop;
    mouseDownOnBackdrop = false;
    if (dismiss) onClose();
  }

  // ── Focus trap ──────────────────────────────────────────────────────────
  // Tab/Shift+Tab cycle within the modal so focus can't escape to the main
  // app underneath. On open we move focus to the first focusable inside the
  // modal; on close we restore the previously-focused element.
  let modalEl: HTMLDivElement | undefined = $state();

  const FOCUSABLE_SEL = [
    'a[href]',
    'button:not([disabled])',
    'input:not([disabled])',
    'select:not([disabled])',
    'textarea:not([disabled])',
    '[tabindex]:not([tabindex="-1"])',
  ].join(',');

  function getFocusables(): HTMLElement[] {
    if (!modalEl) return [];
    return Array.from(modalEl.querySelectorAll<HTMLElement>(FOCUSABLE_SEL))
      .filter(el => el.offsetParent !== null && el.getAttribute('aria-hidden') !== 'true');
  }

  /** True when `el` lives inside the modal header chrome — header actions
   *  are toolbar shortcuts (share, refresh, settings…), not the primary
   *  intent of opening the dialog, so we skip them when picking initial
   *  focus. They stay reachable via Tab. */
  function isInHeader(el: HTMLElement): boolean {
    return !!modalEl && !!el.closest('.modal-header') && modalEl.contains(el);
  }

  // Unique token identifying this modal in the stack — created lazily so we
  // don't depend on lifecycle order between multiple modals mounting in the
  // same tick.
  const stackToken = Symbol('modal');

  onMount(() => {
    modalStack.push(stackToken);

    const previouslyFocused = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      const focusables = getFocusables();
      // Prefer body/footer focusables over header actions: header actions
      // (share, refresh, export…) are secondary tooling, not the primary
      // intent on opening. Always skip the close button: landing there
      // means Enter dismisses the modal immediately, which is jarring.
      const nonClose = focusables.filter(el => el.getAttribute('aria-label') !== 'Close');
      const initial =
        nonClose.find(el => !isInHeader(el))
        ?? nonClose[0]
        ?? focusables[0]
        ?? modalEl;
      // Suppress the tooltip-on-focus side effect for this initial focus
      // call: it's programmatic, not a user-driven Tab/click, so popping
      // a tooltip on the freshly-focused control would be noise. The
      // window covers just this synchronous focus dispatch.
      tooltipState.suppressFocusFor(150);
      initial?.focus();
    });
    return () => {
      const idx = modalStack.indexOf(stackToken);
      if (idx !== -1) modalStack.splice(idx, 1);
      // `preventScroll: true` keeps focus restoration from yanking the
      // viewport: without it, if the previously-focused element ended up
      // off-screen while the modal was open (e.g. deep-link dispatcher
      // scrolled the commit graph), the browser would auto-scroll it back
      // into view and undo the new scroll position.
      try { previouslyFocused?.focus({ preventScroll: true }); } catch { /* element may be gone */ }
    };
  });

  function handleTabTrap(e: KeyboardEvent) {
    if (!modalEl) return;
    const active = document.activeElement as HTMLElement | null;
    // Only trap when focus is actually inside this modal — otherwise a
    // lower modal in a stack would yank focus away from the top one.
    if (!active || !modalEl.contains(active)) return;

    const focusables = getFocusables();
    if (focusables.length === 0) {
      e.preventDefault();
      modalEl.focus();
      return;
    }
    const first = focusables[0];
    const last  = focusables[focusables.length - 1];
    if (e.shiftKey && active === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      // Only the topmost modal reacts to ESC; everything below it must
      // ignore the keystroke so a nested confirm/picker doesn't close its
      // parent on dismiss.
      if (modalStack[modalStack.length - 1] !== stackToken) return;
      onClose();
    } else if (e.key === 'Tab') {
      handleTabTrap(e);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop"
     role="dialog"
     aria-modal="true"
     aria-label={ariaLabel}
     tabindex="-1"
     style={zIndex ? `z-index: ${zIndex};` : ''}
     onmousedown={handleBackdropMouseDown}
     onclick={handleBackdrop}
     transition:fade={{ duration: animStore.dBase }}
>
  <div class="modal" data-size={size} style={sizeStyle}
       role="presentation"
       tabindex="-1"
       bind:this={modalEl}
       onclick={(e) => e.stopPropagation()}
       transition:fly={{ y: 24, duration: animStore.dPanel, easing: cubicOut }}
  >
    {#if header}
      <div class="modal-header">{@render header()}</div>
    {/if}
    <!-- Middle band: rails live HERE so they only span the body
         height, never spilling under the header or footer. When
         neither rail is provided this collapses to a plain row
         containing just the body — visually identical to the
         previous header/body/footer layout (which had the body as
         the modal's direct child). -->
    <div class="modal-mid">
      {#if leftRail}
        <ActivityBar side="left" ariaLabel="Modal tool rail">
          {#snippet top()}{@render leftRail()}{/snippet}
        </ActivityBar>
      {/if}
      <div class="modal-body" class:has-footer={!!footer} class:no-pad={!padBody} class:top-gap={topGap}>
        {@render children()}
      </div>
      {#if rightRail}
        <ActivityBar side="right" ariaLabel="Modal tool rail">
          {#snippet top()}{@render rightRail()}{/snippet}
        </ActivityBar>
      {/if}
    </div>
    {#if footer}
      <div class="modal-footer">{@render footer()}</div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    /* Slightly darker than the original 0.55 to compensate for the loss of
       the blur — the visual separation between modal and underlying app
       was largely carried by the soft blur, not the dim. */
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: var(--z-modal-bg);
    /* `backdrop-filter: blur(...)` was REMOVED here.  On WebView2/Chromium
       Windows, blur forces the GPU compositor to capture+blur the
       underlying view every frame as long as the modal is open, which on
       integrated GPUs / busy systems pushes texture data through the disk
       I/O scheduler and starves child processes (notably Maven/Java's
       per-class-file write workload) — observed as `mvn` builds dropping
       from 7 MB/s to 0.1 MB/s while ANY modal is open.
       If we ever want the blur back, gate it behind a "performance mode"
       setting and document the trade-off. */
    padding: 24px;
  }

  /* Outer chrome on --bg-elevated so the body can sit as a --bg-base card —
     same rhythm used by MR detail / theme editor / plugin manager.
     Width/height are transitioned so consumers that toggle size at runtime
     (e.g. PipelineRunDetailModal's expand button) animate smoothly. CSS
     transitions don't fire on initial values, so static-size modals are
     unaffected. The duration tracks the global animation speed setting. */
  .modal {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
    max-width: 100%;
    max-height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition:
      width  var(--anim-dur-panel, 200ms) cubic-bezier(.16, 1, .3, 1),
      height var(--anim-dur-panel, 200ms) cubic-bezier(.16, 1, .3, 1);
  }
  /* Middle band — flex-row holding the optional rails plus the body
     card. Always rendered (with or without rails) so the body's
     own margin rhythm stays consistent. `min-height: 0` so the
     flex child can shrink, letting body's `overflow: auto` actually
     take effect (otherwise it'd grow to its content height). */
  .modal-mid {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: row;
    align-items: stretch;
    overflow: hidden;
  }
  .modal:focus { outline: none; }
  .modal[data-size="sm"]   { min-width: 380px; max-width: 92vw; }
  .modal[data-size="md"]   { width:     520px; max-width: 92vw; }
  .modal[data-size="lg"]   { width:     720px; max-width: 92vw; }
  .modal[data-size="full"] { width: 92vw; height: 92vh; }

  /* Header chrome — content lives in the snippet (typically <ModalHeader>). */
  .modal-header {
    display: flex;
    align-items: center;
    padding: 5px 14px;
    background: var(--modal-chrome-bg);
    gap: 8px;
    flex-shrink: 0;
  }

  /* Floats as a --bg-base card on the chrome. */
  .modal-body {
    flex: 1;
    overflow: auto;
    padding: 16px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    margin: 0 4px 4px;
  }
  .modal-body.has-footer { margin: 0 4px; }
  .modal-body.no-pad     { padding: 0; }
  /* topGap exposes a 4px chrome strip between header and body so the
     body card's rounded top corners read clearly. */
  .modal-body.top-gap            { margin-top: 4px; }
  .modal-body.top-gap.has-footer { margin-top: 4px; }

  /* Footer chrome — same recipe as the header but at the bottom. Buttons
     are right-aligned by default; <ModalFooter align="..."> overrides. */
  .modal-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: var(--modal-footer-padding);
    background: var(--modal-chrome-bg);
    flex-shrink: 0;
  }
</style>
