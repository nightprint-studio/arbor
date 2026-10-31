import { describe, expect, it } from 'vitest';
import { categoryOf, groupByCategory } from './memory-categories';
import type { MemoryItem } from '$lib/ipc/processes';

const item = (label: string, bytes: number | null = 100, mapped = false): MemoryItem => ({ scope: 'process', label, bytes, count: 1, exact: false, mapped });

describe('memory categories', () => {
  it('reads a heading off the words of a label', () => {
    expect(categoryOf({ label: 'Reference index (use sites)' }).heading).toBe('Code index and checks');
    expect(categoryOf({ label: 'JDK classes decoded' }).heading).toBe('Classes and libraries');
    expect(categoryOf({ label: 'jdtls server — diagnostics held' }).heading).toBe('Language servers');
    expect(categoryOf({ label: 'Something new' }).heading).toBe('Other');
  });

  it('strips the Learned prefix and uses it as the heading', () => {
    expect(categoryOf({ label: 'Learned · rows you stopped offering (rules)' })).toEqual({ heading: 'What Bennu learned', text: 'rows you stopped offering (rules)' });
  });

  it('orders the headings, sums them without mapped files, and puts the larger line first', () => {
    const groups = groupByCategory([item('Reference index (use sites)', 50), item('Learned · counters', 10), item('Symbol index files (memory-mapped)', 999, true), item('Parsed symbols per file', 70)]);
    expect(groups.map((g) => g.heading)).toEqual(['What Bennu learned', 'Code index and checks']);
    expect(groups[1].bytes).toBe(120);
    expect(groups[1].items[0].text).toBe('Symbol index files (memory-mapped)');
  });
});
