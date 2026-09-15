/**
 * The editor's navigation, as one operation instead of a conversation between events.
 *
 * ## Why this exists at all
 *
 * A jump is several things happening: a tab becomes active, its text is fetched, an editor mounts,
 * a layout happens, a scroll lands. Every one of those produces caret events, and none of them is
 * a place the reader chose. Here a navigation has an **identity** and owns its own transit: while
 * one is in flight the caret is in transit by definition, and when it finishes it knows exactly
 * where it put the caret, because it is the thing that put it there.
 *
 * ## What the history is told
 *
 * The history ({@link NavRing}, `nav-ring.ts`) is IntelliJ's: it stores the places navigations
 * **left**, never the present. So the flow tells it two things only:
 *
 * - **`leave(origin)`** when a navigation starts, with the last place the reader was *settled* at —
 *   the caret as it was before anything moved, attributed to the document it belongs to;
 * - **`arrive(place)`** when a recorded navigation lands, for Recent Locations.
 *
 * Back/Forward are navigations with `record: false`: they tell the history nothing, because the
 * history has already moved.
 *
 * ### The panel jump
 *
 * Almost every panel navigates as `openFile(f)` followed by a go-to. The open is an arrival in
 * another file, so it already records the origin; the go-to that follows lands in the file just
 * arrived in, and recording *its* origin would add the mount caret (line 1, or a restored view
 * state) as a stop nobody chose — which is exactly where Back used to take you. So an arrival that
 * the **user has not touched yet** is not an origin for a navigation into the same file.
 *
 * ## The rules
 *
 * 1. **Wait for a document that can hold the line.** A tab becomes active before its text
 *    arrives, and scrolling to line 400 of an empty buffer lands on line 1.
 * 2. **Record the landing, not the request** — a line past the end clamps.
 * 3. **A request is not a result** — the scroll and the caret are re-checked until both agree.
 * 4. **Arriving is not staying** — see {@link NavFlow.hold}.
 *
 * Everything the editor does with this is in {@link NavHost}: the flow performs no DOM work, knows
 * nothing about CodeMirror or Svelte, and is therefore testable as what it is — a small state
 * machine with an async body.
 */

import { defaultSameFile, type NavPlace } from './nav-ring';

export type { NavPlace };

/** The history, as much of it as a navigation touches. */
export interface NavRing {
  /** A navigation is leaving `origin`: record it as a stop (truncating Forward). */
  leave(origin: NavPlace): void;
  /** A recorded navigation landed at `place`. Not a stop — a visit, for Recent Locations. */
  arrive(place: NavPlace): void;
}

/** Everything a navigation needs from the editor it runs in. */
export interface NavHost {
  /** The file the editor is showing, or `null` when it is showing none. */
  activeFile(): string | null;
  /** Make `file` the active tab. Resolves once it is active — its **text may not have arrived**. */
  openFile(file: string): Promise<void>;
  /**
   * Whether a line number means something in `file` right now: the **mounted editor is the one
   * for this file** and its buffer is loaded. Scrolling before that scrolls an empty document, or
   * the outgoing file's editor, which is still mounted for a moment after the tab has changed.
   */
  ready(file: string): boolean;
  /** Move the caret to `line`/`col` and reveal it. Called only when {@link ready} is true. */
  scrollTo(line: number, col: number): void;
  /**
   * Whether `line` is **actually visible** right now.
   *
   * An editor that has just mounted has not been measured, so `scrollIntoView` computes against a
   * viewport that does not exist: the caret moves and the view does not. A host that cannot tell
   * must answer `false`, not `true`: unknown is the state this exists to wait out.
   */
  inView(line: number): boolean;
  /**
   * Where `line` sits in the viewport right now, in pixels from its top edge — or `null` when it
   * cannot be measured. For about half a second after a file opens the editor keeps adding height
   * above the caret (usage counts, inlay hints, folds) and each one slides the landed line.
   *
   * Optional: a host with no layout to measure simply does not hold.
   */
  lineOffset?(line: number): number | null;
  /**
   * Bring `line` back into view **without moving the caret and without taking the focus** — a
   * correction half a second after a jump must not pull the focus back from wherever the reader
   * went. Paired with {@link lineOffset}: a host that has neither simply does not hold.
   */
  reveal?(line: number): void;
  /**
   * Where a **byte offset** lands, as a line and column — or `null` when it cannot be answered.
   * Asked **after** {@link ready}, never before: for a cross-file jump the editor is still holding
   * the file being left for a moment, and an offset read against it belongs to the wrong document.
   */
  lineOfByteOffset?(file: string, offset: number): { line: number; col: number } | null;
  /** Yield until the world may have changed — one frame, in the editor; immediate, in a test. */
  settle(): Promise<void>;
  /** Where the caret is, attributed to the document it is in. Used to record the truth rather
   *  than the request. */
  caret(): NavPlace | null;
  /** File identity. Defaults to comparing with separators normalised. */
  sameFile?(a: string, b: string): boolean;
  ring: NavRing;
}

