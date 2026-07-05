<script lang="ts">
  /**
   * Build (bottom tool window section) — the compile/run console.
   *
   * Shows the parsed compiler diagnostics (clickable → open the file + jump to the
   * line) followed by the raw streamed log (`arbor://bennu/build-output` +
   * `run-output`). Data + lifecycle live in {@link bennuRunStore}; this is pure
   * presentation. The dock owns the header (tab switcher + Stop/Rerun actions), so
   * this renders body-only.
   */
  import { CircleAlert, AlertTriangle, Info, Hammer, CircleCheckBig, ListChecks, ArrowRight } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore, formatMs } from '$lib/stores/bennu/run.svelte';
  import type { BuildDiagnostic } from '$lib/types/bennu';

  const diags = $derived(bennuRunStore.diagnostics);
  const lines = $derived(bennuRunStore.lines);
  const building = $derived(bennuRunStore.building);
  const running = $derived(bennuRunStore.running);
  const ok = $derived(bennuRunStore.ok);

  const errorCount = $derived(diags.filter((d) => d.severity === 'error').length);
  const warnCount = $derived(diags.filter((d) => d.severity === 'warning').length);

  // ── project validation (the split-button's `validate` build type) ──────────────
  const validating = $derived(bennuRunStore.validating);
  const validateProgress = $derived(bennuRunStore.validateProgress);
  const vres = $derived(bennuRunStore.validationResult);
  const validatePct = $derived(
    validateProgress && validateProgress.total > 0
      ? Math.round((validateProgress.done / validateProgress.total) * 100)
      : 0,
  );
  // A plain-language verdict on the per-file speed (validation-without-compiling is meant to be
  // near-instant — a Maven compile of the same files would be seconds).
  const speed = $derived.by(() => {
    if (!vres || vres.total_files === 0) return null;
    const avg = vres.avg_ms;
    if (avg < 10) return { label: 'Fast', tone: 'ok' };
    if (avg < 30) return { label: 'Normal', tone: 'mid' };
    return { label: 'Slow', tone: 'warn' };
  });

  // Auto-scroll the log to the bottom as new lines stream in.
  let logEl = $state<HTMLDivElement | null>(null);
  $effect(() => {
    void lines.length;
    const el = logEl;
    if (el) queueMicrotask(() => { el.scrollTop = el.scrollHeight; });
  });

  function iconFor(sev: string) {
    return sev === 'error' ? CircleAlert : sev === 'warning' ? AlertTriangle : Info;
  }

  /** Resolve a compiler-emitted path to an absolute one and open it at the line.
   *  Compiler paths may be absolute or project-relative; join with the root when
   *  relative. Best-effort — a path we can't open just no-ops. */
  function openDiagnostic(d: BuildDiagnostic) {
    if (!d.file) return;
    const root = projectStore.project?.root ?? '';
    const isAbs = /^([a-zA-Z]:[\\/]|[\\/])/.test(d.file);
    const path = isAbs || !root ? d.file : `${root.replace(/[\\/]+$/, '')}/${d.file}`;
    void projectStore.openFile(path).then(() => {
      if (d.line) bennuUiStore.requestGoto(d.line);
    });
  }

  /** Open one of the slowest-files stat rows. */
  function openFileAt(file: string) {
    void projectStore.openFile(file);
  }

  /** The last path segment (file name) of a forward-slashed path. */
  function baseName(p: string): string {
    return p.split('/').pop() ?? p;
  }
</script>

