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
  /**
   * Where `line` sits in the viewport right now, in pixels from its top edge — or `null` when it
   * cannot be measured.
   *
   * The third thing a scroll request is not: **permanent**. For about half a second after a file
   * opens the editor keeps adding and removing height *above* the caret — usage counts drawn over
   * members, inlay hints, folded regions collapsing — and every one of those moves the line that
   * was just landed on, without anyone scrolling. The reader sees the right place, looks away for
   * a moment, and is somewhere else.
   *
   * `inView` cannot see it: a line dragged a third of a screen is still in view. Only its
   * position can, which is why this is a number and not a boolean.
   *
   * Optional: a host with no layout to measure (a test, a headless embed) simply does not hold.
   */
  lineOffset?(line: number): number | null;
  /**
   * Bring `line` back into view **without moving the caret and without taking the focus**.
   *
   * The other half of the hold, and it has to be a different verb from {@link scrollTo}: that one
   * ends by focusing, because a jump from a panel is meant to put you *in* the editor. A
   * correction half a second later is not a jump — by then the reader may be typing in the search
   * field or picking a file in the tree, and pulling the focus back would be the editor
   * interrupting them to fix its own drawing.
   *
   * Paired with {@link lineOffset}: a host that has neither simply does not hold.
   */
  reveal?(line: number): void;
  /**
   * Where a **byte offset** lands, as a line and column — or `null` when it cannot be answered.
   *
   * A byte offset is meaningless against the wrong text, and for a cross-file jump the wrong text
   * is what the editor is holding for a moment: the tab has changed and the view that is mounted
   * is still the file being left. So this is asked **after** {@link ready}, never before, which is
   * the whole reason it is a host call and not something the caller resolves and passes in.
   */
  lineOfByteOffset?(file: string, offset: number): { line: number; col: number } | null;
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

/**
 * How many `settle()` turns a landing is held in place for afterwards.
 *
 * Sized to two things and the longer one wins. The editor's own late decoration passes are timers
 * of up to ~600ms — about forty frames. But the hold also has to outlast a viewport that does not
 * exist yet: a large file in a container still being laid out can go a good deal longer than that
 * before there is anything to measure, and giving up first is what leaves the caret on the right
 * line with the view somewhere else.
 *
 * Each turn costs one measurement, and the hold ends the moment the reader does anything at all —
 * so the cost of it being generous is paid only by somebody who jumped somewhere and is looking at
 * it.
 */
export const HOLD_ATTEMPTS = 150;

/**
 * How many consecutive settled frames end the hold early.
 *
 * The ordinary landing is settled on the first frame and stays settled, and watching it for
 * another two seconds proves nothing — so the watch stops once the line has been **in view and
 * still** for this long. It cannot be much shorter: the editor's own late decoration passes are
 * timers of up to ~600ms, and stopping before those have run is stopping just before the thing
 * the hold is for.
 *
 * So the common case costs about three quarters of a second of one measurement per frame, and the
 * full budget above exists only for the case that actually needs it — a viewport that has not
 * appeared yet.
 */
export const HOLD_SETTLED = 45;

