import {
  tooltipState,
  normalizeOptions,
  type TooltipInput,
  type TooltipPlacement,
  type NormalizedTooltipOptions,
} from '$lib/stores/tooltip.svelte';

/** What a call site may pass. `null` / `undefined` are accepted (and normalized to an
 *  inert, content-less tooltip) so a conditional `use:tooltip={cond ? '…' : undefined}`
 *  type-checks — `normalizeOptions` already handles the empty case at runtime. */
export type TooltipArg = TooltipInput | null | undefined;

function withPlacement(input: TooltipArg, placement: TooltipPlacement): TooltipInput {
  if (input == null) return { content: '', placement };
  if (typeof input === 'string') return { content: input, placement };
  return { ...input, placement };
}

function sameShortcut(a?: string[], b?: string[]): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) if (a[i] !== b[i]) return false;
  return true;
}

function sameOpts(a: NormalizedTooltipOptions, b: NormalizedTooltipOptions): boolean {
  return (
    a.content === b.content &&
    a.description === b.description &&
    a.placement === b.placement &&
    a.delay === b.delay &&
    a.offset === b.offset &&
    a.maxWidth === b.maxWidth &&
    a.maxHeight === b.maxHeight &&
    a.markdown === b.markdown &&
    a.disabled === b.disabled &&
    a.className === b.className &&
    sameShortcut(a.shortcut, b.shortcut)
  );
}

/**
 * Svelte action that attaches a tooltip to any element.
 *
 * Usage:
 *   <button use:tooltip={'Refresh'}>...</button>
 *   <button use:tooltip={{ content: 'Refresh', shortcut: 'Ctrl+R' }}>...</button>
 *   <button use:tooltip={{ content: 'Long help', description: 'second line', placement: 'right' }}>...</button>
 *
 * Behaviour:
 *  - Mouse: opens after `delay` ms on hover; quick re-hover (within ~250ms of close) opens instantly.
 *  - Keyboard: opens immediately on focus-visible (no delay), hides on blur.
 *  - Closes on mousedown (so tooltips don't linger over the click target), mouseleave, blur, Escape.
 *  - Stays in sync if the props change while it's open.
 */
/** Both element families and not just `HTMLElement`: a tooltip belongs on an SVG node too — the
 *  module graph hangs one off every box it draws. Named as the union rather than widened to
 *  `Element`, which drops the typed event map and turns every `addEventListener` here into an
 *  unresolved overload. */
export function tooltip(node: HTMLElement | SVGElement, input: TooltipArg) {
  let opts = normalizeOptions(input);
  let openTimer: number | null = null;

  function clearOpenTimer() {
    if (openTimer !== null) {
      window.clearTimeout(openTimer);
      openTimer = null;
    }
  }

  function show() {
    clearOpenTimer();
    tooltipState.show(node, opts);
  }

  function hide() {
    clearOpenTimer();
    tooltipState.hide(node);
  }

  function onMouseEnter() {
    if (opts.disabled || !opts.content) return;
    clearOpenTimer();
    const delay = tooltipState.shouldSkipDelay() ? 0 : opts.delay;
    if (delay <= 0) {
      show();
    } else {
      openTimer = window.setTimeout(show, delay);
    }
  }

  function onMouseLeave() {
    hide();
  }

  function onMouseDown() {
    // Don't keep the tooltip up over the element being clicked.
    hide();
  }

  function onFocus(e: FocusEvent) {
    if (opts.disabled || !opts.content) return;
    // Skip if a recent programmatic focus (e.g. Modal initial focus) is
    // active — that focus isn't a user-driven keyboard intent, so popping
    // a tooltip on it would be noisy.
    if (tooltipState.isFocusSuppressed()) return;
    const target = e.target as HTMLElement;
    // Only show on keyboard focus — mouse focus is already covered by hover.
    if (target.matches?.(':focus-visible')) {
      show();
    }
  }

  function onBlur() {
    hide();
  }

  // Both families carry every listener used here, but TypeScript resolves `addEventListener` on the
  // UNION down to `EventTarget`'s untyped overload, which loses the event types. One narrowing, at
  // the only place it matters, rather than untyping five handlers.
  const listens = node as HTMLElement;
  listens.addEventListener('mouseenter', onMouseEnter);
  listens.addEventListener('mouseleave', onMouseLeave);
  listens.addEventListener('mousedown', onMouseDown);
  listens.addEventListener('focus', onFocus, true);
  listens.addEventListener('blur', onBlur, true);

  return {
    update(next: TooltipArg) {
      const nextOpts = normalizeOptions(next);
      // Cheap structural skip: if nothing meaningful changed, don't churn the store.
      if (sameOpts(opts, nextOpts)) return;
      opts = nextOpts; // local var (not $state) — safe to assign synchronously.
      // Defer the singleton-store write to a microtask. The action's `update`
      // runs inside Svelte's reactive flush (a re-eval of the `use:tooltip`
      // param can coincide with a `$derived` recompute elsewhere — e.g. a
      // sidebar repaint that re-renders tooltipped rows). Writing `$state`
      // synchronously there trips `state_unsafe_mutation` and the uncaught
      // error breaks the render (the visible "freeze"). One microtask hop
      // moves the write past the frame — imperceptible for a tooltip.
      queueMicrotask(() => {
        // Disabled OR content went empty/falsy → hide if currently shown for this trigger.
        if (opts.disabled || !opts.content) { tooltipState.hide(node); return; }
        tooltipState.update(node, opts);
      });
    },
    destroy() {
      clearOpenTimer();
      // Same reasoning as `update`: `destroy` runs during effect teardown,
      // which is inside the flush — defer the `active` write so it never
      // lands during a derived/teardown. `hide(node)` is a no-op once another
      // trigger owns the tooltip, so the deferral is safe even if the node is
      // already detached by the time the microtask runs.
      queueMicrotask(() => tooltipState.hide(node));
      listens.removeEventListener('mouseenter', onMouseEnter);
      listens.removeEventListener('mouseleave', onMouseLeave);
      listens.removeEventListener('mousedown', onMouseDown);
      listens.removeEventListener('focus', onFocus, true);
      listens.removeEventListener('blur', onBlur, true);
    },
  };
}

/**
 * Variants that force a placement override regardless of what the input
 * specifies. Useful for vertical icon rails (left/right ActivityBar) where
 * tooltips should always fly out horizontally away from the bar — too
 * verbose to add `placement` at every call site.
 *
 *   import { tooltipRight as tooltip } from '$lib/actions/tooltip';
 */
function makeForcedPlacement(placement: TooltipPlacement) {
  return function (node: HTMLElement, input: TooltipArg) {
    const inner = tooltip(node, withPlacement(input, placement));
    return {
      update(next: TooltipArg) {
        inner.update(withPlacement(next, placement));
      },
      destroy() {
        inner.destroy();
      },
    };
  };
}

export const tooltipRight = makeForcedPlacement('right');
export const tooltipLeft = makeForcedPlacement('left');
export const tooltipTop = makeForcedPlacement('top');
export const tooltipBottom = makeForcedPlacement('bottom');
