import { describe, expect, it } from 'vitest';
import { eulerToQuat, hexToRgb, linearToSrgb, quatToEuler, rgbaToHex, round, srgbToLinear } from './brp-math';

const near = (a: number[], b: number[], eps = 1e-6) => a.forEach((v, i) => expect(Math.abs(v - b[i])).toBeLessThan(eps));

describe('rotations', () => {
  it('maps the identity to zero angles and back', () => {
    near(quatToEuler([0, 0, 0, 1]), [0, 0, 0]);
    near(eulerToQuat([0, 0, 0]), [0, 0, 0, 1]);
  });

  it('turns a quarter turn about Y into 90 degrees', () => {
    const s = Math.SQRT1_2;
    near(quatToEuler([0, s, 0, s]), [0, 90, 0], 1e-4);
    near(eulerToQuat([0, 90, 0]), [0, s, 0, s]);
  });

  it('round-trips angles away from the poles', () => {
    for (const e of [[30, 20, 10], [-45, 60, 120], [170, -30, -80]] as [number, number, number][]) {
      near(quatToEuler(eulerToQuat(e)), e, 1e-6);
    }
  });

  it('tolerates a quaternion that is not quite unit length', () => {
    near(quatToEuler([0, 0, 0, 2]), [0, 0, 0]);
  });
});

describe('colours', () => {
  it('writes and reads hex', () => {
    expect(rgbaToHex([1, 0.5, 0, 1])).toBe('#ff8000');
    expect(hexToRgb('#FF8000')).toEqual([1, 128 / 255, 0]);
    expect(hexToRgb('red')).toBeNull();
    expect(rgbaToHex([2, -1, 0.5, 1])).toBe('#ff0080');
  });

  it('converts between sRGB and linear', () => {
    expect(srgbToLinear(0)).toBe(0);
    expect(srgbToLinear(1)).toBeCloseTo(1, 9);
    expect(linearToSrgb(srgbToLinear(0.5))).toBeCloseTo(0.5, 9);
  });
});

describe('round', () => {
  it('trims float noise', () => {
    expect(round(0.30000001)).toBe(0.3);
    expect(round(12.345678, 2)).toBe(12.35);
  });
});
