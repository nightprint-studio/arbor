<script lang="ts">
  /**
   * Single row in the security detail modal.
   *
   * Layout (left → right):
   *   [severity pill] [title + meta line]                [age] [state] [↗]
   *
   * Click → `onSelect(finding)`. The parent modal owns navigation: typically
   * it opens `SecurityFindingDetailModal` with the full description rendered
   * as markdown. The trailing `↗` chip is rendered when `web_url` exists so
   * the user knows the upstream provider page is reachable from the detail
   * view.
   */
  import { ExternalLink, FileCode2, Bug, Tag } from 'lucide-svelte';
  import { SEVERITY_META } from './severity-meta';
  import type { SecurityFinding } from '$lib/types/security';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    finding:   SecurityFinding;
    onSelect?: (finding: SecurityFinding) => void;
  }

  let { finding, onSelect }: Props = $props();

  const sev = $derived(SEVERITY_META[finding.severity]);

  const fileLabel = $derived.by(() => {
    if (!finding.file_path) return null;
    return finding.start_line != null
      ? `${finding.file_path}:${finding.start_line}`
      : finding.file_path;
  });

  const ageLabel = $derived.by(() => {
    const d = finding.age_days;
    if (d < 1)   return '<1d';
    if (d < 30)  return `${d}d`;
    if (d < 365) return `${Math.round(d / 30)}mo`;
    return `${(d / 365).toFixed(1)}y`;
  });

  const stateMeta = $derived.by(() => {
    switch (finding.state) {
      case 'detected':  return { label: 'Detected',  color: 'var(--text-secondary)' };
      case 'confirmed': return { label: 'Confirmed', color: 'var(--warning)'        };
      case 'resolved':  return { label: 'Resolved',  color: 'var(--success)'        };
      case 'dismissed': return { label: 'Dismissed', color: 'var(--text-muted)'     };
    }
  });

  const cveIds = $derived(
    finding.identifiers.filter(i => i.kind.toLowerCase() === 'cve').slice(0, 2),
  );

  function open() {
    onSelect?.(finding);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<button
  class="row clickable"
  type="button"
  onclick={open}
  style:--sev-color={sev.color}
  style:--sev-bg={sev.bgColor}
  use:tooltip={finding.title}
>
  <span class="sev-pill">{sev.label}</span>

  <div class="body">
    <span class="title">{finding.title}</span>
    <div class="meta">
      {#if fileLabel}
        <span class="meta-chip file" use:tooltip={fileLabel}>
          <FileCode2 size={10} />
          <span class="file-text">{fileLabel}</span>
        </span>
      {/if}
      {#if finding.scanner}
        <span class="meta-chip">
          <Bug size={10} />
          {finding.scanner}
        </span>
      {/if}
      {#if finding.report_type}
        <span class="meta-chip type">{finding.report_type}</span>
      {/if}
      {#each cveIds as id (id.value)}
        <span class="meta-chip cve">
          <Tag size={10} />
          {id.value}
        </span>
      {/each}
    </div>
  </div>

  <span class="right">
    <span class="age" use:tooltip={'Age'}>{ageLabel}</span>
    <span class="state" style:color={stateMeta.color}>{stateMeta.label}</span>
    {#if finding.web_url}<ExternalLink size={11} class="ext" />{/if}
  </span>
</button>

<style>
  .row {
    display: grid;
    grid-template-columns: 84px 1fr auto;
    align-items: center;
    gap: 12px;
    width: 100%;
    /* Fixed height so the parent virtualizer can compute offsets without
       measuring each row.  Meta line below truncates instead of wrapping
       to keep this stable. */
    height: 64px;
    padding: 0 14px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    text-align: left;
    cursor: default;
    transition: background var(--transition-fast);
    box-sizing: border-box;
  }
  .row.clickable { cursor: pointer; }
  .row.clickable:hover { background: var(--bg-hover); }
  .row:disabled { color: inherit; }

  .sev-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    background: var(--sev-bg);
    color: var(--sev-color);
    border: 1px solid color-mix(in srgb, var(--sev-color) 35%, transparent);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    line-height: 1;
    height: 18px;
    align-self: center;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }
  .title {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    flex-wrap: nowrap;
    gap: 4px 6px;
    font-size: 10px;
    color: var(--text-muted);
    overflow: hidden;
    /* Single-line, anything past the viewport edge is clipped. The full
       value is still on the row's `title` attribute via the per-chip
       `title` so users can hover to see truncated paths. */
    min-width: 0;
  }
  .meta > .meta-chip { flex-shrink: 0; }
  .meta > .meta-chip.file { flex-shrink: 1; min-width: 0; }
  .meta-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    line-height: 1.4;
    max-width: 100%;
  }
  .meta-chip.file { font-family: var(--font-code); }
  .file-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 320px;
  }
  .meta-chip.type {
    color: var(--accent);
    background: var(--accent-subtle);
    border-color: color-mix(in srgb, var(--accent) 25%, transparent);
    text-transform: lowercase;
  }
  .meta-chip.cve {
    font-family: var(--font-code);
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    border-color: color-mix(in srgb, var(--warning) 28%, transparent);
  }

  .right {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    align-self: center;
    flex-shrink: 0;
    font-size: 10px;
    color: var(--text-muted);
  }
  .age {
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
  }
  .state {
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-weight: 600;
  }
  :global(.row .ext) { color: var(--text-muted); }
</style>
