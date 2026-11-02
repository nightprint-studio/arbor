<script lang="ts">
  /**
   * Problems — parser / eval diagnostics, with a text search (Ctrl+F focuses it)
   * and severity filter chips (errors / warnings). Click (or Enter) a row to jump
   * the editor to the diagnostic's source span.
   */
  import { AlertTriangle, CircleAlert, CircleCheckBig } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { listNav } from '$lib/components/shared/ui/list-nav';
  import { merulaStore } from '../merula-store.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { diagnosticsStore } from '../stores/engine.svelte';
  import { keyStore } from '../stores/key.svelte';
  import { clipLintStore } from '../stores/clip-lint.svelte';
  import { levelAnalysisStore } from '../stores/level-analysis.svelte';
  import { makeByteToU16 } from '../editor/merula-lang';
  import type { MerulaDiagnostic } from '$lib/ipc/merula/merula';

  let query = $state('');
  let searchEl = $state<HTMLInputElement | null>(null);
  let showError = $state(true);
  let showWarning = $state(true);

  $effect(() => {
    if (merulaStore.findPending) { searchEl?.focus(); searchEl?.select(); merulaStore.clearFind(); }
  });

  // Editor lint that lives client-side (out-of-scale notes, clip-risk gain, offline
  // level analysis) shows as editor underlines — surface it in Problems too, as
  // warnings, so the panel is the single place to see everything the editor flags.
  // Deduped by span+message (the off-scale + clip passes can't overlap, but a span
  // could repeat across stores).
  const clientWarnings = $derived.by<MerulaDiagnostic[]>(() => {
    const seen = new Set<string>();
    const out: MerulaDiagnostic[] = [];
    for (const m of [...keyStore.offScale, ...clipLintStore.marks, ...levelAnalysisStore.marks]) {
      const k = `${m.from}:${m.to}:${m.message}`;
      if (seen.has(k)) continue;
      seen.add(k);
      out.push({ message: m.message, severity: 'warning', start: m.from, end: m.to });
    }
    return out;
  });

  // 'info' is folded into the "warnings" bucket (a third minor category with no
  // dedicated chip yet) so every diagnostic is reachable.
  const problems = $derived([...diagnosticsStore.errors, ...clientWarnings]);
  const isError  = (sev: string) => sev === 'error';
  const errorCount = $derived(problems.filter(p => isError(p.severity)).length);
  const warnCount  = $derived(problems.filter(p => !isError(p.severity)).length);

  /** Byte-range location label (the editor/tree-sitter mapping isn't here yet). */
  function loc(start: number | null, end: number | null): string {
    if (start === null) return '';
    return end !== null && end !== start ? `@${start}–${end}` : `@${start}`;
  }

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return problems.filter(p =>
      (isError(p.severity) ? showError : showWarning) &&
      (!q || p.message.toLowerCase().includes(q)),
    );
  });

  /** Jump the editor to a diagnostic's span. The backend reports byte offsets;
   *  the editor wants UTF-16 — convert against the active source (the eval'd
   *  file). The relay then scrolls + selects there. */
  function jumpTo(p: MerulaDiagnostic) {
    if (p.start == null) return;
    const offset = makeByteToU16(projectStore.activeSource)(p.start);
    merulaStore.requestGoto(offset, 1);
  }
</script>

<div class="prob" use:listNav={{ fromInput: '.prob-toolbar input' }}>
  <BottomPanelHeader title="Problems" onClose={() => merulaStore.toggleBottom('problems')}>
    {#snippet icon()}<AlertTriangle size={13} />{/snippet}
    {#snippet children()}<span class="prob-meta">{visible.length}/{problems.length}</span>{/snippet}
  </BottomPanelHeader>

  <div class="prob-toolbar">
    <FilterInput bind:value={query} bind:element={searchEl} placeholder="Search… (Ctrl+F)" />
    <div class="prob-filters">
      <button class="sev-chip sev-error" class:off={!showError} onclick={() => showError = !showError} use:tooltip={'Errors'} aria-pressed={showError}>
        <CircleAlert size={11} /> {errorCount}
      </button>
      <button class="sev-chip sev-warning" class:off={!showWarning} onclick={() => showWarning = !showWarning} use:tooltip={'Warnings'} aria-pressed={showWarning}>
        <AlertTriangle size={11} /> {warnCount}
      </button>
    </div>
  </div>

  <div class="prob-body">
    {#if problems.length === 0}
      <div class="prob-clear-state"><CircleCheckBig size={14} /> No problems detected</div>
    {:else if visible.length === 0}
      <div class="prob-empty">{query ? `No problems match “${query}”.` : 'No problems for the current filter.'}</div>
    {:else}
      {#each visible as p, i (i)}
        {@const sevClass = isError(p.severity) ? 'error' : 'warning'}
        <button class="prob-line" data-list-item onclick={() => jumpTo(p)} disabled={p.start == null} use:tooltip={p.start != null ? 'Jump to source' : ''}>
          <span class="prob-icon sev-{sevClass}">
            {#if isError(p.severity)}<CircleAlert size={13} />{:else}<AlertTriangle size={13} />{/if}
          </span>
          <span class="prob-msg">{p.message}</span>
          <span class="prob-loc">{loc(p.start, p.end)}</span>
        </button>
      {/each}
    {/if}
  </div>
</div>

<style>
  .prob { display: flex; flex-direction: column; height: 100%; background: var(--bg-base); }
  .prob-meta { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .prob-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 10px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  .prob-filters { display: flex; gap: 4px; flex-shrink: 0; }
  .sev-chip {
    display: flex; align-items: center; gap: 4px;
    height: 24px; padding: 0 7px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: var(--bg-input); cursor: pointer;
    font-size: var(--font-size-xs); font-weight: 600; font-variant-numeric: tabular-nums;
    transition: opacity var(--transition-fast);
  }
  .sev-chip.off { opacity: 0.35; }
  .sev-error { color: var(--error); border-color: color-mix(in srgb, var(--error) 40%, var(--border-subtle)); }
  .sev-warning { color: var(--warning); border-color: color-mix(in srgb, var(--warning) 40%, var(--border-subtle)); }

  .prob-body { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; }
  .prob-clear-state {
    display: flex; align-items: center; gap: 6px;
    padding: 14px 16px; color: var(--success); font-size: var(--font-size-sm);
  }
  .prob-empty { padding: 14px 16px; font-size: var(--font-size-xs); color: var(--text-muted); font-style: italic; }
  .prob-line {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left;
    padding: 5px 12px; font-size: var(--font-size-sm); cursor: pointer;
    background: transparent; border: none;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .prob-line:hover:not(:disabled) { background: var(--bg-hover); }
  .prob-line:disabled { cursor: default; }
  .prob-icon { display: flex; flex-shrink: 0; }
  .sev-error { color: var(--error); }
  .sev-warning { color: var(--warning); }
  .prob-msg { flex: 1; min-width: 0; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .prob-loc { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); flex-shrink: 0; }
</style>
