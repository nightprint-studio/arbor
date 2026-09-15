/**
 * The navigation failures this file exists to prevent — each test is one of them, by name.
 *
 * They are questions about ORDER (a tab is active before its text arrives, a scroll runs before a
 * layout exists, a second jump starts before the first lands), and order is exactly what a
 * component test buried in CodeMirror and Svelte cannot ask cleanly. The history's own rules
 * (merging, truncation, mapping through edits) are tested in `nav-ring.test.ts`.
 */

import { describe, expect, it } from 'vitest';
import {
  HOLD_ATTEMPTS, HOLD_SETTLED, HOLD_TOLERANCE, NavFlow, READY_ATTEMPTS, REVEAL_ATTEMPTS,
  type NavHost, type NavPlace,
} from './nav-flow';

/** A world the test drives by hand: nothing is timed, everything is stepped. */
function world(
  opts: {
    activeFile?: string | null;
    readyFiles?: string[];
    blindFor?: number;
    /** The text of each file, for the byte-offset resolver. */
    texts?: Record<string, string>;
    /** How many measurements answer "there is no viewport yet". */
    unmeasuredFor?: number;
  } = {},
) {
  const readyFiles = new Set(opts.readyFiles ?? []);
  const texts = opts.texts ?? {};
  let blind = opts.blindFor ?? 0;
  let active: string | null = opts.activeFile ?? null;
  let caret: NavPlace | null = active ? { file: active, line: 1, col: 1 } : null;
  /** What the history was told a navigation left. */
  const origins: NavPlace[] = [];
  /** Where the history was told a recorded navigation landed. */
  const arrivals: NavPlace[] = [];
  const scrolls: { line: number; col: number }[] = [];
  /** The corrections the hold made — a reveal, never a scroll: a scroll would take the focus. */
  const reveals: number[] = [];
  let settles = 0;
  const RESTING = 100;
  let offset = RESTING;
  let unmeasured = opts.unmeasuredFor ?? 0;
  const visible = new Set<number>();

  const host: NavHost = {
    activeFile: () => active,
    async openFile(file) {
      active = file;
      // The reported shape: the tab is active and its TEXT is not here yet.
      caret = { file, line: 1, col: 1 };
    },
    ready: (file) => readyFiles.has(file),
    scrollTo(line, col) {
      scrolls.push({ line, col });
      if (active) caret = { file: active, line, col };
      offset = RESTING;
      visible.add(line);
    },
    lineOffset: () => {
      if (unmeasured > 0) {
        unmeasured -= 1;
        return null;
      }
      return offset;
    },
    lineOfByteOffset(file, byteOffset) {
      if (file !== active || !readyFiles.has(file)) return null;
      const lines = (texts[file] ?? '').split('\n');
      let seen = 0;
      for (let i = 0; i < lines.length; i += 1) {
        const end = seen + lines[i].length;
        if (byteOffset <= end) return { line: i + 1, col: byteOffset - seen + 1 };
        seen = end + 1;
      }
      return { line: lines.length, col: 1 };
    },
    reveal(line) {
      reveals.push(line);
      offset = RESTING;
      visible.add(line);
    },
    async settle() {
      settles += 1;
    },
    inView: (line) => {
      if (blind > 0) {
        blind -= 1;
        return false;
      }
      return visible.has(line) || scrolls.some((sc) => sc.line === line);
    },
    caret: () => caret,
    ring: {
      leave: (p) => void origins.push({ ...p }),
      arrive: (p) => void arrivals.push({ ...p }),
    },
  };

  return {
    host,
    origins,
    arrivals,
    scrolls,
    reveals,
    get settles() { return settles; },
    get active() { return active; },
    get caret() { return caret; },
    arrive: (file: string) => readyFiles.add(file),
    drift: (px: number) => { offset = RESTING + px; },
    loseSight: () => {
      visible.clear();
      scrolls.length = 0;
    },
    setCaret: (p: NavPlace) => { caret = p; },
    /** The caret moved, and the flow was told — the two halves of one real event. `user: false`
     *  is a programmatic selection (an editor mounting, a view state restored). */
    move(flow: NavFlow, p: NavPlace, user = true) {
      caret = p;
      flow.onCaret(p, user);
    },
  };
}

const at = (file: string, line: number, col = 1): NavPlace => ({ file, line, col });

describe('a navigation inside the file already open', () => {
  it('scrolls once, records where it started and where it landed', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 12));
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 71, col: 1 }]);
    expect(w.origins).toEqual([at('A.java', 12)]);
    expect(w.arrivals).toEqual([at('A.java', 71)]);
  });
});

