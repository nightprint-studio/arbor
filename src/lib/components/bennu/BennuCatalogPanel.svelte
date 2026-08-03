<script lang="ts">
  /**
   * Framework catalog panel (bottom dock) — **one** panel behind Endpoints, Beans, Config and
   * Bound properties.
   *
   * The backend returns every framework list in the same row shape (`ExtEntry`: primary,
   * secondary, a badge, tags, optional source site, optional sub-rows), which is what lets one
   * component render all of them with grouping, filtering and expansion. A new catalog is a row
   * in {@link FRAMEWORK_CATALOGS} plus a backend `catalog(kind)` arm.
   *
   * Three things earn their complexity here:
   *
   * - **Grouping.** A flat list of two hundred routes is a haystack. Which groupings a catalog
   *   offers is per-catalog (by path / by controller / by method for endpoints), and the first
   *   in the list is the default because a good default beats a menu nobody opens.
   * - **Expansion.** A route's parameters are what you actually need when you are about to call
   *   it, and they come down as `children` — no second request, no second panel.
   * - **Colour by kind.** `GET`, `POST` and `DELETE` in one colour means reading every badge.
   *   Destructive red, mutating warm, reads cool: the convention every API console uses, so it
   *   needs no legend.
   *
   * The Config catalog additionally carries the property-file picker, because that is where the
   * question "which of these five `application.yml`s am I looking at" is actually asked.
   */
  import { RefreshCw, ChevronRight, ChevronDown } from 'lucide-svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { springStore } from '$lib/stores/bennu/spring.svelte';
  import type { ExtEntry } from '$lib/ipc/bennu/ext';
  import {
    catalogFor, childrenOf, groupKeyOf, kindClass, searchTextOf,
    type FrameworkCatalogId, type GroupMode,
  } from './framework-catalogs';

  interface Props {
    /** Which catalog this panel shows — the bottom-dock id it was opened as. */
    id: FrameworkCatalogId;
  }
  let { id }: Props = $props();

  const spec = $derived(catalogFor(id));
  const rows = $derived(springStore.rows(spec.kind));
  const loading = $derived(springStore.isLoading(spec.kind));

  let query = $state('');
  let group = $state<GroupMode>('none');
  // Collapsed groups + expanded rows, by key. Sets rather than flags on the rows: the rows are
  // replaced wholesale on every refresh, and state that lives on them would be lost each time.
  const collapsed = new SvelteSet<string>();
  const expanded = new SvelteSet<string>();

  // The catalog's default grouping, re-applied when the panel switches catalogs.
  $effect(() => {
    group = spec.groups[0]?.id ?? 'none';
  });

  $effect(() => {
    const root = projectStore.project?.root;
    const kind = spec.kind;
    if (!root) return;
    void springStore.loadOverview(root);
    void springStore.loadCatalog(root, kind);
  });

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter((r) => searchTextOf(r).includes(q));
  });

  /** `[groupLabel, rows][]`, in first-seen order. One unnamed group when not grouping. */
  const groups = $derived.by<[string, ExtEntry[]][]>(() => {
    if (group === 'none') return [['', shown]];
    const map = new Map<string, ExtEntry[]>();
    for (const r of shown) {
      const key = groupKeyOf(group, r);
      const arr = map.get(key);
      if (arr) arr.push(r);
      else map.set(key, [r]);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  function rowKey(r: ExtEntry, i: number): string {
    return `${r.id}|${r.file ?? ''}|${r.line ?? i}`;
  }
  function toggleGroup(k: string) { if (collapsed.has(k)) collapsed.delete(k); else collapsed.add(k); }
  function toggleRow(k: string) { if (expanded.has(k)) expanded.delete(k); else expanded.add(k); }

  function open(r: ExtEntry) {
    if (!r.file) return;
    void projectStore.openFile(r.file).then(() => {
      if (r.line) bennuUiStore.requestGoto(r.line);
    });
  }

  function refresh() {
    const root = projectStore.project?.root;
    if (root) void springStore.refresh(root);
  }

  // ── Property-file picker (Config catalog only) ────────────────────────────
  const propertyOptions = $derived([
    { value: '', label: 'Default (base files)' },
    ...springStore.propertyFiles.map((f) => ({
      value: f.path,
      label: f.profile ? `${f.name} — profile ${f.profile}` : f.name,
    })),
  ]);
  const activeProperty = $derived(springStore.activePropertyFile ?? '');

  function pickPropertyFile(v: string | number) {
    const root = projectStore.project?.root;
    if (root) void springStore.setPropertyFile(root, String(v) || null);
  }

  const groupOptions = $derived(spec.groups.map((g) => ({ value: g.id, label: g.label })));
</script>

<div class="cat">
  <BottomPanelHeader
    title={spec.title}
    count={rows.length}
    onClose={() => bennuUiStore.closeBottom()}
  >
    {#snippet actions()}
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'Rebuild the framework model'}
        aria-label="Refresh"
        disabled={loading}
        onclick={refresh}
      >
        <RefreshCw size={13} />
      </button>
    {/snippet}
  </BottomPanelHeader>

  <div class="cat-bar">
    <input
      class="cat-filter"
      type="text"
      bind:value={query}
      placeholder={spec.placeholder}
      aria-label="Filter {spec.title}"
    />
    {#if spec.groups.length > 1}
      <Select value={group} options={groupOptions} narrow onchange={(v) => (group = v as GroupMode)} />
    {/if}
    {#if spec.picker}
      <label class="cat-pick">
        <span>Resolve against</span>
        <Select value={activeProperty} options={propertyOptions} narrow onchange={pickPropertyFile} />
      </label>
    {/if}
  </div>

  {#if loading}
    <div class="state"><Spinner size={13} /> Loading…</div>
  {:else if shown.length === 0}
    <div class="cat-empty">
      <EmptyState message={rows.length ? 'Nothing matches the filter.' : spec.empty} />
    </div>
  {:else}
    <div class="list">
      {#each groups as [key, items] (key)}
        {#if key}
          <button class="grp" type="button" onclick={() => toggleGroup(key)}>
            {#if collapsed.has(key)}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
            <span class="grp-name">{key}</span>
            <span class="grp-n">{items.length}</span>
          </button>
        {/if}
        {#if !collapsed.has(key)}
          {#each items as r, i (rowKey(r, i))}
            {@const rk = rowKey(r, i)}
            <div class="row-wrap">
              <div class="row" class:nested={!!key}>
                {#if childrenOf(r).length > 0}
                  <button
                    class="twist"
                    type="button"
                    aria-label={expanded.has(rk) ? 'Collapse parameters' : 'Expand parameters'}
                    onclick={() => toggleRow(rk)}
                  >
                    {#if expanded.has(rk)}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
                  </button>
                {:else}
                  <span class="twist-gap"></span>
                {/if}
                <span class="badge {kindClass(r.kind)}">{r.kind}</span>
                <button
                  class="row-main"
                  class:openable={!!r.file}
                  type="button"
                  disabled={!r.file}
                  onclick={() => open(r)}
                >
                  <span class="row-primary">{r.primary}</span>
                  <span class="row-secondary">{r.secondary}</span>
                </button>
                {#each r.tags as t (t)}<span class="tag">{t}</span>{/each}
                {#if childrenOf(r).length > 0}
                  <span class="argn">{childrenOf(r).length}</span>
                {/if}
                {#if r.line}<span class="row-line">{r.line}</span>{/if}
              </div>
              {#if expanded.has(rk)}
                {#each childrenOf(r) as c (c.id + c.secondary)}
                  <div class="child">
                    <span class="badge sm {kindClass(c.kind)}">{c.kind}</span>
                    <span class="child-name">{c.primary}</span>
                    <span class="child-type">{c.secondary}</span>
                    {#each c.tags as t (t)}<span class="tag">{t}</span>{/each}
                  </div>
                {/each}
              {/if}
            </div>
          {/each}
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .cat { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden; }
  .cat-bar {
    display: flex; align-items: center; gap: 8px; padding: 6px 10px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .cat-filter {
    flex: 1; min-width: 0;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); color: var(--text-primary);
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs);
    padding: 3px 8px;
  }
  .cat-filter:focus { outline: none; border-color: var(--accent); }
  .cat-pick { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .cat-pick span {
    font-size: var(--font-size-2xs); color: var(--text-muted);
    text-transform: uppercase; letter-spacing: 0.4px;
  }

  .state { display: flex; align-items: center; gap: 7px; padding: 12px 14px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .cat-empty { flex: 1; display: flex; align-items: center; justify-content: center; }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 3px 0; }
  .grp {
    display: flex; align-items: center; gap: 6px; width: 100%; text-align: left;
    padding: 4px 10px; background: transparent; border: none; cursor: pointer;
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs);
    color: var(--text-primary); font-weight: 500;
    position: sticky; top: 0; z-index: 1;
    background: var(--bg-base);
  }
  .grp:hover { background: var(--bg-hover); }
  .grp :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .grp-name { font-family: var(--font-code); }
  .grp-n { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .row-wrap { display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: 7px; padding: 2px 10px 2px 4px; }
  .row.nested { padding-left: 16px; }
  .row:hover { background: var(--bg-hover); }
  .twist {
    display: flex; align-items: center; justify-content: center;
    width: 14px; height: 14px; flex-shrink: 0;
    background: transparent; border: none; padding: 0; cursor: pointer; color: var(--text-muted);
  }
  .twist:hover { color: var(--text-primary); }
  .twist-gap { width: 14px; flex-shrink: 0; }

  .badge {
    flex-shrink: 0; min-width: 58px; text-align: center;
    font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.4px;
    padding: 1px 5px; border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
  }
  .badge.sm { min-width: 46px; font-weight: 600; }
  /* One colour per meaning. Destructive red, mutating warm, reads cool — no legend needed. */
  .k-get { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); }
  .k-post { color: var(--info); background: color-mix(in srgb, var(--info) 14%, transparent); }
  .k-put { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); }
  .k-delete { color: var(--error); background: color-mix(in srgb, var(--error) 14%, transparent); }
  .k-any { color: var(--text-muted); background: var(--bg-overlay); }
  .k-service { color: var(--success); background: color-mix(in srgb, var(--success) 12%, transparent); }
  .k-repository { color: var(--info); background: color-mix(in srgb, var(--info) 12%, transparent); }
  .k-controller { color: var(--warning); background: color-mix(in srgb, var(--warning) 12%, transparent); }
  .k-config { color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .k-xml { color: var(--text-secondary); background: var(--bg-overlay); }
  .k-neutral { color: var(--text-secondary); background: var(--bg-overlay); }

  .row-main {
    flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 10px;
    background: transparent; border: none; padding: 0; text-align: left;
    font-family: var(--font-ui-sans); cursor: default;
  }
  .row-main.openable { cursor: pointer; }
  .row-primary {
    flex-shrink: 0; max-width: 55%;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .row-secondary {
    flex: 1; min-width: 0;
    font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .tag {
    flex-shrink: 0; font-size: var(--font-size-3xs); color: var(--text-secondary);
    padding: 0 5px; border-radius: 999px; border: 1px solid var(--border-subtle);
  }
  .argn {
    flex-shrink: 0; font-size: var(--font-size-3xs); color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .row-line { flex-shrink: 0; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }

  .child {
    display: flex; align-items: center; gap: 8px;
    padding: 1px 10px 1px 42px;
    font-family: var(--font-ui-sans);
  }
  .child-name { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-secondary); }
  .child-type { flex: 1; min-width: 0; font-size: var(--font-size-2xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
