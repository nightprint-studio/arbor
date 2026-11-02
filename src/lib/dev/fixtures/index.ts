/**
 * Answers the Tauri mock gives back, keyed by Tauri command or `program.method` for `rpc`.
 *
 * A value is returned as-is; a function receives the call's params. Grow this file one
 * screen at a time: boot the screen, read `window.__mock.missing`, add what it asked for.
 * Split into per-product files once it passes a few hundred lines.
 */
// Not `unknown | fn`: `unknown` absorbs the union and the function arm loses its contextual type,
// so every `(p) => …` fixture would have an implicit-any parameter.
export type FixtureHandler =
  | string | number | boolean | null | object
  | ((params: Record<string, any>) => unknown);

const windowPlugin: Record<string, FixtureHandler> = {
  'plugin:window|scale_factor': 1,
  'plugin:window|inner_size': { width: 1440, height: 900 },
  'plugin:window|outer_size': { width: 1440, height: 900 },
  'plugin:window|inner_position': { x: 0, y: 0 },
  'plugin:window|outer_position': { x: 0, y: 0 },
  'plugin:window|is_fullscreen': false,
  'plugin:window|is_maximized': false,
  'plugin:window|is_minimized': false,
  'plugin:window|is_focused': true,
  'plugin:window|is_visible': true,
  'plugin:window|is_decorated': false,
  'plugin:window|theme': 'dark',
  'plugin:window|title': 'Arbor',
};

import { platformFixtures } from './platform';
import { corvusFixtures } from './corvus';
import { bennuFixtures } from './bennu';
import { picusFixtures } from './picus';
import { merulaFixtures } from './merula';
import { tytoFixtures } from './tyto';
import { studioFixtures } from './studio';
import { garrulusFixtures } from './garrulus';
import { sittaFixtures } from './sitta';
import { sharedFixtures } from './shared';

export const fixtures: Record<string, FixtureHandler> = {
  ...windowPlugin,
  ...platformFixtures,
  ...sharedFixtures,
  ...corvusFixtures,
  ...bennuFixtures,
  ...picusFixtures,
  ...merulaFixtures,
  ...tytoFixtures,
  ...studioFixtures,
  ...garrulusFixtures,
  ...sittaFixtures,
};
