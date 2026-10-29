import { describe, expect, it } from 'vitest';
import type { BrpShape } from '$lib/ipc/bennu/brp';
import {
  currentVariant, elementLabel, entityTag, elementPath, elementShape, isInlineNumbers, jsonKind, payloadShape,
  shortTypeName,
} from './brp-value';

const vec3: BrpShape = {
  kind: 'struct',
  type_path: 'glam::Vec3',
  fields: [
    { name: 'x', shape: { kind: 'float' } },
    { name: 'y', shape: { kind: 'float' } },
    { name: 'z', shape: { kind: 'float' } },
  ],
};

describe('brp-value', () => {
  it('classifies json values', () => {
    expect(jsonKind(null)).toBe('null');
    expect(jsonKind([1])).toBe('array');
    expect(jsonKind({ a: 1 })).toBe('object');
    expect(jsonKind(2.5)).toBe('number');
  });

  it('shortens type paths, keeping a generic argument', () => {
    expect(shortTypeName('my_game::combat::Health')).toBe('Health');
    expect(shortTypeName('bevy_ecs::schedule::State<my_game::Phase>')).toBe('State<Phase>');
    expect(shortTypeName('f32')).toBe('f32');
  });

  it('addresses a struct-shaped array by field name, because that is how it is reflected', () => {
    // A Vec3 travels as [x, y, z] but `.translation[1]` is not a valid reflect path into a struct.
    expect(elementPath('.translation', vec3, 1)).toBe('.translation.y');
    expect(elementLabel(vec3, 2)).toBe('z');
    expect(elementShape(vec3, 0)).toEqual({ kind: 'float' });
  });

  it('addresses tuples by position and lists by index', () => {
    const tuple: BrpShape = { kind: 'tuple', type_path: 'X', items: [{ kind: 'int' }, { kind: 'int' }] };
    expect(elementPath('.p', tuple, 1)).toBe('.p.1');
    expect(elementPath('.items', { kind: 'list', item: { kind: 'int' } }, 2)).toBe('.items[2]');
    expect(elementPath('.unknown', undefined, 0)).toBe('.unknown[0]');
  });

  it('reads the current enum variant from either wire form', () => {
    expect(currentVariant('Idle')).toBe('Idle');
    expect(currentVariant({ Moving: 3 })).toBe('Moving');
    expect(currentVariant(3)).toBeNull();
  });

  it('flattens a newtype variant to its single payload shape', () => {
    const v = { kind: 'tuple' as const, name: 'Moving', sample: { Moving: 0 }, items: [{ kind: 'float' as const }] };
    expect(payloadShape(v)).toEqual({ kind: 'float' });
  });

  it('prints an entity id as index and generation', () => {
    expect(entityTag(4294967299)).toBe('3v1');
    expect(entityTag(7)).toBe('7v0');
  });

  it('puts short number arrays on one line', () => {
    expect(isInlineNumbers([0, 1, 2])).toBe(true);
    expect(isInlineNumbers([0, 1, 2, 3, 4])).toBe(false);
    expect(isInlineNumbers(['a'])).toBe(false);
    expect(isInlineNumbers([])).toBe(false);
  });
});
