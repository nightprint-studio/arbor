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
  import { Play } from 'lucide-svelte';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuConsole from './BennuConsole.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import BennuTestStatusIcon from './BennuTestStatusIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { activeTestStore } from '$lib/stores/bennu/test-runner.svelte';
  import { formatDuration, type TestRow } from '$lib/stores/bennu/test-tree';

  /** The runner for the open project — Maven's or cargo's. This view never learns which: a row
   *  runs itself (`runRow`), because what a row means is the runner's business and this one draws
   *  a two-level Java tree and a four-level Rust one from the same rows. */
  const store = $derived(activeTestStore());
  const root = $derived(projectStore.project?.root ?? '');
  const rows = $derived(store.flatRows);
  const selected = $derived(store.selected);

  /** The detail pane shows a selected row's failure when there is one, and the raw Maven log
   *  otherwise — the log being what you want while the run is still going. */
  const detail = $derived(
    selected && (selected.trace || selected.message || selected.systemOut) ? selected : null,
  );

  // ── running things ─────────────────────────────────────────────────────────

  /** Run whatever a row stands for — a crate, a target, a module, a class, one test. */
  function runRow(row: TestRow) {
    if (!root) return;
    void store.runRow(root, row);
  }

  /** What ▷ on this row would run, in words. */
  function runTip(row: TestRow): string {
    switch (row.kind) {
      case 'crate':  return 'Run every test in this crate';
      case 'target': return 'Run this target';
      case 'module': return 'Run this module';
      case 'class':  return 'Run this class';
      default:       return 'Run this test';
    }
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

  /**
   * Ctrl/Cmd+Enter runs the row under the cursor.
   *
   * The only key this view binds: the shared Tree owns ↑↓←→, Home/End, Enter and Space, which is
   * the whole of IntelliJ's tree map — and it also **virtualises**, which is why the rows are no
   * longer walked by hand here. A 2 000-test workspace was 2 000 DOM rows rebuilt on every result;
   * the widget mounts what fits on screen plus a margin.
   */
  function onRowKey(row: TestRow, e: KeyboardEvent) {
    if (e.key !== 'Enter' || !(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    runRow(row);
  }

  // Discovery is kicked off at the window level (the project tree's context menu needs it before
  // this view has ever been shown), and `discover` is a no-op once a project has been scanned —
  // so there is nothing to do here.
</script>

<div class="tp-body">
  <ResizablePanel direction="horizontal" initialSize={380} minSize={240} maxSize={760}>
    <div class="tp-tree">
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
        <Tree
          nodes={store.rows}
          expandedIds={store.expandedIds}
          onExpandToggle={(id) => store.toggleCollapsed(id)}
          selectedId={store.selectedId}
          onSelect={(node) => store.select(node.id)}
          onActivate={openRow}
          onRowKeydown={onRowKey}
          rowTitle={(node) => node.classname}
          toggleOnClick={false}
          rowHeight={22}
          ariaLabel="Tests"
        >
          {#snippet row({ node }: RowSnippetCtx<TestRow>)}
            <BennuTestStatusIcon status={node.status} />
            <!-- The kind class lives on the label rather than on the widget's row wrapper, so the
                 typography rules stay scoped to this component instead of reaching in globally. -->
            <span
              class="tr-label tl-{node.kind}"
              class:tl-dim={node.status === 'pending' || node.disabled}
            >{node.label}</span>

            {#if node.flaky}
              <span class="tr-tag tag-flaky" use:tooltip={'Failed, then passed on a rerun'}>flaky</span>
            {/if}
            {#if node.tag}<span class="tr-tag tag-note">{node.tag}</span>{/if}
            {#if node.disabled}
              <span class="tr-tag tag-off" use:tooltip={node.disabledReason ?? 'Disabled'}>disabled</span>
            {/if}
            {#if node.counts && node.counts.bad > 0}
              <span class="tr-tag tag-bad">{node.counts.bad}</span>
            {/if}
            {#if node.timeMs !== null}<span class="tr-time">{formatDuration(node.timeMs)}</span>{/if}

            <button
              class="tr-run"
              type="button"
              tabindex="-1"
              disabled={store.running || !root}
              use:tooltip={{ content: runTip(node), shortcut: 'Ctrl+Enter' }}
              aria-label="Run"
              onclick={(e) => { e.stopPropagation(); runRow(node); }}
            >
              <Play size={11} />
            </button>
          {/snippet}
        </Tree>
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
  /* This element IS the scroller, and it has to be: the Tree widget brings no scroller of its own
     — it walks up for the first scrollable ancestor and measures its virtualisation window against
     that. Without one here it would find the document, take the whole window as its viewport, and
     mount all 2 000 rows. */
  .tp-tree { height: 100%; min-height: 0; overflow: auto; }
  .tp-mid {
    height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; color: var(--text-disabled); font-size: var(--font-size-xs);
  }

  /* The row wrapper, the chevron and the indentation are the Tree widget's; everything below styles
     what the row snippet puts inside one. */
  .tr-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Not-run rows recede so the results stand out against them. */
  .tl-dim { color: var(--text-muted); }
  /* A code identifier gets the code font; a container does not. Per KIND and not per depth, because
     the cargo tree is four levels deep and the Java one is two. */
  .tl-case, .tl-module { font-family: var(--font-code); font-size: var(--font-size-xs); }
  /* A crate is the coarsest grouping in a workspace of twenty, so it carries the weight. */
  .tl-crate { font-weight: 600; }
  .tl-target { color: var(--text-secondary); }

  .tr-tag {
    flex-shrink: 0; padding: 0 5px; border-radius: var(--radius-sm);
    font-size: var(--font-size-3xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.03em;
  }
  .tag-flaky { color: var(--warning); background: var(--warning-subtle); }
  .tag-off { color: var(--text-muted); background: var(--bg-overlay); }
  .tag-bad { color: var(--error); background: var(--error-subtle, var(--bg-overlay)); }
  /* `async`, `bench`, `should panic`, an ignore reason — informational, so it must not shout. */
  .tag-note { color: var(--text-muted); background: var(--bg-overlay); text-transform: none; font-weight: 500; }
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
  /* The hover/selected state lives on the widget's row element, so reaching it needs `:global` —
     written as one global sequence, which is the only placement Svelte accepts. */
  :global(.tree-row:hover) .tr-run,
  :global(.tree-row[aria-selected='true']) .tr-run,
  .tr-run:focus-visible { opacity: 1; }
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
