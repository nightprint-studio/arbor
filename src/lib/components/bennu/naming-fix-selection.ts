/**
 * Grouping, filtering and selection for the bulk naming-fix review.
 *
 * Kept out of the component because it is arithmetic, not markup: which rows are visible, which
 * are selected, and which edits that adds up to. A project-wide fix reaches thousands of names, so
 * this runs on every keystroke of the filter — it stays in one place where it can be read and,
 * if it ever gets slow, made faster once.
 *
 * ## Filtering and selecting are the same answer
 *
 * A name hidden by a filter is a name that will not be renamed. There is no third state where
 * something is invisible but still applied: the count in the footer and the list on screen have to
 * describe the same operation, or the review is lying about what Apply does.
 */
import type { RenameEdit, RenameFileMove } from '$lib/ipc/bennu/nav';
import type { NamingTarget, RenamedName } from '$lib/ipc/bennu/naming';

/** What the list is grouped by. */
export type GroupBy = 'file' | 'target' | 'none';

/** The user's choices in the review — everything that narrows the plan. */
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
}

/** A filter that narrows nothing — the state the review opens in. */
export function noFilter(by: GroupBy = 'file'): FixFilter {
  return {
    by,
    hiddenTargets: new Set(),
    hiddenGroups: new Set(),
    excluded: new Set(),
    search: '',
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

/** Whether a name survives the filter's *visibility* rules — kind, group and search, but not the
 *  per-row tick, which decides selection rather than visibility. */
function isVisible(name: RenamedName, filter: FixFilter, needle: string): boolean {
  if (filter.hiddenTargets.has(name.target)) return false;
  if (!needle) return true;
  return name.from.toLowerCase().includes(needle) || name.to.toLowerCase().includes(needle);
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

/** Run `visit` over every name Apply would touch, in plan order. */
function forEachChosen(
  renamed: readonly RenamedName[],
  filter: FixFilter,
  visit: (name: RenamedName) => void,
): void {
  const needle = filter.search.trim().toLowerCase();
  for (const [index, name] of renamed.entries()) {
    if (isVisible(name, filter, needle) && isSelected(name, index, filter)) visit(name);
  }
}

/**
 * How much Apply would do — names and distinct files.
 *
 * Separate from [`selectedEdits`] because the footer re-reads this on every keystroke of the filter
 * and only needs two numbers; building the whole edit list to count it would allocate thousands of
 * objects per character typed, and throw them away again.
 */
export function selectionCounts(
  renamed: readonly RenamedName[],
  filter: FixFilter,
): { names: number; files: number } {
  const files = new Set<string>();
  let names = 0;
  forEachChosen(renamed, filter, (name) => {
    names += 1;
    for (const e of name.edits) files.add(e.file);
  });
  return { names, files: files.size };
}

/** The edits Apply will actually write — built once, when the user commits. */
export function selectedEdits(renamed: readonly RenamedName[], filter: FixFilter): RenameEdit[] {
  const edits: RenameEdit[] = [];
  forEachChosen(renamed, filter, (name) => edits.push(...name.edits));
  return edits;
}

/**
 * The file moves Apply must carry out, for the names still selected.
 *
 * Renaming a public top-level type without its file leaves code that does not compile, so the move
 * belongs to the name that causes it — untick the name and its file stays put. Applied AFTER the
 * edits, which are addressed to the old paths.
 */
export function selectedFileMoves(
  renamed: readonly RenamedName[],
  filter: FixFilter,
): RenameFileMove[] {
  const moves: RenameFileMove[] = [];
  forEachChosen(renamed, filter, (name) => {
    if (name.file_rename) moves.push(name.file_rename);
  });
  return moves;
}
