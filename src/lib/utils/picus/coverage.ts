/**
 * Folding per-folder coverage into something a person can read.
 *
 * `InventoryObject.coverage` is keyed by folder path, and a real repository has
 * hundreds of folders — `AGGIORNAMENTO/4.13.2/ORA`, `AGGIORNAMENTO/4.13.3/ORA`,
 * one per delivered version. A matrix with a column per folder is not a dense
 * table, it is a horizontal scroll nobody reads, and the answer it is asked for
 * is not "how many statements are in this exact directory" anyway.
 *
 * The question the consistency rules ask — and the one the user asks — is
 * **"does the Oracle side say what the PostgreSQL side says, and do the updates
 * say what the initialisation says"**. Both are properties of the *effective
 * dialect* and the *effective role*, not of the directory. So the columns are
 * those axes: at most one per engine × role pair, six or eight in practice,
 * stable no matter how many versions the repository accumulates.
 *
 * Nothing is lost by folding: {@link folderBreakdown} gives the per-folder
 * numbers back for the single object being looked at, which is where that detail
 * is actually wanted, and {@link elsewhereCount} accounts for every statement
 * that landed outside the columns so a folded matrix can never look complete
 * when it is not.
 */

import {
  DIALECTS,
  FOLDER_ROLE_LABELS,
  GENERIC_ENGINE,
  dialectOf,
  engineIsUnsupported,
  isDialect,
  isGeneric,
  type FolderNode,
  type FolderRole,
  type InventoryObject,
  type TargetScope,
} from '$lib/types/picus';

/** Column order: the roles that carry statements, in the order they run. */
const ROLE_ORDER: FolderRole[] = ['init', 'update', 'routines', 'data'];

/**
 * Column order: engines, then portable, then "no engine" last.
 *
 * Portable sits after the two dialects and before the odd one out, because that
 * is what it is: a real classification whose numbers count, not a gap.
 */
const DIALECT_ORDER: (TargetScope | null)[] = ['oracle', 'postgres', GENERIC_ENGINE, null];

/**
 * One column of the coverage matrix: an engine and a role, plus every folder
 * whose effective classification lands in it.
 */
export interface CoverageBucket {
  /** Stable key — usable as an `{#each}` key and as a lookup. */
  key: string;
  /** The engine this column is about; `generic` for portable, `null` for none. */
  dialect: TargetScope | null;
  role: FolderRole;
  /** Short column header, e.g. `Oracle · update`. */
  label: string;
  /** Project-relative paths folded into this column, in tree order. */
  folders: string[];
  /** Files under those folders — how much the column actually stands on. */
  fileCount: number;
}

/** Every folder of the tree, depth-first, in display order. */
export function flattenFolders(tree: FolderNode[]): FolderNode[] {
  const out: FolderNode[] = [];
  const walk = (n: FolderNode) => {
    out.push(n);
    for (const child of n.children) walk(child);
  };
  for (const n of tree) walk(n);
  return out;
}

function bucketKey(dialect: TargetScope | null, role: FolderRole): string {
  return `${dialect ?? 'unclassified'}/${role}`;
}

function bucketLabel(dialect: TargetScope | null, role: FolderRole): string {
  const engine = isDialect(dialect)
    ? DIALECTS[dialect].short
    : dialect === GENERIC_ENGINE
      ? 'Portable'
      : 'No engine';
  return `${engine} · ${FOLDER_ROLE_LABELS[role]}`;
}

/**
 * The columns this repository actually has.
 *
 * Only folders that hold files take part: a directory with nothing in it can
 * only ever read zero, and a column of zeroes that means "there is nothing here"
 * is indistinguishable from one that means "something is missing here" — which
 * is the single distinction this whole table exists to make.
 *
 * Folders whose effective role is `ignored` are left out for the same reason:
 * they are not indexed, so their zeroes would be lies. So are folders written in
 * an engine Picus does not support — their files are deliberately never parsed,
 * so a column for them could only ever read zero, and a permanent row of zeroes
 * against the SQL Server folders would say "these are missing everything" when
 * the truth is "these are none of Picus's business". {@link ignoredFileCount}
 * says how many files both exclusions hide, and the view states it.
 */