describe('a navigation into a file whose text has not arrived', () => {
  it('waits for a buffer that can hold the line instead of scrolling an empty one', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const run = flow.go(at('B.java', 200), { record: true });
    await Promise.resolve();
    expect(w.scrolls).toEqual([]);
    w.arrive('B.java');
    expect(await run).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 200, col: 1 }]);
    expect(w.arrivals).toEqual([at('B.java', 200)]);
  });

  it('gives up rather than wedging the editor when the buffer never arrives', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('Gone.java', 10), { record: true })).toBe('unavailable');
    expect(w.scrolls).toEqual([]);
    expect(w.arrivals).toEqual([]);
    expect(w.settles).toBe(READY_ATTEMPTS);
    expect(flow.inFlight).toBe(false);
  });
});

describe('what the history is told', () => {
  /** Back and Forward have already moved the history; telling it anything would record a stop
   *  while navigating — which truncated Forward. */
  it('a Back records nothing at either end', async () => {
    const w = world({ activeFile: 'B.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('B.java', 200));
    expect(await flow.go(at('A.java', 71), { record: false })).toBe('landed');
    expect(w.origins).toEqual([]);
    expect(w.arrivals).toEqual([]);
  });

  it('records the landing, not the request', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    w.host.scrollTo = (line, col) => {
      const clamped = Math.min(line, 40);
      w.scrolls.push({ line: clamped, col });
      w.setCaret(at('A.java', clamped, col));
    };
    await flow.go(at('A.java', 900), { record: true });
    expect(w.arrivals).toEqual([at('A.java', 40)]);
  });

  it('after a Back, the next navigation starts from where Back landed', async () => {
    const w = world({ activeFile: 'B.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('B.java', 200));
    await flow.go(at('A.java', 71), { record: false });
    await flow.go(at('A.java', 5), { record: true });
    expect(w.origins).toEqual([at('A.java', 71)]);
  });
});

describe('caret events', () => {
  it('are transit while a navigation is in flight, and touch nothing', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 30));
    const run = flow.go(at('B.java', 200), { record: true });
    expect(w.origins).toEqual([at('A.java', 30)]);
    // The buffer landing produces a caret at the top of the new file; recording it is how Back
    // used to end up at line 1 of the class.
    flow.onCaret(at('B.java', 1));
    flow.onCaret(at('B.java', 1));
    expect(w.origins).toEqual([at('A.java', 30)]);
    expect(w.arrivals).toEqual([]);
    w.arrive('B.java');
    await run;
    expect(w.arrivals).toEqual([at('B.java', 200)]);
  });

  it('a caret in another file with nothing in flight records the place left', () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    flow.onCaret(at('A.java', 12));
    flow.onCaret(at('B.java', 3)); // a tab switch the user made
    expect(w.origins).toEqual([at('A.java', 12)]);
    expect(w.arrivals).toEqual([at('B.java', 3)]);
  });

  it('moving inside a file records nothing', () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    flow.onCaret(at('A.java', 12), true);
    flow.onCaret(at('A.java', 13), true);
    flow.onCaret(at('A.java', 400), true);
    expect(w.origins).toEqual([]);
    expect(w.arrivals).toEqual([]);
  });

  it('the origin is where the reader actually was when the jump started', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 1));
    w.move(flow, at('A.java', 71));
    await flow.go(at('B.java', 200), { record: true });
    expect(w.origins).toEqual([at('A.java', 71)]);
  });

  it('with no caret event yet, the origin is the caret the host reports', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.setCaret(at('A.java', 71));
    await flow.go(at('B.java', 200), { record: true });
    expect(w.origins).toEqual([at('A.java', 71)]);
  });
});

/**
 * The shape almost every panel navigates with: `openFile(f)` first, then a go-to into it. By the
 * time the go-to runs the tab has switched and the new editor has produced its own caret (line 1,
 * then a restored view state).
 */
describe('a panel jump: open the file, then go to a line in it', () => {
  it('records the place left once, not the mount caret of the destination', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 50));
    await w.host.openFile('B.java');
    w.move(flow, at('B.java', 1), false); // the editor mounting
    w.move(flow, at('B.java', 300), false); // a view state restored
    await flow.go(at('B.java', 200), { record: true });
    expect(w.origins).toEqual([at('A.java', 50)]);
    expect(w.arrivals.at(-1)).toEqual(at('B.java', 200));
  });

  it('once the reader has touched the arrived file, a jump records where they are', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 50));
    await w.host.openFile('B.java');
    w.move(flow, at('B.java', 1), false);
    w.move(flow, at('B.java', 77), true); // a click
    await flow.go(at('B.java', 200), { record: true });
    expect(w.origins).toEqual([at('A.java', 50), at('B.java', 77)]);
  });

  it('an untouched arrival is still an origin for a jump to a third file', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java', 'C.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 50));
    await w.host.openFile('B.java');
    w.move(flow, at('B.java', 9), false);
    await flow.go(at('C.java', 4), { record: true });
    expect(w.origins).toEqual([at('A.java', 50), at('B.java', 9)]);
  });
});

