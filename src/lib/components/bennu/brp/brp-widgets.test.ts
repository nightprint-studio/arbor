import { describe, expect, it } from 'vitest';
import { classify, dateToSeconds, durationToSeconds, hintFor, hintKey, secondsToDate, secondsToDuration } from './brp-widgets';

describe('classify', () => {
  it('knows a transform by its three members', () => {
    expect(classify('bevy_transform::components::transform::Transform', { translation: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] })).toBe('transform');
  });

  it('knows colours, durations and handles by their shape', () => {
    expect(classify(undefined, { red: 1, green: 0, blue: 0, alpha: 1 })).toBe('color');
    expect(classify(undefined, { secs: 1, nanos: 500 })).toBe('duration');
    expect(classify(undefined, { Strong: { Uuid: 'x' } })).toBe('handle');
  });

  it('tells a quaternion from a vector by its type name', () => {
    expect(classify('glam::Quat', [0, 0, 0, 1])).toBe('quat');
    expect(classify('glam::Vec4', [0, 0, 0, 1])).toBe('vec');
    expect(classify(undefined, [1.5, 2, 3])).toBe('vec');
    expect(classify(undefined, [111, 112])).toBe('generic');
    expect(classify('bevy_ecs::hierarchy::Children', [111, 112])).toBe('generic');
    expect(classify(undefined, [1, 2, 3, 4, 5])).toBe('generic');
  });

  it('follows a hint', () => {
    expect(classify(undefined, [1, 0, 0], { color: true })).toBe('color');
    expect(classify(undefined, 0.5, { range: [0, 1] })).toBe('slider');
    expect(classify(undefined, 90, { angle: true })).toBe('angle');
    expect(classify(undefined, 1700000000, { date: true })).toBe('date');
    expect(classify(undefined, 3)).toBe('generic');
  });
});

describe('hints', () => {
  it('keys a reflect path the way the macro does', () => {
    expect(hintKey('.stats.hp')).toBe('stats.hp');
    expect(hintKey('.items[2].x')).toBe('items.2.x');
    expect(hintKey('')).toBe('');
  });

  it('finds the hint for a field of a type', () => {
    const map = { 'game::Light': { intensity: { range: [0, 10] as [number, number] } } };
    expect(hintFor(map, 'game::Light', '.intensity')).toEqual({ range: [0, 10] });
    expect(hintFor(map, 'game::Light', '.other')).toBeUndefined();
    expect(hintFor(undefined, 'game::Light', '.intensity')).toBeUndefined();
  });
});

describe('conversions', () => {
  it('round-trips a date and a duration', () => {
    expect(secondsToDate(0)).toBe('1970-01-01');
    expect(dateToSeconds('2024-02-29')).toBe(1709164800);
    expect(dateToSeconds('nope')).toBeNull();
    expect(durationToSeconds(secondsToDuration(2.5))).toBeCloseTo(2.5, 9);
    expect(secondsToDuration(-1)).toEqual({ secs: 0, nanos: 0 });
  });
});
