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
  import { CircleAlert, AlertTriangle, Info, Hammer, CircleCheckBig } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import type { BuildDiagnostic } from '$lib/types/bennu';

  const diags = $derived(bennuRunStore.diagnostics);
  const lines = $derived(bennuRunStore.lines);
  const building = $derived(bennuRunStore.building);
  const running = $derived(bennuRunStore.running);
  const ok = $derived(bennuRunStore.ok);

  const errorCount = $derived(diags.filter((d) => d.severity === 'error').length);
  const warnCount = $derived(diags.filter((d) => d.severity === 'warning').length);

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
</script>

<div class="build">
  {#if !building && ok === null && lines.length === 0}
    <div class="build-empty">
      <Hammer size={20} />
      <EmptyState message="Nothing built yet. Press Ctrl+F9 to compile, or ▷ to run." />
    </div>
  {:else}
    <!-- Status strip -->
    <div class="build-status">
      {#if building}
        <Spinner size={13} /><span class="st-text">Compiling…</span>
      {:else if running}
        <Spinner size={13} /><span class="st-text">Running…</span>
      {:else if ok === true}
        <span class="st-ok"><CircleCheckBig size={13} /></span>
        <span class="st-text">Build succeeded{bennuRunStore.tool ? ` · ${bennuRunStore.tool}` : ''}</span>
      {:else if ok === false}
        <span class="st-fail"><CircleAlert size={13} /></span>
        <span class="st-text">Build failed{bennuRunStore.tool ? ` · ${bennuRunStore.tool}` : ''}</span>
      {/if}
      {#if diags.length}
        <span class="st-counts">{errorCount} errors · {warnCount} warnings</span>
      {/if}
    </div>

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
  .st-counts { margin-left: auto; font-size: 10.5px; color: var(--text-muted); }

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