<div class="build">
  {#if !building && !validating && ok === null && !vres && lines.length === 0}
    <div class="build-empty">
      <Hammer size={20} />
      <EmptyState message="Nothing built yet. Press Ctrl+F9 to build/validate, or ▷ to run." />
    </div>
  {:else}
    <!-- Status strip -->
    <div class="build-status">
      {#if building}
        <Spinner size={13} /><span class="st-text">Compiling…</span>
      {:else if validating}
        <Spinner size={13} />
        <span class="st-text">Validating…</span>
        {#if validateProgress && validateProgress.total > 0}
          <span class="st-progress">{validateProgress.done} / {validateProgress.total} ({validatePct}%)</span>
        {/if}
      {:else if running}
        <Spinner size={13} /><span class="st-text">Running…</span>
      {:else if vres}
        <span class="st-ok"><ListChecks size={13} /></span>
        <span class="st-text">Validated {vres.total_files} file(s) · {formatMs(vres.total_ms)}</span>
        {#if speed}<span class="st-speed tone-{speed.tone}">{speed.label}</span>{/if}
      {:else if ok === true}
        <span class="st-ok"><CircleCheckBig size={13} /></span>
        <span class="st-text">Build succeeded{bennuRunStore.tool ? ` · ${bennuRunStore.tool}` : ''}</span>
      {:else if ok === false}
        <span class="st-fail"><CircleAlert size={13} /></span>
        <span class="st-text">Build failed{bennuRunStore.tool ? ` · ${bennuRunStore.tool}` : ''}</span>
      {/if}
      {#if vres}
        <span class="st-counts">{vres.error_count} errors · {vres.warning_count} warnings</span>
      {:else if diags.length}
        <span class="st-counts">{errorCount} errors · {warnCount} warnings</span>
      {/if}
    </div>

    <!-- Validation statistics (the compile-time proxy) -->
    {#if vres && !validating}
      <div class="vstats">
        <div class="vstat"><span class="vstat-k">Files</span><span class="vstat-v">{vres.total_files}</span></div>
        <div class="vstat"><span class="vstat-k">Total</span><span class="vstat-v">{formatMs(vres.total_ms)}</span></div>
        <div class="vstat"><span class="vstat-k">Average</span><span class="vstat-v">{vres.avg_ms.toFixed(1)}ms</span></div>
        <div class="vstat">
          <span class="vstat-k">Slowest</span>
          <span class="vstat-v">{vres.max_ms}ms{vres.max_file ? ` · ${baseName(vres.max_file)}` : ''}</span>
        </div>
      </div>
      {#if vres.total_diagnostics > 0}
        <button class="vproblems" onclick={() => bennuUiStore.showBottom('problems')}>
          <AlertTriangle size={13} />
          <span>{vres.total_diagnostics} problem(s) in {vres.diagnostics.length} file(s)</span>
          <span class="vproblems-go">Open Problems <ArrowRight size={12} /></span>
        </button>
      {/if}
      {#if vres.files.length}
        <div class="vslow">
          <div class="vslow-head">Slowest files</div>
          {#each vres.files.slice(0, 5) as f (f.file)}
            <button class="vslow-row" onclick={() => openFileAt(f.file)} title={f.file}>
              <span class="vslow-name">{baseName(f.file)}</span>
              {#if f.errors || f.warnings}
                <span class="vslow-diags">{f.errors ? `${f.errors}e` : ''}{f.errors && f.warnings ? ' ' : ''}{f.warnings ? `${f.warnings}w` : ''}</span>
              {/if}
              <span class="vslow-ms">{f.ms}ms</span>
            </button>
          {/each}
        </div>
      {/if}
    {/if}

    <!-- Parsed diagnostics (clickable) -->
    {#if diags.length}
      <div class="diag-list">
        {#each diags as d, i (i)}
          {@const Ic = iconFor(d.severity)}
          <button
            class="diag-row"
            onclick={() => openDiagnostic(d)}
            disabled={!d.file}
            title={d.file ?? ''}
          >
            <span class="diag-icon sev-{d.severity}"><Ic size={13} /></span>
            <span class="diag-msg">{d.message}</span>
            {#if d.file}
              <span class="diag-loc">{d.file.split(/[\\/]/).pop()}{d.line ? `:${d.line}` : ''}</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <!-- Raw streamed log -->
    <div class="log" bind:this={logEl}>
      {#each lines as l, i (i)}
        <div class="log-line stream-{l.stream}">{l.text}</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .build { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden; }
  .build-empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 6px; color: var(--text-disabled);
  }

  .build-status {
    display: flex; align-items: center; gap: 7px; flex-shrink: 0;
    padding: 6px 12px; font-size: 11.5px; color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }
  .st-text { font-weight: 500; }
  .st-ok { display: flex; color: var(--success); }
  .st-fail { display: flex; color: var(--error); }
  .st-progress { font-family: var(--font-code); font-size: 10.5px; color: var(--text-muted); }
  .st-counts { margin-left: auto; font-size: 10.5px; color: var(--text-muted); }
  .st-speed {
    font-size: 9.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em;
    padding: 1px 6px; border-radius: var(--radius-sm);
  }
  .st-speed.tone-ok { color: var(--success); background: var(--success-subtle); }
  .st-speed.tone-mid { color: var(--text-secondary); background: var(--bg-overlay); }
  .st-speed.tone-warn { color: var(--warning); background: var(--warning-subtle); }

  .vproblems {
    flex-shrink: 0; display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
    padding: 6px 12px; font-size: 11.5px; cursor: pointer; background: transparent; border: none;
    border-bottom: 1px solid var(--border-subtle); color: var(--error);
    font-family: var(--font-ui-sans); transition: background var(--transition-fast);
  }
  .vproblems:hover { background: var(--bg-hover); }
  .vproblems-go { margin-left: auto; display: inline-flex; align-items: center; gap: 3px; color: var(--text-muted); font-size: 10.5px; }

  /* Validation statistics grid + slowest-files table. */
  .vstats {
    flex-shrink: 0; display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 1px; padding: 8px 12px; border-bottom: 1px solid var(--border-subtle);
  }
  .vstat { display: flex; flex-direction: column; gap: 2px; }
  .vstat-k { font-size: 10px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); }
  .vstat-v { font-size: 12.5px; font-weight: 600; color: var(--text-primary); font-family: var(--font-code); }
  .vslow { flex-shrink: 0; padding: 4px 0 6px; border-bottom: 1px solid var(--border-subtle); }
  .vslow-head { padding: 2px 12px 4px; font-size: 10px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); }
  .vslow-row {
    display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
    padding: 3px 12px; font-size: 11.5px; cursor: pointer; background: transparent; border: none;
    font-family: var(--font-ui-sans); transition: background var(--transition-fast);
  }
  .vslow-row:hover { background: var(--bg-hover); }
  .vslow-name { flex: 1; min-width: 0; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vslow-diags { font-size: 10px; color: var(--warning); flex-shrink: 0; }
  .vslow-ms { font-family: var(--font-code); font-size: 10.5px; color: var(--text-muted); flex-shrink: 0; }

  .diag-list { flex-shrink: 0; max-height: 45%; overflow-y: auto; padding: 3px 0; border-bottom: 1px solid var(--border-subtle); }
  .diag-row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left;
    padding: 4px 12px; font-size: 12px; cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .diag-row:hover:not(:disabled) { background: var(--bg-hover); }
  .diag-row:disabled { cursor: default; }
  .diag-icon { display: flex; flex-shrink: 0; }
  .sev-error { color: var(--error); }
  .sev-warning { color: var(--warning); }
  .sev-note, .sev-info { color: var(--info); }
  .diag-msg { flex: 1; min-width: 0; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diag-loc { font-family: var(--font-code); font-size: 10.5px; color: var(--text-muted); flex-shrink: 0; }

  .log { flex: 1; min-height: 0; overflow-y: auto; padding: 6px 12px; font-family: var(--font-code); font-size: 11.5px; line-height: 1.5; }
  .log-line { white-space: pre-wrap; word-break: break-word; color: var(--text-secondary); }
  .log-line.stream-err { color: var(--error); }
  .log-line.stream-meta { color: var(--text-muted); font-style: italic; }
</style>
