<script lang="ts">
  /**
   * Tests (right tool window) — every test the project **declares**, as a list you can sort
   * and filter, with a ▷ on each row.
   *
   * ## Why this is not the Run console
   *
   * There are two different things here and they were one panel. *What tests exist* is a
   * property of the sources: it is stable, you browse it, you sort it, and it is how you start
   * a run. *What a run did* is an event: it has a transcript, a Stop button and an outcome, and
   * it belongs beside the other things you have launched — which is where it is now, as a tab
   * of the Run console that appears when there is a run and closes with it.
   *
   * Merging them meant a permanent tab in a strip whose every other tab was a run, which is
   * what made it read wrong.
   *
   * ## The list
   *
   * Flat and sortable rather than a tree, because that is what a catalogue is for: "which test
   * classes are in this module", "which ones are TestNG", "where is the one whose name I half
   * remember". A class expands to its methods so a single one can be run without leaving the
   * panel. Sorting and filtering are session state — a view of a list, not a preference.
   *
   * Discovery itself lives in {@link bennuTestStore}; it is kicked off at the window level and
   * is a no-op once a project has been scanned.
   */
  import {
    ArrowDownAZ, Check, ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, Play, RefreshCw,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { testIcon } from './test-icon';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuTestStore, baseMethodName } from '$lib/stores/bennu/tests.svelte';
  import type { DiscoveredTest } from '$lib/types/bennu';

  const store = bennuTestStore;
  const root = $derived(projectStore.project?.root ?? '');

  /** How the list is ordered. Each is a question someone actually arrives with. */
  type Sort = 'name' | 'package' | 'module' | 'framework' | 'size';
  const SORT_LABELS: Record<Sort, string> = {
    name: 'Name',
    package: 'Package',
    module: 'Module',
    framework: 'Framework',
    size: 'Most tests',
  };

  let sort = $state<Sort>('name');

  /** The menu, with the current choice ticked — a picker that does not say what it picked is
   *  a picker you have to open to read. */
  const sortItems = $derived<DropdownItem[]>(
    (Object.keys(SORT_LABELS) as Sort[]).map((id) => ({
      id,
      label: SORT_LABELS[id],
      icon: id === sort ? Check : undefined,
      onclick: () => (sort = id),
    })),
  );
  let filter = $state('');
  let expanded = $state<Set<string>>(new Set());

  /** The class's own name, without its package. */
  function simpleOf(t: DiscoveredTest): string {
    const cut = t.fqcn.lastIndexOf('.');
    return cut < 0 ? t.fqcn : t.fqcn.slice(cut + 1);
  }

  /** Matched against the class, its package and its module — someone filtering by `order`
   *  means any of the three, and asking which they meant is a question with no good answer. */
  function matches(t: DiscoveredTest, needle: string): boolean {
    if (!needle) return true;
    const hay = `${t.fqcn} ${t.module ?? ''} ${t.framework}`.toLowerCase();
    return hay.includes(needle);
  }

  const shown = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    const list = store.discovered.filter((t) => matches(t, needle));
    const by: Record<Sort, (a: DiscoveredTest, b: DiscoveredTest) => number> = {
      name: (a, b) => simpleOf(a).localeCompare(simpleOf(b)),
      package: (a, b) => a.fqcn.localeCompare(b.fqcn),
      module: (a, b) => (a.module ?? '').localeCompare(b.module ?? '') || a.fqcn.localeCompare(b.fqcn),
      framework: (a, b) => a.framework.localeCompare(b.framework) || a.fqcn.localeCompare(b.fqcn),
      // Descending: "most tests" is the question, and a class with one is not the answer.
      size: (a, b) => b.methods.length - a.methods.length || a.fqcn.localeCompare(b.fqcn),
    };
    return [...list].sort(by[sort]);
  });

  function toggle(fqcn: string) {
    const next = new Set(expanded);
    if (next.has(fqcn)) next.delete(fqcn);
    else next.add(fqcn);
    expanded = next;
  }

  /** Open every class that has methods — over the **shown** list, so it follows the filter: with
   *  `order` typed in, "expand all" means the ones you can see. Classes with no methods are left
   *  out because there is nothing under them to reveal. */
  function expandAll() {
    expanded = new Set(shown.filter((t) => t.methods.length).map((t) => t.fqcn));
  }

  function collapseAll() {
    expanded = new Set();
  }

  /** Open the declaration. Clicking a row reads it; ▷ runs it — the same split the project
   *  tree uses, so neither is a surprise. */
  function open(file: string, line: number) {
    void projectStore.openFile(file).then(() => {
      if (line) bennuUiStore.requestGoto(line);
    });
  }

  function runClass(t: DiscoveredTest) {
    if (root) void store.runClass(root, t.selector);
  }

  function runMethod(t: DiscoveredTest, method: string) {
    if (root) void store.runCase(root, t.selector, baseMethodName(method));
  }
