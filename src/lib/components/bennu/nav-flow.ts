/**
 * The editor's navigation, as one operation instead of a conversation between events.
 *
 * ## Why this exists at all
 *
 * A jump is several things happening: a tab becomes active, its text is fetched, an editor mounts,
 * a layout happens, a scroll lands. The history has to record ONE stop out of that, and the
 * previous design tried to work out afterwards which of the caret events belonged to the
 * navigation — with a pending-jump budget, a "provisional" file, an "opening for" file, a nonce
 * relay and a remembered-caret claim, five pieces of state each inferring intent from a stream
 * that does not carry it. Every fix added a sixth. The failures all had the same shape and none of
 * them named it: Back landed on line 1, or the caret was right and the viewport was not, or a
 * forward branch vanished.
 *
 * Here a navigation has an **identity** and owns its own transit. Nothing has to be inferred:
 * while one is in flight the caret is in transit by definition, and when it finishes it knows
 * exactly where it put the caret, because it is the thing that put it there.
 *
 * ## The two rules
 *
 * 1. **Wait for a document that can hold the line.** A tab becomes active before its text
 *    arrives, and scrolling to line 400 of an empty buffer lands on line 1 — indistinguishable
 *    afterwards from a jump to line 1 that worked. So the scroll happens when, and only when, the
 *    host says the buffer is there.
 * 2. **A stop is recorded after the landing, never before.** The ring gets the place the caret is
 *    actually at.
 *
 * Everything the editor does with this is in {@link NavHost}: the flow performs no DOM work, knows
 * nothing about CodeMirror or Svelte, and is therefore testable as what it is — a small state
 * machine with an async body.
 */

/** A place in the project: a file and a 1-based line/column. */
export interface NavPlace {
  file: string;
  line: number;
  col: number;
}

/** The history ring, as much of it as a navigation touches. */
export interface NavRing {
  /** Record a new stop, truncating any forward branch. */
  push(place: NavPlace): void;
  /** Keep the stop we are sitting on pointing at where the caret is. Adds nothing. */
  refine(place: NavPlace): void;
}

/** Everything a navigation needs from the editor it runs in. */
export interface NavHost {
  /** The file the editor is showing, or `null` when it is showing none. */
  activeFile(): string | null;
  /** Make `file` the active tab. Resolves once it is active — its **text may not have arrived**. */
  openFile(file: string): Promise<void>;
  /**
   * Whether a line number means something in `file` right now: the **mounted editor is the one
   * for this file** and its buffer is loaded. The one question the old design could not ask, and
   * the reason it scrolled empty documents — and, worse, the outgoing file's editor, which is
   * still mounted for a moment after the tab has changed.
   */
  ready(file: string): boolean;
  /** Move the caret to `line`/`col` and reveal it. Called only when {@link ready} is true. */
  scrollTo(line: number, col: number): void;
  /**
   * Whether `line` is **actually visible** right now.
   *
   * The second thing a scroll request is not: an editor that has just mounted has not been
   * measured, so its scroll container has no height yet and `scrollIntoView` computes a
   * destination against a viewport that does not exist. The caret moves — it is a document
   * position — and the view stays where it was. That is "the cursor is in the right place and it
   * did not scroll", and nothing about the caret can detect it, which is why this asks about the
   * viewport instead.
   *
   * A host that cannot tell (no layout at all) must answer `false`, not `true`: unknown is the
   * state this exists to wait out.
   */
  inView(line: number): boolean;
  /** Yield until the world may have changed — one frame, in the editor; immediate, in a test. */
  settle(): Promise<void>;
  /** Where the caret ended up. Used to record the truth rather than the request. */
  caret(): NavPlace | null;
  ring: NavRing;
}

/** How many `settle()` turns a navigation waits for a buffer before giving up. */
export const READY_ATTEMPTS = 120;

/**
 * How many times a landing is re-asked for before the flow accepts it cannot prove it.
 *
 * Small on purpose: this waits for a **measure**, which is one or two frames, not for I/O. A
 * larger budget would not fix anything — it would only delay the moment the flow stops arguing
 * with a viewport that is never going to exist (a tab in the background, a collapsed panel).
 */
