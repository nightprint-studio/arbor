import { describe, expect, it } from 'vitest';
import type { FixRefusal, RenamedName } from '$lib/ipc/bennu/naming';
import { diffNames } from '$lib/utils/name-diff';
import {
  buildLines,
  effectiveName,
  filesOf,
  invertSelection,
  isValidRename,
  noFilter,
  referenceCount,
  refusalKind,
  refusalSummary,
  refusedLines,
  selectAll,
  selectNone,
  selectOnly,
  selectedEdits,
  selectedFileMoves,
  selectionCounts,
  setRename,
  setRow,
} from './naming-fix-selection';

function edit(file: string, start: number, newText: string, reason = 'use') {
  return { file, start, end: start + 3, new_text: newText, old: 'old', reason, inferred: false };
}

function name(
  file: string,
  from: string,
  to: string,
  target = 'method',
  uses: { file: string; start: number }[] = [],
  line = 1,
): RenamedName {
  return {
    file,
    line,
    from,
    to,
    target: target as RenamedName['target'],
    file_rename: null,
    edits: [edit(file, 0, to, 'declaration'), ...uses.map((u) => edit(u.file, u.start, to))],
  };
}

const PLAN: RenamedName[] = [
  name('/p/A.java', 'get_a', 'getA', 'method', [{ file: '/p/B.java', start: 10 }]),
  name('/p/A.java', 'set_a', 'setA', 'method'),
  name('/p/B.java', 'tmp_x', 'tmpX', 'local'),
  name('/p/C.java', 'max_n', 'maxN', 'constant'),
];

describe('selection', () => {
  it('starts with everything selected and counts it', () => {
    const f = noFilter('file');
    expect(selectionCounts(PLAN, f)).toEqual({ names: 4, files: 3, edits: 5 });
  });

  it('unticking a row removes it from the count and from the edits', () => {
    const f = setRow(noFilter('file'), 0, false);
    expect(selectionCounts(PLAN, f).names).toBe(3);
    expect(selectedEdits(PLAN, f).every((e) => e.new_text !== 'getA')).toBe(true);
  });

  it('filtering hides a name AND leaves it out of the apply', () => {
    const f = { ...noFilter('file'), search: 'tmp' };
    expect(selectionCounts(PLAN, f).names).toBe(1);
    expect(selectedEdits(PLAN, f)).toHaveLength(1);
  });

  it('hiding a kind leaves its names out as well', () => {
    const f = { ...noFilter('file'), hiddenTargets: new Set(['local']) };
    expect(selectionCounts(PLAN, f).names).toBe(3);
  });

  it('select none / all act on the visible rows only', () => {
    // A row already unticked, then a filter that shows one other row.
    let f = { ...setRow(noFilter('file'), 2, false), search: 'set' };
    f = selectNone(PLAN, f);
    expect(selectionCounts(PLAN, f).names).toBe(0);
    // Clearing the filter shows that the rows it hid kept their state: 0, 3 on; 1, 2 off.
    expect(selectionCounts(PLAN, { ...f, search: '' }).names).toBe(2);
    f = selectAll(PLAN, f);
    expect(selectionCounts(PLAN, f).names).toBe(1);
    expect(selectionCounts(PLAN, { ...f, search: '' }).names).toBe(3);
  });

  it('inverting flips every visible row', () => {
    let f = setRow(noFilter('none'), 0, false);
    f = invertSelection(PLAN, f);
    expect(selectionCounts(PLAN, f).names).toBe(1);
    expect(selectedEdits(PLAN, f).some((e) => e.new_text === 'getA')).toBe(true);
  });

  it('selects only one kind, or one file', () => {
    const methods = selectOnly(PLAN, noFilter('file'), (n) => n.target === 'method');
    expect(selectionCounts(PLAN, methods).names).toBe(2);
    const fileA = selectOnly(PLAN, noFilter('file'), (n) => n.file === '/p/A.java');
    expect(selectionCounts(PLAN, fileA).names).toBe(2);
  });

  it('a group header ticks off a whole file through hiddenGroups', () => {
    const f = { ...noFilter('file'), hiddenGroups: new Set(['/p/A.java']) };
    expect(selectionCounts(PLAN, f).names).toBe(2);
    // …and "select all" brings the group back.
    expect(selectionCounts(PLAN, selectAll(PLAN, f)).names).toBe(4);
  });
});

describe('lines', () => {
  it('groups by file in first-appearance order with a header per group', () => {
    const lines = buildLines(PLAN, noFilter('file'));
    expect(lines.filter((l) => l.kind === 'group').map((l) => (l.kind === 'group' ? l.label : ''))).toEqual([
      'A.java',
      'B.java',
      'C.java',
    ]);
    expect(lines).toHaveLength(7);
  });

  it('has no headers when not grouped', () => {
    expect(buildLines(PLAN, noFilter('none')).every((l) => l.kind === 'item')).toBe(true);
  });
});

describe('reach', () => {
  it('counts references besides the declaration, and the files a rename touches', () => {
    expect(referenceCount(PLAN[0])).toBe(1);
    expect(referenceCount(PLAN[1])).toBe(0);
    expect(filesOf(PLAN[0])).toEqual([
      { file: '/p/A.java', edits: 1 },
      { file: '/p/B.java', edits: 1 },
    ]);
  });
});

