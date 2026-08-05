/**
 * How wide the debugger's two columns are, and whether they are open at all.
 *
 * Pure layout: no project knows about it, and it is not a *setting* — it is where you last
 * dragged a divider. CLAUDE.md's rule 11 names exactly this case as the one thing
 * `localStorage` is still for ("ratio dei panel resizable"), so that is where it goes:
 * per-machine, across projects, and no round trip to a backend to draw a panel.
 *
 * Collapsing rather than dragging to zero: a column pulled to nothing is indistinguishable
 * from a broken layout, and there is nothing left to grab to bring it back. Collapsed leaves a
 * labelled strip you can click.
 */

const KEY = 'arbor:bennu:debug-layout';

interface Layout {
  framesWidth: number;
  valuesWidth: number;
  framesOpen: boolean;
  valuesOpen: boolean;
}

const DEFAULTS: Layout = {
  framesWidth: 260,
  valuesWidth: 340,
  framesOpen: true,
  valuesOpen: true,
};

function read(): Layout {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULTS };
    const saved = JSON.parse(raw) as Partial<Layout>;
    return {
      // Clamped on the way in: a stored width from a wider monitor, or a hand-edited entry,
      // must not be able to leave the console with no room.
      framesWidth: clamp(saved.framesWidth ?? DEFAULTS.framesWidth, 140, 640),
      valuesWidth: clamp(saved.valuesWidth ?? DEFAULTS.valuesWidth, 180, 720),
      framesOpen: saved.framesOpen ?? DEFAULTS.framesOpen,
      valuesOpen: saved.valuesOpen ?? DEFAULTS.valuesOpen,
    };
  } catch {
    return { ...DEFAULTS };
  }
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

function createDebugLayout() {
  let state = $state<Layout>(read());

  /** Best-effort: a full or disabled storage costs the memory of where you dragged it, and
   *  nothing more. */
  function save() {
    try {
      localStorage.setItem(KEY, JSON.stringify(state));
    } catch {
      /* the session's layout still applies */
    }
  }

  return {
    get framesWidth() { return state.framesWidth; },
    get valuesWidth() { return state.valuesWidth; },
    get framesOpen() { return state.framesOpen; },
    get valuesOpen() { return state.valuesOpen; },

    setFramesWidth(w: number) { state = { ...state, framesWidth: Math.round(w) }; save(); },
    setValuesWidth(w: number) { state = { ...state, valuesWidth: Math.round(w) }; save(); },
    toggleFrames() { state = { ...state, framesOpen: !state.framesOpen }; save(); },
    toggleValues() { state = { ...state, valuesOpen: !state.valuesOpen }; save(); },
  };
}

export const bennuDebugLayout = createDebugLayout();
