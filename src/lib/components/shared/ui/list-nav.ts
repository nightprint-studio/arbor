/**
 * `listNav` — arrow-key focus movement for a list, as a Svelte action on the list's CONTAINER.
 *
 * It moves DOM focus between the elements matching `selector` (default `[data-list-item]`) and nothing
 * else: no state, no selection model, no markup change. Items must already be focusable (a native
 * `<button>`, or `role` + `tabindex`); the action does not add tabindex, so the existing Tab order is
 * untouched. Enter/click/context-menu stay the item's own business.
 *
 * It never fights a text field: keys typed in an input/textarea/select/contenteditable are ignored,
 * except the one explicit hand-off `fromInput` (ArrowDown in the search box -> first item). Modified
 * keys (Ctrl/Alt/Meta/Shift) are ignored too, and `preventDefault` is called only when focus really
 * moved, so a key that did nothing keeps its native behaviour (page scroll, caret, ...).
 *
 * Deliberately generic: no Arbor imports, usable by any Svelte app.
 */

export interface ListNavOptions {
  /** CSS selector of the navigable items. Default `[data-list-item]`. */
  selector?: string;
  /** `vertical` -> Up/Down, `horizontal` -> Left/Right. Default `vertical`. */
  orientation?: 'vertical' | 'horizontal';
  /** Wrap from last to first and vice versa. Default false. */
  loop?: boolean;
  /** Handle Home/End. Default true. */
  homeEnd?: boolean;
  /** Handle PageUp/PageDown (jump `pageSize` items). Default false. */
  pageKeys?: boolean;
  /** Items skipped by PageUp/PageDown. Default 10. */
  pageSize?: number;
  /** Skip items that are `disabled`, `aria-disabled="true"`, `hidden` or not rendered. Default true. */
  skipDisabled?: boolean;
  /** Called when "previous" is pressed on the first item. Defaults to focusing the `fromInput` field, if set. */
  onEscapeFromTop?: () => void;
  /** Selector of a text field inside the container from which ArrowDown jumps to the first item. */
  fromInput?: string;
}

const TEXT_FIELD = 'input, textarea, select, [contenteditable=""], [contenteditable="true"]';

function isUsable(el: HTMLElement, skipDisabled: boolean): boolean {
  if (!skipDisabled) return true;
  if (el.hasAttribute('disabled') || el.getAttribute('aria-disabled') === 'true' || el.hidden) return false;
  // Not rendered (display:none ancestor) -> cannot take focus.
  return el.getClientRects().length > 0;
}

function focusItem(el: HTMLElement): void {
  el.focus();
  el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

export function listNav(node: HTMLElement, initial: ListNavOptions = {}) {
  let opts = initial;

  function onKeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey || e.isComposing) return;
    const target = e.target as HTMLElement | null;
    if (!target) return;

    const selector = opts.selector ?? '[data-list-item]';
    const skip = opts.skipDisabled ?? true;
    const vertical = (opts.orientation ?? 'vertical') === 'vertical';
    const prevKey = vertical ? 'ArrowUp' : 'ArrowLeft';
    const nextKey = vertical ? 'ArrowDown' : 'ArrowRight';

    const items = () => Array.from(node.querySelectorAll<HTMLElement>(selector)).filter((el) => isUsable(el, skip));

    // Typing context: only the explicit search-field hand-off is allowed through.
    if (target.closest(TEXT_FIELD)) {
      if (opts.fromInput && e.key === nextKey && target.matches(opts.fromInput)) {
        const first = items()[0];
        if (first) { focusItem(first); e.preventDefault(); }
      }
      return;
    }

    const current = target.closest<HTMLElement>(selector);
    if (!current && target !== node) return;
    const list = items();
    if (list.length === 0) return;
    const index = current ? list.indexOf(current) : -1;

    let next: number | null = null;
    switch (e.key) {
      case nextKey:
        next = index < 0 ? 0 : index + 1 < list.length ? index + 1 : opts.loop ? 0 : null;
        break;
      case prevKey:
        if (index <= 0) {
          if (index === 0) {
            // Default hand-back: the same field `fromInput` came from.
            const back = opts.onEscapeFromTop ?? (opts.fromInput
              ? () => node.querySelector<HTMLElement>(opts.fromInput!)?.focus()
              : undefined);
            if (back) { back(); e.preventDefault(); return; }
          }
          next = opts.loop ? list.length - 1 : null;
        } else next = index - 1;
        break;
      case 'Home':
        if (opts.homeEnd ?? true) next = 0;
        break;
      case 'End':
        if (opts.homeEnd ?? true) next = list.length - 1;
        break;
      case 'PageDown':
        if (opts.pageKeys) next = Math.min(list.length - 1, Math.max(index, 0) + (opts.pageSize ?? 10));
        break;
      case 'PageUp':
        if (opts.pageKeys) next = Math.max(0, Math.max(index, 0) - (opts.pageSize ?? 10));
        break;
    }

    if (next === null || next === index) return;
    focusItem(list[next]);
    e.preventDefault();
  }

  node.addEventListener('keydown', onKeydown);
  return {
    update(o: ListNavOptions = {}) { opts = o; },
    destroy() { node.removeEventListener('keydown', onKeydown); },
  };
}

/**
 * Space/Enter activation for a non-native `role="button"` item. Call from its `onkeydown`; fires only
 * when the item itself is focused (not an inner control), and prevents the page-scroll Space would do.
 * Returns true when it activated.
 */
export function activateOnKey(e: KeyboardEvent, activate: () => void): boolean {
  if (e.target !== e.currentTarget) return false;
  if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return false;
  if (e.key !== 'Enter' && e.key !== ' ') return false;
  e.preventDefault();
  activate();
  return true;
}
