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
   *
   * ## One table per kind
   *
   * The rows are split into a table per object kind rather than run together under
   * heading rows: the question people bring here is about one kind at a time. The
   * columns still line up across all of them — see `InventoryMatrix` — so the
   * comparison the single-table shape was protecting is intact.
   */
  import { TriangleAlert, CheckCircle2, Eye, EyeOff } from 'lucide-svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import InventoryLegend from '../inventory/InventoryLegend.svelte';
  import InventoryMatrix from '../inventory/InventoryMatrix.svelte';
  import NoticeList from '../panels/NoticeList.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { coverageBuckets, ignoredFileCount, objectsWithGaps } from '$lib/utils/picus/coverage';
  import type { InventoryObject, ObjectKind } from '$lib/types/picus';

  let query = $state('');
  const needle = $derived(query.trim().toLowerCase());

  /** Matrix columns: one per engine × role that this repository actually has. */
  const buckets = $derived(coverageBuckets(picusProjectStore.tree));
  const hidden = $derived(ignoredFileCount(picusProjectStore.tree));

  /**
   * Objects this repository only reads — a table another repository installs,
   * read here by a view.
   *
   * Shown by default because "we read something we do not own" is worth knowing,
   * and hideable because on a repository full of views it is most of the list.
   */
  let showExternal = $state(true);
  const externalCount = $derived(picusProjectStore.inventory.filter((o) => o.external).length);

  const rows = $derived(
    picusProjectStore.inventory.filter(
      (o) => (showExternal || !o.external) && (!needle || o.name.toLowerCase().includes(needle)),
    ),
  );

  // Counted through the same `gapKeys` the cells are marked from, so the headline
  // figure and the marks below it can never disagree. That they once could — this
  // number was its own `=== 0` test — is what made the table look alarming next to
  // a consistency report holding two findings.
  const gapCount = $derived(objectsWithGaps(picusProjectStore.inventory, buckets).length);

  /**
   * Rows grouped by what they are — one table each.
   *
   * A flat list of four hundred names, tables and triggers and sequences mixed
   * together, is a list you can only search — and searching is what the filter
   * above is for. Split, the same list can be *read*: the twelve views are twelve
   * rows in one place, and "do we have a PostgreSQL counterpart for every Oracle
   * package" becomes a question you can answer by looking.
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
   * Which object's detail is open, and what kind of detail — one at a time across
   * every table, because both are deep and two open at once is a page you scroll
   * rather than read.
   *
   * `bucket` set means the user clicked a *cell* and wants the mentions behind
   * that number; `bucket` null means they clicked the twisty and want the
   * folder-by-folder breakdown.
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
        define it. A marked dash is one side staying silent about something another side
        installs. Open a row for the folder-by-folder detail behind its columns.
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
    {#if externalCount}
      <Button
        variant={showExternal ? 'secondary' : 'primary'}
        size="xs"
        tooltip={'Objects nothing here creates, alters or writes to — a table another repository installs, read by a view. They are never counted as gaps, whether they are shown or not.'}
        onclick={() => (showExternal = !showExternal)}
      >
        {#snippet iconStart()}
          {#if showExternal}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
        {/snippet}
        {showExternal ? `Hide ${externalCount} read from elsewhere` : `Show ${externalCount} read from elsewhere`}
      </Button>
    {/if}
  </div>

  <InventoryLegend />

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
    <div class="iv-tables">
      {#each groups as group (group.kind)}
        <InventoryMatrix
          kind={group.kind}
          objects={group.objects}
          {buckets}
          {openObject}
          {openBucket}
          onToggleRow={toggleRow}
          onToggleCell={toggleCell}
        />
      {/each}
    </div>

    {#if hidden}
      <p class="iv-note">
        {hidden} file{hidden === 1 ? '' : 's'} sit under folders whose role is <b>ignored</b>.
        They are not indexed, so they are not a column here — a column of dashes for them
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

  .iv-search { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .iv-search > :global(:first-child) { max-width: 340px; flex: 1; }

  /* One table per kind, stacked. The gap is generous on purpose: the point of the
     split is that each table reads as its own answer. */
  .iv-tables { display: flex; flex-direction: column; gap: 16px; min-width: 0; }

  .iv-note { font-size: 11.5px; line-height: 1.55; color: var(--text-muted); max-width: 90ch; }
</style>
