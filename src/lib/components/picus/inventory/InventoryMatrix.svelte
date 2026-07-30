<script lang="ts">
  /**
   * The coverage matrix for **one kind of object** — tables, or views, or
   * sequences, one table each.
   *
   * A single table with heading rows in it was the older shape, and it was wrong
   * for the way this page is used. Nobody scans a four-hundred-row matrix top to
   * bottom; they come here asking a question about one kind — "does every Oracle
   * package have a PostgreSQL counterpart" — and a heading row buried in a long
   * scroll answers that question no better than a filter would.
   *
   * The columns still line up across every kind, which is what the one-table shape
   * was protecting: `table-layout: fixed` plus an identical column set gives each
   * table the same geometry, so an Oracle package and a PostgreSQL function are
   * still compared by looking straight down.
   *
   * Each table repeats the header. That costs a row per kind and is worth it: a
   * header you have scrolled past is a header that is not there.
   */
  import { ChevronRight } from 'lucide-svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import ObjectKindIcon, { OBJECT_KIND_LABELS } from '../PicusObjectKindIcon.svelte';
  import InventoryUsages from '../panels/InventoryUsages.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import {
    bucketCoverage,
    elsewhereCount,
    folderBreakdown,
    gapKeys,
    type CoverageBucket,
  } from '$lib/utils/picus/coverage';
  import type { InventoryObject, ObjectKind } from '$lib/types/picus';

  interface Props {
    kind: ObjectKind;
    objects: InventoryObject[];
    buckets: CoverageBucket[];
    /** `kind/name` of the object whose detail is open anywhere on the page. */
    openObject: string | null;
    /** Which column's mentions are open; `null` means the folder breakdown. */
    openBucket: string | null;
    onToggleRow: (obj: InventoryObject) => void;
    onToggleCell: (obj: InventoryObject, bucketKey: string) => void;
  }

  let { kind, objects, buckets, openObject, openBucket, onToggleRow, onToggleCell }: Props =
    $props();

  function objectKey(obj: InventoryObject) {
    return `${obj.kind}/${obj.name}`;
  }
</script>

