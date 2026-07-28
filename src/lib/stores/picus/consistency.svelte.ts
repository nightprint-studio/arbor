/**
 * Picus consistency — the findings the analysis rules produce, plus the grouping,
 * filtering and step-through the bottom dock offers.
 *
 * A finding is judged by its **consequence**, not by its rule text: every entry
 * says what breaks in practice if it is left alone. Suppressions are declared in
 * the script (`-- picus: ignore DML001 — reason`) and stay visible with their
 * reason attached — silencing without a motivation is not possible.
 *
 * ## This store is a sink, not a driver
 *
 * The analysis takes a project root and returns findings, an inventory, and the
 * rules that could not run — three things belonging to two stores. Rather than
 * have both call the backend (and disagree about when it last ran), the *project*
 * store owns the round trip and pushes the result here through
 * `beginRun` / `acceptAnalysis` / `failRun`. That keeps the import graph a line
 * (project → consistency) instead of a cycle, and keeps "when was this checked"
 * a single fact.
 *
 * ## Skipped rules are part of the report
 *
 * A rule that could not run is **not** a rule that passed. `VER003` standing down
 * because the update files carry no readable version bounds means the version
 * chain is unchecked, and a report that stays silent about that is claiming
 * something it never verified.
 */

import type { AnalyzeScriptsResult, ProjectNote, SkippedRule } from '$lib/ipc/picus/scripts';
import { suppressionNote } from '$lib/ipc/picus/scripts';
import type { Finding, Severity } from '$lib/types/picus';

export type FindingGrouping = 'severity' | 'folder' | 'file';

/**
 * The folder a finding sits in, taken from its own path.
 *
 * Derived rather than carried as a field: the folder of `X/Y/z.sql` is `X/Y`
 * whatever the backend chooses to say about it, and asking the project store
 * instead would turn the import line (project → consistency) into a cycle.
 */
function folderOf(file: string): string {
  const cut = file.lastIndexOf('/');
  return cut > 0 ? file.slice(0, cut) : '(repository root)';
}

