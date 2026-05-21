import { untrack } from 'svelte';

export type AnimSpeed = 'fast' | 'normal' | 'slow';

const SPEED_SCALE: Record<AnimSpeed, number> = {
  fast:   0.5,
  normal: 1.0,
  slow:   1.5,
};

// Base durations (ms) at Normal speed.
const BASE_FAST    = 80;    // micro-interactions, hover states
const BASE_BASE    = 150;   // standard overlay / backdrop
const BASE_SLOW    = 240;   // (unused directly, available for dSlow)
const BASE_PANEL   = 200;   // panels sliding in/out
const BASE_OVERLAY = 150;   // floating overlays

function applyToRoot(enabled: boolean, speed: AnimSpeed) {
  const scale = enabled ? SPEED_SCALE[speed] : 0;
  const root = document.documentElement;
  root.style.setProperty('--anim-dur-fast',    `${Math.round(BASE_FAST    * scale)}ms`);
  root.style.setProperty('--anim-dur-base',    `${Math.round(BASE_BASE    * scale)}ms`);
  root.style.setProperty('--anim-dur-slow',    `${Math.round(BASE_SLOW    * scale)}ms`);
  root.style.setProperty('--anim-dur-panel',   `${Math.round(BASE_PANEL   * scale)}ms`);
  root.style.setProperty('--anim-dur-overlay', `${Math.round(BASE_OVERLAY * scale)}ms`);
}

function createAnimStore() {
  let enabled = $state(localStorage.getItem('arbor:anim-enabled') !== 'false');
  let speed   = $state<AnimSpeed>(
    (localStorage.getItem('arbor:anim-speed') as AnimSpeed | null) ?? 'normal',
  );

  // Apply immediately so first render uses the correct durations.
  // Wrapped in `untrack` because this initialiser runs once at module load —
  // the setEnabled/setSpeed methods explicitly re-call applyToRoot afterwards.
  untrack(() => applyToRoot(enabled, speed));

  // Derived JS values for Svelte transition `duration` props.
  const dFast   = $derived(Math.round(BASE_FAST    * (enabled ? SPEED_SCALE[speed] : 0)));
  const dBase   = $derived(Math.round(BASE_BASE    * (enabled ? SPEED_SCALE[speed] : 0)));
  const dPanel  = $derived(Math.round(BASE_PANEL   * (enabled ? SPEED_SCALE[speed] : 0)));
  const dSlow   = $derived(Math.round(BASE_SLOW    * (enabled ? SPEED_SCALE[speed] : 0)));

  return {
    get enabled() { return enabled; },
    get speed()   { return speed; },

    /** Duration for micro-interactions (hover states, toggles). */
    get dFast()  { return dFast;  },
    /** Duration for standard UI transitions (backdrop fades). */
    get dBase()  { return dBase;  },
    /** Duration for panels sliding in/out. */
    get dPanel() { return dPanel; },
    /** Duration for slower decorative animations. */
    get dSlow()  { return dSlow;  },

    setEnabled(v: boolean) {
      enabled = v;
      localStorage.setItem('arbor:anim-enabled', String(v));
      applyToRoot(enabled, speed);
    },

    setSpeed(s: AnimSpeed) {
      speed = s;
      localStorage.setItem('arbor:anim-speed', s);
      applyToRoot(enabled, speed);
    },
  };
}

export const animStore = createAnimStore();
