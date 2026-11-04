import { describe, expect, it } from 'vitest';
import type { Schema } from '$lib/types/hirundo';
import { fieldsAfter, follow, jsonContext, requiredPaths, sampleOf, skeletonText } from './schema';

const scalar = (type_text: string): Schema => ({ type_text, kind: 'scalar' });
const order: Schema = {
  type_text: 'OrderDto',
  kind: 'object',
  fields: [
    { name: 'id', type_text: 'long', required: true },
    { name: 'status', type_text: 'Status', required: false, schema: { type_text: 'Status', kind: 'enum', constants: ['OPEN', 'CLOSED'] } },
    {
      name: 'lines',
      type_text: 'List<Line>',
      required: false,
      schema: { type_text: 'List<Line>', kind: 'array', item: { type_text: 'Line', kind: 'object', fields: [{ name: 'sku', type_text: 'String', required: false }, { name: 'qty', type_text: 'int', required: true }] } },
    },
    { name: 'note', type_text: 'String', required: false },
  ],
};

describe('schema', () => {
  it('builds a sample with every field', () => {
    expect(sampleOf(order)).toEqual({ id: 1, status: 'OPEN', lines: [{ sku: '', qty: 1 }], note: '' });
    expect(JSON.parse(skeletonText(order))).toHaveProperty('lines');
  });

  it('lists the required paths, nested ones included', () => {
    expect(requiredPaths(order)).toEqual(['id', 'lines[].qty']);
  });

  it('follows a path through fields and arrays', () => {
    expect(follow(order, ['lines'])?.fields?.map((f) => f.name)).toEqual(['sku', 'qty']);
    expect(follow(order, ['lines', '[*]'])?.fields?.length).toBe(2);
    expect(follow(order, ['nope'])).toBeNull();
  });

  it('offers the fields after a body path', () => {
    expect(fieldsAfter(order, '').map((f) => f.name)).toEqual(['id', 'status', 'lines', 'note']);
    expect(fieldsAfter(order, 'lines[*]').map((f) => f.name)).toEqual(['sku', 'qty']);
  });

  it('reads the object the caret is inside', () => {
    const text = '{ "id": 1, "lines": [ { "sku": "a" }, { ';
    expect(jsonContext(text, text.length)).toEqual(['lines', '[*]']);
    expect(jsonContext('{ "a": ', 7)).toEqual([]);
    expect(jsonContext('{ "x": { ', 9)).toEqual(['x']);
  });

  it('a scalar schema has no fields', () => {
    expect(fieldsAfter(scalar('String'), '')).toEqual([]);
  });
});