/** How many `settle()` turns a navigation waits for a buffer before giving up. */
export const READY_ATTEMPTS = 120;

/**
 * How many times a landing is re-asked for before the flow accepts it cannot prove it. Small on
 * purpose: this waits for a **measure**, which is one or two frames, not for I/O.
 */
export const REVEAL_ATTEMPTS = 12;

/**
 * How many `settle()` turns a landing is held in place for afterwards. Sized to outlast both the
 * editor's late decoration passes (~600ms) and a viewport still being laid out; the hold ends the
 * moment the reader does anything at all.
 */
export const HOLD_ATTEMPTS = 150;

/**
 * How many consecutive settled frames end the hold early — long enough to outlast the editor's own
 * late decoration timers, so the common case costs about three quarters of a second of one
 * measurement per frame.
 */
export const HOLD_SETTLED = 45;

/**
 * How far the landed line may drift before it is put back, in pixels. One line's worth is the
 * threshold at which a reader notices their place has changed; less would make the view twitch.
 */
export const HOLD_TOLERANCE = 12;

/** Why a navigation ended — returned so a caller can say something, and so tests can assert it. */
export type NavOutcome =
  /** The caret is at the destination. */
  | 'landed'
  /** A newer navigation started; this one stopped touching anything. */
  | 'superseded'
  /** The destination's buffer never arrived. Nothing was scrolled and no stop was recorded. */
  | 'unavailable';

/**
 * The editor's one navigator. A single instance per editor; one navigation at a time, and a newer
 * one supersedes an older rather than racing it.
 */
export class NavFlow {
  private seq = 0;
  private active: { id: number; dest: NavPlace; record: boolean } | null = null;
  /** The last place the reader was settled at — outside any transit, in the file it belongs to. */
  private settled: NavPlace | null = null;
  /** The reader arrived in `settled`'s file (a tab switch, a file opened by a panel) and has not
   *  touched it since. See "The panel jump" above. */
  private arrivedUntouched = false;

  constructor(private host: NavHost) {}

  /** Whether a navigation is in flight, i.e. whether the caret is in transit. */
  get inFlight(): boolean {
    return this.active !== null;
  }

  /** Where the navigation in flight is going, for a host that needs to claim a tab. */
  get destination(): NavPlace | null {
    return this.active?.dest ?? null;
  }

  /**
   * Where the reader is, for Back / Forward to step away from. While a navigation is in flight it
   * is that navigation's destination: a second Back pressed before the first landed steps on from
   * where the first was going, not from the place both have already left.
   */
  get here(): NavPlace | null {
    return this.active?.dest ?? this.settled ?? this.host.caret();
  }

