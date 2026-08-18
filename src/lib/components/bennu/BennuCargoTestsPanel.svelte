<script lang="ts">
  /**
   * The Tests tool window for a Cargo workspace — the **catalogue**, in the right rail.
   *
   * Its Java counterpart ({@link BennuTestsCatalogPanel}) lists classes and methods, and could not
   * be reused: what a Rust test is identified by is four things, not two, and the whole reason this
   * panel earns its space in a workspace of twenty crates is that it groups by the first two.
   *
   * ## Catalogue, not results
   *
   * It draws {@link bennuCargoTestStore}'s tree, which holds the declared tests **before anything
   * has run** and fills with verdicts as a run streams. So this is one view with two jobs — a place
   * to find a test and launch it, and, once a run is on, a live status column. That is deliberate:
   * a catalogue that went blank the moment you used it would send you to the other panel to watch,
   * and a second tree of the same rows is a second thing to keep in sync.
   *
   * The Run console's Tests tab remains where a run is *read* — it has the transcript and the panic
   * output beside the tree. This one is narrow, always available, and filterable.
   *
   * Keyboard: the shared Tree owns ↑↓←→ and Enter, so finding and opening a test needs no mouse;
   * Ctrl+Enter on a row runs it.
   */
  import { ChevronsDownUp, ChevronsUpDown, Play, RefreshCw, Square } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tree, { type RowSnippetCtx, type TreeController } from '$lib/components/shared/ui/Tree.svelte';
  import BennuTestStatusIcon from './BennuTestStatusIcon.svelte';
  import RustTestIcon from './RustTestIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuCargoTestStore } from '$lib/stores/bennu/cargo-tests.svelte';
  import { formatDuration, type TestRow } from '$lib/stores/bennu/test-tree';

  const store = bennuCargoTestStore;
  const root = $derived(projectStore.project?.root ?? '');
  const rows = $derived(store.rows);

  let filter = $state('');

  /** The tree's imperative handle — what the two fold buttons drive. Expansion lives inside the
   *  widget here (the run panel's copy of this tree is the one that persists it in the store), so
   *  "all of it" is a request the widget answers rather than state to mirror. */
  let tree = $state<TreeController | null>(null);

  /** Match on the row's own label **and** on the path it stands for, so typing `parse` finds a
   *  test inside `util::parse` as well as one called `parses`. */
  function match(node: TestRow, q: string): boolean {
    const needle = q.toLowerCase();
    return (
      node.label.toLowerCase().includes(needle) ||
      node.classname.toLowerCase().includes(needle)
    );
  }

  function openRow(row: TestRow) {
    if (!row.file) return;
    void projectStore.openFile(row.file).then(() => {
      if (row.line) bennuUiStore.requestGoto(row.line);
    });
  }

  function runRow(row: TestRow) {
    if (root) void store.runRow(root, row);
  }

  /** Enter opens the source, Ctrl/Cmd+Enter runs — the same pair the run panel's tree uses, so the
   *  two trees are one habit. */
  function onRowKey(row: TestRow, e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    runRow(row);
  }

  const total = $derived(store.discovered.length);
</script>

