<script lang="ts">
  /**
   * How each foreign key of the table becomes an association — fetch, cascade, and what the database
   * does when the referenced row goes. One row per `@ManyToOne` the generator will write; the defaults
   * are the safe ones (lazy, no cascade, no `@OnDelete`), so a row is only touched to depart from them.
   */
  import Select from '$lib/components/shared/ui/Select.svelte';
  import {
    DEFAULT_JOIN,
    type CascadeKind,
    type ForeignKeyAssociation,
    type JoinTweak,
  } from './entity-generate';

  let { associations, joins, onchange }: {
    associations: ForeignKeyAssociation[];
    joins: Record<string, Partial<JoinTweak>>;
    onchange: (column: string, tweak: Partial<JoinTweak>) => void;
  } = $props();

  const CASCADES = [
    { value: '', label: 'No cascade' },
    { value: 'ALL', label: 'Cascade ALL' },
    { value: 'PERSIST,MERGE', label: 'Persist + merge' },
    { value: 'REMOVE', label: 'Remove' },
  ];
  const FETCHES = [{ value: 'lazy', label: 'Lazy' }, { value: 'eager', label: 'Eager' }];
  const ON_DELETE = [
    { value: 'none', label: 'Nothing on delete' },
    { value: 'cascade', label: 'Delete with parent' },
    { value: 'set-null', label: 'Set null' },
  ];

  const valueOf = (column: string): JoinTweak => ({ ...DEFAULT_JOIN, ...joins[column] });
</script>

{#if associations.length}
  <div class="joins" role="group" aria-label="Associations">
    {#each associations as a (a.column)}
      {@const v = valueOf(a.column)}
      <div class="join">
        <div class="head">
          <code class="col">{a.column}</code>
          <span class="arrow">→</span>
          <span class="target" class:unknown={!a.known}>{a.target}</span>
        </div>
        <div class="row">
          <Select fill size="sm" ariaLabel="Fetch for {a.column}" value={v.fetch}
            onchange={(x) => onchange(a.column, { fetch: x as JoinTweak['fetch'] })} options={FETCHES} />
          <Select fill size="sm" ariaLabel="Cascade for {a.column}" value={v.cascade.join(',')}
            onchange={(x) => onchange(a.column, { cascade: (x ? x.split(',') : []) as CascadeKind[] })} options={CASCADES} />
          <Select fill size="sm" ariaLabel="On delete for {a.column}" value={v.onDelete}
            onchange={(x) => onchange(a.column, { onDelete: x as JoinTweak['onDelete'] })} options={ON_DELETE} />
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .joins { display: flex; flex-direction: column; gap: 8px; }
  .join { display: flex; flex-direction: column; gap: 4px; padding: 6px 8px; border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-surface, transparent); }
  .head { display: flex; align-items: center; gap: 6px; min-width: 0; font-size: var(--font-size-xs); }
  .col { font-family: var(--font-code); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arrow { color: var(--text-muted); }
  .target { color: var(--text-primary); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .target.unknown { color: var(--warning); }
  .row { display: grid; grid-template-columns: 1fr 1.2fr 1.4fr; gap: 6px; }
</style>
