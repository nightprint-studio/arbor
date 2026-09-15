/**
 * Bennu's navigation history as pure data — IntelliJ's model, testable without an editor.
 *
 * ## The model
 *
 * Two stacks and a caret. **Back** holds the places navigations started from, **Forward** the
 * places Back left. The place you are at right now is not stored anywhere: it is the caret, and
 * the caller hands it in at the only two moments it matters — when a navigation leaves it, and
 * when Back/Forward step away from it.
 *
 * That is the whole point of the shape. The previous design kept one ring whose *current slot
 * followed the caret*, which meant every caret event could overwrite a stop: the mount caret of a
 * freshly opened tab, a view state restored a frame late, the outgoing editor's caret filed under
 * the incoming file's path. Each of those silently rewrote the place Back was going to take you,
 * and from the outside it read as Back going "wherever it wants". With nothing stored for the
 * present, there is nothing for a stray caret event to corrupt.
 *
 * ## Rules
 *
 * - A navigation **records its origin** ({@link NavHistory.leave}) and truncates Forward.
 * - Two consecutive origins on the **same line of the same file** are one stop.
 * - Back / Forward **never record**: they move the present onto the opposite stack.
 * - A stop equal to where you already are is skipped, so Back always visibly moves.
 * - Every stored line is **mapped through edits** ({@link NavHistory.applyEdits}), so typing
 *   above a stop does not make it point at the wrong line.
 * - A file that cannot be opened any more is **forgotten** ({@link NavHistory.forget}).
 */

/** A place in the project: a file and a 1-based line/column. */
export interface NavPlace {
  file: string;
  line: number;
  col: number;
}

/** A place, and why it is remembered — the popup shows edits differently from visits. */
export interface RecentPlace extends NavPlace {
  kind: 'visit' | 'edit';
  /** Monotonic, for ordering and for a stable keyed `{#each}`. */
  at: number;
}

/**
 * One document change, as the **lines** it touched — 1-based and inclusive, before (`A`) and after
 * (`B`) the change. An insertion inside one line is `fromA = toA = fromB = toB`; pressing Enter on
 * line 10 is `A = 10..10`, `B = 10..11`.
 */
export interface LineEdit {
  fromA: number;
  toA: number;
  fromB: number;
  toB: number;
}

/** Cap each stack so a long session can't grow it without bound. */
export const MAX_PLACES = 100;
/** How many recent places the popup offers. Beyond this it stops being a list you scan. */
export const MAX_RECENT = 40;
/** A burst of typing within this many lines is one edit place — Last Edit Location walks edits,
 *  not keystrokes. Only for edits: two navigation stops merge on the same line only. */
export const EDIT_MERGE_LINES = 5;

export type SameFile = (a: string, b: string) => boolean;

const slashes = (p: string) => p.replace(/\\/g, '/');
/** Default file identity: separators do not make two files different. */
export const defaultSameFile: SameFile = (a, b) => slashes(a) === slashes(b);

/**
 * Where `line` of the document before `edits` is afterwards.
 *
 * A line in an untouched stretch shifts by what the changes above it added or removed. A line
 * inside a changed range keeps its offset into the range, clamped to what the range became — so a
 * stop on a deleted block lands on the line that replaced it rather than past it.
 */
export function mapLine(line: number, edits: readonly LineEdit[]): number {
  let shift = 0;
  for (const e of edits) {
    if (line < e.fromA) return line + (e.fromB - e.fromA);
    if (line <= e.toA) return Math.min(e.fromB + (line - e.fromA), e.toB);
    shift = e.toB - e.toA;
  }
  return line + shift;
}

/** The pieces of a CodeMirror `ChangeSet` / `Text` this module needs — structural, so it stays
 *  free of the editor and testable with plain objects. */
export interface ChangesLike {
  iterChanges(f: (fromA: number, toA: number, fromB: number, toB: number) => void): void;
}
export interface DocLike {
  lineAt(pos: number): { number: number };
}

/** A change set as the lines it touched, in document order. */
export function lineEditsOf(changes: ChangesLike, before: DocLike, after: DocLike): LineEdit[] {
  const out: LineEdit[] = [];
  changes.iterChanges((fromA, toA, fromB, toB) => {
    out.push({
      fromA: before.lineAt(fromA).number,
      toA: before.lineAt(toA).number,
      fromB: after.lineAt(fromB).number,
      toB: after.lineAt(toB).number,
    });
  });
  return out;
}

export class NavHistory {
  private backStack: NavPlace[] = [];
  private forwardStack: NavPlace[] = [];
  private edits: NavPlace[] = [];
  /** How far back through `edits` the last-edit stepper has walked; reset by every new edit. */
  private editStep = -1;
  private recentList: RecentPlace[] = [];
  private clock = 0;

  constructor(private readonly sameFile: SameFile = defaultSameFile) {}

  get canBack(): boolean { return this.backStack.length > 0; }
  get canForward(): boolean { return this.forwardStack.length > 0; }
  get hasEdits(): boolean { return this.edits.length > 0; }
  get recent(): readonly RecentPlace[] { return this.recentList; }
  /** Both stacks, oldest first — for tests and diagnostics. */
  snapshot(): { back: NavPlace[]; forward: NavPlace[] } {
    return { back: [...this.backStack], forward: [...this.forwardStack] };
  }