describe('where Back steps from', () => {
  it('is the settled caret when nothing is in flight', () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 33));
    expect(flow.here).toEqual(at('A.java', 33));
  });

  /** A second Back pressed before the first landed must step on from where the first was going. */
  it('is the destination of a navigation still in flight', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 33));
    const run = flow.go(at('B.java', 8), { record: false });
    expect(flow.here).toEqual(at('B.java', 8));
    w.arrive('B.java');
    await run;
    expect(flow.here).toEqual(at('B.java', 8));
  });

  it('reset forgets it', () => {
    const w = world({ activeFile: null });
    const flow = new NavFlow(w.host);
    flow.onCaret(at('Old.java', 3));
    flow.reset();
    flow.onCaret(at('New.java', 1));
    // No origin from the previous project.
    expect(w.origins).toEqual([]);
  });
});

describe('the scroll actually happening', () => {
  it('asks again until the line is really on screen', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], blindFor: 3 });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.scrolls.length).toBe(4);
    expect(w.scrolls.every((s) => s.line === 71)).toBe(true);
    expect(w.arrivals).toEqual([at('A.java', 71)]);
  });

  it('stops asking, and still records the landing', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], blindFor: 10_000 });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.scrolls.length).toBe(REVEAL_ATTEMPTS + 1);
    expect(w.arrivals).toEqual([at('A.java', 71)]);
    expect(flow.inFlight).toBe(false);
  });

  it('a newer navigation cuts the retries short', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], blindFor: 10_000 });
    const flow = new NavFlow(w.host);
    const first = flow.go(at('A.java', 71), { record: true });
    const second = flow.go(at('A.java', 5), { record: true });
    expect(await first).toBe('superseded');
    await second;
    expect(w.arrivals).toEqual([at('A.java', 5)]);
  });
});

describe('a caret something else moves back', () => {
  it('is put back, until it stays', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    let stubborn = 2;
    const scrollTo = w.host.scrollTo;
    w.host.scrollTo = (line, col) => {
      scrollTo(line, col);
      if (stubborn-- > 0) w.setCaret(at('A.java', 1));
    };
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.caret).toEqual(at('A.java', 71));
    expect(w.arrivals).toEqual([at('A.java', 71)]);
  });

  it('does not fight a line the file does not have', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    w.host.scrollTo = (line, col) => {
      const clamped = Math.min(line, 40);
      w.scrolls.push({ line: clamped, col });
      w.setCaret(at('A.java', clamped, col));
    };
    await flow.go(at('A.java', 900), { record: true });
    expect(w.scrolls.length).toBe(1);
    expect(w.arrivals).toEqual([at('A.java', 40)]);
  });
});

describe('two navigations at once', () => {
  it('the older one stops touching anything the moment a newer one starts', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const first = flow.go(at('B.java', 200), { record: true });
    const second = flow.go(at('C.java', 5), { record: true });
    w.arrive('C.java');
    w.arrive('B.java');
    expect(await first).toBe('superseded');
    expect(await second).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 5, col: 1 }]);
    expect(w.arrivals).toEqual([at('C.java', 5)]);
  });

  it('a superseded navigation does not declare the current one finished', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const first = flow.go(at('B.java', 200), { record: true });
    const second = flow.go(at('C.java', 5), { record: true });
    await first;
    expect(flow.inFlight).toBe(true);
    w.arrive('C.java');
    await second;
    expect(flow.inFlight).toBe(false);
  });
});

