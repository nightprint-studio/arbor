/**
 * Picus consistency — the findings the analysis rules produce, plus the grouping
 * and filtering the bottom dock offers.
 *
 * A finding is judged by its **consequence**, not by its rule text: every entry
 * says what breaks in practice if it is left alone. Suppressions are declared in
 * the script (`-- picus: ignore DML001 — reason`) and stay visible with their
 * reason attached — silencing without a motivation is not possible.
 *
 * MOCK: fed from `ipc/picus/mock` until `picus-analyze` runs behind `picus-be`.
 */

import type { Finding, Severity } from '$lib/types/picus';
import { MOCK_FINDINGS } from '$lib/ipc/picus/mock';

export type FindingGrouping = 'severity' | 'branch' | 'file';

function createConsistencyStore() {
  let findings = $state<Finding[]>(MOCK_FINDINGS);
  let grouping = $state<FindingGrouping>('severity');
  let filter = $state('');
  /** Show findings silenced by a declared suppression. Off by default. */
  let showSuppressed = $state(false);
  /** When the analysis last ran; `null` before the first pass. */
  let lastRunAt = $state<string | null>('14:31');
  let running = $state(false);

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
    const key = grouping === 'branch' ? 'branchId' : 'file';
    const buckets = new Map<string, Finding[]>();
    for (const f of visible) {
      const k = String(f[key as keyof Finding] ?? '—');
      const list = buckets.get(k);
      if (list) list.push(f);
      else buckets.set(k, [f]);
    }
    return [...buckets.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([k, items]) => ({ key: k, label: k, items }));
  });

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

    setGrouping(g: FindingGrouping) { grouping = g; },
    setFilter(v: string) { filter = v; },
    toggleSuppressed() { showSuppressed = !showSuppressed; },

    /** Re-run the rules. MOCK: flips the busy flag and re-stamps the time. */
    run() {
      running = true;
      setTimeout(() => {
        running = false;
        lastRunAt = new Date().toTimeString().slice(0, 5);
      }, 600);
    },

    /** Drop a finding once its corrective patch has been applied. */
    resolve(id: string) {
      findings = findings.filter((f) => f.id !== id);
    },
  };
}

export const consistencyStore = createConsistencyStore();
