/**
 * Grouping, filtering and selection for the bulk naming-fix review.
 *
 * Kept out of the component because it is arithmetic, not markup: which rows are visible, which
 * are selected, which name each will be given, and which edits that adds up to. A project-wide fix
 * reaches thousands of names, so this runs on every keystroke of the filter — it stays in one place
 * where it can be read and tested.
 *
 * ## Filtering and selecting are the same answer
 *
 * A name hidden by a filter is a name that will not be renamed. There is no third state where
 * something is invisible but still applied: the count in the footer and the list on screen have to
 * describe the same operation, or the review is lying about what Apply does.
 *
 * ## A row is identified by its index in the plan
 *
 * Never by file + name: one file holds several declarations with the same name (a local called
 * `source_directory` in five methods is five renames).
 */
import type { RenameEdit, RenameFileMove } from '$lib/ipc/bennu/nav';
import type { FixRefusal, NamingTarget, RenamedName } from '$lib/ipc/bennu/naming';

/** What the list is grouped by. */
export type GroupBy = 'file' | 'target' | 'none';

/** The user's choices in the review — everything that narrows or edits the plan. */
export interface FixFilter {
  by: GroupBy;
  /** Target kinds switched OFF. Absent = on, so a plan with a kind nobody has seen yet is
   *  included rather than silently dropped. */
  hiddenTargets: ReadonlySet<string>;
  /** Group keys switched OFF (a whole file or a whole kind unticked at once). */
  hiddenGroups: ReadonlySet<string>;
  /** Individual rows unticked, by their index in the plan. */
  excluded: ReadonlySet<number>;
  /** Free-text match against the old or the new name. Empty matches everything. */
  search: string;
  /** Names the person typed over the proposed ones, by plan index. */
  renames: ReadonlyMap<number, string>;
}

/** A filter that narrows nothing — the state the review opens in. */
export function noFilter(by: GroupBy = 'file'): FixFilter {
  return {
    by,
    hiddenTargets: new Set(),
    hiddenGroups: new Set(),
    excluded: new Set(),
    search: '',
    renames: new Map(),
  };
}

/** One line in the rendered list. Headers and rows are both lines so the whole thing can be
 *  windowed as a single flat array — see `VirtualList`. */
export type FixLine =
  | { kind: 'group'; key: string; label: string; total: number; selected: number }
  | { kind: 'item'; index: number; name: RenamedName; selected: boolean };

/** The group a name belongs to under `by`. `file` groups by declaring file, which for Java is
 *  its class; `target` groups by kind of declaration. */
export function groupKey(name: RenamedName, by: GroupBy): string {
  if (by === 'target') return name.target;
  if (by === 'file') return name.file;
  return '';
}

/** How a group is labelled. A file shows its basename — for Java that IS the class name. */
export function groupLabel(key: string, by: GroupBy): string {
  if (by === 'file') return key.split(/[\\/]/).pop() ?? key;
  return key;
}

/** Every target kind present in the plan, in a stable order, with how many names each holds. */
export function targetCounts(renamed: readonly RenamedName[]): { target: NamingTarget; count: number }[] {
  const counts = new Map<string, number>();
  for (const r of renamed) counts.set(r.target, (counts.get(r.target) ?? 0) + 1);
  return [...counts.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([target, count]) => ({ target: target as NamingTarget, count }));
}

// ── what a name will be, and what it reaches ────────────────────────────────────

/** A valid Java/Rust-ish identifier: what the inline editor accepts. */
const IDENTIFIER = /^[\p{L}_$][\p{L}\p{N}_$]*$/u;

/** Whether `typed` may replace the proposal for `name`: an identifier, and not the name it replaces. */
export function isValidRename(name: RenamedName, typed: string): boolean {
  return typed !== name.from && IDENTIFIER.test(typed);
}

/** The name `index` will be given: what the person typed, else the proposal. */
export function effectiveName(name: RenamedName, index: number, filter: FixFilter): string {
  return filter.renames.get(index) ?? name.to;
}

/** How many places besides the declaration the rename rewrites. */
export function referenceCount(name: RenamedName): number {
  return Math.max(0, name.edits.length - 1);
}

