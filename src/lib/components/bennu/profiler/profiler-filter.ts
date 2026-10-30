/**
 * Which classes the profiler views keep, and how serious a reading is. Pure, so it is tested.
 *
 * The filter is applied on the panel's side to the rows it already holds: flipping it costs no
 * request, and the same predicate serves every tab, so "only my code" means the same thing in the
 * flame graph, the tables and the memory view.
 */

import type {
  ProfilerFilter,
  ProfilerPrefs,
  ProfilerThresholds,
  SeverityLevels,
} from '$lib/ipc/bennu/profiler';

/** Mirrors `Thresholds::default()` in `profiler/prefs.rs` — what the panel uses until the project's own load. */
export const DEFAULT_THRESHOLDS: ProfilerThresholds = {
  cpu_self_pct: { yellow: 5, orange: 15, red: 30 },
  gc_pause_pct: { yellow: 2, orange: 5, red: 10 },
  alloc_share_pct: { yellow: 10, orange: 25, red: 50 },
  lock_total_ms: { yellow: 50, orange: 250, red: 1000 },
};

export const DEFAULT_PREFS: ProfilerPrefs = {
  filter: { project_only: false, include: [], exclude: [] },
  thresholds: DEFAULT_THRESHOLDS,
};

// ── severity ──────────────────────────────────────────────────────────────────────────────

/** 0 = unremarkable, then yellow, orange, red. */
export type Severity = 0 | 1 | 2 | 3;

export function severityOf(value: number, levels: SeverityLevels): Severity {
  if (value >= levels.red) return 3;
  if (value >= levels.orange) return 2;
  if (value >= levels.yellow) return 1;
  return 0;
}

const SEVERITY_COLORS: Record<1 | 2 | 3, string> = {
  1: 'var(--warning)',
  2: 'hsl(25 92% 55%)',
  3: 'var(--error)',
};

export const SEVERITY_NAMES: Record<Severity, string> = { 0: 'normal', 1: 'elevated', 2: 'high', 3: 'critical' };

/** The colour of a level, or `undefined` for an unremarkable reading (which keeps the default look). */
export function severityColor(s: Severity): string | undefined {
  return s === 0 ? undefined : SEVERITY_COLORS[s];
}

/** A faint tint of the level's colour, for a row or cell background. */
export function severityTint(s: Severity): string | undefined {
  const c = severityColor(s);
  return c ? `color-mix(in srgb, ${c} 16%, transparent)` : undefined;
}

// ── class filter ──────────────────────────────────────────────────────────────────────────

/** `a.b.Outer$Inner$1` → `a.b.Outer`. A project's index lists the outer type. */
export function topLevelClass(cls: string): string {
  const i = cls.indexOf('$');
  return i < 0 ? cls : cls.slice(0, i);
}

/** `byte[]` → `byte`; `[Ljava.lang.String;` → `java.lang.String`. Allocation types are spelled both ways. */
export function elementClass(type: string): string {
  let t = type;
  while (t.endsWith('[]')) t = t.slice(0, -2);
  if (t.startsWith('[')) t = t.replace(/^\[+L?/, '').replace(/;$/, '');
  return t;
}

/** The class of a `class.method` name (an allocation site, a lock site). */
export function classOfMember(qualified: string): string {
  const i = qualified.lastIndexOf('.');
  return i < 0 ? qualified : qualified.slice(0, i);
}

const globCache = new Map<string, RegExp>();

/** `it.acme.*.dto` → an anchored expression: `*` is any run of characters (dots included), `?` is one. */
function globToRegExp(glob: string): RegExp {
  let re = globCache.get(glob);
  if (!re) {
    const body = glob.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.');
    re = new RegExp(`^${body}$`);
    globCache.set(glob, re);
  }
  return re;
}

/**
 * Whether `cls` matches a pattern the user typed.
 *
 * Without a wildcard a pattern is a package or a class and covers everything inside it: `it.acme` matches
 * `it.acme.Foo` and `it.acme.web.Bar`, not `it.acmeco.Foo`. With `*` or `?` it is a glob over the whole class
 * name: `it.acme.*` is the same as the bare form, `*.service.*` is any class in a `service` package, `*Dao` any
 * class ending in `Dao`.
 */
export function matchesPattern(cls: string, pattern: string): boolean {
  const p = pattern.trim();
  if (!p) return false;
  if (/[*?]/.test(p)) return globToRegExp(p).test(cls);
  const bare = p.replace(/\.$/, '');
  return cls === bare || cls.startsWith(`${bare}.`) || cls.startsWith(`${bare}$`);
}

/** The text of the package field as patterns: `it.acme.*, !*.generated.*` — a leading `!` hides. */
export function parsePatterns(text: string): { include: string[]; exclude: string[] } {
  const include: string[] = [];
  const exclude: string[] = [];
  for (const raw of text.split(/[\s,;]+/)) {
    if (!raw) continue;
    if (raw.startsWith('!')) {
      if (raw.length > 1) exclude.push(raw.slice(1));
    } else {
      include.push(raw);
    }
  }
  return { include, exclude };
}

/** The inverse of {@link parsePatterns}, for putting the stored filter back in the field. */
export function formatPatterns(f: Pick<ProfilerFilter, 'include' | 'exclude'>): string {
  return [...f.include, ...f.exclude.map((p) => `!${p}`)].join(', ');
}

/**
 * The ghost text for the word being typed: the longest package that every candidate starting with it agrees on, or
 * `''` when nothing is certain. A fact the project's own packages already told us, never a guess — with two
 * branches it stops at the fork rather than choosing one.
 */
export function completePackage(token: string, packages: readonly string[]): string {
  const negated = token.startsWith('!');
  const word = negated ? token.slice(1) : token;
  if (word === '' || /[*?]/.test(word)) return '';
  const candidates = packages.filter((p) => p.startsWith(word));
  if (candidates.length === 0) return '';
  let common = candidates[0];
  for (const c of candidates) while (!c.startsWith(common)) common = common.slice(0, -1);
  return common.length > word.length ? (negated ? '!' : '') + common : '';
}

export interface FilterContext {
  filter: ProfilerFilter;
  /** The project's own classes, or `null` while they are not known (the project-only rule then keeps everything). */
  projectClasses: ReadonlySet<string> | null;
}

/** Whether a row about `cls` stays visible. An empty class name (a thread, the root) always does. */
export function keepClass(ctx: FilterContext, cls: string): boolean {
  if (!cls) return true;
  const { filter, projectClasses } = ctx;
  if (filter.project_only && projectClasses && !projectClasses.has(topLevelClass(cls))) return false;
  if (filter.include.length > 0 && !filter.include.some((p) => matchesPattern(cls, p))) return false;
  return !filter.exclude.some((p) => matchesPattern(cls, p));
}

/** Whether the filter restricts anything — what lights the filter button. */
export function filterIsActive(filter: ProfilerFilter): boolean {
  return filter.project_only || filter.include.length > 0 || filter.exclude.length > 0;
}

/** How many separate restrictions are set, for the count on the button. */
export function filterCount(filter: ProfilerFilter): number {
  return (filter.project_only ? 1 : 0) + filter.include.length + filter.exclude.length;
}

/** The package of a class: `a.b.Outer$Inner` → `a.b`. Empty for the default package. */
export function packageOf(cls: string): string {
  const top = topLevelClass(cls);
  const i = top.lastIndexOf('.');
  return i < 0 ? '' : top.slice(0, i);
}
