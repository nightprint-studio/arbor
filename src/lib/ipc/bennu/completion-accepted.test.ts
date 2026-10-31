import { describe, it, expect, vi, beforeEach } from 'vitest';

const bennu = vi.fn(async () => undefined);
vi.mock('../rpc', () => ({ bennu: (...a: unknown[]) => (bennu as (...x: unknown[]) => unknown)(...a) }));

import { completionAccepted } from './index';

describe('completionAccepted', () => {
  beforeEach(() => bennu.mockClear());

  it('sends the pick alone when it is not told where the list was asked', async () => {
    await completionAccepted('java/util/List', 'stream', 'method');
    expect(bennu).toHaveBeenCalledWith('bennu_completion_accepted', {
      args: { owner: 'java/util/List', label: 'stream', kind: 'method', file: null, source: null, offset: null, signature: null },
    });
  });

  it('adds the file, the buffer the list was built from and the byte offset of the replaced word', async () => {
    await completionAccepted(null, 'Boolean', 'class', { file: '/p/A.java', source: 'x.filter(Boo', offset: 9 });
    expect(bennu).toHaveBeenCalledWith('bennu_completion_accepted', {
      args: { owner: null, label: 'Boolean', kind: 'class', file: '/p/A.java', source: 'x.filter(Boo', offset: 9, signature: null },
    });
  });

  it('adds the parameter list of the method row, which is the overload that was chosen', async () => {
    await completionAccepted('java/util/stream/IntStream', 'range', 'method', { file: '/p/A.java', source: 'IntStream.ra', offset: 10 }, '(int, int)');
    expect(bennu).toHaveBeenCalledWith('bennu_completion_accepted', {
      args: { owner: 'java/util/stream/IntStream', label: 'range', kind: 'method', file: '/p/A.java', source: 'IntStream.ra', offset: 10, signature: '(int, int)' },
    });
  });
});
