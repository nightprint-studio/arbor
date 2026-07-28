<script lang="ts">
  /**
   * Inventory — the coverage matrix: every indexed object against every place
   * that could define or touch it.
   *
   * ## Why the columns are not folders
   *
   * Coverage arrives keyed by folder path, and a real repository has hundreds of
   * folders — one per delivered version, eleven of them called `ORA`. A column
   * each is not a matrix, it is a horizontal scroll, and it answers a question
   * nobody asks: "how many statements are in this exact directory".
   *
   * The question the rules ask, and the one a person asks, is **"does the Oracle
   * side say what the PostgreSQL side says, and do the updates say what the
   * initialisation says"**. Both are properties of the effective engine and the
   * effective role. So those are the columns: six or eight, stable however many
   * versions accumulate.
   *
   * Nothing is folded away for good. Expanding a row gives the per-folder numbers
   * behind every column for that one object — which is where "*which* of the
   * eleven version folders is missing it" is actually asked — and anything that
   * landed outside the columns entirely is counted rather than rounded off, so a
   * folded matrix can never look complete when it is not.
   */
  import { TriangleAlert, CheckCircle2, ChevronRight } from 'lucide-svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import ObjectKindIcon, { OBJECT_KIND_LABELS as KIND_LABELS } from '../PicusObjectKindIcon.svelte';
  import InventoryUsages from '../panels/InventoryUsages.svelte';
  import NoticeList from '../panels/NoticeList.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import {
    bucketCoverage,
    coverageBuckets,
    elsewhereCount,
    folderBreakdown,
    ignoredFileCount,
  } from '$lib/utils/picus/coverage';
  import type { InventoryObject, ObjectKind } from '$lib/types/picus';

  let query = $state('');
  const needle = $derived(query.trim().toLowerCase());

  /** Matrix columns: one per engine × role that this repository actually has. */
  const buckets = $derived(coverageBuckets(picusProjectStore.tree));
  const hidden = $derived(ignoredFileCount(picusProjectStore.tree));

  const rows = $derived(
    picusProjectStore.inventory.filter((o) => !needle || o.name.toLowerCase().includes(needle)),
  );

  const gapCount = $derived(
    picusProjectStore.inventory.filter((o) => buckets.some((b) => bucketCoverage(o, b) === 0)).length,
  );

  /**
   * Rows grouped by what they are.
   *
   * A flat list of four hundred names, tables and triggers and sequences mixed
   * together, is a list you can only search — and searching is what the filter
   * above is for. Grouped, the same list can be *read*: the twelve views are
   * twelve rows in one place, and "do we have a PostgreSQL counterpart for every
   * Oracle package" becomes a question you can answer by looking.
   */
  const groups = $derived.by(() => {
    const by = new Map<ObjectKind, InventoryObject[]>();
    for (const obj of rows) {
      const list = by.get(obj.kind);
      if (list) list.push(obj);
      else by.set(obj.kind, [obj]);
    }
    // The backend already orders rows by kind then name; this keeps that order
    // rather than imposing a second one that could disagree with it.
    return [...by.entries()].map(([kind, objects]) => ({ kind, objects }));
  });

  /**
   * Which object's detail is open, and what kind of detail.
   *
   * One at a time, because both are deep. `bucket` set means the user clicked a
   * *cell* and wants the mentions behind that number; `bucket` null means they
   * clicked the twisty and want the folder-by-folder breakdown.
   */
  let openObject = $state<string | null>(null);
  let openBucket = $state<string | null>(null);
  function objectKey(obj: InventoryObject) { return `${obj.kind}/${obj.name}`; }

  /** Clicking a cell opens what it counts; clicking it again closes it. */
  function toggleCell(obj: InventoryObject, bucketKey: string) {
    const key = objectKey(obj);
    if (openObject === key && openBucket === bucketKey) {
      openObject = null;
      openBucket = null;
      return;
    }
    openObject = key;
    openBucket = bucketKey;
  }

  function toggleRow(obj: InventoryObject) {
    const key = objectKey(obj);
    const showing = openObject === key && openBucket === null;
    openObject = showing ? null : key;
    openBucket = null;
  }
</script>

