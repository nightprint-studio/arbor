/**
 * The navigation failures this file exists to prevent — each test is one of them, by name.
 *
 * They are not hypothetical: every one was reported, diagnosed and patched separately before the
 * flow became a single operation, and each patch produced the next failure. The point of testing
 * the flow rather than the component is that these are questions about ORDER, and order is exactly
 * what a component test buried in CodeMirror and Svelte cannot ask cleanly.
 */

import { describe, expect, it } from 'vitest';
import { NavFlow, READY_ATTEMPTS, REVEAL_ATTEMPTS, type NavHost, type NavPlace } from './nav-flow';

/** A world the test drives by hand: nothing is timed, everything is stepped. */
function world(opts: { activeFile?: string | null; readyFiles?: string[]; blindFor?: number } = {}) {
  const readyFiles = new Set(opts.readyFiles ?? []);
  // How many `inView` questions answer "I cannot see it yet" — a stand-in for an editor that has
  // been created but not measured, which is the state a scroll request is lost in.
  let blind = opts.blindFor ?? 0;
  let active: string | null = opts.activeFile ?? null;
  let caret: NavPlace | null = active ? { file: active, line: 1, col: 1 } : null;
  const pushes: NavPlace[] = [];
  const refines: NavPlace[] = [];
  const scrolls: { line: number; col: number }[] = [];
  let settles = 0;

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
    },
    async settle() {
      settles += 1;
    },
    inView: (line) => {
      if (blind > 0) {
        blind -= 1;
        return false;
      }
      return scrolls.some((sc) => sc.line === line);
    },
    caret: () => caret,
    ring: {
      push: (p) => void pushes.push({ ...p }),
      refine: (p) => void refines.push({ ...p }),
    },
  };

  return {
    host,
    pushes,
    refines,
    scrolls,
    get settles() { return settles; },
    get active() { return active; },
    get caret() { return caret; },
    arrive: (file: string) => readyFiles.add(file),
    setCaret: (p: NavPlace) => { caret = p; },
    /** The caret moved, and the flow was told — the two halves of one real event. A test that
     *  only calls `onCaret` is describing a world where `caret()` lies. */
    move(flow: NavFlow, p: NavPlace) {
      caret = p;
      flow.onCaret(p);
    },
  };
}

const at = (file: string, line: number, col = 1): NavPlace => ({ file, line, col });

describe('a navigation inside the file already open', () => {
  it('scrolls once and records the stop', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 71, col: 1 }]);
    expect(w.pushes).toEqual([at('A.java', 71)]);
  });
});

describe('a navigation into a file whose text has not arrived', () => {
  /**
   * The one that produced "sometimes it does not scroll". A tab becomes active before its content
   * is fetched, and scrolling to line 400 of an empty document lands on line 1 — after which
   * nothing can tell that apart from a jump to line 1 that worked.
   */
  it('waits for a buffer that can hold the line instead of scrolling an empty one', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const run = flow.go(at('B.java', 200), { record: true });
    // Three turns with no buffer: nothing must have been scrolled.
    await Promise.resolve();
    expect(w.scrolls).toEqual([]);
    w.arrive('B.java');
    expect(await run).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 200, col: 1 }]);
    expect(w.pushes).toEqual([at('B.java', 200)]);
  });

  it('gives up rather than wedging the editor when the buffer never arrives', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('Gone.java', 10), { record: true })).toBe('unavailable');
    expect(w.scrolls).toEqual([]);
    expect(w.pushes).toEqual([]);
    expect(w.settles).toBe(READY_ATTEMPTS);
    // And the editor is not left believing a navigation is still running — which would swallow
    // every caret event from then on.
    expect(flow.inFlight).toBe(false);
  });
});