function createConsistencyStore() {
  let findings = $state<Finding[]>([]);
  let skipped = $state<SkippedRule[]>([]);
  /** Suppression comments the analysis refused: malformed, or naming nothing that fired. */
  let rejectedSuppressions = $state<ProjectNote[]>([]);

  let grouping = $state<FindingGrouping>('severity');
  let filter = $state('');
  /** Show findings silenced by a declared suppression. Off by default. */
  let showSuppressed = $state(false);
  /** When the analysis last ran; `null` before the first pass. */
  let lastRunAt = $state<string | null>(null);
  let running = $state(false);
  /** Why the last analysis failed; empty when it succeeded (or never ran). */
  let error = $state('');
  /** The analysis has produced a verdict for the current project at least once. */
  let hasRun = $state(false);

  /** The finding F8 last landed on — highlighted in the list and scrolled to. */
  let focusedId = $state<string | null>(null);

  const visible = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return findings.filter((f) => {
      if (!showSuppressed && f.suppressedBecause) return false;
      if (!q) return true;
      return (
        f.title.toLowerCase().includes(q) ||
        f.rule.toLowerCase().includes(q) ||
        f.file.toLowerCase().includes(q)
      );
    });
  });

  /** Open blockers — the number the rail badge and the status bar report. */
  const blockingCount = $derived(
    findings.filter((f) => f.severity === 'blocking' && !f.suppressedBecause).length,
  );
  const reviewCount = $derived(
    findings.filter((f) => f.severity === 'review' && !f.suppressedBecause).length,
  );
  const suppressedCount = $derived(findings.filter((f) => !!f.suppressedBecause).length);

  /** Findings bucketed by the active grouping, in a stable display order. */
  const groups = $derived.by<{ key: string; label: string; items: Finding[] }[]>(() => {
    if (grouping === 'severity') {
      const order: Severity[] = ['blocking', 'review'];
      return order
        .map((sev) => ({
          key: sev,
          label: sev === 'blocking' ? 'Blocking' : 'Worth checking',
          items: visible.filter((f) => f.severity === sev),
        }))
        .filter((g) => g.items.length > 0);
    }
    const buckets = new Map<string, Finding[]>();
    for (const f of visible) {
      const k = grouping === 'folder' ? folderOf(f.file) : f.file;
      const list = buckets.get(k);
      if (list) list.push(f);
      else buckets.set(k, [f]);
    }
    return [...buckets.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([k, items]) => ({ key: k, label: k, items }));
  });

  /** Step order for F8 — the report as it reads, group by group. */
  const walkOrder = $derived<Finding[]>(groups.flatMap((g) => g.items));

  const focused = $derived(walkOrder.find((f) => f.id === focusedId) ?? null);

  /** Move the cursor `step` places through the report and return where it landed. */
  function step(delta: number): Finding | null {
    if (!walkOrder.length) return null;
    const i = walkOrder.findIndex((f) => f.id === focusedId);
    // No cursor yet: forwards starts at the first, backwards at the last.
    const next = i < 0 ? (delta > 0 ? 0 : walkOrder.length - 1)
                       : (i + delta + walkOrder.length) % walkOrder.length;
    focusedId = walkOrder[next].id;
    return walkOrder[next];
  }

  return {
    get findings() { return findings; },
    get visible() { return visible; },
    get groups() { return groups; },
    get grouping() { return grouping; },
    get filter() { return filter; },
    get showSuppressed() { return showSuppressed; },
    get blockingCount() { return blockingCount; },
    get reviewCount() { return reviewCount; },
    get suppressedCount() { return suppressedCount; },
    get totalCount() { return blockingCount + reviewCount; },
    get lastRunAt() { return lastRunAt; },
    get running() { return running; },
    get error() { return error; },
    get hasRun() { return hasRun; },
    get skipped() { return skipped; },
    get rejectedSuppressions() { return rejectedSuppressions; },
    get focusedId() { return focusedId; },
    get focused() { return focused; },
    /** True when nothing was found AND every rule actually ran. */
    get fullyClean() { return hasRun && !visible.length && !skipped.length; },

    setGrouping(g: FindingGrouping) { grouping = g; },
    setFilter(v: string) { filter = v; },
    toggleSuppressed() { showSuppressed = !showSuppressed; },
    setShowSuppressed(v: boolean) { showSuppressed = v; },
    focus(id: string | null) { focusedId = id; },

    /** Next / previous finding in the report — the F8 pair. */
    next(): Finding | null { return step(1); },
    previous(): Finding | null { return step(-1); },

    // ── Written by the project store, which owns the round trip ───────────────

    /** An analysis is out. Keeps the previous result on screen rather than blanking it. */
    beginRun() {
      running = true;
      error = '';
    },

    acceptAnalysis(res: AnalyzeScriptsResult) {
      findings = res.findings ?? [];
      skipped = res.skipped ?? [];
      rejectedSuppressions = (res.rejectedSuppressions ?? []).map(suppressionNote);
      if (!findings.some((f) => f.id === focusedId)) focusedId = null;
      lastRunAt = new Date().toTimeString().slice(0, 5);
      error = '';
      hasRun = true;
      running = false;
    },

    failRun(message: string) {
      // The previous findings are dropped: keeping them would let a failed run
      // masquerade as a passed one, which is the exact confusion the skipped-rule
      // list exists to prevent.
      findings = [];
      skipped = [];
      rejectedSuppressions = [];
      focusedId = null;
      error = message;
      hasRun = false;
      running = false;
    },

    /** No project is open — there is nothing to be consistent about. */
    clear() {
      findings = [];
      skipped = [];
      rejectedSuppressions = [];
      focusedId = null;
      lastRunAt = null;
      error = '';
      hasRun = false;
      running = false;
    },

    /** Drop a finding once its corrective patch has been applied. */
    resolve(id: string) {
      findings = findings.filter((f) => f.id !== id);
      if (focusedId === id) focusedId = null;
    },
  };
}

export const consistencyStore = createConsistencyStore();
