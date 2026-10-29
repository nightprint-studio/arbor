import { describe, expect, it } from 'vitest';
import type { GeneratorField, NewProjectGenerator } from '$lib/ipc/bennu/new-project';
import {
  buildRequest, defaultsOf, freeName, initialGenerator, isVisible,
  joinForDisplay, mergeValues, neighbour, newRunId, problemFor, splitFields, strayProblems,
} from './new-project-helpers';

const field = (over: Partial<GeneratorField> & Pick<GeneratorField, 'key' | 'kind'>): GeneratorField => ({
  label: over.key, default: '', options: [], advanced: false, ...over,
});

const gen = (id: string, over: Partial<NewProjectGenerator> = {}): NewProjectGenerator => ({
  id, category: 'java', label: id, description: '', icon: 'x', name_rule: 'artifact',
  requires: [], remote: false, available: true, ...over,
});

describe('defaults and merging', () => {
  const fields = [
    field({ key: 'group', kind: 'text', default: 'com.example' }),
    field({ key: 'java', kind: 'select', default: '21', options: [{ value: '17', label: '17' }, { value: '21', label: '21' }] }),
    field({ key: 'sample', kind: 'toggle', default: true }),
    field({ key: 'deps', kind: 'multi', default: [], options: [{ value: 'web', label: 'Web' }, { value: 'jpa', label: 'JPA' }] }),
  ];

  it('collects defaults, copying lists', () => {
    const d = defaultsOf(fields);
    expect(d).toEqual({ group: 'com.example', java: '21', sample: true, deps: [] });
    (d.deps as string[]).push('web');
    expect(fields[3].default).toEqual([]);
  });

  it('keeps choices that still make sense', () => {
    const merged = mergeValues(fields, { group: 'org.acme', java: '17', sample: false, deps: ['web', 'gone'] });
    expect(merged).toEqual({ group: 'org.acme', java: '17', sample: false, deps: ['web', 'gone'].filter((x) => x !== 'gone') });
  });

  it('drops a select value that is no longer an option, and values of the wrong kind', () => {
    const merged = mergeValues(fields, { java: '8', sample: 'yes', group: 5 as unknown as string });
    expect(merged.java).toBe('21');
    expect(merged.sample).toBe(true);
    expect(merged.group).toBe('com.example');
  });

  it('keeps a multi-select as is while the option list is still empty (skeleton form)', () => {
    const skeleton = [field({ key: 'deps', kind: 'multi', default: [] })];
    expect(mergeValues(skeleton, { deps: ['web'] }).deps).toEqual(['web']);
  });
});

describe('visibility', () => {
  const author = field({ key: 'author', kind: 'text', show_when: { key: 'license', value: 'MIT' } });
  const toggle = field({ key: 'x', kind: 'text', show_when: { key: 'on', value: 'true' } });

  it('follows the controlling field', () => {
    expect(isVisible(author, { license: 'MIT' })).toBe(true);
    expect(isVisible(author, { license: '' })).toBe(false);
    expect(isVisible(toggle, { on: true })).toBe(true);
    expect(isVisible(toggle, { on: false })).toBe(false);
  });

  it('splits basic from advanced after hiding', () => {
    const f = [field({ key: 'a', kind: 'text' }), field({ key: 'b', kind: 'text', advanced: true }), author];
    const { basic, advanced } = splitFields(f, { license: '' });
    expect(basic.map((x) => x.key)).toEqual(['a']);
    expect(advanced.map((x) => x.key)).toEqual(['b']);
  });
});

describe('problems', () => {
  const problems = [{ field: 'name', message: 'bad name' }, { field: 'hidden', message: 'x' }];
  it('finds a field message and the ones nobody draws', () => {
    expect(problemFor(problems, 'name')).toBe('bad name');
    expect(problemFor(problems, 'other')).toBeNull();
    expect(strayProblems(problems, new Set(['name']))).toEqual([{ field: 'hidden', message: 'x' }]);
  });
});

describe('generators', () => {
  const list = [gen('a', { available: false }), gen('b'), gen('c')];

  it('opens on the remembered generator, else the first usable one', () => {
    expect(initialGenerator(list, 'c')?.id).toBe('c');
    expect(initialGenerator(list, 'a')?.id).toBe('b');
    expect(initialGenerator(list, 'zzz')?.id).toBe('b');
    expect(initialGenerator([gen('x', { available: false })], '')?.id).toBe('x');
    expect(initialGenerator([], '')).toBeNull();
  });

  it('walks the list in a circle', () => {
    expect(neighbour(list, 'c', 1)?.id).toBe('a');
    expect(neighbour(list, 'a', -1)?.id).toBe('c');
    expect(neighbour(list, 'nope', 1)?.id).toBe('a');
    expect(neighbour([], 'a', 1)).toBeNull();
  });
});

describe('names, paths, requests', () => {
  it('finds a free name', () => {
    const taken = new Set(['demo', 'demo-2']);
    expect(freeName('demo', (n) => taken.has(n))).toBe('demo-3');
    expect(freeName('fresh', (n) => taken.has(n))).toBe('fresh');
  });

  it('joins for display with forward slashes', () => {
    expect(joinForDisplay('C:\\Users\\me\\proj\\', ' app ')).toBe('C:/Users/me/proj/app');
    expect(joinForDisplay('', 'app')).toBe('');
  });

  it('builds a trimmed request with only the form’s own options', () => {
    const fields = [field({ key: 'group', kind: 'text' }), field({ key: 'sample', kind: 'toggle' })];
    const req = buildRequest('maven', ' demo ', ' C:\\p\\ ', true, fields, { group: ' org.x ', sample: false, stale: 'x' });
    expect(req).toEqual({ generator: 'maven', name: 'demo', location: 'C:\\p', git: true, options: { group: 'org.x', sample: false } });
  });

  it('makes run ids the backend accepts', () => {
    const id = newRunId(1_700_000_000_000, 0.5);
    expect(id).toMatch(/^[A-Za-z0-9_-]{1,40}$/);
    expect(newRunId(1, 0.1)).not.toBe(newRunId(1, 0.9));
  });
});
