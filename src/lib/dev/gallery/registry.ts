/**
 * Per-scene overrides and extra (non-Modal) scenes, merged from one file per area so parallel
 * work on different areas never collides. Keys are scene ids (`corvus/CloneRepoModal`).
 */
import type { SceneOverride, ExtraScene } from './scenes';
import { corvusOverrides, corvusExtras } from './corvus';
import { bennuOverrides, bennuExtras } from './bennu';
import { picusOverrides, picusExtras } from './picus';
import { merulaOverrides, merulaExtras } from './merula';
import { tytoOverrides, tytoExtras } from './tyto';
import { studioOverrides, studioExtras } from './studio';
import { garrulusOverrides, garrulusExtras } from './garrulus';
import { hirundoOverrides, hirundoExtras } from './hirundo';
import { sittaOverrides, sittaExtras } from './sitta';
import { sharedOverrides, sharedExtras } from './shared';
import { genOverrides, genExtras } from './gen';
import { panelsOverrides, panelsExtras } from './panels';
import { editorOverrides, editorExtras } from './editor';

export const overrides: Record<string, SceneOverride> = {
  ...sharedOverrides,
  ...corvusOverrides,
  ...bennuOverrides,
  ...picusOverrides,
  ...merulaOverrides,
  ...tytoOverrides,
  ...studioOverrides,
  ...garrulusOverrides,
  ...hirundoOverrides,
  ...sittaOverrides,
  ...genOverrides,
  ...panelsOverrides,
  ...editorOverrides,
};

export const extras: ExtraScene[] = [
  ...sharedExtras,
  ...corvusExtras,
  ...bennuExtras,
  ...picusExtras,
  ...merulaExtras,
  ...tytoExtras,
  ...studioExtras,
  ...garrulusExtras,
  ...hirundoExtras,
  ...sittaExtras,
  ...genExtras,
  ...panelsExtras,
  ...editorExtras,
];
