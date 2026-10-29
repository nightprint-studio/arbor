/**
 * How a menu appears: it grows out of the edge it hangs from, fading in, instead of sliding as a flat box.
 *
 * One transition for the dropdown, the split button's menu and every submenu, so they open the same way.
 * `origin` is where it grows from (the trigger's edge); `slide` is the small drift along the axis it opens
 * on. Pure CSS from Svelte's transition hook, so it costs nothing while the menu is shut.
 */
import { cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

export interface MenuMotion {
  /** CSS transform-origin: where the menu grows from. */
  origin?: string;
  /** Drift in px, as [x, y] at the start. */
  from?: [number, number];
  /** Scale at the start. */
  scale?: number;
  duration: number;
}

export function menuReveal(_node: Element, { origin = 'top left', from = [0, -6], scale = 0.95, duration }: MenuMotion): TransitionConfig {
  return {
    duration,
    easing: cubicOut,
    css: (t, u) =>
      `opacity:${t};transform-origin:${origin};` +
      `transform:translate(${from[0] * u}px,${from[1] * u}px) scale(${scale + (1 - scale) * t});`,
  };
}

/** Where a menu opening in `direction` grows from, and which way it drifts. */
export function motionFor(direction: 'up' | 'down' | 'left' | 'right', duration: number): MenuMotion {
  switch (direction) {
    case 'up': return { origin: 'bottom left', from: [0, 6], duration };
    case 'left': return { origin: 'top right', from: [6, 0], duration };
    case 'right': return { origin: 'top left', from: [-6, 0], duration };
    default: return { origin: 'top left', from: [0, -6], duration };
  }
}