describe('typed names', () => {
  it('accepts an identifier that is not the old name', () => {
    expect(isValidRename(PLAN[0], 'fetchA')).toBe(true);
    expect(isValidRename(PLAN[0], 'get_a')).toBe(false);
    expect(isValidRename(PLAN[0], '2fast')).toBe(false);
    expect(isValidRename(PLAN[0], 'a b')).toBe(false);
    expect(isValidRename(PLAN[0], '')).toBe(false);
  });

  it('writes the typed name in every edit of that rename, and only that rename', () => {
    const f = setRename(PLAN, noFilter('file'), 0, 'fetchA');
    expect(effectiveName(PLAN[0], 0, f)).toBe('fetchA');
    expect(effectiveName(PLAN[1], 1, f)).toBe('setA');
    const edits = selectedEdits(PLAN, f);
    expect(edits.filter((e) => e.new_text === 'fetchA')).toHaveLength(2);
    expect(edits.some((e) => e.new_text === 'getA')).toBe(false);
    expect(edits.some((e) => e.new_text === 'setA')).toBe(true);
  });

  it('an invalid or proposal-equal entry clears the override', () => {
    let f = setRename(PLAN, noFilter('file'), 0, 'fetchA');
    f = setRename(PLAN, f, 0, 'not valid');
    expect(f.renames.has(0)).toBe(false);
    f = setRename(PLAN, setRename(PLAN, f, 0, 'fetchA'), 0, 'getA');
    expect(f.renames.has(0)).toBe(false);
  });

  it('moves the file to the typed name', () => {
    const type: RenamedName = {
      ...name('/p/old_thing.java', 'old_thing', 'OldThing', 'type'),
      file_rename: { from: '/p/old_thing.java', to: '/p/OldThing.java' },
    };
    const f = setRename([type], noFilter('none'), 0, 'Thing');
    expect(selectedFileMoves([type], f)).toEqual([{ from: '/p/old_thing.java', to: '/p/Thing.java' }]);
    expect(selectedFileMoves([type], noFilter('none'))).toEqual([{ from: '/p/old_thing.java', to: '/p/OldThing.java' }]);
    // Unticked: the file stays put.
    expect(selectedFileMoves([type], setRow(noFilter('none'), 0, false))).toEqual([]);
  });
});

describe('refusals', () => {
  const refusal = (reason: string, n = 'x'): FixRefusal => ({ file: '/p/A.java', line: 3, name: n, reason });

  it('reads the backend sentences', () => {
    expect(refusalKind('more than one name in this file would become `getA`')).toBe('collision');
    expect(refusalKind('`getA` already appears in this file')).toBe('collision');
    expect(refusalKind('its edits overlap a rename already planned in B.java')).toBe('collision');
    expect(refusalKind('serialisation: @JsonProperty takes the Java name')).toBe('serialisation');
    expect(refusalKind('a package segment is not renamed by editing text')).toBe('package');
    expect(refusalKind("this file is not valid in the project's declared encoding")).toBe('encoding');
    expect(refusalKind('stopped before this type could be planned')).toBe('stopped');
    expect(refusalKind('the engine found no editable site for this declaration')).toBe('unsupported');
    expect(refusalKind('it overrides a method of a library type')).toBe('library');
    expect(refusalKind('something new')).toBe('other');
  });

  it('summarises and groups them in a stable order', () => {
    const list = [
      refusal('stopped before'),
      refusal('`a` already appears in this file', 'a'),
      refusal('`b` already appears in this file', 'b'),
    ];
    expect(refusalSummary(list)).toEqual([
      { kind: 'collision', label: 'Collision', count: 2 },
      { kind: 'stopped', label: 'Stopped', count: 1 },
    ]);
    const lines = refusedLines(list, '');
    expect(lines.map((l) => l.kind)).toEqual(['group', 'item', 'item', 'group', 'item']);
    expect(refusedLines(list, '`b`').filter((l) => l.kind === 'item')).toHaveLength(1);
  });
});

describe('diffNames', () => {
  it('marks only the middle', () => {
    expect(diffNames('get_user_name', 'getUserName')).toEqual({
      prefix: 'get',
      removed: '_user_n',
      added: 'UserN',
      suffix: 'ame',
    });
    expect(diffNames('fooBar_', 'fooBar')).toEqual({ prefix: 'fooBar', removed: '_', added: '', suffix: '' });
  });

  it('keeps a shared suffix and does not overlap prefix and suffix', () => {
    expect(diffNames('aXb', 'aYb')).toEqual({ prefix: 'a', removed: 'X', added: 'Y', suffix: 'b' });
    expect(diffNames('aa', 'aaa')).toEqual({ prefix: 'aa', removed: '', added: 'a', suffix: '' });
  });

  it('never splits a surrogate pair', () => {
    expect(diffNames('a😀b', 'a😁b')).toEqual({ prefix: 'a', removed: '😀', added: '😁', suffix: 'b' });
  });
});
