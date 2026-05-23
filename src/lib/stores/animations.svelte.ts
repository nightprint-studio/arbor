import { untrack } from 'svelte';
import { getAnimationsConfig, setAnimationsConfig } from '$lib/ipc/config';
import type { AnimSpeed, AnimationsConfig } from '$lib/types/config';

export type { AnimSpeed };

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
  // Defaults applied synchronously so first paint already has sane durations;
  // `loadConfig()` (called from AppShell.onMount) overwrites with disk values.
  let enabled = $state<boolean>(true);
  let speed   = $state<AnimSpeed>('normal');
  let loaded  = $state(false);

  // Wrapped in `untrack` because this initialiser runs once at module load —
  // setEnabled / setSpeed / loadConfig explicitly re-call applyToRoot after.
  untrack(() => applyToRoot(enabled, speed));

  // Derived JS values for Svelte transition `duration` props.
  const dFast   = $derived(Math.round(BASE_FAST    * (enabled ? SPEED_SCALE[speed] : 0)));
  const dBase   = $derived(Math.round(BASE_BASE    * (enabled ? SPEED_SCALE[speed] : 0)));
  const dPanel  = $derived(Math.round(BASE_PANEL   * (enabled ? SPEED_SCALE[speed] : 0)));
  const dSlow   = $derived(Math.round(BASE_SLOW    * (enabled ? SPEED_SCALE[speed] : 0)));

  async function loadConfig() {
    try {
      const cfg = await getAnimationsConfig();
      enabled = !!cfg.enabled;
      speed   = (cfg.speed === 'fast' || cfg.speed === 'slow') ? cfg.speed : 'normal';
      applyToRoot(enabled, speed);
      loaded = true;
    } catch {
      // First-run / backend not ready — keep defaults; next call will retry.
    }
  }

  function persist() {
    const next: AnimationsConfig = { enabled, speed };
    void setAnimationsConfig(next).catch(() => {});
  }

  return {
    get enabled() { return enabled; },
    get speed()   { return speed; },
    get loaded()  { return loaded; },

    /** Duration for micro-interactions (hover states, toggles). */
    get dFast()  { return dFast;  },
    /** Duration for standard UI transitions (backdrop fades). */
    get dBase()  { return dBase;  },
    /** Duration for panels sliding in/out. */
    get dPanel() { return dPanel; },
    /** Duration for slower decorative animations. */
    get dSlow()  { return dSlow;  },

    loadConfig,

    setEnabled(v: boolean) {
      if (enabled === v) return;
      enabled = v;
      applyToRoot(enabled, speed);
      persist();
    },

    setSpeed(s: AnimSpeed) {
      if (speed === s) return;
      speed = s;
      applyToRoot(enabled, speed);
      persist();
    },
  };
}

export const animStore = createAnimStore();