export function coverageBuckets(tree: FolderNode[]): CoverageBucket[] {
  const byKey = new Map<string, CoverageBucket>();
  for (const folder of flattenFolders(tree)) {
    if (!folder.files.length) continue;
    if (folder.effectiveRole === 'ignored') continue;
    if (engineIsUnsupported(folder)) continue;
    // A portable folder gets a **column of its own** rather than being counted
    // into both dialects'. Counting it twice would make one INSERT read as two
    // statements in the totals, and a reader comparing the Oracle and PostgreSQL
    // columns could no longer tell what each engine's own scripts do. Its zeroes
    // in the dialect columns are handled by {@link coverageGaps}, which knows a
    // covered portable column is not a gap.
    const column: TargetScope | null = isGeneric(folder)
      ? GENERIC_ENGINE
      : dialectOf(folder);
    const key = bucketKey(column, folder.effectiveRole);
    let bucket = byKey.get(key);
    if (!bucket) {
      bucket = {
        key,
        dialect: column,
        role: folder.effectiveRole,
        label: bucketLabel(column, folder.effectiveRole),
        folders: [],
        fileCount: 0,
      };
      byKey.set(key, bucket);
    }
    bucket.folders.push(folder.path);
    bucket.fileCount += folder.files.length;
  }
  return [...byKey.values()].sort(
    (a, b) =>
      DIALECT_ORDER.indexOf(a.dialect) - DIALECT_ORDER.indexOf(b.dialect) ||
      ROLE_ORDER.indexOf(a.role) - ROLE_ORDER.indexOf(b.role),
  );
}

/** Files sitting under folders nobody indexes — stated, never silently dropped. */
export function ignoredFileCount(tree: FolderNode[]): number {
  return flattenFolders(tree)
    .filter((f) => f.effectiveRole === 'ignored' || engineIsUnsupported(f))
    .reduce((n, f) => n + f.files.length, 0);
}

/** How many statements touch `obj` inside one column. */
export function bucketCoverage(obj: InventoryObject, bucket: CoverageBucket): number {
  let total = 0;
  for (const path of bucket.folders) total += obj.coverage[path] ?? 0;
  return total;
}

/**
 * The columns that are a real gap for `obj`, by bucket key.
 *
 * **The single definition of "gap" in the frontend**, and it has to be: the same
 * question is asked by the matrix (which cells to mark), by the sidebar (which
 * objects get a warning) and by the header count, and three separate `=== 0`
 * tests answered it three different ways. The result was a table lit up with
 * marks against a report holding two findings — a tool contradicting itself,
 * which costs more trust than the marks were ever worth.
 *
 * A zero is **not** a gap when:
 *
 *  - the object is **external** — nothing here creates, alters or writes to it,
 *    so its zeroes are the boundary of the repository rather than something to
 *    go and fix. The backend never reports these either;
 *  - a **portable** column at the same role covers it. Portable scripts run on
 *    both engines, so the object really is installed there and calling the
 *    dialect column missing would report the opposite of the truth. This mirrors
 *    the backend's lane rule, and the two must agree: a cell marked here that
 *    `CONS001` does not raise reads as one of them being broken.
 */
export function gapKeys(obj: InventoryObject, buckets: CoverageBucket[]): Set<string> {
  if (obj.external) return new Set();
  const portableRoles = new Set(
    buckets
      .filter((b) => b.dialect === GENERIC_ENGINE && bucketCoverage(obj, b) > 0)
      .map((b) => b.role),
  );
  return new Set(
    buckets
      .filter((b) => bucketCoverage(obj, b) === 0)
      .filter((b) => !(isDialect(b.dialect) && portableRoles.has(b.role)))
      .map((b) => b.key),
  );
}

/** The same gaps, as column labels — for a tooltip or a sentence. */
export function coverageGaps(obj: InventoryObject, buckets: CoverageBucket[]): string[] {
  const keys = gapKeys(obj, buckets);
  return buckets.filter((b) => keys.has(b.key)).map((b) => b.label);
}

/** Objects with at least one real gap — the figure the header reports. */
export function objectsWithGaps(
  objects: InventoryObject[],
  buckets: CoverageBucket[],
): InventoryObject[] {
  return objects.filter((o) => gapKeys(o, buckets).size > 0);
}

/** One line of the per-object detail: a folder and what it says about the object. */
export interface FolderCoverage {
  path: string;
  count: number;
}

/**
 * The per-folder numbers behind one column, for one object — the detail the
 * matrix folds away, given back where it is asked for. Silent folders are kept:
 * "which of the eleven version folders is the one missing it" is the question.
 */
export function folderBreakdown(obj: InventoryObject, bucket: CoverageBucket): FolderCoverage[] {
  return bucket.folders.map((path) => ({ path, count: obj.coverage[path] ?? 0 }));
}

/**
 * Statements that landed in no column at all — a folder the tree does not carry,
 * one whose role is `ignored`, or one in an engine Picus does not read. Zero for a
 * healthy repository; anything else is worth showing rather than rounding away.
 */
export function elsewhereCount(obj: InventoryObject, buckets: CoverageBucket[]): number {
  const claimed = new Set(buckets.flatMap((b) => b.folders));
  let total = 0;
  for (const [path, n] of Object.entries(obj.coverage)) {
    if (!claimed.has(path)) total += n;
  }
  return total;
}
