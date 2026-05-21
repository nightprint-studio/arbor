<script lang="ts">
  /**
   * StatusList — itemised summary widget.
   *
   *   • Header row: scan-state / "all clean" / "N of M need attention" plus
   *     auto-derived per-severity summary pills.
   *   • Body: scrollable list of rows, each with a label + a flexible row of
   *     severity-coded status chips.
   *   • Optional footnote line below the list.
   *
   * Designed for "preview before bulk action" panels (workspace bulk ops, batch
   * imports, validation reports) — anywhere you have a list of items and each
   * one carries zero or more categorised flags. The diagnostic case
   * (RepoHealth → blocks/warnings) is one of these uses; the widget itself is
   * domain-agnostic.
   *
   *   <StatusList
   *     items={items}
   *     totalCount={total}
   *     noun={{ singular: 'repository', plural: 'repositories' }}
   *     footnote="Blocked items are skipped." />
   */
  import { AlertTriangle, Loader } from 'lucide-svelte';
  import type { Component } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';

  export type Severity = 'block' | 'warn' | 'info' | 'success';

  export interface StatusChip {
    severity: Severity;
    /** Optional leading icon (lucide component). */
    icon?: Component<any> | typeof AlertTriangle;
    text:  string;
  }

  export interface StatusItem {
    id:    string;
    label: string;
    chips: StatusChip[];
  }

  interface Noun {
    singular: string;
    plural:   string;
  }

  interface Props {
    items:           StatusItem[];
    /** Total number of items considered (≥ items with chips). Used in the
     *  header's "N of M" / "All M …" message. Falls back to items.length. */
    totalCount?:     number;
    scanning?:       boolean;
    scanningLabel?:  string;
    /** Override the default "All N <noun.plural> look clean." message. */
    cleanLabel?:     string;
    noun?:           Noun;
    footnote?:       string;
    /** Max height of the scrolling list in pixels. */
    maxListHeight?:  number;
    /** Override the default per-severity pill labels. Keys are the severity
     *  strings; the renderer interpolates the count via {n}. */
    severityLabels?: Partial<Record<Severity, { singular: string; plural: string }>>;
  }

  let {
    items,
    totalCount,
    scanning       = false,
    scanningLabel  = 'Scanning…',
    cleanLabel,
    noun           = { singular: 'item', plural: 'items' },
    footnote,
    maxListHeight  = 160,
    severityLabels,
  }: Props = $props();

  const total      = $derived(totalCount ?? items.length);
  const issuedRows = $derived(items.filter(i => i.chips.length > 0));
  const issued     = $derived(issuedRows.length);

  // Aggregate per severity: how many items carry at least one chip of that
  // severity. Matches the way "X blocked / Y warnings" pills read.
  const summary = $derived((() => {
    const counts: Record<Severity, number> = { block: 0, warn: 0, info: 0, success: 0 };
    for (const it of items) {
      const seen: Set<Severity> = new Set();
      for (const c of it.chips) {
        if (!seen.has(c.severity)) {
          seen.add(c.severity);
          counts[c.severity]++;
        }
      }
    }
    return counts;
  })());

  const defaultSeverityLabels: Record<Severity, { singular: string; plural: string }> = {
    block:   { singular: 'blocked',  plural: 'blocked' },
    warn:    { singular: 'warning',  plural: 'warnings' },
    info:    { singular: 'note',     plural: 'notes' },
    success: { singular: 'passed',   plural: 'passed' },
  };

  function pillLabel(sev: Severity, n: number): string {
    const m = severityLabels?.[sev] ?? defaultSeverityLabels[sev];
    return `${n} ${n === 1 ? m.singular : m.plural}`;
  }

  // Order severities so the most urgent reads first.
  const SEVERITY_ORDER: Severity[] = ['block', 'warn', 'info', 'success'];
</script>

<div class="status-list" class:has-issues={issued > 0}>
  <div class="header">
    <span class="title">
      {#if scanning}
        <Loader size={12} class="spin" /> {scanningLabel}
      {:else if issued === 0}
        {cleanLabel ?? `All ${total} ${total === 1 ? noun.singular : noun.plural} look clean.`}
      {:else}
        <AlertTriangle size={12} />
        {issued} of {total}
        {issued === 1 ? `${noun.singular} needs` : `${noun.plural} need`} your attention
      {/if}
    </span>

    {#if !scanning && issued > 0}
      <span class="summary">
        {#each SEVERITY_ORDER as sev}
          {#if summary[sev] > 0}
            <span class="pill pill-{sev}">{pillLabel(sev, summary[sev])}</span>
          {/if}
        {/each}
      </span>
    {/if}
  </div>

  {#if !scanning && issued > 0}
    <div class="rows" style="max-height: {maxListHeight}px">
      {#each issuedRows as it (it.id)}
        <div class="row">
          <span class="row-label" use:tooltip={it.label}>{it.label}</span>
          <span class="row-chips">
            {#each it.chips as c}
              {@const Icon = c.icon}
              <span class="chip chip-{c.severity}">
                {#if Icon}<Icon size={10} />{/if}
                {c.text}
              </span>
            {/each}
          </span>
        </div>
      {/each}
    </div>
    {#if footnote}
      <div class="foot">{footnote}</div>
    {/if}
  {/if}
</div>

<style>
  .status-list {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 14px;
  }
  .status-list.has-issues {
    border-color: color-mix(in srgb, var(--warning) 45%, var(--border-subtle));
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }
  .summary { display: inline-flex; gap: 6px; }

  .pill {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: var(--radius-md);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .pill-block   { color: var(--error);   background: var(--error-subtle); }
  .pill-warn    { color: var(--warning); background: color-mix(in srgb, var(--warning) 16%, transparent); }
  .pill-info    { color: var(--info);    background: color-mix(in srgb, var(--info) 14%, transparent); }
  .pill-success { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); }

  .rows {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    padding-right: 2px;
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 0;
    border-top: 1px solid var(--border-subtle);
  }
  .row:first-child { border-top: none; padding-top: 6px; }

  .row-label {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-primary);
    flex-shrink: 0;
    min-width: 110px;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10.5px;
    padding: 1px 6px 1px 5px;
    border-radius: var(--radius-md);
    line-height: 14px;
  }
  .chip-block   { color: var(--error);   background: var(--error-subtle); }
  .chip-warn    { color: var(--warning); background: color-mix(in srgb, var(--warning) 16%, transparent); }
  .chip-info    { color: var(--info);    background: color-mix(in srgb, var(--info) 14%, transparent); }
  .chip-success { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); }

  .foot {
    margin-top: 8px;
    font-size: 10.5px;
    color: var(--text-muted);
    line-height: 1.4;
  }
</style>
