import { describe, expect, it } from 'vitest';
import { applySlash, formatSkillSize, matchSkills, slashQuery, sourceLabel } from './assistant-skills';

const skills = [
  { name: 'review', description: 'Review a change' },
  { name: 'release-notes', description: 'Write the notes for a release' },
  { name: 'ship', description: 'Review then release' },
];

describe('slashQuery', () => {
  it('sees a slash word at the start', () => {
    expect(slashQuery('/')).toEqual({ query: '', start: 0 });
    expect(slashQuery('/rev')).toEqual({ query: 'rev', start: 0 });
    expect(slashQuery('  /rev')).toEqual({ query: 'rev', start: 2 });
  });
  it('follows a run of skills', () => {
    expect(slashQuery('/review /sh')).toEqual({ query: 'sh', start: 8 });
  });
  it('ignores a slash later in the sentence and finished words', () => {
    expect(slashQuery('look at /rev')).toBeNull();
    expect(slashQuery('/review ')).toBeNull();
    expect(slashQuery('/review the code')).toBeNull();
    expect(slashQuery('src/main')).toBeNull();
    expect(slashQuery('')).toBeNull();
  });
});

describe('matchSkills', () => {
  it('ranks name prefix, then name, then description', () => {
    expect(matchSkills(skills, 're').map((s) => s.name)).toEqual(['release-notes', 'review', 'ship']);
    expect(matchSkills(skills, 'notes').map((s) => s.name)).toEqual(['release-notes']);
    expect(matchSkills(skills, 'zzz')).toEqual([]);
    expect(matchSkills(skills, '')).toHaveLength(3);
  });
});

describe('applySlash', () => {
  it('replaces the word being typed', () => {
    expect(applySlash('/rev', 'review')).toBe('/review ');
    expect(applySlash('/review /sh', 'ship')).toBe('/review /ship ');
  });
});

describe('labels', () => {
  it('formats sizes and sources', () => {
    expect(formatSkillSize(900)).toBe('900 B');
    expect(formatSkillSize(2048)).toBe('2.0 KB');
    expect(formatSkillSize(40 * 1024)).toBe('40 KB');
    expect(sourceLabel({ scope: 'project', source: '.claude' })).toBe('Project · .claude');
  });
});