/** The files a rename touches, each with the number of edits in it, declaring file first. */
export function filesOf(name: RenamedName): { file: string; edits: number }[] {
  const counts = new Map<string, number>();
  for (const e of name.edits) counts.set(e.file, (counts.get(e.file) ?? 0) + 1);
  return [...counts.entries()]
    .map(([file, edits]) => ({ file, edits }))
    .sort((a, b) => Number(b.file === name.file) - Number(a.file === name.file) || a.file.localeCompare(b.file));
}

// ── visibility and selection ────────────────────────────────────────────────────

/** Whether a name survives the filter's *visibility* rules — kind and search, but not the
 *  per-row tick, which decides selection rather than visibility. */
function isVisible(name: RenamedName, filter: FixFilter, needle: string): boolean {
  if (filter.hiddenTargets.has(name.target)) return false;
  if (!needle) return true;
  return (
    name.from.toLowerCase().includes(needle) ||
    name.to.toLowerCase().includes(needle) ||
    name.file.toLowerCase().includes(needle)
  );
}

/** Whether a visible name will actually be renamed. */
function isSelected(name: RenamedName, index: number, filter: FixFilter): boolean {
  return !filter.excluded.has(index) && !filter.hiddenGroups.has(groupKey(name, filter.by));
}

/**
 * The rendered lines: visible names, grouped, each group preceded by its header.
 *
 * Groups come out in first-appearance order rather than sorted, so the list matches the order the
 * plan was built in and does not reshuffle when a filter changes.
 */
export function buildLines(renamed: readonly RenamedName[], filter: FixFilter): FixLine[] {
  const needle = filter.search.trim().toLowerCase();
  const order: string[] = [];
  const buckets = new Map<string, { index: number; name: RenamedName }[]>();

  for (const [index, name] of renamed.entries()) {
    if (!isVisible(name, filter, needle)) continue;
    const key = groupKey(name, filter.by);
    let bucket = buckets.get(key);
    if (!bucket) {
      bucket = [];
      buckets.set(key, bucket);
      order.push(key);
    }
    bucket.push({ index, name });
  }

  const lines: FixLine[] = [];
  for (const key of order) {
    const bucket = buckets.get(key) ?? [];
    if (filter.by !== 'none') {
      lines.push({
        kind: 'group',
        key,
        label: groupLabel(key, filter.by),
        total: bucket.length,
        selected: bucket.filter((b) => isSelected(b.name, b.index, filter)).length,
      });
    }
    for (const { index, name } of bucket) {
      lines.push({ kind: 'item', index, name, selected: isSelected(name, index, filter) });
    }
  }
  return lines;
}

/** The plan indices a group currently holds, under the visibility rules. */
export function indicesInGroup(
  renamed: readonly RenamedName[],
  filter: FixFilter,
  key: string,
): number[] {
  const needle = filter.search.trim().toLowerCase();
  const out: number[] = [];
  for (const [index, name] of renamed.entries()) {
    if (isVisible(name, filter, needle) && groupKey(name, filter.by) === key) out.push(index);
  }
  return out;
}

/** The plan indices currently visible, in plan order. */
export function visibleIndices(renamed: readonly RenamedName[], filter: FixFilter): number[] {
  const needle = filter.search.trim().toLowerCase();
  const out: number[] = [];
  for (const [index, name] of renamed.entries()) if (isVisible(name, filter, needle)) out.push(index);
  return out;
}

/** The filter with every visible row ticked (and its groups switched back on). */
export function selectAll(renamed: readonly RenamedName[], filter: FixFilter): FixFilter {
  const excluded = new Set(filter.excluded);
  const hiddenGroups = new Set(filter.hiddenGroups);
  for (const index of visibleIndices(renamed, filter)) {
    excluded.delete(index);
    hiddenGroups.delete(groupKey(renamed[index], filter.by));
  }
  return { ...filter, excluded, hiddenGroups };
}

/** The filter with every visible row unticked. Rows hidden by the filter keep their state. */
export function selectNone(renamed: readonly RenamedName[], filter: FixFilter): FixFilter {
  const excluded = new Set(filter.excluded);
  for (const index of visibleIndices(renamed, filter)) excluded.add(index);
  return { ...filter, excluded };
}