/**
 * How far the landed line may drift before it is put back, in pixels.
 *
 * Not zero: sub-pixel layout differences and a scrollbar appearing are movement nobody perceives,
 * and correcting those would make the view twitch. One line's worth is the threshold at which a
 * reader notices their place has changed.
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
    const outcome = await this.travel(id, dest, opts);
    // Deliberately NOT awaited: the caller asked to go somewhere and it has gone. The hold is
    // about the seconds *after*, and a caller that waited for it would be waiting for nothing it
    // is going to do anything with.
    if (outcome === 'landed') void this.hold(id, dest);
    return outcome;
  }

  /**
   * Go to a **byte offset** in `file` — what a backend-sourced destination is.
   *
   * The offsets every framework answer carries (a `<form>` tag, the `<action>` element a JSP
   * reference resolves to, a diagnostic's span) are counted in the file's bytes. Turning one into
   * a line needs the file's text, and this is the one place that knows when that text is there.
   *
   * Its own entry point rather than a caller mapping the offset first, because a caller cannot:
   * at the moment a cross-file jump is requested the editor still holds the file being left, so
   * anything resolved then is resolved against the wrong document. That was a real defect — the
   * line relay waited for the buffer and this path did not, so the two behaved differently for
   * reasons nobody could see from either call site.
   */
  async goToOffset(file: string, offset: number): Promise<NavOutcome> {
    const resolve = this.host.lineOfByteOffset?.bind(this.host);
    if (!resolve) return 'unavailable';
    const id = ++this.seq;
    const origin = this.host.caret();
    if (origin) this.host.ring.refine(origin);
    // Claimed for the whole resolve, so the caret events a mounting editor produces are transit
    // rather than stops — exactly as they are for a jump whose line was known in advance.
    this.active = { id, dest: { file, line: 1, col: 1 }, record: true };
    let landed: NavPlace | null = null;
    let outcome: NavOutcome;
    try {
      outcome = await (async (): Promise<NavOutcome> => {
        if (file !== this.host.activeFile()) {
          await this.host.openFile(file);
          if (this.active?.id !== id) return 'superseded';
        }
        // Rule 1, and here it is load-bearing twice over: the offset cannot even be *read*
        // against a buffer that has not arrived.
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

  private async travel(
    id: number,
    dest: NavPlace,
    opts: { record: boolean },
  ): Promise<NavOutcome> {
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
      return await this.land(id, dest, opts);
    } finally {
      // Only the navigation that is still current clears the flag — an older one finishing late
      // must not declare the newer one's transit over.
      if (this.active?.id === id) this.active = null;
    }
  }

  /**
   * The landing itself: scroll, make sure it took, and record where the caret ended up.
   *
   * Shared by the two entry points because everything from here down is identical — what differs
   * is only how the destination line was arrived at, which is the whole of what a byte offset
   * changes.
   */
  private async land(id: number, dest: NavPlace, opts: { record: boolean }): Promise<NavOutcome> {
    {
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
    }
  }

  /**
   * Keep the landing where it was put, while the editor finishes decorating around it.
   *
   * Rule 4, and the last one a jump needs: **arriving is not staying**. Everything the editor
   * draws asynchronously changes the height of what is above the caret — the usage count over
   * every member, an inlay hint, a fold that resolves — and each of those slides the line that was
   * just landed on. It reads as the editor drifting off on its own a moment after a go-to, which
   * is a thing nobody reports precisely because nothing did it.
   *
   * > [!CAUTION] This is the BACKSTOP, and a backstop must never be the cure
   * > A watch that puts the view back for a while and then stops does not fix drift — it moves
   * > it. The reported failure became "it scrolls away after two and a half seconds", which is
   * > this budget to the frame, and is strictly harder to recognise than scrolling away at once.
   * >
   * > Drift is cured where it is caused, at the dispatch that changes the height: `CodeEditor`'s
   * > `keepingPlace` holds the reader's line across a lens or inlay-hint push. What is left here
   * > is the case an anchor cannot answer, because it is not drift at all — a viewport that did
   * > not exist when the landing scrolled, where there is nothing measured to hold on to. If this
   * > ever starts *correcting* rather than waiting, something upstream has lost its anchor.
   *
   * Two rules keep this from becoming a nuisance of its own:
   *
   * 1. **the reader always wins.** The moment the caret leaves the destination line — a click, an
   *    arrow key, a keystroke — the hold is over. It never fights somebody who is now reading
   *    somewhere else;
   * 2. **a newer navigation ends it too.** An older hold putting the view back would undo the jump
   *    that replaced it.
   *
   * Runs after the stop is recorded and after `active` is cleared, so the corrections it makes are
   * ordinary caret events rather than transit — there is nothing left in flight to be in transit.
   */
  private async hold(id: number, dest: NavPlace): Promise<void> {
    const measure = this.host.lineOffset?.bind(this.host);
    const reveal = this.host.reveal?.bind(this.host);
    // One capability in two methods: a host that cannot measure cannot tell it drifted, and one
    // that cannot reveal could only correct by scrolling, which takes the focus.
    if (!measure || !reveal) return;
    // Where the line was when it was last put somewhere. `null` means "no baseline yet" — before
    // there is a viewport to measure, and again after every correction, because a correction moved
    // it deliberately and the position it moved to is the new resting place.
    let settled: number | null = null;
    /** Consecutive frames the line has been in view and still — what ends this early. */
    let quiet = 0;
    for (let i = 0; i < HOLD_ATTEMPTS; i++) {
      await this.host.settle();
      if (this.seq !== id) return;
      if (this.host.caret()?.line !== dest.line) return;
      const now = measure(dest.line);
      // **No viewport yet**, and this is the case that must not end the hold — it is the one the
      // landing itself already gave up in. An editor created a frame ago has not been measured, so
      // `scrollIntoView` computed against a container with no height and moved nothing: the caret
      // is on the right line, being a document position, and the view is wherever it was. Waiting
      // is the only thing that helps, and returning here is what left it there.
      if (now == null) continue;
      // Not on screen at all. Nothing to compare against — put it there, and take the baseline
      // next time round.
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
