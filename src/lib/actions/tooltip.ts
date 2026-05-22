import {
  tooltipState,
  normalizeOptions,
  type TooltipInput,
  type TooltipPlacement,
  type NormalizedTooltipOptions,
} from '$lib/stores/tooltip.svelte';

function withPlacement(input: TooltipInput, placement: TooltipPlacement): TooltipInput {
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
export function tooltip(node: HTMLElement, input: TooltipInput) {
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

  node.addEventListener('mouseenter', onMouseEnter);
  node.addEventListener('mouseleave', onMouseLeave);
  node.addEventListener('mousedown', onMouseDown);
  node.addEventListener('focus', onFocus, true);
  node.addEventListener('blur', onBlur, true);

  return {
    update(next: TooltipInput) {
      const nextOpts = normalizeOptions(next);
      // Cheap structural skip: if nothing meaningful changed, don't churn the store.
      if (sameOpts(opts, nextOpts)) return;
      opts = nextOpts;
      // Disabled OR content went empty/falsy → hide if currently shown for this trigger.
      if (opts.disabled || !opts.content) {
        hide();
        return;
      }
      tooltipState.update(node, opts);
    },
    destroy() {
      hide();
      node.removeEventListener('mouseenter', onMouseEnter);
      node.removeEventListener('mouseleave', onMouseLeave);
      node.removeEventListener('mousedown', onMouseDown);
      node.removeEventListener('focus', onFocus, true);
      node.removeEventListener('blur', onBlur, true);
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
  return function (node: HTMLElement, input: TooltipInput) {
    const inner = tooltip(node, withPlacement(input, placement));
    return {
      update(next: TooltipInput) {
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