/** The filter with every visible row's tick flipped. */
export function invertSelection(renamed: readonly RenamedName[], filter: FixFilter): FixFilter {
  const excluded = new Set(filter.excluded);
  const hiddenGroups = new Set(filter.hiddenGroups);
  for (const index of visibleIndices(renamed, filter)) {
    const name = renamed[index];
    if (isSelected(name, index, filter)) {
      excluded.add(index);
    } else {
      excluded.delete(index);
      hiddenGroups.delete(groupKey(name, filter.by));
    }
  }
  return { ...filter, excluded, hiddenGroups };
}

/**
 * Tick exactly the visible rows `keep` accepts and untick the rest — "only methods", "only this
 * file". Selecting by kind or by file is this with a different predicate.
 */
export function selectOnly(
  renamed: readonly RenamedName[],
  filter: FixFilter,
  keep: (name: RenamedName) => boolean,
): FixFilter {
  const excluded = new Set(filter.excluded);
  const hiddenGroups = new Set(filter.hiddenGroups);
  for (const index of visibleIndices(renamed, filter)) {
    const name = renamed[index];
    if (keep(name)) {
      excluded.delete(index);
      hiddenGroups.delete(groupKey(name, filter.by));
    } else {
      excluded.add(index);
    }
  }
  return { ...filter, excluded, hiddenGroups };
}

/** The filter with one row ticked or unticked. */
export function setRow(filter: FixFilter, index: number, on: boolean): FixFilter {
  const excluded = new Set(filter.excluded);
  if (on) excluded.delete(index);
  else excluded.add(index);
  return { ...filter, excluded };
}

/** The filter with one row given a typed name, or back to the proposal when `typed` is empty or the proposal. */
export function setRename(
  renamed: readonly RenamedName[],
  filter: FixFilter,
  index: number,
  typed: string,
): FixFilter {
  const renames = new Map(filter.renames);
  const name = renamed[index];
  if (!name || typed === '' || typed === name.to || !isValidRename(name, typed)) renames.delete(index);
  else renames.set(index, typed);
  return { ...filter, renames };
}

/** Run `visit` over every name Apply would touch, in plan order. */
function forEachChosen(
  renamed: readonly RenamedName[],
  filter: FixFilter,
  visit: (name: RenamedName, index: number) => void,
): void {
  const needle = filter.search.trim().toLowerCase();
  for (const [index, name] of renamed.entries()) {
    if (isVisible(name, filter, needle) && isSelected(name, index, filter)) visit(name, index);
  }
}

/**
 * How much Apply would do — names, distinct files and edits.
 *
 * Separate from [`selectedEdits`] because the footer re-reads this on every keystroke of the filter
 * and only needs numbers; building the whole edit list to count it would allocate thousands of
 * objects per character typed, and throw them away again.
 */
export function selectionCounts(
  renamed: readonly RenamedName[],
  filter: FixFilter,
): { names: number; files: number; edits: number } {
  const files = new Set<string>();
  let names = 0;
  let edits = 0;
  forEachChosen(renamed, filter, (name) => {
    names += 1;
    edits += name.edits.length;
    for (const e of name.edits) files.add(e.file);
  });
  return { names, files: files.size, edits };
}

/** The edits Apply will actually write — built once, when the user commits. A name the person
 *  retyped is written in every edit of that rename. */
export function selectedEdits(renamed: readonly RenamedName[], filter: FixFilter): RenameEdit[] {
  const edits: RenameEdit[] = [];
  forEachChosen(renamed, filter, (name, index) => {
    const typed = filter.renames.get(index);
    if (typed === undefined) edits.push(...name.edits);
    else edits.push(...name.edits.map((e) => (e.new_text === name.to ? { ...e, new_text: typed } : e)));
  });
  return edits;
}

/**
 * The file moves Apply must carry out, for the names still selected.
 *
 * Renaming a public top-level type without its file leaves code that does not compile, so the move
 * belongs to the name that causes it — untick the name and its file stays put. Applied AFTER the
 * edits, which are addressed to the old paths. A retyped name moves the file to the retyped name.
 */
