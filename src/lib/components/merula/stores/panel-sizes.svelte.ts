/**
 * Persisted sizes for the merula dock panels + the arrangement↔editor split.
 *
 * Mirrors Arbor's main-window behaviour (see `$lib/stores/ui.svelte.ts`): panel
 * sizes are ephemeral UI chrome, so they live in localStorage as viewport
 * ratios via the shared `panel-ratio` helper — survive a window resize, clamp
 * to each panel's [min, max] on restore. The *which-panel-is-open* layout state
 * stays in the typed merula window state (workspaceStore); only the pixel sizes
 * are here.
 *
 * Defaults + bounds mirror the `<PanelCard>` / `<ResizablePanel>` props in
 * MerulaShell — keep them in sync if those change.
 */
import { loadPixels, saveRatio } from '$lib/utils/panel-ratio';

const LEFT_KEY   = 'merula:left-ratio';
const RIGHT_KEY  = 'merula:right-ratio';
const BOTTOM_KEY = 'merula:bottom-ratio';
const VIZ_KEY    = 'merula:viz-ratio';

function createPanelSizes() {
  let left   = $state(loadPixels(LEFT_KEY,   240, 170, 460));
  let right  = $state(loadPixels(RIGHT_KEY,  300, 210, 520));
  let bottom = $state(loadPixels(BOTTOM_KEY, 220,  90, 560, true));
  let viz    = $state(loadPixels(VIZ_KEY,    600, 320, 1100));

  return {
    get left()   { return left; },
    get right()  { return right; },
    get bottom() { return bottom; },
    get viz()    { return viz; },
    setLeft(px: number)   { left = px;   saveRatio(LEFT_KEY,   px); },
    setRight(px: number)  { right = px;  saveRatio(RIGHT_KEY,  px); },
    setBottom(px: number) { bottom = px; saveRatio(BOTTOM_KEY, px, true); },
    setViz(px: number)    { viz = px;    saveRatio(VIZ_KEY,    px); },
  };
}

export const panelSizes = createPanelSizes();