<section class="im" aria-label={OBJECT_KIND_LABELS[kind] ?? kind}>
  <h2 class="im-title">
    <ObjectKindIcon {kind} />
    <span class="im-title-name">{OBJECT_KIND_LABELS[kind] ?? kind}</span>
    <span class="im-title-n">{objects.length}</span>
  </h2>

  <div class="im-scroll">
    <table class="im-table">
      <!-- Fixed geometry, identical across kinds: the columns of the sequences
           table land under the columns of the tables table, so the comparison the
           single-table shape protected still works by looking down. -->
      <colgroup>
        <col class="im-obj-col" />
        {#each buckets as bucket (bucket.key)}<col />{/each}
      </colgroup>
      <thead>
        <tr>
          <th class="im-obj-th" scope="col">Object</th>
          {#each buckets as bucket (bucket.key)}
            <!-- Spelled out rather than left to two terse chips: `ORA · init`
                 assumes the reader knows both vocabularies, and the number
                 underneath counts *statements*, which is not what "3 folders"
                 leads anyone to expect. Both are said. -->
            <th scope="col" class="im-slot-th">
              <span class="im-slot">
                <PicusDialectChip engine={bucket.dialect} terse />
                <PicusRoleChip role={bucket.role} terse />
              </span>
              <span class="im-slot-label">{bucket.label}</span>
              <span
                class="im-slot-name"
                use:tooltip={{
                  content: `Statements under ${bucket.folders.length} folder(s), ${bucket.fileCount} file(s)`,
                  description:
                    bucket.folders.slice(0, 12).join('\n')
                    + (bucket.folders.length > 12 ? `\n… and ${bucket.folders.length - 12} more` : ''),
                }}
              >
                {bucket.folders.length} folder{bucket.folders.length === 1 ? '' : 's'} · statements
              </span>
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        <!-- Kind AND name: a name alone is not unique — the same identifier can be
             indexed under two kinds — and a duplicate key is a hard Svelte error
             that takes the panel down rather than drawing one row twice. -->
        {#each objects as obj (objectKey(obj))}
          {@const key = objectKey(obj)}
          {@const gaps = gapKeys(obj, buckets)}
          {@const showingFolders = openObject === key && openBucket === null}
          {@const stray = elsewhereCount(obj, buckets)}
          <tr>
            <th scope="row" class="im-obj">
              <button
                class="im-twist"
                class:im-open={showingFolders}
                aria-expanded={showingFolders}
                aria-label={`Folder detail for ${obj.name}`}
                onclick={() => onToggleRow(obj)}
              >
                <ChevronRight size={12} />
              </button>
              <span class="im-obj-name" class:im-obj-external={obj.external}>{obj.name}</span>
              {#if obj.external}
                <!-- Said on the row, because otherwise a line of dashes is
                     indistinguishable from a real gap — and it is the reader
                     noticing that difference that this whole view is for. -->
                <span
                  class="im-external"
                  use:tooltip={'Nothing here creates, alters or writes to it — another repository installs it and this one reads it. Never counted as a gap.'}
                >
                  read only
                </span>
              {/if}
              {#if stray}
                <span
                  class="im-stray"
                  use:tooltip={'Statements in folders no column covers — an ignored folder, or one with no engine'}
                >
                  +{stray} elsewhere
                </span>
              {/if}
            </th>
            {#each buckets as bucket (bucket.key)}
              {@const n = bucketCoverage(obj, bucket)}
              {@const gap = gaps.has(bucket.key)}
              {@const open = openObject === key && openBucket === bucket.key}
              <!-- Only a real gap is marked. A count above one used to be tinted
                   as well, which made a table created once and altered by four
                   updates look like a problem — the ordinary shape of a healthy
                   repository, reported as a warning on nearly every row. -->
              <td class="im-cell" class:im-gap={gap}>
                <!-- Clickable including the dashes: "nothing here" is the answer
                     people most want to check, and opening it to see an empty list
                     is what turns a suspicion into a fact. -->
                <button
                  class="im-cell-btn"
                  class:im-cell-open={open}
                  aria-expanded={open}
                  use:tooltip={n === 0
                    ? gap
                      ? `${obj.name} is never touched under ${bucket.label}, and something else installs it — open to check`
                      : `${obj.name} is never touched under ${bucket.label}`
                    : `${n} statement${n === 1 ? '' : 's'} under ${bucket.label} — open to see where`}
                  onclick={() => onToggleCell(obj, bucket.key)}
                >
                  {n === 0 ? '—' : n}
                </button>
              </td>
            {/each}
          </tr>

          {#if showingFolders}
            <tr class="im-detail-row">
              <td class="im-detail" colspan={buckets.length + 1}>
                <!-- The folded detail, for this object only: which folder in each
                     column says something, and which stays quiet. -->
                <div class="im-detail-grid">
                  {#each buckets as bucket (bucket.key)}
                    <div class="im-detail-col">
                      <span class="im-detail-head">{bucket.label}</span>
                      {#each folderBreakdown(obj, bucket) as line (line.path)}
                        <span class="im-detail-line" class:im-detail-zero={line.count === 0}>
                          <span class="im-detail-path">{line.path}</span>
                          <span class="im-detail-n">{line.count === 0 ? '—' : line.count}</span>
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
              <tr class="im-detail-row">
                <td class="im-detail" colspan={buckets.length + 1}>
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
    </table>
  </div>
</section>

<style>
  .im { display: flex; flex-direction: column; gap: 6px; min-width: 0; }

  .im-title {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .im-title :global(svg) { color: var(--text-secondary); }
  .im-title-n {
    letter-spacing: 0;
    font-weight: 500;
    color: var(--text-disabled);
    font-variant-numeric: tabular-nums;
  }

  .im-scroll {
    overflow: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }

  .im-table { width: 100%; table-layout: fixed; border-collapse: collapse; font-size: 12px; }
  .im-obj-col { width: 42%; }
  .im-table th, .im-table td { border-bottom: 1px solid var(--border-subtle); }
  .im-table tbody tr:last-child > * { border-bottom: none; }

  .im-slot-th, .im-obj-th {
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
  }
  .im-slot { display: flex; align-items: center; gap: 4px; margin-bottom: 3px; }
  /* The column said in words under the two chips: the chips are the shorthand and
     this is what they stand for. */
  .im-slot-label {
    display: block;
    font-size: 10.5px;
    color: var(--text-secondary);
    text-transform: none;
    letter-spacing: 0;
    font-weight: 600;
  }
  .im-slot-name {
    display: block;
    font-size: 10px;
    color: var(--text-disabled);
    text-transform: none;
    letter-spacing: 0;
  }

  .im-obj {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 10px;
    text-align: left;
    font-weight: 400;
    min-width: 0;
  }
  .im-obj-name {
    font-family: var(--font-code);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Disclosure for the per-folder detail of one object. */
  .im-twist {
    display: inline-flex;
    flex-shrink: 0;
    padding: 0;
    background: none;
    border: none;
    color: var(--text-disabled);
    cursor: pointer;
    transition: transform var(--transition-fast);
  }
  .im-twist.im-open { transform: rotate(90deg); }
  .im-twist:hover { color: var(--text-primary); }

  /* An object this repository only reads: dimmed, because its dashes are the
     boundary of the repository rather than something to go and fix. */
  .im-obj-external { color: var(--text-muted); }
  .im-external {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--text-disabled);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }

  /* Statements the columns do not account for — never rounded away. */
  .im-stray { flex-shrink: 0; font-size: 10px; color: var(--warning); }

  .im-cell {
    padding: 0;
    text-align: center;
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
  }
  /* The number IS the control: every cell opens what it counts, dashes included. */
  .im-cell-btn {
    width: 100%;
    padding: 6px 10px;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .im-cell-btn:hover { background: var(--bg-hover); }
  .im-cell-open { background: var(--bg-active); box-shadow: inset 0 -2px 0 var(--accent); }
  /* The one verdict this table renders as colour — and it agrees, cell for cell,
     with what the consistency report raises. */
  .im-gap { color: var(--error); font-weight: 700; background: var(--error-subtle); }

  .im-table tbody tr:hover td,
  .im-table tbody tr:hover th { background: var(--bg-hover); }

  .im-detail-row:hover td { background: var(--bg-elevated); }
  .im-detail { padding: 8px 10px 10px 28px; background: var(--bg-elevated); }
  .im-detail-grid { display: flex; gap: 22px; flex-wrap: wrap; }
  .im-detail-col { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .im-detail-head {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    margin-bottom: 2px;
  }
  .im-detail-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-secondary);
  }
  .im-detail-path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 46ch;
  }
  .im-detail-n { margin-left: auto; font-variant-numeric: tabular-nums; }
  .im-detail-zero { color: var(--text-disabled); }
</style>
