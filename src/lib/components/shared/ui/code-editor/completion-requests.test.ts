import { describe, expect, it } from 'vitest';

import { BURST_MS, createCompletionRequests, MAX_STALE_MS, type Clock } from './completion-requests';

/** A clock the test moves by hand; a sleep resolves only when `wake` is called. */
function manualClock() {
  let time = 1_000;
  const sleepers: (() => void)[] = [];
  const clock: Clock = {
    now: () => time,
    sleep: (ms) => new Promise<void>((resolve) => sleepers.push(() => {
      time += ms;
      resolve();
    })),
  };
  return {
    clock,
    advance(ms: number) {
      time += ms;
    },
    wake() {
      sleepers.splice(0).forEach((resume) => resume());
    },
  };
}

/** A fetch that counts its calls and answers with the key it was asked. */
function counter() {
  const asked: string[] = [];
  return {
    asked,
    fetch: (key: string) => () => {
      asked.push(key);
      return Promise.resolve([key]);
    },
  };
}

describe('completion requests', () => {
  it('asks at once for a key typed after a pause', () => {
    const { clock } = manualClock();
    const { asked, fetch } = counter();
    const requests = createCompletionRequests<string[]>(clock);
    void requests.request('a', fetch('a'));
    expect(asked).toEqual(['a']);
  });

  it('asks once for a burst, when the typing pauses, and drops what the burst superseded', async () => {
    const time = manualClock();
    const { asked, fetch } = counter();
    const requests = createCompletionRequests<string[]>(time.clock);
    await requests.request('i', fetch('i'));
    time.advance(BURST_MS / 2);
    const second = requests.request('id', fetch('id'));
    time.advance(BURST_MS / 2);
    const third = requests.request('ide', fetch('ide'));
    expect(asked).toEqual(['i']);
    time.wake();
    expect(await second).toBeNull();
    expect(await third).toEqual(['ide']);
    expect(asked).toEqual(['i', 'ide']);
  });

  it('samples a long burst so the list does not freeze while the typing lasts', async () => {
    const time = manualClock();
    const { asked, fetch } = counter();
    const requests = createCompletionRequests<string[]>(time.clock);
    await requests.request('k0', fetch('k0'));
    const step = BURST_MS / 2;
    for (let i = 1; step * i < MAX_STALE_MS; i++) {
      time.advance(step);
      void requests.request(`k${i}`, fetch(`k${i}`));
    }
    expect(asked).toEqual(['k0']);
    time.advance(step);
    void requests.request('late', fetch('late'));
    expect(asked).toEqual(['k0', 'late']);
  });

  it('never delays an explicit request', () => {
    const time = manualClock();
    const { asked, fetch } = counter();
    const requests = createCompletionRequests<string[]>(time.clock);
    void requests.request('a', fetch('a'));
    time.advance(10);
    void requests.request('ab', fetch('ab'), true);
    expect(asked).toEqual(['a', 'ab']);
  });

  it('shares the answer of an identical question still in flight', async () => {
    const time = manualClock();
    const asked: string[] = [];
    let answer!: (value: string[]) => void;
    const slow = () => {
      asked.push('same');
      return new Promise<string[]>((resolve) => {
        answer = resolve;
      });
    };
    const requests = createCompletionRequests<string[]>(time.clock);
    const first = requests.request('same', slow);
    time.advance(MAX_STALE_MS * 2);
    const second = requests.request('same', slow);
    expect(asked).toEqual(['same']);
    answer(['x']);
    expect(await first).toBeNull();
    expect(await second).toEqual(['x']);
  });
});
