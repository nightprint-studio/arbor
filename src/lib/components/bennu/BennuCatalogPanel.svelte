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
   * - **Types you can open.** A row that says it returns a `QFormDto` has told you nothing; the
   *   fields are the answer. Any chip naming a composite type expands into them, and each field
   *   that is itself composite expands in turn. Fetched on the click and never before
   *   (`bennu_type_shape`): a list of two hundred routes names two hundred types, and you came
   *   to look at one. The recursion has no end condition other than the user, because a DTO
   *   graph can be cyclic — `Order` → `Customer` → `List<Order>` — and there is no complete tree
   *   to build, only the next level of the one in front of you.
   * - **Colour by kind.** `GET`, `POST` and `DELETE` in one colour means reading every badge.
   *   Destructive red, mutating warm, reads cool: the convention every API console uses, so it
   *   needs no legend.
   *
   * The Config catalog additionally carries the property-file picker, because that is where the
   * question "which of these five `application.yml`s am I looking at" is actually asked.
   *
   * **Export** takes what is on screen — filtered, grouped, with each route's parameters
   * flattened onto its row — through the shared {@link ExportButton}, which already owns the
   * three formats, the two destinations and the save picker.
   */
  import { RefreshCw, ChevronRight, ChevronDown } from 'lucide-svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import ExportButton, { type Rendition } from '$lib/components/shared/internal/ExportButton.svelte';
  import {
    EXPORT_EXTENSION, exportRows, type ExportColumn, type ExportFormat,
  } from '$lib/utils/tabular-export';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { springStore } from '$lib/stores/bennu/spring.svelte';
  import type { ExtEntry } from '$lib/ipc/bennu/ext';
  import { typeShape, type TypeShape } from '$lib/ipc/bennu/nav';
  import {
    catalogFor, childrenOf, groupKeyOf, kindClass, looksComposite, mediaAliases, searchTextOf,
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

  // ── Type expansion ──────────────────────────────────────────────────────────
  /**
   * What a row's type actually holds, fetched on the click that asks for it.
   *
   * Keyed by `<context file>|<type as written>`, which is the pair that identifies the answer: a
   * bare `QFormDto` means whatever *that controller's* imports say it means. `null` is a real,
   * cached answer — "there is nothing inside this" — and is why a second click on a leaf costs
   * nothing.
   *
   * Never populated by the catalog load. A list of two hundred routes names two hundred types and
   * you came to look at one of them; resolving them all on open would be paying for a hundred
   * and ninety-nine answers nobody asked for.
   */
  const shapes = new SvelteMap<string, TypeShape | null>();
  const loadingShapes = new SvelteSet<string>();
  /**
   * The type opened under each row, by the path that reached it.
   *
   * Keyed by path rather than by type, because the same type appears in several places in one
   * tree — an `Order` inside a `Customer` inside an `Order` — and they open and close apart. The
   * value carries which type it is, so the row does not have to re-derive it to render what is
   * under it.
   */
  const openTypes = new SvelteMap<string, { file: string; type: string }>();

  function shapeKey(file: string, typeText: string): string {
    return `${file}|${typeText}`;
  }

  /** Ask for a type's members once, remembering the answer — including "none". */
  async function loadShape(file: string, typeText: string) {
    const key = shapeKey(file, typeText);
    if (shapes.has(key) || loadingShapes.has(key)) return;
    const root = projectStore.project?.root;
    if (!root) return;
    loadingShapes.add(key);
    try {
      shapes.set(key, await typeShape(root, file, typeText));
    } catch {
      // A backend that cannot answer is the same as a type with nothing in it, as far as this
      // panel is concerned — and one that says so once rather than retrying on every click.
      shapes.set(key, null);
    } finally {
      loadingShapes.delete(key);
    }
  }

  function toggleType(path: string, file: string, typeText: string) {
    // Clicking the chip that is already open closes it; clicking a different one at the same
    // place swaps rather than doing nothing, which is what a second chip on a row would need.
    if (openTypes.get(path)?.type === typeText) {
      openTypes.delete(path);
      return;
    }
    openTypes.set(path, { file, type: typeText });
    void loadShape(file, typeText);
  }

  // ── Row rendering ───────────────────────────────────────────────────────────
  /** A path split into its literal and `{variable}` parts, so the template can light the second. */
  function pathParts(path: string): { text: string; variable: boolean }[] {
    return path
      .split(/(\{[^}]*\})/)
      .filter(Boolean)
      .map((text) => ({ text, variable: text.startsWith('{') }));
  }

  interface RowTag {
    text: string;
    /** A type worth offering to open. */
    type?: boolean;
    /** A media type, already shortened. */
    media?: boolean;
  }

  /**
   * A row's trailing chips, classified by what they *are* rather than by their position.
   *
   * By content and not by index because the tags are a per-catalog list and reading them
   * positionally is how a panel starts lying the day a catalog adds one.
   */
  function rowTags(r: ExtEntry): RowTag[] {
    const out: RowTag[] = [];
    for (const t of r.tags) {
      if (isMediaTag(t)) {
        for (const alias of mediaAliases(t)) out.push({ text: alias, media: true });
      } else if (looksComposite(t)) {
        out.push({ text: t, type: true });
      } else {
        out.push({ text: t });
      }
    }
    return out;
  }

  /** Whether a tag is a media type — the constant form (`MediaType.APPLICATION_JSON_VALUE`) or
   *  the literal one (`application/json`). */
  function isMediaTag(t: string): boolean {
    return t.includes('MediaType.') || /^[{["']*\s*[a-z*]+\/[-+.\w*]+/.test(t.trim());
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

  // ── Export ──────────────────────────────────────────────────────────────────
  /**
   * What leaves the panel is **what is on screen**: the filter and the grouping are applied, and
   * the file is the answer to the question you were asking rather than a dump of the catalog. A
   * route list is exported to be handed to somebody — a spreadsheet of every endpoint an audit
   * asked about, a Markdown table pasted into a ticket — and that list is nearly always a subset.
   *
   * The columns are the row, opened out: the chips are split back into what they meant (the type
   * it returns, the media types it speaks) rather than exported as the rendering, and a route's
   * parameters — which live in the expansion nobody would think to open first — come along
   * flattened, because the whole point of taking the table elsewhere is not having to come back.
   */
  const exportColumns = $derived.by<ExportColumn<ExtEntry>[]>(() => {
    const labels = spec.columns ?? { primary: 'name', secondary: 'detail' };
    const columns: ExportColumn<ExtEntry>[] = [];
    if (group !== 'none') {
      // "Group by controller" is a menu entry; the column it produces is called `controller`.
      const label = spec.groups.find((g) => g.id === group)?.label ?? group;
      columns.push({ key: label.replace(/^group by /i, ''), value: (r) => groupKeyOf(group, r) });
    }
    columns.push(
      { key: 'kind', value: (r) => r.kind },
      { key: labels.primary, value: (r) => r.primary },
      { key: labels.secondary, value: (r) => r.secondary },
      { key: 'returns', value: (r) => rowTags(r).filter((t) => t.type).map((t) => t.text).join(', ') },
      { key: 'media', value: (r) => rowTags(r).filter((t) => t.media).map((t) => t.text).join(', ') },
      { key: 'tags', value: (r) => rowTags(r).filter((t) => !t.type && !t.media).map((t) => t.text).join(', ') },
      {
        key: 'parameters',
        value: (r) => childrenOf(r).map((c) => `${c.primary}: ${c.secondary} (${c.kind})`).join('; '),
      },
      { key: 'file', value: (r) => r.file ?? '' },
      { key: 'line', value: (r) => r.line ?? '' },
    );
    return columns;
  });

  const renditions = $derived<Rendition[]>(
    (['csv', 'json', 'markdown'] as ExportFormat[]).map((format) => ({
      id: format,
      label: format === 'csv' ? 'As CSV' : format === 'json' ? 'As JSON' : 'As a Markdown table',
      extension: EXPORT_EXTENSION[format],
      text: () => exportRows(shown, exportColumns, format),
    })),
  );

  const exportName = $derived(
    [projectStore.project?.name, spec.id].filter(Boolean).join('-').replace(/\s+/g, '-'),
  );
  const exportSubject = $derived.by(() => {
    const noun = spec.title.toLowerCase();
    return `${shown.length} ${shown.length === 1 ? noun.replace(/s$/, '') : noun}`;
  });
</script>

<div class="cat">
  <BottomPanelHeader
    title={spec.title}
    count={rows.length}
    onClose={() => bennuUiStore.closeBottom()}
  >
    {#snippet actions()}
      <!-- What leaves is what is on screen — the filter and the grouping included. Exporting the
           whole catalog when you had narrowed it to eleven rows is answering a question nobody
           asked. -->
      <ExportButton
        {renditions}
        variant="icon"
        fileName={exportName}
        subject={exportSubject}
        empty={shown.length === 0}
        tooltip={`Take these ${spec.title.toLowerCase()} out of Arbor — to the clipboard, or to a file`}
        emptyTooltip="There is nothing here to export"
      />
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
    <!-- What the filter left, next to the filter that left it. The header's count is the
         catalog's size and does not move; this one is the answer to what you just typed. -->
    <span class="cat-count">
      {shown.length}
      {#if shown.length !== rows.length}<span class="cat-count-of">of {rows.length}</span>{/if}
    </span>
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
                  <span class="row-primary">
                    <!-- The `{var}` segments lit apart from the literal ones: a path is read for
                         its shape, and which parts of it are yours to fill in is the shape. -->
                    {#each pathParts(r.primary) as p, pi (pi)}
                      {#if p.variable}<span class="pv">{p.text}</span>{:else}{p.text}{/if}
                    {/each}
                  </span>
                  <span class="row-secondary">{r.secondary}</span>
                </button>
                {#each rowTags(r) as t, ti (t.text + ti)}
                  {#if t.type && r.file}
                    {@render typeChip(t.text, r.file ?? '', rk)}
                  {:else}
                    <span class="tag" class:media={t.media}>{t.text}</span>
                  {/if}
                {/each}
                {#if childrenOf(r).length > 0}
                  <span class="argn" title="{childrenOf(r).length} parameters">{childrenOf(r).length}</span>
                {/if}
                {#if r.line}<span class="row-line">{r.line}</span>{/if}
              </div>
              {#if openTypes.has(rk)}
                {@render typeTree(rk, openTypes.get(rk)!, 1)}
              {/if}
              {#if expanded.has(rk)}
                {#each childrenOf(r) as c (c.id + c.secondary)}
                  {@const ck = `${rk}>${c.id}`}
                  <div class="child">
                    <span class="badge sm {kindClass(c.kind)}">{c.kind}</span>
                    <span class="child-name">{c.primary}</span>
                    {#if looksComposite(c.secondary) && r.file}
                      {@render typeChip(c.secondary, r.file ?? '', ck)}
                      <span class="child-spacer"></span>
                    {:else}
                      <span class="child-type">{c.secondary}</span>
                    {/if}
                    {#each c.tags as t (t)}<span class="tag">{t}</span>{/each}
                  </div>
                  {#if openTypes.has(ck)}
                    {@render typeTree(ck, openTypes.get(ck)!, 2)}
                  {/if}
                {/each}
              {/if}
            </div>
          {/each}
        {/if}
      {/each}
    </div>
  {/if}
</div>

<!--
  A type worth opening, as a chip that says so. The chevron is on the chip rather than beside it
  because the chip IS the thing being expanded — a `QFormDto` is not a label on the row, it is a
  door.
-->
{#snippet typeChip(text: string, file: string, key: string)}
  {@const on = openTypes.get(key)?.type === text}
  <button
    class="tag type"
    class:on
    type="button"
    title={on ? `Hide the fields of ${text}` : `Show the fields of ${text}`}
    onclick={() => toggleType(key, file, text)}
  >
    {#if on}<ChevronDown size={9} />{:else}<ChevronRight size={9} />{/if}
    {text}
  </button>
{/snippet}

<!--
  One level of a type's members, and itself again under any member worth opening.
  Recursive rather than pre-flattened because a DTO graph can be cyclic (`Order` → `Customer` →
  `List<Order>`): there is no complete tree to build, only the next level of the one you are
  looking at.
-->
{#snippet typeTree(key: string, site: { file: string; type: string }, depth: number)}
  {@const k = shapeKey(site.file, site.type)}
  {@const shape = shapes.get(k)}
  {#if loadingShapes.has(k)}
    <div class="tn tn-note" style="--d: {depth}"><Spinner size={11} /> Reading {site.type}…</div>
  {:else if shape}
    {#each shape.members as m (m.name + m.type_text)}
      {@const mk = `${key}>${m.name}`}
      <div class="tn" style="--d: {depth}">
        {#if m.expand}
          <button
            class="twist"
            type="button"
            aria-label={openTypes.has(mk) ? `Collapse ${m.name}` : `Expand ${m.name}`}
            onclick={() => toggleType(mk, site.file, m.expand ?? '')}
          >
            {#if openTypes.has(mk)}<ChevronDown size={10} />{:else}<ChevronRight size={10} />{/if}
          </button>
        {:else}
          <span class="twist-gap"></span>
        {/if}
        <span class="tn-name">{m.name}</span>
        <span class="tn-type">{m.type_text}</span>
        <!-- Said out loud, because it changes what you are reading: an interface has no fields,
             and what is listed is what its getters expose. -->
        {#if m.kind === 'property'}<span class="tn-kind">property</span>{/if}
      </div>
      {#if openTypes.has(mk)}
        {@render typeTree(mk, openTypes.get(mk) ?? site, depth + 1)}
      {/if}
    {/each}
  {:else}
    <div class="tn tn-note" style="--d: {depth}">Nothing to open inside {site.type}.</div>
  {/if}
{/snippet}

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
  .cat-count {
    flex-shrink: 0; font-size: var(--font-size-2xs); color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .cat-count-of { color: var(--text-disabled); margin-left: 3px; }

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
  /* A route list is read by skimming, and skimming needs a baseline grid: every row the same
     height, the badge the same width, the trailing chips ending in the same place. The old rows
     were 2px tall with the chips wherever the text left them. */
  .row {
    display: flex; align-items: center; gap: 8px;
    min-height: 22px; padding: 1px 10px 1px 4px;
    border-bottom: 1px solid transparent;
  }
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
    display: inline-flex; align-items: center; gap: 2px;
    flex-shrink: 0; height: 15px;
    font-family: var(--font-ui-sans); font-size: var(--font-size-3xs); line-height: 1;
    color: var(--text-secondary);
    padding: 0 6px; border-radius: 999px; border: 1px solid var(--border-subtle);
    background: transparent;
  }
  /* Two kinds of chip, told apart at a glance because they answer different questions: what this
     hands back (a type — and a door into it), and how it is encoded (a media type). */
  .tag.media {
    color: var(--text-muted);
    border-color: transparent;
    background: var(--bg-overlay);
    letter-spacing: 0.2px;
  }
  .tag.type {
    padding-left: 3px;
    cursor: pointer;
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
    font-family: var(--font-code);
  }
  .tag.type:hover { background: var(--accent-subtle); }
  .tag.type.on { background: var(--accent-subtle); }
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
  /* A parameter whose type is a door has the chip instead of the plain text, and the spacer
     keeps the row's trailing tags where they are on every other row. */
  .child-spacer { flex: 1; min-width: 0; }

  /* The `{var}` parts of a path. The one place colour is doing work in this list besides the
     verb: it is what tells `/bandi/{idcom}` from `/bandi/nuovo` at a glance. */
  .pv { color: var(--accent); }

  /* ── The type tree ────────────────────────────────────────────────────────
     Indented by depth through a custom property rather than by a class per level: the depth is
     data, and a stylesheet with `.d1 .d2 .d3` in it is a tree that stops working at four. */
  .tn {
    display: flex; align-items: center; gap: 8px;
    min-height: 19px;
    padding-left: calc(30px + var(--d, 1) * 14px);
    padding-right: 10px;
    font-family: var(--font-ui-sans);
    border-left: 1px solid transparent;
  }
  .tn:hover { background: var(--bg-hover); }
  .tn-name {
    flex-shrink: 0;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-secondary);
  }
  .tn-type {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .tn-kind {
    flex-shrink: 0; font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .tn-note {
    gap: 6px; color: var(--text-disabled); font-size: var(--font-size-2xs); font-style: italic;
  }
</style>