  /**
   * Go to `dest`.
   *
   * `record` says whether this is a **new** navigation (a go-to, a panel jump, a usage), which
   * records where it started, or a move **within** the history (Back / Forward), which records
   * nothing — the history has already moved.
   */
  async go(dest: NavPlace, opts: { record: boolean }): Promise<NavOutcome> {
    const id = ++this.seq;
    if (opts.record) this.recordOrigin(dest.file);
    const outcome = await this.travel(id, dest, opts);
    // Deliberately NOT awaited: the hold is about the seconds *after* the landing.
    if (outcome === 'landed') void this.hold(id, dest);
    return outcome;
  }

  /**
   * Go to a **byte offset** in `file` — what a backend-sourced destination is. Its own entry
   * point because the offset can only be read against the destination's text, which the editor
   * holds only once {@link NavHost.ready} says so.
   */
  async goToOffset(file: string, offset: number): Promise<NavOutcome> {
    const resolve = this.host.lineOfByteOffset?.bind(this.host);
    if (!resolve) return 'unavailable';
    const id = ++this.seq;
    this.recordOrigin(file);
    // Claimed for the whole resolve, so the caret events a mounting editor produces are transit.
    this.active = { id, dest: { file, line: 1, col: 1 }, record: true };
    let landed: NavPlace | null = null;
    let outcome: NavOutcome;
    try {
      outcome = await (async (): Promise<NavOutcome> => {
        if (!this.same(file, this.host.activeFile())) {
          await this.host.openFile(file);
          if (this.active?.id !== id) return 'superseded';
        }
        for (let i = 0; !this.host.ready(file); i++) {
          if (i >= READY_ATTEMPTS) return 'unavailable';
          await this.host.settle();
          if (this.active?.id !== id) return 'superseded';
        }
        const place = resolve(file, offset);
        if (!place) return 'unavailable';
        landed = { file, ...place };
        this.active = { id, dest: landed, record: true };
        return await this.land(id, landed, { record: true });
      })();
    } finally {
      if (this.active?.id === id) this.active = null;
    }
    if (outcome === 'landed' && landed) void this.hold(id, landed);
    return outcome;
  }

  /**
   * Record where a navigation into `destFile` starts: the last settled place, attributed to its
   * own document.
   *
   * Not the live caret of whatever the host considers active: a panel jump has already switched
   * the tab by the time the go-to runs, and the caret the host holds at that moment is the OUTGOING
   * editor's line filed under the INCOMING file's path — a stop that exists in neither.
   */
  private recordOrigin(destFile: string): void {
    const origin = this.settled ?? this.host.caret();
    if (!origin) return;
    // The open that preceded this go-to already recorded the real origin — see "The panel jump".
    if (this.arrivedUntouched && this.same(origin.file, destFile)) return;
    this.host.ring.leave(origin);
  }

  private async travel(id: number, dest: NavPlace, opts: { record: boolean }): Promise<NavOutcome> {
    this.active = { id, dest, record: opts.record };
    try {
      if (!this.same(dest.file, this.host.activeFile())) {
        await this.host.openFile(dest.file);
        if (this.active?.id !== id) return 'superseded';
      }
      // Rule 1. A tab can be active with nothing in it.
      for (let i = 0; !this.host.ready(dest.file); i++) {
        if (i >= READY_ATTEMPTS) return 'unavailable';
        await this.host.settle();
        if (this.active?.id !== id) return 'superseded';
      }
      return await this.land(id, dest, opts);
    } finally {
      // Only the navigation that is still current clears the flag — an older one finishing late
      // must not declare the newer one's transit over.
      if (this.active?.id === id) this.active = null;
    }
  }