export const REVEAL_ATTEMPTS = 12;

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
  /** The caret's last known place — what tells an arrival in a new file from movement inside one. */
  private lastPlace: NavPlace | null = null;

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
   * Go to `dest`.
   *
   * `record` says whether this is a **new** stop (a go-to, a panel jump, a usage) or a move
   * **within** the ring (Back / Forward), which has already positioned itself and must not push —
   * pushing there is what truncated the forward branch and made Forward stop working.
   */
  async go(dest: NavPlace, opts: { record: boolean }): Promise<NavOutcome> {
    const id = ++this.seq;
    // **The checkpoint, taken as an act rather than hoped for.**
    //
    // The stop you come back to is where you were when you jumped, and the ring keeps its current
    // entry pointing at the caret — but only as caret events arrive. Scrolling with the wheel
    // produces none; nor does anything that moves the view without moving the caret. So the entry
    // could be an hour old, and Back went there instead of to the place you had just left.
    //
    // Recorded here, at the one moment that is certainly still "before": a navigation is the only
    // thing that makes a previous position worth remembering, so it is the navigation's job to
    // remember it.
    const origin = this.host.caret();
    if (origin && opts.record) this.host.ring.refine(origin);
    this.active = { id, dest, record: opts.record };
    try {
      if (dest.file !== this.host.activeFile()) {
        await this.host.openFile(dest.file);
        if (this.active?.id !== id) return 'superseded';
      }
      // Rule 1. A tab can be active with nothing in it.
      for (let i = 0; !this.host.ready(dest.file); i++) {
        if (i >= READY_ATTEMPTS) return 'unavailable';
        await this.host.settle();
        if (this.active?.id !== id) return 'superseded';
      }
      this.host.scrollTo(dest.line, dest.col);
      // Where that first scroll actually put the caret. Not `dest.line`: a line past the end of a
      // shorter file clamps, and comparing against what was ASKED for would then never agree and
      // burn the whole retry budget on a navigation that had already succeeded.
      const target = this.host.caret()?.line ?? dest.line;
      // Rule 3. **A request is not a result**, and it is not one twice over.
      //
      // The scroll: an editor that has just been created has not been measured, so
      // `scrollIntoView` computes against a viewport with no height and moves nothing.
      //
      // The caret: a selection dispatched afterwards by anything else — a view state restored on
      // mount, a remembered caret placed a tick late — silently wins, and the reader is left where
      // they were with the view somewhere else.
      //
      // Neither is visible from the other, so both are checked, and the ask is repeated until they
      // agree. Bounded and best-effort: if the viewport never appears (a background tab, a
      // collapsed panel) the caret is still in the right place and the stop is worth recording.
      for (let i = 0; !this.arrived(target); i++) {
        if (i >= REVEAL_ATTEMPTS) break;
        await this.host.settle();
        if (this.active?.id !== id) return 'superseded';
        this.host.scrollTo(dest.line, dest.col);
      }
      // Rule 2. Record where the caret IS, which is not always what was asked for.
      const landed = this.host.caret() ?? dest;
      this.lastPlace = landed;
      if (opts.record) this.host.ring.push(landed);
      else this.host.ring.refine(landed);
      return 'landed';
    } finally {
      // Only the navigation that is still current clears the flag — an older one finishing late
      // must not declare the newer one's transit over.
      if (this.active?.id === id) this.active = null;
    }
  }

  /** Both halves of a landing: the caret is on the line, and the line can be seen. */
  private arrived(line: number): boolean {
    return this.host.caret()?.line === line && this.host.inView(line);
  }

  /**
   * A caret event.
   *
   * While a navigation is in flight this records where the caret is and tells the ring nothing:
   * every caret event between the request and the landing is transit, by construction rather than
   * by inference. Otherwise a caret in a **different file** is an arrival somebody caused (a tab
   * switch, a file opened from the tree) and is a stop; a caret in the same file is reading, and
   * only keeps the current stop up to date.
   */
  onCaret(place: NavPlace): void {
    const arriving = !this.lastPlace || this.lastPlace.file !== place.file;
    this.lastPlace = place;
    if (this.active) return;
    if (arriving) this.host.ring.push(place);
    else this.host.ring.refine(place);
  }

  /** Forget where the caret was — the project changed and the paths in here belong to the old one. */
  reset(): void {
    this.active = null;
    this.lastPlace = null;
  }
}
