import { describe, expect, it } from 'vitest';
import { filterOptions, groupOptions, toggled, type ListOption } from './option-list';

describe('option lists', () => {
  const opts: ListOption[] = [
    { value: 'web', label: 'Spring Web', description: 'Build web applications with Spring MVC', group: 'Web' },
    { value: 'data-jpa', label: 'Spring Data JPA', description: 'Persist data with Hibernate', group: 'SQL' },
    { value: 'security', label: 'Spring Security', description: 'Authentication and access control', group: 'Security' },
    { value: 'webflux', label: 'Spring Reactive Web', group: 'Web' },
  ];

  it('returns everything, in order, for an empty query', () => {
    expect(filterOptions(opts, '  ').map((o) => o.value)).toEqual(['web', 'data-jpa', 'security', 'webflux']);
  });

  it('matches by name and by id, name hits first', () => {
    const r = filterOptions(opts, 'web').map((o) => o.value);
    expect(r.slice(0, 2).sort()).toEqual(['web', 'webflux']);
    expect(r).not.toContain('security');
  });

  it('falls back to the description, ranked below name hits', () => {
    const r = filterOptions(opts, 'hibernate').map((o) => o.value);
    expect(r).toEqual(['data-jpa']);
    const mixed = filterOptions(opts, 'spring').map((o) => o.value);
    expect(mixed).toHaveLength(4);
  });

  it('matches several terms independently', () => {
    expect(filterOptions(opts, 'data jpa').map((o) => o.value)).toEqual(['data-jpa']);
    expect(filterOptions(opts, 'zzz')).toEqual([]);
  });

  it('groups by heading in first-seen order', () => {
    const g = groupOptions(opts);
    expect(g.map((x) => x.group)).toEqual(['Web', 'SQL', 'Security']);
    expect(g[0].items.map((o) => o.value)).toEqual(['web', 'webflux']);
    expect(groupOptions([{ value: 'a', label: 'A' }])[0].group).toBe('');
  });

  it('toggles membership', () => {
    expect(toggled(['a'], 'b')).toEqual(['a', 'b']);
    expect(toggled(['a', 'b'], 'a')).toEqual(['b']);
  });
});
