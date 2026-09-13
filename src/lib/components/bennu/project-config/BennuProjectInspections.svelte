<script lang="ts">
  /**
   * Project Configuration › Inspections — which checks this project reports, and how loudly.
   *
   * The backend has served this since the checks were given stable codes; there was simply no
   * screen, and the documentation promised one. A check you cannot turn down is a check you turn
   * off by ignoring the whole panel, which is how a legacy project ends up with validation
   * switched off wholesale.
   *
   * ## Per project, not per profile
   *
   * Unlike naming and spelling, this one has no profile level and should not get one: "is an
   * unused import worth a warning" is a question about *this* codebase's state, and a legacy tree
   * being brought back under control answers it differently every month. A default carried between
   * projects would be the wrong answer in the one that needed it most.
   *
   * ## Writes as you go
   *
   * No Apply. A severity is one choice about one check, and the loop worth having is "turn it down,
   * look at the file, turn it down further" — a button in the middle of that is a step that does
   * nothing but wait.
   */
  import { ListFilter, RotateCcw } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuInspectionsStore } from '$lib/stores/bennu/inspections.svelte';
  import { CHECK_SEVERITIES, type CheckSeverity } from '$lib/ipc/bennu/inspections';

  const store = bennuInspectionsStore;
  const root = $derived(projectStore.project?.root ?? null);

  $effect(() => {
    const r = root;
    if (r) void store.load(r);
  });

  let query = $state('');

  /** The severity choices, each saying what it means rather than only what it is called. */
  const SEVERITY_LABELS: Record<CheckSeverity, string> = {
    error: 'error — red, and wrong',
    warning: 'warning — worth fixing',
    weak: 'weak — true, but not a defect',
    off: 'off — never reported',
  };
  const severityOptions = CHECK_SEVERITIES.map((s) => ({ value: s, label: SEVERITY_LABELS[s] }));

  /** The tone a severity wears, so a page of rows reads at a glance. */
  const TONE: Record<CheckSeverity, 'error' | 'warning' | 'info' | 'neutral'> = {
    error: 'error',
    warning: 'warning',
    weak: 'info',
    off: 'neutral',
  };

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return store.checks;
    return store.checks.filter(
      (c) => c.title.toLowerCase().includes(q) || c.code.toLowerCase().includes(q),
    );
  });
</script>

<div class="section-header">
  <h2>Inspections</h2>
  <p>
    Every check Bennu can report here, and how loudly each one does. A level is remembered for this
    project — a legacy tree being brought back under control does not want the same answers as a
    green-field one.
  </p>
</div>

<div class="card fills">
  <div class="card-section-title">
    <ListFilter size={12} />
    Checks — {store.checks.length}
    {#if store.changedCount}<span class="changed">{store.changedCount} changed</span>{/if}
  </div>
  <div class="bar">
    <SearchBar
      bind:query
      showCounter={false}
      placeholder="Search checks…"
      ariaLabel="Search checks"
      onClear={() => (query = '')}
    />
    <Button
      variant="ghost"
      size="sm"
      disabled={store.changedCount === 0}
      onclick={() => void store.resetAll()}
    >
      {#snippet iconStart()}<RotateCcw size={13} />{/snippet}
      Back to defaults
    </Button>
  </div>

  {#if store.loading && store.checks.length === 0}
    <p class="set-empty">Loading the checks…</p>
  {:else if shown.length === 0}
    <EmptyState message={query ? 'No check matches that.' : 'No checks to configure here.'} compact />
  {:else}
    <ul class="checks">
      {#each shown as check (check.code)}
        {@const current = store.severityOf(check)}
        <li class:configured={store.isConfigured(check)}>
          <span class="what">
            <span class="title">{check.title}</span>
            <code class="code">{check.code}</code>
          </span>
          <span class="level">
            {#if store.isConfigured(check)}
              <!-- Said, not inferred from the dropdown: "this is not the default" is the one thing
                   you cannot read off a value you are looking at. -->
              <Badge variant="tone" tone={TONE[current]} label="changed" />
            {/if}
            <Select
              value={current}
              options={severityOptions}
              quiet
              highlight={store.isConfigured(check)}
              ariaLabel={`${check.title} severity`}
              onchange={(v) => void store.setSeverity(check, v as CheckSeverity)}
            />
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<p class="set-hint">
  This is a policy over <em>kinds</em>. To silence a single place instead, the source says so:
  <code>@SuppressWarnings("unused-import")</code> on the declaration it covers, or
  <code>// bennu:ignore unused-import</code> on the line — the code is the one shown beside each
  check above.
</p>

<style>
  .card.fills { display: flex; flex-direction: column; min-height: 0; }
  .changed {
    margin-left: auto; text-transform: none; letter-spacing: 0;
    font-weight: 500; color: var(--accent);
  }
  .bar {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 12px; border-bottom: 1px solid var(--border-subtle);
  }
  .bar :global(.search-bar) { flex: 1; min-width: 0; }

  .checks {
    list-style: none; margin: 0; padding: 4px 0;
    flex: 1; min-height: 0; overflow-y: auto;
  }
  .checks li {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 4px 14px; min-height: 30px;
  }
  .checks li.configured { background: color-mix(in srgb, var(--accent) 6%, transparent); }
  .what { display: flex; align-items: baseline; gap: 8px; min-width: 0; flex: 1; }
  .title {
    font-size: var(--font-size-sm); color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .code {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    flex-shrink: 0;
  }
  .level { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
</style>
