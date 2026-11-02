/**
 * Morphing a list when its contents change, instead of cutting to the new one: what leaves fades out, what stays glides
 * to its new place (FLIP), what is new rises in one after another, and the box eases to its new height.
 *
 * The caller says how to find the items (`items`), what identifies one (`keyOf`), which of the current ones survive
 * the change (`survives`, asked after `change` ran) and how to redraw (`render`). Every animation honours the app's
 * animations switch: with it off the change is applied at once.
 */
import { animStore } from '$lib/stores/animations.svelte';

export interface MorphSpec {
  box: HTMLElement;
  items: () => HTMLElement[];
  keyOf: (el: HTMLElement) => string;
  /** Apply the state change. */
  change: () => void;
  /** Keys that will still be there after `change`. */
  survives: () => Set<string>;
  /** Redraw the DOM for the new state, and resolve when it is in place. */
  render: () => Promise<void> | void;
  /** `true` while a newer morph has superseded this one. */
  superseded: () => boolean;
}

const EASE = 'cubic-bezier(.22,.8,.22,1)';

export async function morph(s: MorphSpec): Promise<void> {
  if (!animStore.enabled) { s.change(); await s.render(); return; }
  const old = s.items();
  const from = new Map(old.map((el) => [s.keyOf(el), el.getBoundingClientRect()]));
  const h0 = s.box.getBoundingClientRect().height;
  s.change();
  const keep = s.survives();
  await Promise.all(
    old.filter((el) => !keep.has(s.keyOf(el))).map((el) => el.animate(
      [{ opacity: 1, transform: 'scale(1)' }, { opacity: 0, transform: 'scale(.92)' }],
      { duration: 280, easing: 'ease-in', fill: 'forwards' },
    ).finished.catch(() => undefined)),
  );
  if (s.superseded()) return;
  await s.render();
  let enter = 0;
  for (const el of s.items()) {
    const was = from.get(s.keyOf(el));
    if (was) {
      const r = el.getBoundingClientRect(), dx = was.left - r.left, dy = was.top - r.top;
      if (dx || dy) el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], { duration: 700, easing: EASE });
    } else {
      el.animate(
        [{ opacity: 0, transform: 'translateY(16px) scale(.96)' }, { opacity: 1, transform: 'none' }],
        { duration: 650, delay: 160 + enter++ * 55, easing: EASE, fill: 'backwards' },
      );
    }
  }
  const h1 = s.box.getBoundingClientRect().height;
  if (h0 !== h1) {
    s.box.style.overflow = 'clip';
    const done = () => { s.box.style.overflow = ''; };
    s.box.animate([{ height: `${h0}px` }, { height: `${h1}px` }], { duration: 700, easing: EASE }).finished.then(done, done);
  }
}