export function selectedFileMoves(
  renamed: readonly RenamedName[],
  filter: FixFilter,
): RenameFileMove[] {
  const moves: RenameFileMove[] = [];
  forEachChosen(renamed, filter, (name, index) => {
    if (!name.file_rename) return;
    const typed = filter.renames.get(index);
    if (typed === undefined) {
      moves.push(name.file_rename);
      return;
    }
    const to = name.file_rename.to;
    const base = to.split('/').pop() ?? to;
    const extension = base.includes('.') ? base.slice(base.lastIndexOf('.')) : '';
    moves.push({ ...name.file_rename, to: to.slice(0, to.length - base.length) + typed + extension });
  });
  return moves;
}

// ── what was left alone ─────────────────────────────────────────────────────────

/** Why a name was refused, as a kind the review can group and explain. */
export type RefusalKind =
  | 'collision'
  | 'library'
  | 'serialisation'
  | 'package'
  | 'encoding'
  | 'unsupported'
  | 'stopped'
  | 'other';

/** Order and wording of the refusal kinds. */
export const REFUSAL_KINDS: { kind: RefusalKind; label: string; hint: string }[] = [
  { kind: 'collision', label: 'Collision', hint: 'Another name in the file would end up spelled the same.' },
  { kind: 'library', label: 'Library override', hint: 'It overrides or implements a library member, whose name a jar fixes.' },
  { kind: 'serialisation', label: 'Serialisation', hint: 'An annotation writes the Java name into JSON, XML or a column.' },
  { kind: 'package', label: 'Package', hint: 'A package is named by its directory; move the files instead.' },
  { kind: 'encoding', label: 'Encoding', hint: 'The file is not valid in the project’s declared encoding.' },
  { kind: 'unsupported', label: 'Not renameable here', hint: 'The engine found no editable site for it.' },
  { kind: 'stopped', label: 'Stopped', hint: 'The run was stopped before it could be planned.' },
  { kind: 'other', label: 'Other', hint: '' },
];

/** The kind of refusal a backend reason sentence is. The sentences are the contract; this reads them. */
export function refusalKind(reason: string): RefusalKind {
  const r = reason.toLowerCase();
  if (r.startsWith('serialisation')) return 'serialisation';
  if (r.includes('package segment')) return 'package';
  if (r.includes('declared encoding')) return 'encoding';
  if (r.startsWith('stopped')) return 'stopped';
  if (r.includes('would become') || r.includes('already appears') || r.includes('overlap')) return 'collision';
  if (r.includes('librar') || r.includes('jar') || r.includes('overrid') || r.includes('inherit')) return 'library';
  if (r.includes('no editable site') || r.includes('incomplete') || r.includes('nothing to rename')) {
    return 'unsupported';
  }
  return 'other';
}

/** The refusals with their kind, each kind's count, and kinds in the order of [`REFUSAL_KINDS`]. */
export function refusalSummary(refused: readonly FixRefusal[]): { kind: RefusalKind; label: string; count: number }[] {
  const counts = new Map<RefusalKind, number>();
  for (const r of refused) {
    const kind = refusalKind(r.reason);
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }
  return REFUSAL_KINDS.filter((k) => counts.has(k.kind)).map((k) => ({
    kind: k.kind,
    label: k.label,
    count: counts.get(k.kind) ?? 0,
  }));
}

/** A line of the "left alone" list: a kind header or one refusal. */
export type RefusedLine =
  | { kind: 'group'; key: RefusalKind; label: string; hint: string; count: number }
  | { kind: 'item'; refusal: FixRefusal; reason: RefusalKind };

/** The refusals matching `search` (name, file or reason), grouped by kind. */
export function refusedLines(refused: readonly FixRefusal[], search: string): RefusedLine[] {
  const needle = search.trim().toLowerCase();
  const buckets = new Map<RefusalKind, FixRefusal[]>();
  for (const r of refused) {
    if (needle && !`${r.name} ${r.file} ${r.reason}`.toLowerCase().includes(needle)) continue;
    const kind = refusalKind(r.reason);
    const bucket = buckets.get(kind) ?? [];
    bucket.push(r);
    buckets.set(kind, bucket);
  }
  const lines: RefusedLine[] = [];
  for (const k of REFUSAL_KINDS) {
    const bucket = buckets.get(k.kind);
    if (!bucket) continue;
    lines.push({ kind: 'group', key: k.kind, label: k.label, hint: k.hint, count: bucket.length });
    for (const refusal of bucket) lines.push({ kind: 'item', refusal, reason: k.kind });
  }
  return lines;
}
