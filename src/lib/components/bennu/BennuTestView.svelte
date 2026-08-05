<script lang="ts">
  /**
   * The test run itself — the tree on the left, what happened on the right.
   *
   * The **body** only. The chrome around it belongs to the Run console now, which is where a
   * test run lives: running the tests is running something, and giving it a tool window of its
   * own meant two panels with the same lifecycle, the same Stop button and — once the console
   * learned to interpret and virtualise its output — two different qualities of transcript for
   * the same kind of text.
   *
   * The layout is IntelliJ's because the job is IntelliJ's: while a suite runs you watch a tree
   * fill in, and the moment something goes red you want the trace without losing your place in
   * the tree. Two panes, not two tabs.
   *
   * **The tree is also the launcher.** Before anything has run it lists what the project
   * declares, greyed out, and every node can be run from where it sits — a class, a single
   * method, or the lot. A view that is empty until you have already found some other way to
   * start a run is a view that arrives one step too late; it is also why the Tests tab is
   * always there rather than appearing after the first run.
   *
   * Data and lifecycle live in {@link bennuTestStore}; this is presentation plus the keyboard
   * map. The header's buttons are {@link BennuTestActions}, its verdict {@link BennuTestSummary}.
   */
  import { ChevronDown, ChevronRight, Play } from 'lucide-svelte';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuConsole from './BennuConsole.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import BennuTestStatusIcon from './BennuTestStatusIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import {
    bennuTestStore, formatDuration, baseMethodName, type TestRow,
  } from '$lib/stores/bennu/tests.svelte';

  const store = bennuTestStore;
  const root = $derived(projectStore.project?.root ?? '');
  const rows = $derived(store.flatRows);
  const selected = $derived(store.selected);

  /** The detail pane shows a selected row's failure when there is one, and the raw Maven log
   *  otherwise — the log being what you want while the run is still going. */
  const detail = $derived(
    selected && (selected.trace || selected.message || selected.systemOut) ? selected : null,
  );

  // ── running things ─────────────────────────────────────────────────────────

  /** Run whatever a row stands for: a whole class, or one method. The row carries its own
   *  Surefire selector, so nothing here has to re-derive one. */
  function runRow(row: TestRow) {
    if (!root) return;
    if (row.kind === 'class') void store.runClass(root, row.selector);
    else void store.runCase(root, row.selector, baseMethodName(row.method ?? row.label));
  }

  /** Open a row's declaration. Only rows discovery could place are openable — a
   *  parameterized invocation falls back to its class's line, which is still the right
   *  place to land. */
  function openRow(row: TestRow) {
    if (!row.file) return;
    void projectStore.openFile(row.file).then(() => {
      if (row.line) bennuUiStore.requestGoto(row.line);
    });
  }

  // ── keyboard ───────────────────────────────────────────────────────────────

  let treeEl = $state<HTMLDivElement | null>(null);

  function move(delta: number) {
    if (!rows.length) return;
    const at = rows.findIndex((r) => r.id === store.selectedId);
    const next = at === -1 ? 0 : Math.max(0, Math.min(rows.length - 1, at + delta));
    store.select(rows[next].id);
    queueMicrotask(() => {
      treeEl?.querySelector('[data-selected="true"]')?.scrollIntoView({ block: 'nearest' });
    });
  }

  /**
   * The tree's keyboard map, IntelliJ's: arrows navigate and fold, Enter goes to the source,
   * Ctrl+Enter runs what is under the cursor. Every one of those is otherwise a double-click
   * or a context menu, and this view is where a keyboard-first session spends its time.
   */
  function onTreeKey(e: KeyboardEvent) {
    const row = selected;
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); move(1); return;
      case 'ArrowUp':   e.preventDefault(); move(-1); return;
      case 'ArrowRight':
        if (row?.kind === 'class' && store.isCollapsed(row.id)) {
          e.preventDefault(); store.toggleCollapsed(row.id);
        } else if (row?.kind === 'class' && row.children.length) {
          e.preventDefault(); move(1);
        }
        return;
      case 'ArrowLeft':
        if (row?.kind === 'class' && !store.isCollapsed(row.id)) {
          e.preventDefault(); store.toggleCollapsed(row.id);
        } else if (row?.kind === 'case' && row.parentId) {
          e.preventDefault(); store.select(row.parentId);
        }
        return;
      case 'Enter':
        if (!row) return;
        e.preventDefault();
        if (e.ctrlKey || e.metaKey) runRow(row);
        else openRow(row);
        return;
      case 'Home': e.preventDefault(); if (rows.length) store.select(rows[0].id); return;
      case 'End':  e.preventDefault(); if (rows.length) store.select(rows[rows.length - 1].id); return;
    }
  }

  // Discovery is kicked off at the window level (the project tree's context menu needs it before
  // this view has ever been shown), and `discover` is a no-op once a project has been scanned —
  // so there is nothing to do here.