  /** The landing itself: scroll, make sure it took, and settle where the caret ended up. */
  private async land(id: number, dest: NavPlace, opts: { record: boolean }): Promise<NavOutcome> {
    this.host.scrollTo(dest.line, dest.col);
    // Where that first scroll actually put the caret — a line past the end of a shorter file
    // clamps, and comparing against what was ASKED for would burn the retry budget for nothing.
    const target = this.host.caret()?.line ?? dest.line;
    // Rule 3: the scroll may have hit an unmeasured viewport, and a selection dispatched afterwards
    // by anything else (a view state restored on mount) may have won. Bounded and best-effort.
    for (let i = 0; !this.arrived(target); i++) {
      if (i >= REVEAL_ATTEMPTS) break;
      await this.host.settle();
      if (this.active?.id !== id) return 'superseded';
      this.host.scrollTo(dest.line, dest.col);
    }
    // Rule 2. The reader is now where the caret IS, which is not always what was asked for.
    const landed = this.host.caret() ?? dest;
    this.settled = landed;
    this.arrivedUntouched = false;
    if (opts.record) this.host.ring.arrive(landed);
    return 'landed';
  }

  /**
   * Keep the landing where it was put, while the editor finishes decorating around it.
   *
   * > [!CAUTION] This is the BACKSTOP, and a backstop must never be the cure
   * > Drift is cured where it is caused, at the dispatch that changes the height: `CodeEditor`'s
   * > `keepingPlace` holds the reader's line across a lens or inlay-hint push. What is left here
   * > is a viewport that did not exist when the landing scrolled. If this ever starts
   * > *correcting* rather than waiting, something upstream has lost its anchor.
   *
   * The reader always wins (the moment the caret leaves the destination line the hold is over),
   * and a newer navigation ends it too.
   */
  private async hold(id: number, dest: NavPlace): Promise<void> {
    const measure = this.host.lineOffset?.bind(this.host);
    const reveal = this.host.reveal?.bind(this.host);
    if (!measure || !reveal) return;
    // `null` = no baseline yet: before there is a viewport, and after every correction.
    let settled: number | null = null;
    let quiet = 0;
    for (let i = 0; i < HOLD_ATTEMPTS; i++) {
      await this.host.settle();
      if (this.seq !== id) return;
      if (this.host.caret()?.line !== dest.line) return;
      const now = measure(dest.line);
      // No viewport yet — the case that must not end the hold; waiting is the only thing that helps.
      if (now == null) continue;
      if (!this.host.inView(dest.line)) {
        reveal(dest.line);
        settled = null;
        quiet = 0;
        continue;
      }
      if (settled == null) {
        settled = now;
        quiet = 0;
        continue;
      }
      if (Math.abs(now - settled) > HOLD_TOLERANCE) {
        reveal(dest.line);
        settled = null;
        quiet = 0;
        continue;
      }
      quiet += 1;
      if (quiet >= HOLD_SETTLED) return;
    }
  }

  /** Both halves of a landing: the caret is on the line, and the line can be seen. */
  private arrived(line: number): boolean {
    return this.host.caret()?.line === line && this.host.inView(line);
  }

  private same(a: string | null, b: string | null): boolean {
    if (a == null || b == null) return false;
    return (this.host.sameFile ?? defaultSameFile)(a, b);
  }

  /**
   * A caret event, attributed to the document it happened in. `user` is true when the reader
   * caused it (a click, a cursor key, typing) rather than a programmatic selection.
   *
   * While a navigation is in flight it is transit and changes nothing. Otherwise a caret in a
   * **different file** is an arrival somebody caused (a tab switch, a file opened from the tree or
   * by a panel): the place left is recorded, the new one is visited. A caret in the same file is
   * reading — it only moves where the reader is settled, which is what the next navigation will
   * record as its origin.
   */
  onCaret(place: NavPlace, user = false): void {
    if (this.active) return;
    const prev = this.settled;
    if (prev && !this.same(prev.file, place.file)) {
      this.host.ring.leave(prev);
      this.host.ring.arrive(place);
      this.arrivedUntouched = !user;
    } else if (user) {
      this.arrivedUntouched = false;
    }
    this.settled = place;
  }

  /** Forget where the reader was — the project changed and the paths belong to the old one. */
  reset(): void {
    this.active = null;
    this.settled = null;
    this.arrivedUntouched = false;
  }
}