  /** Same file and same line: one stop. */
  samePlace(a: NavPlace, b: NavPlace): boolean {
    return a.line === b.line && this.sameFile(a.file, b.file);
  }

  /**
   * A navigation is leaving `origin`. Records it as a stop and starts a new branch — anything
   * Forward held belonged to the branch this navigation abandons, exactly like a browser.
   */
  leave(origin: NavPlace): void {
    this.touchRecent(origin, 'visit');
    this.forwardStack = [];
    pushMerged(this.backStack, { ...origin }, (a, b) => this.samePlace(a, b));
  }

  /** A navigation landed at `place`. Not a stop — it becomes one when you leave it — but it is a
   *  place you visited, for Recent Locations. */
  arrive(place: NavPlace): void {
    this.touchRecent(place, 'visit');
  }

  /** Step back from `current` (where the caret is), or `null` when there is nowhere to go. */
  back(current: NavPlace | null): NavPlace | null {
    return this.step(this.backStack, this.forwardStack, current);
  }

  /** Step forward from `current`, or `null` when there is nowhere to go. */
  forward(current: NavPlace | null): NavPlace | null {
    return this.step(this.forwardStack, this.backStack, current);
  }

  private step(from: NavPlace[], to: NavPlace[], current: NavPlace | null): NavPlace | null {
    while (from.length > 0) {
      const target = from.pop()!;
      // The stop you are already on is not a destination — pressing Back must visibly go somewhere.
      if (current && this.samePlace(target, current)) continue;
      if (current) pushMerged(to, { ...current }, (a, b) => this.samePlace(a, b));
      return target;
    }
    return null;
  }

  /** Note that the document was edited at `place`. Merged by region: one burst of typing is one
   *  edit place. */
  noteEdit(place: NavPlace): void {
    this.editStep = -1;
    const last = this.edits[this.edits.length - 1];
    if (last && this.sameFile(last.file, place.file) && Math.abs(last.line - place.line) <= EDIT_MERGE_LINES) {
      this.edits[this.edits.length - 1] = { ...place };
    } else {
      this.edits.push({ ...place });
      if (this.edits.length > MAX_PLACES) this.edits.shift();
    }
    this.touchRecent(place, 'edit');
  }

  /** The next place back through the edit history — newest first, one further per call. */
  stepEdit(): NavPlace | null {
    if (this.editStep + 1 >= this.edits.length) return null;
    this.editStep += 1;
    return this.edits[this.edits.length - 1 - this.editStep];
  }

  /**
   * `file` changed: move every remembered line in it to where that line is now. Consecutive stops
   * the edit collapsed onto one line become one.
   */
  applyEdits(file: string, edits: readonly LineEdit[]): void {
    if (edits.length === 0) return;
    const move = (p: NavPlace): NavPlace =>
      this.sameFile(p.file, file) ? { ...p, line: Math.max(1, mapLine(p.line, edits)) } : p;
    const same = (a: NavPlace, b: NavPlace) => this.samePlace(a, b);
    this.backStack = dedupeAdjacent(this.backStack.map(move), same);
    this.forwardStack = dedupeAdjacent(this.forwardStack.map(move), same);
    this.edits = this.edits.map(move);
    this.recentList = this.recentList.map((r) => ({ ...move(r), kind: r.kind, at: r.at }));
  }

  /** `file` cannot be opened any more (deleted, renamed away): drop every stop in it. */
  forget(file: string): void {
    const keep = (p: NavPlace) => !this.sameFile(p.file, file);
    const same = (a: NavPlace, b: NavPlace) => this.samePlace(a, b);
    this.backStack = dedupeAdjacent(this.backStack.filter(keep), same);
    this.forwardStack = dedupeAdjacent(this.forwardStack.filter(keep), same);
    this.edits = this.edits.filter(keep);
    this.editStep = -1;
    this.recentList = this.recentList.filter(keep);
  }

  /** Drop everything — the project changed and these paths belong to the old one. */
  reset(): void {
    this.backStack = [];
    this.forwardStack = [];
    this.edits = [];
    this.editStep = -1;
    this.recentList = [];
  }

  /** Front of the recent list, collapsing a place that is already there. */
  private touchRecent(place: NavPlace, kind: RecentPlace['kind']): void {
    this.clock += 1;
    const kept = this.recentList.filter(
      (r) => !(this.sameFile(r.file, place.file) && Math.abs(r.line - place.line) <= EDIT_MERGE_LINES),
    );
    kept.unshift({ file: place.file, line: place.line, col: place.col, kind, at: this.clock });
    this.recentList = kept.slice(0, MAX_RECENT);
  }
}

/** Push onto a capped stack, replacing the top instead when it is the same place (fresher column). */
function pushMerged(stack: NavPlace[], place: NavPlace, same: (a: NavPlace, b: NavPlace) => boolean): void {
  const top = stack[stack.length - 1];
  if (top && same(top, place)) {
    stack[stack.length - 1] = place;
    return;
  }
  stack.push(place);
  if (stack.length > MAX_PLACES) stack.shift();
}

function dedupeAdjacent(list: NavPlace[], same: (a: NavPlace, b: NavPlace) => boolean): NavPlace[] {
  const out: NavPlace[] = [];
  for (const p of list) {
    const last = out[out.length - 1];
    if (last && same(last, p)) out[out.length - 1] = p;
    else out.push(p);
  }
  return out;
}