</script>

<div class="tp-body">
  <ResizablePanel direction="horizontal" initialSize={380} minSize={240} maxSize={760}>
    <div
      class="tp-tree"
      bind:this={treeEl}
      role="tree"
      aria-label="Tests"
      tabindex="0"
      onkeydown={onTreeKey}
    >
      {#if store.discovering && !rows.length}
        <div class="tp-mid"><Spinner size={16} /><span>Looking for tests…</span></div>
      {:else if !rows.length}
        <div class="tp-mid">
          <EmptyState
            message={store.onlyFailed
              ? 'Nothing failed.'
              : root
                ? 'No tests found in this project.'
                : 'Open a project to run its tests.'}
          />
        </div>
      {:else}
        {#each rows as row (row.id)}
          {@const open = !store.isCollapsed(row.id)}
          <div
            class="tr tr-{row.kind}"
            class:tr-selected={store.selectedId === row.id}
            class:tr-dim={row.status === 'pending' || row.disabled}
            data-selected={store.selectedId === row.id}
            role="treeitem"
            aria-selected={store.selectedId === row.id}
            aria-expanded={row.kind === 'class' ? open : undefined}
            tabindex="-1"
            onclick={() => store.select(row.id)}
            ondblclick={() => openRow(row)}
            onkeydown={(e) => {
              // The tree container owns the arrows; a row only ever sees a key when a
              // click has focused it, and then Enter should do what a double-click does.
              if (e.key !== 'Enter' && e.key !== ' ') return;
              e.preventDefault();
              store.select(row.id);
              if (e.ctrlKey || e.metaKey) runRow(row);
              else openRow(row);
            }}
          >
            {#if row.kind === 'class'}
              <button
                class="tr-twisty"
                type="button"
                tabindex="-1"
                aria-label={open ? 'Collapse' : 'Expand'}
                onclick={(e) => { e.stopPropagation(); store.toggleCollapsed(row.id); }}
              >
                {#if row.children.length}
                  {#if open}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
                {/if}
              </button>
            {:else}
              <span class="tr-indent"></span>
            {/if}

            <BennuTestStatusIcon status={row.status} />
            <span class="tr-label">{row.label}</span>

            {#if row.flaky}<span class="tr-tag tag-flaky" use:tooltip={'Failed, then passed on a rerun'}>flaky</span>{/if}
            {#if row.disabled}
              <span class="tr-tag tag-off" use:tooltip={row.disabledReason ?? 'Disabled'}>disabled</span>
            {/if}
            {#if row.counts && row.counts.bad > 0}
              <span class="tr-tag tag-bad">{row.counts.bad}</span>
            {/if}
            {#if row.timeMs !== null}<span class="tr-time">{formatDuration(row.timeMs)}</span>{/if}

            <button
              class="tr-run"
              type="button"
              tabindex="-1"
              disabled={store.running || !root}
              use:tooltip={{ content: row.kind === 'class' ? 'Run this class' : 'Run this test', shortcut: 'Ctrl+Enter' }}
              aria-label="Run"
              onclick={(e) => { e.stopPropagation(); runRow(row); }}
            >
              <Play size={11} />
            </button>
          </div>
        {/each}
      {/if}
    </div>
  </ResizablePanel>

  <div class="tp-detail">
    {#if detail}
      <div class="td-head">
        <BennuTestStatusIcon status={detail.status} />
        <span class="td-name">{detail.classname}{detail.method ? `.${detail.method}` : ''}</span>
        {#if detail.timeMs !== null}<span class="td-time">{formatDuration(detail.timeMs)}</span>{/if}
        <button class="td-back" type="button" onclick={() => store.select(null)}>Show output</button>
      </div>
      <div class="td-body">
        {#if detail.errorKind}<div class="td-kind">{detail.errorKind}</div>{/if}
        {#if detail.message}<div class="td-msg">{detail.message}</div>{/if}
        {#if detail.trace}<pre class="td-trace">{detail.trace}</pre>{/if}
        {#if detail.systemOut}
          <div class="td-sub">Standard output</div>
          <pre class="td-trace td-out">{detail.systemOut}</pre>
        {/if}
      </div>
    {:else if store.lines.length}
      <BennuConsole lines={store.lines} emptyMessage="No test output yet." />
    {:else}
      <div class="tp-mid">
        <EmptyState message="Select a test to see its output, or press ▷ to run." />
      </div>
    {/if}
  </div>
</div>

<style>
  .tp-body { flex: 1; min-height: 0; display: flex; align-items: stretch; overflow: hidden; }

  /* Tree */
  .tp-tree { height: 100%; overflow: auto; padding: 3px 0; outline: none; }
  .tp-tree:focus-visible { box-shadow: inset 0 0 0 1px var(--accent); }
  .tp-mid {
    height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; color: var(--text-disabled); font-size: var(--font-size-xs);
  }

  .tr {
    display: flex; align-items: center; gap: 6px;
    padding: 2px 8px 2px 4px; min-height: 22px;
    font-size: var(--font-size-sm); color: var(--text-primary);
    cursor: default; user-select: none;
  }
  .tr:hover { background: var(--bg-hover); }
  .tr-selected, .tr-selected:hover { background: var(--accent-subtle); }
  /* Not-run rows recede so the results stand out against them. */
  .tr-dim .tr-label { color: var(--text-muted); }
  .tr-case { padding-left: 20px; }
  .tr-case .tr-label { font-family: var(--font-code); font-size: var(--font-size-xs); }

  .tr-twisty, .tr-indent { width: 14px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; }
  .tr-twisty { background: none; border: none; padding: 0; color: var(--text-muted); cursor: pointer; }
  .tr-twisty:hover { color: var(--text-primary); }
  .tr-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .tr-tag {
    flex-shrink: 0; padding: 0 5px; border-radius: var(--radius-sm);
    font-size: var(--font-size-3xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.03em;
  }
  .tag-flaky { color: var(--warning); background: var(--warning-subtle); }
  .tag-off { color: var(--text-muted); background: var(--bg-overlay); }
  .tag-bad { color: var(--error); background: var(--error-subtle, var(--bg-overlay)); }
  .tr-time { flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }

  /* The per-row run button appears on hover / selection: always-on it would put a column of
     play triangles down a tree whose job is to be read. */
  .tr-run {
    flex-shrink: 0; display: flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; padding: 0;
    background: none; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer; opacity: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast);
  }
  .tr:hover .tr-run, .tr-selected .tr-run, .tr-run:focus-visible { opacity: 1; }
  .tr-run:hover:not(:disabled) { color: var(--success); background: var(--bg-hover); }
  .tr-run:disabled { opacity: 0; }

  /* Detail / log pane */
  /* No border of its own: ResizablePanel's handle already draws the divider between the two,
     and a second line beside it reads as a seam. */
  .tp-detail { flex: 1; min-width: 0; display: flex; flex-direction: column; min-height: 0; }
  .td-head {
    display: flex; align-items: center; gap: 7px; flex-shrink: 0;
    padding: 6px 10px; border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
  }
  .td-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-code); color: var(--text-secondary); }
  .td-time { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }
  .td-back { background: none; border: none; cursor: pointer; color: var(--text-muted); font-size: var(--font-size-2xs); padding: 1px 6px; border-radius: var(--radius-sm); }
  .td-back:hover { color: var(--accent); background: var(--bg-hover); }

  .td-body { flex: 1; min-height: 0; overflow: auto; padding: 8px 10px; }
  .td-kind { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--error); font-weight: 600; }
  .td-msg { margin-top: 3px; font-size: var(--font-size-sm); color: var(--text-primary); white-space: pre-wrap; word-break: break-word; }
  .td-sub { margin-top: 12px; font-size: var(--font-size-2xs); text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); }
  .td-trace {
    margin: 8px 0 0; padding: 0;
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.55;
    color: var(--text-secondary); white-space: pre-wrap; word-break: break-word;
  }
  .td-out { color: var(--text-muted); }
</style>
