<script lang="ts">
  /**
   * Inventory — the coverage matrix: every indexed object against every
   * branch/folder that could define or touch it.
   *
   * Reading it is the point. A row of ones means the two branches agree. A zero
   * is a hole: something exists in Oracle and not in PostgreSQL, or in the
   * initialisation and not in the updates. Those holes are the `CONS001` /
   * `CONS002` / `CONS003` family, and this table is where you see them before
   * an installation does.
   */
  import { Table2, Package, TriangleAlert, CheckCircle2 } from 'lucide-svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';

  let query = $state('');
  const needle = $derived(query.trim().toLowerCase());

  /** Matrix columns: one per branch/folder, carrying its dialect and role. */
  const slots = $derived(
    picusProjectStore.branches.flatMap((b) =>
      b.folders.map((f) => ({
        key: `${b.id}/${f.id}`,
        branch: b.label,
        folder: f.label,
        dialect: b.dialect,
        role: f.role,
      })),
    ),
  );

  const rows = $derived(
    picusProjectStore.inventory.filter((o) => !needle || o.name.toLowerCase().includes(needle)),
  );

  const gapCount = $derived(
    picusProjectStore.inventory.filter((o) => slots.some((s) => (o.coverage[s.key] ?? 0) === 0)).length,
  );
</script>

<div class="iv">
  <header class="iv-head">
    <div>
      <h1>Inventory</h1>
      <p>
        Every object the scripts define or touch, against every place that could define it.
        A zero is a branch that stays silent about something the other one says.
      </p>
    </div>
    <div class="iv-summary">
      {#if gapCount}
        <span class="iv-gaps"><TriangleAlert size={13} /> {gapCount} object{gapCount === 1 ? '' : 's'} with gaps</span>
      {:else}
        <span class="iv-ok"><CheckCircle2 size={13} /> Branches agree</span>
      {/if}
      <button class="iv-link" onclick={() => picusUiStore.showBottom('consistency')}>
        Open the consistency report
      </button>
    </div>
  </header>

  <div class="iv-search">
    <SearchBar bind:query showRegex={false} placeholder="Filter objects" ariaLabel="Filter objects" />
  </div>

  {#if !rows.length}
    <StateBlock
      tone="info"
      fill={false}
      label={picusProjectStore.inventory.length ? `Nothing matches “${query}”.` : 'Nothing indexed yet.'}
    />
  {:else}
    <div class="iv-scroll">
      <table class="iv-table">
        <thead>
          <tr>
            <th class="iv-obj-th" scope="col">Object</th>
            {#each slots as slot (slot.key)}
              <th scope="col" class="iv-slot-th">
                <span class="iv-slot">
                  <PicusDialectChip dialect={slot.dialect} terse />
                  <PicusRoleChip role={slot.role} terse />
                </span>
                <span class="iv-slot-name" title={`${slot.branch} / ${slot.folder}`}>{slot.folder}</span>
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as obj (obj.name)}
            <tr>
              <th scope="row" class="iv-obj">
                {#if obj.kind === 'table'}<Table2 size={13} />{:else}<Package size={13} />{/if}
                <span class="iv-obj-name">{obj.name}</span>
                <Badge variant="tone" tone="neutral" size="sm" label={obj.kind} />
              </th>
              {#each slots as slot (slot.key)}
                {@const n = obj.coverage[slot.key] ?? 0}
                <td class="iv-cell" class:iv-zero={n === 0} class:iv-many={n > 1}>
                  <span
                    use:tooltip={n === 0
                      ? `${obj.name} is never touched in ${slot.branch} / ${slot.folder}`
                      : `${n} statement${n === 1 ? '' : 's'} in ${slot.branch} / ${slot.folder}`}
                  >
                    {n === 0 ? '—' : n}
                  </span>
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
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
  .iv-slot-name { font-size: 10px; color: var(--text-disabled); }

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

  .iv-cell {
    padding: 6px 10px;
    text-align: center;
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
  }
  /* A gap is the thing worth seeing: an object one branch never mentions. */
  .iv-zero { color: var(--error); font-weight: 700; background: var(--error-subtle); }
  /* More than one statement is not wrong, but it is worth a glance (DUP002). */
  .iv-many { color: var(--warning); font-weight: 600; }

  .iv-table tbody tr:hover td,
  .iv-table tbody tr:hover th { background: var(--bg-hover); }
</style>
