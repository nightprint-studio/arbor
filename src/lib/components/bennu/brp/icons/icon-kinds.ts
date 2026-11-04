/** Which icon draws what: an entity by the kind the backend found, a component by its type name. */

import type { BrpIconName } from './icon-set';

const ENTITY_KINDS: readonly string[] = ['camera', 'light', 'mesh', 'ui', 'sprite', 'text', 'window'];

/** The icon of an entity: its kind when a marker component said so, else a cube (a bigger one if it has children). */
export function entityIcon(kind: string | null | undefined, childCount: number): BrpIconName {
  if (kind && ENTITY_KINDS.includes(kind)) return kind as BrpIconName;
  return childCount > 0 ? 'group' : 'entity';
}

const tail = (path: string): string => (path.split('<')[0].split('::').pop() ?? '').trim();

/** The icon of a component or resource, from its type path: `Transform` → axes, `PointLight` → a bulb, … */
export function componentIcon(typePath: string): BrpIconName {
  const name = tail(typePath);
  if (/^(Global)?Transform$/.test(name)) return 'transform';
  if (/^Camera(2d|3d)?$/.test(name)) return 'camera';
  if (/Light$/.test(name)) return 'light';
  if (/^Mesh(2d|3d)?$/.test(name)) return 'mesh';
  if (/^(Node|Button|ImageNode|BackgroundColor|BorderColor)$/.test(name)) return 'ui';
  if (/^Sprite/.test(name)) return 'sprite';
  if (/^Text/.test(name)) return 'text';
  if (name === 'Window') return 'window';
  if (/^(Timer|Stopwatch)$|Timer$/.test(name)) return 'timer';
  if (/^(Handle|AssetId)$|Handle$/.test(name) || /^Handle</.test(typePath.split('::').pop() ?? '')) return 'handle';
  if (/^(State|NextState)$/.test(name)) return 'state';
  return 'component';
}
