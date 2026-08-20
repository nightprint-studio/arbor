/**
 * Persisted sizes for Bennu's docked panels.
 *
 * The one thing in this window that legitimately lives in `localStorage`. Rule 11 puts every
 * *setting* on the filesystem, and these are not settings: they are where you happen to have
 * dragged a divider, they are per-window rather than per-profile, and syncing them across
 * machines with different screens would be worse than not syncing them at all. The rule names
 * exactly this exception — "stato UI puramente effimero/di sessione (es. ratio dei panel
 * resizable)" — and Arbor's own shell and merula both already do it this way.
 *
 * Stored as a **viewport ratio** rather than a pixel count, through the shared helper: a divider
 * a third of the way across stays a third of the way across when the window is resized, or when
 * the same profile is opened on a smaller screen. Clamped to each panel's bounds on restore, so
 * a ratio saved on an ultrawide cannot produce a sidebar wider than the laptop it is reopened on.
 *
 * The right dock has **two** slots, not one. The i18n panel is given a wider default than the
 * tool windows beside it — it shows a sentence where they show names — and one shared slot would
 * mean opening i18n resized Maven, and closing it resized it back. Two keys, so each is
 * remembered as what you set it to.
 *
 * Defaults and bounds mirror the `<PanelCard>` props in `BennuWindow` — keep them in step.
 */
import { loadPixels, saveRatio } from '$lib/utils/panel-ratio';

const LEFT_KEY        = 'bennu:left-ratio';
const BOTTOM_KEY      = 'bennu:bottom-ratio';
const RIGHT_KEY       = 'bennu:right-ratio';
const RIGHT_WIDE_KEY  = 'bennu:right-wide-ratio';

function createPanelSizes() {
  let left       = $state(loadPixels(LEFT_KEY,       260, 180, 460));
  let bottom     = $state(loadPixels(BOTTOM_KEY,     220, 120, 560, true));
  let right      = $state(loadPixels(RIGHT_KEY,      280, 200, 520));
  let rightWide  = $state(loadPixels(RIGHT_WIDE_KEY, 400, 280, 760));

  return {
    get left()      { return left; },
    get bottom()    { return bottom; },
    get right()     { return right; },
    get rightWide() { return rightWide; },

    /** The right dock's size for the panel currently in it. */
    rightFor(wide: boolean): number {
      return wide ? rightWide : right;
    },

    setLeft(px: number)   { left = px;   saveRatio(LEFT_KEY,   px); },
    setBottom(px: number) { bottom = px; saveRatio(BOTTOM_KEY, px, true); },
    setRight(px: number, wide: boolean) {
      if (wide) {
        rightWide = px;
        saveRatio(RIGHT_WIDE_KEY, px);
      } else {
        right = px;
        saveRatio(RIGHT_KEY, px);
      }
    },
  };
}

export const bennuPanelSizes = createPanelSizes();