<PanelShell title="Tests">
  {#snippet icon()}<RustTestIcon size={13} />{/snippet}

  {#snippet actions()}
    {#if store.running}
      <button
        class="ps-btn ps-btn-danger"
        type="button"
        use:tooltip={'Stop'}
        aria-label="Stop the test run"
        onclick={() => void store.stop()}
      ><Square size={12} /></button>
    {:else}
      <button
        class="ps-btn"
        type="button"
        disabled={!root}
        use:tooltip={{ content: 'Run every test in the workspace', shortcut: 'Ctrl+Shift+F5' }}
        aria-label="Run all tests"
        onclick={() => void store.runAll(root)}
      ><Play size={13} /></button>
    {/if}
    <button
      class="ps-btn"
      type="button"
      disabled={!root || store.discovering}
      use:tooltip={'Re-scan for tests'}
      aria-label="Refresh"
      onclick={() => void store.discover(root, true)}
    ><RefreshCw size={13} /></button>
    <!-- Fold the lot. In a workspace of twenty crates the default view is the only one you can
         read, and getting back to it after opening six modules was otherwise twelve clicks. -->
    <button
      class="ps-btn"
      type="button"
      disabled={rows.length === 0}
      use:tooltip={'Collapse all'}
      aria-label="Collapse all"
      onclick={() => tree?.collapseAll()}
    ><ChevronsDownUp size={13} /></button>
    <button
      class="ps-btn"
      type="button"
      disabled={rows.length === 0}
      use:tooltip={'Expand all'}
      aria-label="Expand all"
      onclick={() => tree?.expandAll()}
    ><ChevronsUpDown size={13} /></button>
  {/snippet}

  {#snippet children()}
    <div class="ct">
      <div class="ct-bar">
        <SearchBar
          bind:query={filter}
          showRegex={false}
          showCounter={false}
          placeholder="Filter by test, module or crate…"
        />
      </div>

      {#if store.discovering && total === 0}
        <div class="ct-mid"><Spinner size={14} /><span>Looking for tests…</span></div>
      {:else if rows.length === 0}
        <div class="ct-mid">
          <EmptyState
            message={root
              ? 'No #[test] functions found in this workspace.'
              : 'Open a workspace to see its tests.'}
          />
        </div>
      {:else}
        <div class="ct-tree">
          <Tree
            bind:this={tree}
            nodes={rows}
            {filter}
            {match}
            initialExpanded={(n: TestRow) => n.kind === 'crate'}
            onActivate={openRow}
            onRowKeydown={onRowKey}
            rowTitle={(n: TestRow) => n.classname}
            guides
            ariaLabel="Tests"
          >
            {#snippet row({ node }: RowSnippetCtx<TestRow>)}
              <BennuTestStatusIcon status={node.status} />
              <span class="r-label" class:r-mono={node.kind === 'case' || node.kind === 'module'}>
                {node.label}
              </span>
              {#if node.tag}<span class="r-tag">{node.tag}</span>{/if}
              {#if node.counts && node.counts.bad > 0}
                <span class="r-bad">{node.counts.bad}</span>
              {:else if node.counts}
                <span class="r-count">{node.counts.total}</span>
              {/if}
              {#if node.timeMs !== null}<span class="r-time">{formatDuration(node.timeMs)}</span>{/if}
              <button
                class="r-run"
                type="button"
                tabindex="-1"
                disabled={store.running || !root}
                use:tooltip={{ content: 'Run', shortcut: 'Ctrl+Enter' }}
                aria-label="Run"
                onclick={(e) => { e.stopPropagation(); runRow(node); }}
              ><Play size={11} /></button>
            {/snippet}
          </Tree>
        </div>
      {/if}

      <div class="ct-foot">
        {total} test{total === 1 ? '' : 's'}
        {#if store.compiling}<span class="ct-busy">compiling {store.compiling}…</span>{/if}
      </div>
    </div>
  {/snippet}
</PanelShell>

<style>
  .ct { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .ct-bar { padding: 6px; border-bottom: 1px solid var(--border-subtle); }
  /* This element IS the scroller — the Tree widget has none of its own and measures its
     virtualisation window against the first scrollable ancestor it finds. */
  .ct-tree { flex: 1; min-height: 0; overflow: auto; }
  .ct-mid {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; color: var(--text-disabled); font-size: var(--font-size-xs);
  }
  .ct-foot {
    display: flex; align-items: center; gap: 8px; flex-shrink: 0;
    padding: 4px 8px; border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .ct-busy { color: var(--text-secondary); }

  .r-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .r-mono { font-family: var(--font-code); font-size: var(--font-size-xs); }
  .r-tag {
    flex-shrink: 0; padding: 0 4px; border-radius: var(--radius-sm);
    font-size: var(--font-size-3xs); color: var(--text-muted); background: var(--bg-overlay);
  }
  .r-count, .r-bad, .r-time {
    flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-3xs);
  }
  .r-count { color: var(--text-disabled); }
  .r-bad { color: var(--error); font-weight: 700; }
  .r-time { color: var(--text-muted); }
  /* On hover / selection only: a column of play triangles down a tree whose job is to be read
     is the thing the run panel's tree avoids too. */
  .r-run {
    flex-shrink: 0; display: flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; padding: 0;
    background: none; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer; opacity: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast);
  }
  /* The hover/selected state lives on the widget's row element, so reaching it needs `:global`. */
  :global(.tree-row:hover) .r-run,
  :global(.tree-row[aria-selected='true']) .r-run,
  .r-run:focus-visible { opacity: 1; }
  .r-run:hover:not(:disabled) { color: var(--success); background: var(--bg-hover); }
  .r-run:disabled { opacity: 0; }
</style>