<div class="iv">
  <header class="iv-head">
    <div>
      <h1>Inventory</h1>
      <p>
        Every object the scripts define or touch, against every engine and role that could
        define it. A zero is one side staying silent about something another side says.
        Open a row for the folder-by-folder detail behind its columns.
      </p>
    </div>
    <div class="iv-summary">
      {#if picusProjectStore.analyzing}
        <span class="iv-working"><Spinner size={12} /> Indexing…</span>
      {:else if gapCount}
        <span class="iv-gaps"><TriangleAlert size={13} /> {gapCount} object{gapCount === 1 ? '' : 's'} with gaps</span>
      {:else}
        <span class="iv-ok"><CheckCircle2 size={13} /> Nothing missing</span>
      {/if}
      <button class="iv-link" onclick={() => picusUiStore.showBottom('consistency')}>
        Open the consistency report
      </button>
    </div>
  </header>

  <div class="iv-search">
    <SearchBar bind:query showRegex={false} placeholder="Filter objects" ariaLabel="Filter objects" />
  </div>

  {#if picusProjectStore.unclassifiedFolders.length}
    <!-- Scripts under a folder no engine covers are indexed into no column at all.
         Saying so is the difference between a matrix that is complete and one that
         only looks it. -->
    <Alert
      variant="warning"
      compact
      title={`${picusProjectStore.unclassifiedFolders.length} folder(s) of scripts have no engine`}
      text="Their statements belong to no column below. Classify them — Ctrl+Shift+F — and the matrix accounts for them."
    />
  {/if}

  {#if picusProjectStore.analysisError}
    <Alert
      variant="error"
      title="The index could not be built"
      text={picusProjectStore.analysisError}
    />
  {:else if !rows.length}
    <StateBlock
      tone="info"
      fill={false}
      label={!picusProjectStore.attached
        ? 'No repository attached to this connection — there is nothing to index.'
        : picusProjectStore.inventory.length
          ? `Nothing matches “${query}”.`
          : picusProjectStore.analyzing
            ? 'Indexing the repository…'
            : 'Nothing indexed yet.'}
    />
  {:else}
    <div class="iv-scroll">
      <table class="iv-table">
        <thead>
          <tr>
            <th class="iv-obj-th" scope="col">Object</th>
            {#each buckets as bucket (bucket.key)}
              <!-- Spelled out rather than left to two terse chips. `ORA · init`
                   assumes the reader already knows both vocabularies, and the
                   number underneath is a count of *statements*, which is not
                   what "3 folders" led anyone to expect. Both are said. -->
              <th scope="col" class="iv-slot-th">
                <span class="iv-slot">
                  <PicusDialectChip dialect={bucket.dialect} terse />
                  <PicusRoleChip role={bucket.role} terse />
                </span>
                <span class="iv-slot-label">{bucket.label}</span>
                <span
                  class="iv-slot-name"
                  use:tooltip={{
                    content: `Statements under ${bucket.folders.length} folder(s), ${bucket.fileCount} file(s)`,
                    description: bucket.folders.slice(0, 12).join('\n')
                      + (bucket.folders.length > 12 ? `\n… and ${bucket.folders.length - 12} more` : ''),
                  }}
                >
                  {bucket.folders.length} folder{bucket.folders.length === 1 ? '' : 's'} · statements
                </span>
              </th>
            {/each}
          </tr>
        </thead>
        {#each groups as group (group.kind)}
          <tbody>
            <!-- A heading row rather than a separate table per kind: the columns
                 have to line up across the whole matrix, or comparing an Oracle
                 package against a PostgreSQL function stops being a glance. -->
            <tr class="iv-group-row">
              <th class="iv-group" scope="colgroup" colspan={buckets.length + 1}>
                <ObjectKindIcon kind={group.kind} />
                <span class="iv-group-name">{KIND_LABELS[group.kind] ?? group.kind}</span>
                <span class="iv-group-n">{group.objects.length}</span>
              </th>
            </tr>

            <!-- Kind AND name — see the note in `InventoryPanel`: a name alone is
                 not unique, and a duplicate key is a hard error, not a glitch. -->
            {#each group.objects as obj (objectKey(obj))}
              {@const key = objectKey(obj)}
              {@const showingFolders = openObject === key && openBucket === null}
              {@const stray = elsewhereCount(obj, buckets)}
              <tr>
                <th scope="row" class="iv-obj">
                  <button
                    class="iv-twist"
                    class:iv-open={showingFolders}
                    aria-expanded={showingFolders}
                    aria-label={`Folder detail for ${obj.name}`}
                    onclick={() => toggleRow(obj)}
                  >
                    <ChevronRight size={12} />
                  </button>
                  <ObjectKindIcon kind={obj.kind} />
                  <span class="iv-obj-name">{obj.name}</span>
                  {#if stray}
                    <span
                      class="iv-stray"
                      use:tooltip={'Statements in folders no column covers — an ignored folder, or one with no engine'}
                    >
                      +{stray} elsewhere
                    </span>
                  {/if}
                </th>
                {#each buckets as bucket (bucket.key)}
                  {@const n = bucketCoverage(obj, bucket)}
                  {@const open = openObject === key && openBucket === bucket.key}
                  <td class="iv-cell" class:iv-zero={n === 0} class:iv-many={n > 1}>
                    <!-- Clickable including the zeroes: "nothing here" is the
                         answer people most want to check, and being able to open
                         it and see an empty list is what turns a suspicion into a
                         fact. -->
                    <button
                      class="iv-cell-btn"
                      class:iv-cell-open={open}
                      aria-expanded={open}
                      use:tooltip={n === 0
                        ? `${obj.name} is never touched under ${bucket.label} — open to check`
                        : `${n} statement${n === 1 ? '' : 's'} under ${bucket.label} — open to see where`}
                      onclick={() => toggleCell(obj, bucket.key)}
                    >
                      {n === 0 ? '—' : n}
                    </button>
                  </td>
                {/each}
              </tr>

              {#if showingFolders}
                <tr class="iv-detail-row">
                  <td class="iv-detail" colspan={buckets.length + 1}>
                    <!-- The folded detail, for this object only: which folder in each
                         column says something, and which stays quiet. -->
                    <div class="iv-detail-grid">
                      {#each buckets as bucket (bucket.key)}
                        <div class="iv-detail-col">
                          <span class="iv-detail-head">{bucket.label}</span>
                          {#each folderBreakdown(obj, bucket) as line (line.path)}
                            <span class="iv-detail-line" class:iv-detail-zero={line.count === 0}>
                              <span class="iv-detail-path">{line.path}</span>
                              <span class="iv-detail-n">{line.count === 0 ? '—' : line.count}</span>
                            </span>
                          {/each}
                        </div>
                      {/each}
                    </div>
                  </td>
                </tr>
              {:else if openObject === key && openBucket}
                {@const bucket = buckets.find((b) => b.key === openBucket)}
                {#if bucket}
                  <tr class="iv-detail-row">
                    <td class="iv-detail" colspan={buckets.length + 1}>
                      {#key `${key}/${bucket.key}`}
                        <InventoryUsages
                          name={obj.name}
                          kind={obj.kind}
                          folders={bucket.folders}
                          label={bucket.label}
                        />
                      {/key}
                    </td>
                  </tr>
                {/if}
              {/if}
            {/each}
          </tbody>
        {/each}
      </table>
    </div>

    {#if hidden}
      <p class="iv-note">
        {hidden} file{hidden === 1 ? '' : 's'} sit under folders whose role is <b>ignored</b>.
        They are not indexed, so they are not a column here — a column of zeroes for them
        would read as a gap instead of as a choice.
      </p>
    {/if}
  {/if}

  <!-- Indexed, but claimed by no classified folder: not a gap between engines, a
       place outside the model. Hiding it would make the matrix look complete. -->
  <NoticeList notes={picusProjectStore.orphans} label="Outside every classified folder" />
</div>

<style>
  /* Document-flow view — owns its scroll, children never shrink (see GenerateView). */
  .iv {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 16px 20px 40px;
    gap: 12px;
    overflow-y: auto;
  }
  .iv > :global(*) { flex-shrink: 0; }

  .iv-head { display: flex; align-items: flex-start; gap: 20px; flex-wrap: wrap; }
  .iv-head h1 { font-size: 16px; font-weight: 600; margin-bottom: 3px; }
  .iv-head p { font-size: 12px; line-height: 1.55; color: var(--text-muted); max-width: 70ch; }

  .iv-summary { display: flex; flex-direction: column; gap: 5px; align-items: flex-end; margin-left: auto; }
  .iv-gaps { display: inline-flex; align-items: center; gap: 5px; color: var(--error); font-size: 12px; font-weight: 600; }
  .iv-ok { display: inline-flex; align-items: center; gap: 5px; color: var(--success); font-size: 12px; font-weight: 600; }
  .iv-working { display: inline-flex; align-items: center; gap: 5px; color: var(--text-muted); font-size: 12px; }
  .iv-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font-size: 11.5px;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .iv-search { max-width: 340px; }

  .iv-scroll { overflow: auto; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }

  .iv-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .iv-table th, .iv-table td { border-bottom: 1px solid var(--border-subtle); }

  .iv-slot-th, .iv-obj-th {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--bg-elevated);
    text-align: left;
    padding: 7px 10px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .iv-slot { display: flex; align-items: center; gap: 4px; margin-bottom: 3px; }
  /* The column said in words, under the two chips: the chips are the shorthand
     and this is what they stand for. */
  .iv-slot-label {
    display: block;
    font-size: 10.5px;
    color: var(--text-secondary);
    text-transform: none;
    letter-spacing: 0;
    font-weight: 600;
  }
  .iv-slot-name { font-size: 10px; color: var(--text-disabled); text-transform: none; letter-spacing: 0; }

  /* Kind headings inside the one matrix — the columns still line up across them. */
  .iv-group-row th { border-bottom: 1px solid var(--border); }
  .iv-group {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 10px 5px;
    background: var(--bg-elevated);
    text-align: left;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .iv-group :global(svg) { color: var(--text-secondary); }
  .iv-group-name { letter-spacing: 0.07em; }
  .iv-group-n {
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0;
    color: var(--text-disabled);
    font-variant-numeric: tabular-nums;
  }

  .iv-obj {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 10px;
    text-align: left;
    font-weight: 400;
    white-space: nowrap;
  }
  .iv-obj :global(svg) { color: var(--text-muted); }
  .iv-obj-name { font-family: var(--font-code); font-size: 11.5px; }

  /* Disclosure for the per-folder detail of one object. */
  .iv-twist {
    display: inline-flex;
    padding: 0;
    background: none;
    border: none;
    color: var(--text-disabled);
    cursor: pointer;
    transition: transform var(--transition-fast);
  }
  .iv-twist.iv-open { transform: rotate(90deg); }
  .iv-twist:hover { color: var(--text-primary); }

  /* Statements the columns do not account for — never rounded away. */
  .iv-stray {
    font-size: 10px;
    color: var(--warning);
    text-transform: none;
    letter-spacing: 0;
  }

  .iv-cell {
    padding: 0;
    text-align: center;
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
  }
  /* The number IS the control: every cell opens what it counts, zeroes included —
     "nothing here" is the answer people most want to check. */
  .iv-cell-btn {
    width: 100%;
    padding: 6px 10px;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .iv-cell-btn:hover { background: var(--bg-hover); }
  .iv-cell-open { background: var(--bg-active); box-shadow: inset 0 -2px 0 var(--accent); }
  /* A gap is the thing worth seeing: something one side never mentions. */
  .iv-zero { color: var(--error); font-weight: 700; background: var(--error-subtle); }
  /* More than one statement is not wrong, but it is worth a glance (DUP002). */
  .iv-many { color: var(--warning); font-weight: 600; }

  .iv-table tbody tr:hover td,
  .iv-table tbody tr:hover th { background: var(--bg-hover); }

  .iv-detail-row:hover td { background: var(--bg-elevated); }
  .iv-detail { padding: 8px 10px 10px 28px; background: var(--bg-elevated); }
  .iv-detail-grid {
    display: flex;
    gap: 22px;
    flex-wrap: wrap;
  }
  .iv-detail-col { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .iv-detail-head {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    margin-bottom: 2px;
  }
  .iv-detail-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-secondary);
  }
  .iv-detail-path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 46ch; }
  .iv-detail-n { margin-left: auto; font-variant-numeric: tabular-nums; }
  .iv-detail-zero { color: var(--error); }

  .iv-note { font-size: 11.5px; line-height: 1.55; color: var(--text-muted); max-width: 90ch; }
</style>
