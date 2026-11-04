import { describe, expect, it } from 'vitest';
import { collectVariables, headerNameSuggest, headerValueSuggest, undefinedVariables, variableSuggest } from './completions';

const kv = (key: string, value: string) => ({ key, value, description: '', enabled: true });

describe('variables', () => {
  const vars = collectVariables([
    { scope: 'local', vars: [kv('token', '@secret')] },
    { scope: 'environment', vars: [kv('baseUrl', 'https://stg'), kv('token', 'old')] },
    { scope: 'collection', vars: [kv('baseUrl', 'http://dev')] },
  ]);

  it('the first layer to define a name wins, and secrets show no value', () => {
    expect(vars.find((v) => v.name === 'token')).toMatchObject({ scope: 'local', secret: true, value: '' });
    expect(vars.find((v) => v.name === 'baseUrl')).toMatchObject({ scope: 'environment', value: 'https://stg' });
  });

  it('the dynamic ones are always there', () => {
    expect(vars.some((v) => v.name === '$uuid' && v.scope === 'dynamic')).toBe(true);
  });

  it('{{ offers them, narrowed by what is typed, and replaces the closing braces', () => {
    const r = variableSuggest('{{base}}/x', 6, vars)!;
    expect(r.items.map((i) => i.label)).toEqual(['baseUrl']);
    expect(r.items[0].id).toBe('baseUrl}}');
    expect(r.from).toBe(2);
    expect(r.to).toBe(8);
  });

  it('offers nothing outside braces', () => {
    expect(variableSuggest('http://x/{{a}}/', 15, vars)).toBeNull();
    expect(variableSuggest('plain', 5, vars)).toBeNull();
  });

  it('finds the names nothing defines', () => {
    const known = new Set(['baseUrl']);
    expect(undefinedVariables(['{{baseUrl}}/{{id}}', 'x {{ id }} {{$uuid}} {{tenant}}'], known)).toEqual(['id', 'tenant']);
  });
});

describe('headers', () => {
  it('suggests names that contain what is typed, prefix first', () => {
    const r = headerNameSuggest('con', 3, [])!;
    expect(r.items[0].label).toMatch(/^Con/);
    expect(r.items.map((i) => i.label)).toContain('Content-Type');
  });

  it('knows the usual values of a header', () => {
    const r = headerValueSuggest('Content-Type', 'app', 3, [])!;
    expect(r.items.map((i) => i.label)).toContain('application/json');
    expect(headerValueSuggest('X-Unknown', 'a', 1, [])).toBeNull();
  });

  it('a header value can still ask for a variable', () => {
    const v = collectVariables([{ scope: 'environment', vars: [kv('token', 'x')] }]);
    expect(headerValueSuggest('Authorization', 'Bearer {{to', 11, v)!.items[0].label).toBe('token');
  });
});