describe('what the ring is told', () => {
  /**
   * Back and Forward move the ring themselves; pushing the destination on top truncated the
   * forward branch, which is why Forward stopped working after any Back that crossed a file.
   */
  it('a Back refines the stop it moved to and never pushes', async () => {
    const w = world({ activeFile: 'B.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 71), { record: false })).toBe('landed');
    expect(w.pushes).toEqual([]);
    expect(w.refines).toEqual([at('A.java', 71)]);
  });

  /** The stop is where the caret ENDED UP, not what was asked for — line 900 of a 40-line file. */
  it('records the landing, not the request', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    w.host.scrollTo = (line, col) => {
      const clamped = Math.min(line, 40);
      w.scrolls.push({ line: clamped, col });
      w.setCaret(at('A.java', clamped, col));
    };
    await flow.go(at('A.java', 900), { record: true });
    expect(w.pushes).toEqual([at('A.java', 40)]);
  });
});

describe('caret events', () => {
  it('are transit while a navigation is in flight, and touch nothing', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const run = flow.go(at('B.java', 200), { record: true });
    // The navigation's own first act is to mark where it is leaving from — the checkpoint.
    expect(w.refines).toEqual([at('A.java', 1)]);
    // Everything after that is transit. The buffer landing produces a caret at the top of the new
    // file, and recording it is exactly how Back used to end up at line 1 of the class.
    flow.onCaret(at('B.java', 1));
    flow.onCaret(at('B.java', 1));
    expect(w.pushes).toEqual([]);
    expect(w.refines).toEqual([at('A.java', 1)]);
    w.arrive('B.java');
    await run;
    expect(w.pushes).toEqual([at('B.java', 200)]);
  });

  it('a caret in another file with nothing in flight is a stop', () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    // The first caret a session sees seeds the ring — there has to be something to come back TO
    // before the first jump, or the first Back has nowhere to go.
    flow.onCaret(at('A.java', 12));
    flow.onCaret(at('B.java', 3)); // a tab switch the user made
    expect(w.pushes).toEqual([at('A.java', 12), at('B.java', 3)]);
  });

  it('moving inside a file only keeps the current stop up to date', () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    flow.onCaret(at('A.java', 12));
    flow.onCaret(at('A.java', 13));
    flow.onCaret(at('A.java', 71));
    expect(w.pushes).toEqual([at('A.java', 12)]);
    expect(w.refines).toEqual([at('A.java', 13), at('A.java', 71)]);
  });

  /** The property that makes Back land where you left: the stop follows the caret, so by the time
   *  you jump away the entry below already says the right line. */
  it('leaves the entry for the file you are in saying where you actually are', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.move(flow, at('A.java', 1));
    w.move(flow, at('A.java', 71));
    await flow.go(at('B.java', 200), { record: true });
    expect(w.refines.at(-1)).toEqual(at('A.java', 71));
    expect(w.pushes.at(-1)).toEqual(at('B.java', 200));
  });

  /**
   * The checkpoint, taken rather than hoped for. The ring's current entry follows the caret, but
   * only as caret EVENTS arrive — scrolling with the wheel produces none. So the entry could be
   * an hour old, and Back went there instead of to the place just left.
   */
  it('marks where the jump started even when no caret event said so', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    // The caret is at line 71 — a Ctrl+click put it there — but the flow was never told.
    w.setCaret(at('A.java', 71));
    await flow.go(at('B.java', 200), { record: true });
    expect(w.refines).toEqual([at('A.java', 71)]);
  });

  /** A Back must NOT re-mark: the ring has already moved to the entry it is going to, and
   *  refining the one it is leaving would overwrite the place it came from. */
  it('a Back marks no checkpoint', async () => {
    const w = world({ activeFile: 'B.java', readyFiles: ['A.java', 'B.java'] });
    const flow = new NavFlow(w.host);
    w.setCaret(at('B.java', 200));
    await flow.go(at('A.java', 71), { record: false });
    expect(w.refines).toEqual([at('A.java', 71)]);
  });
});

describe('the scroll actually happening', () => {
  /**
   * The reported shape, and the one nothing about the caret can detect: a freshly mounted editor
   * has not been measured, so `scrollIntoView` runs against a viewport with no height and moves
   * nothing — while the caret, being a document position, arrives exactly where it was asked for.
   * "Cursor in the right place, view did not move."
   */
  it('asks again until the line is really on screen', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], blindFor: 3 });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.scrolls.length).toBe(4); // the first, then one per frame until it took
    expect(w.scrolls.every((s) => s.line === 71)).toBe(true);
    expect(w.pushes).toEqual([at('A.java', 71)]);
  });

  /** A viewport that never appears — a background tab, a collapsed panel — must not turn into a
   *  loop, and the stop is still worth recording: the caret IS in the right place. */
  it('stops asking, and still records the stop', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], blindFor: 10_000 });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.scrolls.length).toBe(REVEAL_ATTEMPTS + 1);
    expect(w.pushes).toEqual([at('A.java', 71)]);
    expect(flow.inFlight).toBe(false);
  });

  it('a newer navigation cuts the retries short', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] , blindFor: 10_000 });
    const flow = new NavFlow(w.host);
    const first = flow.go(at('A.java', 71), { record: true });
    const second = flow.go(at('A.java', 5), { record: true });
    expect(await first).toBe('superseded');
    await second;
    expect(w.pushes).toEqual([at('A.java', 5)]);
  });
});

describe('a caret something else moves back', () => {
  /**
   * The other half of "a request is not a result", and invisible from the viewport: a selection
   * dispatched after the navigation's own — a view state restored on mount, a remembered caret
   * placed a tick late — silently wins, and the reader is left where they were.
   */
  it('is put back, until it stays', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    // Something else drags the caret home on the first two frames after each scroll.
    let stubborn = 2;
    const scrollTo = w.host.scrollTo;
    w.host.scrollTo = (line, col) => {
      scrollTo(line, col);
      if (stubborn-- > 0) w.setCaret(at('A.java', 1));
    };
    expect(await flow.go(at('A.java', 71), { record: true })).toBe('landed');
    expect(w.caret).toEqual(at('A.java', 71));
    expect(w.pushes).toEqual([at('A.java', 71)]);
  });

  /** A destination past the end of a shorter file clamps, and the flow must recognise THAT as the
   *  landing rather than arguing with the editor for twelve frames. */
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
    expect(w.pushes).toEqual([at('A.java', 40)]);
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
    expect(w.pushes).toEqual([at('C.java', 5)]);
  });

  it('a superseded navigation does not declare the current one finished', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const first = flow.go(at('B.java', 200), { record: true });
    const second = flow.go(at('C.java', 5), { record: true });
    await first;
    // The second is still waiting for its buffer — the caret is still in transit.
    expect(flow.inFlight).toBe(true);
    w.arrive('C.java');
    await second;
    expect(flow.inFlight).toBe(false);
  });
});
