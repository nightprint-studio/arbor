/**
 * The navigation failures this file exists to prevent — each test is one of them, by name.
 *
 * They are not hypothetical: every one was reported, diagnosed and patched separately before the
 * flow became a single operation, and each patch produced the next failure. The point of testing
 * the flow rather than the component is that these are questions about ORDER, and order is exactly
 * what a component test buried in CodeMirror and Svelte cannot ask cleanly.
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
    /** How many measurements answer "there is no viewport yet" — an editor created and not laid
     *  out, which is the state a scroll request is lost in. */
    unmeasuredFor?: number;
  } = {},
) {
  const readyFiles = new Set(opts.readyFiles ?? []);
  const texts = opts.texts ?? {};
  // How many `inView` questions answer "I cannot see it yet" — a stand-in for an editor that has
  // been created but not measured, which is the state a scroll request is lost in.
  let blind = opts.blindFor ?? 0;
  let active: string | null = opts.activeFile ?? null;
  let caret: NavPlace | null = active ? { file: active, line: 1, col: 1 } : null;
  const pushes: NavPlace[] = [];
  const refines: NavPlace[] = [];
  const scrolls: { line: number; col: number }[] = [];
  /** The corrections the hold made — a reveal, never a scroll: a scroll would take the focus. */
  const reveals: number[] = [];
  let settles = 0;
  /** Where the landed line sits in the viewport. A scroll puts it back at the resting value; the
   *  decorations that arrive after a jump are what move it (see `drift`). */
  const RESTING = 100;
  let offset = RESTING;
  let unmeasured = opts.unmeasuredFor ?? 0;
  /** The lines the view has actually been brought to — what `inView` answers from. */
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
    // A byte offset means something only against the file's own text, so the fake keeps one text
    // per file and refuses to answer for a file that is not the one mounted — which is exactly
    // what the editor does, and the whole reason this is a host call.
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
      // Either way of being brought to a line counts: a scroll, and a reveal — which moves the
      // view without touching the caret and is what the hold corrects with.
      return visible.has(line) || scrolls.some((sc) => sc.line === line);
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
    reveals,
    get settles() { return settles; },
    get active() { return active; },
    get caret() { return caret; },
    arrive: (file: string) => readyFiles.add(file),
    /** What a usage count drawn over a member, or a fold resolving, does to the line below it. */
    drift: (px: number) => { offset = RESTING + px; },
    /** The view moved and the caret did not — a measurement correcting itself, a container
     *  resizing. The caret is a document position and stays exactly where it was. */
    loseSight: () => {
      visible.clear();
      scrolls.length = 0;
    },
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

/**
 * The failure this whole group is named for: **arriving is not staying.**
 *
 * The go-to lands on the right line, and a moment later the view is somewhere else — reported as
 * "it scrolls off on its own", which is exactly what it looks like and exactly what nobody did.
 * For about half a second after a file opens the editor keeps changing the height of what is
 * *above* the caret: the usage count drawn over every member, an inlay hint, a fold resolving.
 * Each one slides the line that was just landed on, without a scroll anywhere.
 */
describe('holding a landing while the editor finishes decorating', () => {
  /** Let the hold run: it is deliberately not awaited by `go`, so a test has to yield to it. */
  const flush = async (n = 80) => {
    for (let i = 0; i < n; i += 1) await Promise.resolve();
  };

  it('puts the line back when something above it moves it', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    expect(await flow.go(at('A.java', 40), { record: true })).toBe('landed');
    expect(w.scrolls).toEqual([{ line: 40, col: 1 }]);

    // The decorations land, and the line is no longer where it was put.
    w.drift(HOLD_TOLERANCE + 30);
    await flush();

    expect(w.reveals).toContain(40);
    // A REVEAL and not a scroll: a scroll ends by focusing the editor, and by now the reader may
    // be typing somewhere else entirely.
    expect(w.scrolls).toEqual([{ line: 40, col: 1 }]);
  });

  it('ignores movement too small to notice, so the view never twitches', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });

    // A scrollbar appearing, a sub-pixel reflow: real movement, and not movement anybody sees.
    w.drift(HOLD_TOLERANCE - 1);
    await flush();

    expect(w.reveals).toEqual([]);
  });

  /** The rule that keeps the hold from becoming a nuisance of its own. */
  it('stops the moment the reader goes somewhere else', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });

    // A click, an arrow key — the caret is no longer on the line the jump was about.
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

    // An older hold putting the view back would undo the jump that replaced it.
    expect(w.reveals).not.toContain(40);
    expect(w.scrolls.at(-1)).toEqual({ line: 12, col: 1 });
  });

  /**
   * **The one the reader actually reports**, and it is not drift: *the caret is on the right line
   * and the view is somewhere else.*
   *
   * An editor created a frame ago has not been measured, so `scrollIntoView` computed against a
   * container with no height and moved nothing. The caret went to the destination, being a
   * document position; the view stayed where it was. The landing's own retry budget is small on
   * purpose — it waits for a measure, not for I/O — so on a large file in a container still being
   * laid out it runs out first, and the hold used to give up in exactly the same state, because
   * "nothing to measure" was read as "nothing to do".
   */
  it('waits for a viewport instead of giving up when there is none yet', async () => {
    const w = world({
      activeFile: 'A.java',
      readyFiles: ['A.java'],
      // No layout for the whole landing and the first frames of the hold.
      unmeasuredFor: 6,
      // And while there is none, the line is not on screen.
      blindFor: 40,
    });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 400), { record: true });
    // The caret is right — that is what makes this so hard to see.
    expect(w.caret).toEqual(at('A.java', 400));

    await flush();

    // And once there is something to measure, the line is put on screen.
    expect(w.reveals).toContain(400);
  });

  /** The view moved and the caret did not: a measurement correcting itself, a panel resizing. */
  it('puts the line back when the view leaves it behind', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    await flow.go(at('A.java', 40), { record: true });
    expect(w.reveals).toEqual([]);

    w.loseSight();
    await flush();

    expect(w.reveals).toContain(40);
  });

  /**
   * The answer to "does a jump now cost two and a half seconds": **no.**
   *
   * The navigation resolves when the caret lands — the hold is never awaited — and the watch after
   * it stops as soon as the line has been still for long enough to have outlasted the editor's own
   * late passes. The full budget exists for the case that needs it and nothing else pays for it.
   */
  it('stops watching as soon as the landing is settled', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    const before = w.settles;
    await flow.go(at('A.java', 40), { record: true });
    await flush(HOLD_ATTEMPTS * 2);

    const watched = w.settles - before;
    expect(watched).toBeLessThan(HOLD_ATTEMPTS);
    // And it did watch: stopping on the first frame would prove nothing about the decorations
    // that arrive later.
    expect(watched).toBeGreaterThanOrEqual(HOLD_SETTLED);
  });

  it('does not delay the navigation it is watching', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const flow = new NavFlow(w.host);
    // The promise resolves with the caret on the line — before any of the watching happens.
    await flow.go(at('A.java', 40), { record: true });
    expect(w.caret).toEqual(at('A.java', 40));
    expect(w.scrolls).toEqual([{ line: 40, col: 1 }]);
  });

  it('holds nothing in a host with no viewport to measure', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'] });
    const blind: NavHost = { ...w.host, lineOffset: undefined };
    const flow = new NavFlow(blind);
    await flow.go(at('A.java', 40), { record: true });
    w.drift(HOLD_TOLERANCE + 200);
    await flush();
    expect(w.reveals).toEqual([]);
  });

  /** The pair is one capability: measuring without revealing could only correct by scrolling. */
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