</script>

<PanelShell title="Tests">
  {#snippet icon()}{@const TestIcon = testIcon()}<TestIcon size={13} />{/snippet}
  {#snippet actions()}
    <Dropdown items={sortItems} position="fixed" direction="down">
      {#snippet trigger()}
        <span class="tc-sort" use:tooltip={`Sort — ${SORT_LABELS[sort]}`}>
          <ArrowDownAZ size={13} />
        </span>
      {/snippet}
    </Dropdown>
    <button
      class="ps-btn"
      type="button"
      disabled={!root || store.discovering}
      use:tooltip={'Look for tests again'}
      aria-label="Look for tests again"
      onclick={() => void store.discover(root, true)}
    >
      <RefreshCw size={13} />
    </button>
    <!-- Fold the lot — the same pair the Rust catalogue and the run console carry, in the same
         order, so the gesture is one habit across all three. -->
    <button
      class="ps-btn"
      type="button"
      disabled={!expanded.size}
      use:tooltip={'Collapse all'}
      aria-label="Collapse all"
      onclick={collapseAll}
    >
      <ChevronsDownUp size={13} />
    </button>
    <button
      class="ps-btn"
      type="button"
      disabled={!store.discovered.length}
      use:tooltip={'Expand all'}
      aria-label="Expand all"
      onclick={expandAll}
    >
      <ChevronsUpDown size={13} />
    </button>
  {/snippet}

  {#if !root}
    <EmptyState message="Open a project to see its tests." />
  {:else if store.discovering && !store.discovered.length}
    <div class="tc-mid"><Spinner size={16} /><span>Looking for tests…</span></div>
  {:else if !store.discovered.length}
    <EmptyState message="No tests found in this project." />
  {:else}
    <div class="tc-filter">
      <input
        bind:value={filter}
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="Filter…"
        aria-label="Filter the tests"
      />
      {#if filter}<span class="tc-count">{shown.length}</span>{/if}
    </div>

    <div class="tc-list" role="list">
      {#each shown as t (t.fqcn)}
        {@const open_ = expanded.has(t.fqcn)}
        <div class="tc-row" class:tc-off={t.disabled || t.is_abstract} role="listitem">
          <button
            class="tc-twisty"
            type="button"
            tabindex="-1"
            aria-label={open_ ? 'Collapse' : 'Expand'}
            aria-expanded={open_}
            onclick={() => toggle(t.fqcn)}
          >
            {#if t.methods.length}
              {#if open_}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if}
            {/if}
          </button>
          <button class="tc-name" type="button" onclick={() => open(t.file, t.line)} title={t.fqcn}>
            <span class="tc-simple">{simpleOf(t)}</span>
            <!-- The second line answers "which of the three OrderTests is this" — the package,
                 or the module when you are sorting by one. -->
            <span class="tc-where">{sort === 'module' && t.module ? t.module : t.package}</span>
          </button>
          {#if t.methods.length}<span class="tc-n">{t.methods.length}</span>{/if}
          <!-- An abstract class is real but Surefire cannot instantiate it, and a disabled one
               would report as skipped: neither is worth offering a ▷ for. -->
          {#if !t.is_abstract && !t.disabled}
            <button
              class="tc-run"
              type="button"
              tabindex="-1"
              disabled={store.running}
              use:tooltip={'Run this class'}
              aria-label="Run this class"
              onclick={() => runClass(t)}
            >
              <Play size={11} />
            </button>
          {/if}
        </div>

        {#if open_}
          {#each t.methods as m (m.name)}
            <div class="tc-row tc-method" class:tc-off={m.disabled} role="listitem">
              <span class="tc-twisty"></span>
              <button class="tc-name" type="button" onclick={() => open(t.file, m.line)}>
                <span class="tc-simple">{m.name}</span>
              </button>
              {#if !t.is_abstract && !m.disabled}
                <button
                  class="tc-run"
                  type="button"
                  tabindex="-1"
                  disabled={store.running}
                  use:tooltip={'Run this test'}
                  aria-label="Run this test"
                  onclick={() => runMethod(t, m.name)}
                >
                  <Play size={11} />
                </button>
              {/if}
            </div>
          {/each}
        {/if}
      {/each}
    </div>
  {/if}
</PanelShell>

<style>
  .tc-sort { display: inline-flex; align-items: center; color: var(--text-muted); cursor: pointer; }
  .tc-sort:hover { color: var(--text-primary); }

  .tc-mid {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; color: var(--text-disabled); font-size: var(--font-size-xs);
  }

  .tc-filter {
    display: flex; align-items: center; gap: 6px; flex-shrink: 0;
    padding: 4px 8px; border-bottom: 1px solid var(--border-subtle);
  }
  .tc-filter input {
    flex: 1; min-width: 0;
    background: none; border: none; outline: none;
    color: var(--text-primary); font-size: var(--font-size-xs);
  }
  .tc-filter input::placeholder { color: var(--text-disabled); }
  .tc-count { font-size: var(--font-size-2xs); color: var(--text-muted); font-family: var(--font-code); }

  .tc-list { flex: 1; min-height: 0; overflow: auto; padding: 3px 0; }

  .tc-row {
    display: flex; align-items: center; gap: 4px;
    padding: 1px 6px 1px 2px; min-height: 24px;
  }
  .tc-row:hover { background: var(--bg-hover); }
  /* An abstract or disabled class is listed — it is real — but recedes, because it is not
     something you can run. */
  .tc-off .tc-simple { color: var(--text-muted); font-style: italic; }
  .tc-method { padding-left: 16px; }
  .tc-method .tc-simple { font-family: var(--font-code); font-size: var(--font-size-xs); }

  .tc-twisty {
    width: 14px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: none; border: none; padding: 0;
    color: var(--text-muted); cursor: pointer;
  }
  .tc-twisty:hover { color: var(--text-primary); }

  .tc-name {
    flex: 1; min-width: 0;
    display: flex; flex-direction: column; align-items: flex-start; gap: 0;
    background: none; border: none; padding: 0; text-align: left; cursor: pointer;
    font-family: var(--font-ui-sans);
  }
  .tc-simple {
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-sm); color: var(--text-primary);
  }
  .tc-where {
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .tc-name:hover .tc-simple { color: var(--accent); }

  .tc-n {
    flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-3xs);
    color: var(--text-disabled);
  }

  /* The run button appears on hover: always-on it would put a column of play triangles down a
     list whose job is to be read. */
  .tc-run {
    flex-shrink: 0; display: flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; padding: 0;
    background: none; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer; opacity: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast);
  }
  .tc-row:hover .tc-run, .tc-run:focus-visible { opacity: 1; }
  .tc-run:hover:not(:disabled) { color: var(--success); background: var(--bg-hover); }
  .tc-run:disabled { opacity: 0; }
</style>
