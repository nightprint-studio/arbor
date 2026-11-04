/**
 * The arithmetic behind the inspector's friendlier widgets: rotations as Euler angles in degrees,
 * colours as hex. Pure, so a round trip can be tested.
 *
 * Rotations follow glam's `EulerRot::XYZ` (what Bevy's `Transform::rotation.to_euler(XYZ)` gives):
 * the quaternion is `qx · qy · qz`.
 */

export type Quat = [number, number, number, number];
export type Euler = [number, number, number];

const DEG = 180 / Math.PI;
const RAD = Math.PI / 180;

/** `[x, y, z, w]` → Euler angles in degrees (X, Y, Z). */
export function quatToEuler([x, y, z, w]: Quat): Euler {
  const n = Math.hypot(x, y, z, w) || 1;
  const [qx, qy, qz, qw] = [x / n, y / n, z / n, w / n];
  const r00 = 1 - 2 * (qy * qy + qz * qz);
  const r01 = 2 * (qx * qy - qz * qw);
  const r02 = 2 * (qx * qz + qy * qw);
  const r12 = 2 * (qy * qz - qx * qw);
  const r22 = 1 - 2 * (qx * qx + qy * qy);
  const b = Math.asin(Math.max(-1, Math.min(1, r02)));
  return [Math.atan2(-r12, r22) * DEG, b * DEG, Math.atan2(-r01, r00) * DEG];
}

/** Euler angles in degrees (X, Y, Z) → `[x, y, z, w]`. */
export function eulerToQuat([a, b, c]: Euler): Quat {
  const axis = (deg: number, i: 0 | 1 | 2): Quat => {
    const q: Quat = [0, 0, 0, Math.cos((deg * RAD) / 2)];
    q[i] = Math.sin((deg * RAD) / 2);
    return q;
  };
  return multiply(multiply(axis(a, 0), axis(b, 1)), axis(c, 2));
}

/** Hamilton product `p · q`, both `[x, y, z, w]`. */
export function multiply(p: Quat, q: Quat): Quat {
  const [px, py, pz, pw] = p;
  const [qx, qy, qz, qw] = q;
  return [
    pw * qx + px * qw + py * qz - pz * qy,
    pw * qy - px * qz + py * qw + pz * qx,
    pw * qz + px * qy - py * qx + pz * qw,
    pw * qw - px * qx - py * qy - pz * qz,
  ];
}

export type Rgba = [number, number, number, number];

const channel = (v: number) => Math.round(Math.max(0, Math.min(1, v)) * 255);

/** `[r, g, b, a]` in 0..1 → `#rrggbb` (alpha is not part of the hex a colour input edits). */
export function rgbaToHex([r, g, b]: Rgba): string {
  return `#${[r, g, b].map((c) => channel(c).toString(16).padStart(2, '0')).join('')}`;
}

/** `#rrggbb` → `[r, g, b]` in 0..1, or `null` when it is not one. */
export function hexToRgb(hex: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  return m ? [parseInt(m[1], 16) / 255, parseInt(m[2], 16) / 255, parseInt(m[3], 16) / 255] : null;
}

export const srgbToLinear = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
export const linearToSrgb = (c: number) => (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055);

/** Round for display and for sending back: `0.30000001` → `0.3`. */
export function round(v: number, digits = 4): number {
  const f = 10 ** digits;
  return Math.round(v * f) / f;
}