/**
 * Going to a **byte offset**, which is what every backend-sourced destination is.
 *
 * The defect these are named for: the relay that did this used to map the offset after a single
 * flush, and a flush is not a buffer. On a cross-file jump the editor still holds the file being
 * *left*, so the offset resolved to a line in the wrong document — and the jump then landed on
 * that line, in the right file, looking for all the world like the editor had drifted.
 */
describe('going to a byte offset', () => {
  const A = 'aaa\nbbbb\ncc\ndddd';   // the file being left
  const B = 'x\ny\nz\nlonger line\nq'; // the destination

  it('resolves the offset against the destination, never against the file being left', async () => {
    const w = world({
      activeFile: 'A.java',
      readyFiles: ['A.java'],
      texts: { 'A.java': A, 'B.java': B },
    });
    const flow = new NavFlow(w.host);

    // Offset 10 is line 4 of B and line 2 of A. Both are real answers, which is what makes this
    // observable at all: a resolver reading the wrong document does not fail, it succeeds with a
    // line that belongs to another file.
    const going = flow.goToOffset('B.java', 10);
    // The tab is active before its text is: the state the old relay resolved in.
    expect(w.active).toBe('B.java');
    w.arrive('B.java');
    expect(await going).toBe('landed');

    expect(w.scrolls).toEqual([{ line: 4, col: 5 }]);
    expect(w.pushes).toEqual([{ file: 'B.java', line: 4, col: 5 }]);
  });

  it('waits for the buffer rather than resolving against nothing', async () => {
    const w = world({ activeFile: 'A.java', readyFiles: ['A.java'], texts: { 'A.java': A, 'B.java': B } });
    const flow = new NavFlow(w.host);
    const going = flow.goToOffset('B.java', 2);
    // Nothing has been asked of the view while the text is missing.
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
    expect(w.pushes).toEqual([]);
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