describe('holding a landing while the editor finishes decorating', () => {
  const flush = async (n = 80) => {
    for (let i = 0; i < n; i += 1) await Promise.resolve();
  };

  it('puts the line back when something above it moves it', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 40), { record: true })).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 40, col: 1 }]);
    w.drift(HOLD_TOLERANCE + 30);
    await flush();
    expect(w.reveals).toContain(40);
    expect(w.scrolls).toEqual([{ line: 40, col: 1 }]);
  });

  it('ignores movement too small to notice, so the view never twitches', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });
    w.drift(HOLD_TOLERANCE - 1);
    await flush();
    expect(w.reveals).toEqual([]);
  });

  it('stops the moment the reader goes somewhere else', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });
    w.move(flow, { file: 'A.java', line: 7, col: 3 });
    w.drift(HOLD_TOLERANCE + 200);
    await flush();
    expect(w.reveals).toEqual([]);
  });

  it('stops when a newer navigation takes the view', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });
    await flow.go(at('B.java', 12), { record: true });
    w.drift(HOLD_TOLERANCE + 200);
    await flush();
    expect(w.reveals).not.toContain(40);
    expect(w.scrolls.at(-1)).toEqual({ line: 12, col: 1 });
  });

  it('waits for a viewport instead of giving up when there is none yet', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], unmeasuredFor: 6, blindFor: 40 });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 400), { record: true });
    expect(w.caret).toEqual(at('A.java', 400));
    await flush();
    expect(w.reveals).toContain(400);
  });

  it('puts the line back when the view leaves it behind', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });
    expect(w.reveals).toEqual([]);
    w.loseSight();
    await flush();
    expect(w.reveals).toContain(40);
  });

  it('stops watching as soon as the landing is settled', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const before = w.settles;
    await flow.go(at('A.java', 40), { record: true });
    await flush(HOLD_ATTEMPTS * 2);
    const watched = w.settles - before;
    expect(watched).toBeLessThan(HOLD_ATTEMPTS);
    expect(watched).toBeGreaterThanOrEqual(HOLD_SETTLED);
  });

  it('does not delay the navigation it is watching', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });
    expect(w.caret).toEqual(at('A.java', 40));
    expect(w.scrolls).toEqual([{ line: 40, col: 1 }]);
  });

  it('holds nothing in a host with no viewport to measure', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow({ ...w.host, lineOffset: undefined });
    await flow.go(at('A.java', 40), { record: true });
    w.drift(HOLD_TOLERANCE + 200);
    await flush();
    expect(w.reveals).toEqual([]);
  });

  it('holds nothing in a host that can measure but not reveal', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow({ ...w.host, reveal: undefined });
    await flow.go(at('A.java', 40), { record: true });
    w.drift(HOLD_TOLERANCE + 200);
    await flush();
    expect(w.reveals).toEqual([]);
    expect(w.scrolls).toEqual([{ line: 40, col: 1 }]);
  });
});

describe('going to a byte offset', () => {
  const A = 'aaa\nbbbb\ncc\ndddd';
  const B = 'x\ny\nz\nlonger line\nq';

  it('resolves the offset against the destination, never against the file being left', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], texts: { 'A.java': A, 'B.java': B } });
    const flow = new NavFlow(w.host);
    const going = flow.goToOffset('B.java', 10);
    expect(w.active).toBe('B.java');
    w.arrive('B.java');
    expect(await going).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 4, col: 5 }]);
    expect(w.arrivals).toEqual([{ file: 'B.java', line: 4, col: 5 }]);
  });

  it('records where it started', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'], texts: { 'A.java': A, 'B.java': B } });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 3));
    await flow.goToOffset('B.java', 10);
    expect(w.origins).toEqual([at('A.java', 3)]);
  });

  it('waits for the buffer rather than resolving against nothing', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], texts: { 'A.java': A, 'B.java': B } });
    const flow = new NavFlow(w.host);
    const going = flow.goToOffset('B.java', 2);
    expect(w.scrolls).toEqual([]);
    w.arrive('B.java');
    await going;
    expect(w.scrolls).toEqual([{ line: 2, col: 1 }]);
  });

  it('gives up rather than guessing when the buffer never arrives', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], texts: { 'A.java': A } });
    const flow = new NavFlow(w.host);
    expect(await flow.goToOffset('B.java', 2)).toBe('unavailable');
    expect(w.scrolls).toEqual([]);
    expect(w.arrivals).toEqual([]);
  });

  it('is superseded by a newer navigation like any other', async () => {
    const w = world({
      activeFile: 'A.java',
      readyFiles: ['A.java'],
      texts: { 'A.java': A, 'B.java': B, 'C.java': B },
    });
    const flow = new NavFlow(w.host);
    const first = flow.goToOffset('B.java', 10);
    const second = flow.go(at('C.java', 2), { record: true });
    w.arrive('C.java');
    w.arrive('B.java');
    expect(await first).toBe('superseded');
    expect(await second).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 2, col: 1 }]);
  });

  it('holds the landing afterwards, exactly as a line jump does', async () => {
    const w = world({ activeFile: 'B.java', readyFiles: ['B.java'], texts: { 'B.java': B } });
    const flow = new NavFlow(w.host);
    await flow.goToOffset('B.java', 10);
    w.drift(HOLD_TOLERANCE + 30);
    for (let i = 0; i < 80; i += 1) await Promise.resolve();
    expect(w.reveals).toContain(4);
  });
});
