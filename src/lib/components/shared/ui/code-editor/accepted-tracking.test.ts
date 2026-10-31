import { describe, expect, it } from 'vitest';
import { ChangeSet } from '@codemirror/state';
import { mapAccepted, type AcceptedProposal } from './accepted-tracking';

const proposal = (from: number, to: number): AcceptedProposal => ({ from, to, ghost: 'x'.repeat(to - from), meta: null });

describe('following an accepted proposal', () => {
  it('keeps the range when nothing changes', () => {
    const changes = ChangeSet.of([], 20).desc;
    expect(mapAccepted(proposal(5, 10), changes)).toEqual(proposal(5, 10));
  });

  it('extends over text written at its end and leaves out text written at its start', () => {
    const atEnd = ChangeSet.of({ from: 10, insert: ', b' }, 20).desc;
    expect(mapAccepted(proposal(5, 10), atEnd)).toMatchObject({ from: 5, to: 13 });
    const atStart = ChangeSet.of({ from: 5, insert: 'zz' }, 20).desc;
    expect(mapAccepted(proposal(5, 10), atStart)).toMatchObject({ from: 7, to: 12 });
  });

  it('shifts with text written before it', () => {
    const before = ChangeSet.of({ from: 0, insert: '    ' }, 20).desc;
    expect(mapAccepted(proposal(5, 10), before)).toMatchObject({ from: 9, to: 14 });
  });

  it('collapses when it is deleted whole', () => {
    const gone = ChangeSet.of({ from: 5, to: 10 }, 20).desc;
    const mapped = mapAccepted(proposal(5, 10), gone);
    expect(mapped.to).toBe(mapped.from);
  });

  it('comes back to the same range when an edit inside it is put back', () => {
    const typed = ChangeSet.of({ from: 7, insert: 'q' }, 20);
    const removed = ChangeSet.of({ from: 7, to: 8 }, 21);
    const there = mapAccepted(mapAccepted(proposal(5, 10), typed.desc), removed.desc);
    expect(there).toMatchObject({ from: 5, to: 10 });
  });
});
