<script lang="ts">
  /**
   * The expanded body of one connection row: what the session is pinned to, and
   * the four groups the schema divides into.
   *
   * Split out of `ConnectionsPanel` because the two answer different questions —
   * the panel is "which databases do I have", this is "what is inside this one" —
   * and because a real schema is big enough that its rendering deserves its own
   * rules.
   *
   * Density is the design here. A schema with several hundred tables is the
   * normal case, not the pathological one, so an object is **one line**: the name
   * is what the eye scans, and its metadata (columns, foreign keys, row estimate)
   * rides on the same row, revealed on the row under the pointer. Two-line rows
   * cost the same information twice the scroll, and at 700 tables that is the
   * difference between a list and a wall.
   *
   * Groups start collapsed for the same reason: the counts on the headers answer
   * "how much is in here" without rendering any of it, and the filter above is
   * the way in once you know what you are looking for.
   */
  import { ChevronRight, Table2, Eye, ListOrdered, Zap } from 'lucide-svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { SCHEMA_GROUP_LABELS, type Connection, type SchemaGroup } from '$lib/types/picus';

  interface Props {
    connection: Connection;
    /** Lower-cased filter text from the panel's search bar; '' when not filtering. */
    needle: string;
    /** Put the caret back in the filter box — the way out of a truncated group. */
    onNarrow?: () => void;
  }

  let { connection, needle, onNarrow }: Props = $props();

  /**
   * How many rows of one group are drawn before the list stops and points at the
   * filter.
   *
   * Not a display limit for its own sake: every row is a live DOM node, and four
   * groups of a real schema unrolled at once is thousands of them for a list
   * nobody reads to the end. Past this many the honest answer is "narrow it".
   */
  const MAX_ROWS = 300;

  const GROUPS: SchemaGroup[] = ['tables', 'views', 'sequences', 'triggers'];

  const GROUP_ICONS: Record<SchemaGroup, any> = {
    tables: Table2,
    views: Eye,
    sequences: ListOrdered,
    triggers: Zap,
  };

  /** Which groups are open. Collapsed by default — a filter opens them instead. */
  let expanded = $state<Record<string, boolean>>({});

  const filtering = $derived(needle !== '');

  /** True while the loaded snapshot is the one this connection describes. */
  const isLoaded = $derived(schemaStore.connectionId === connection.id);

  function listOf(group: SchemaGroup): { name: string }[] {
    return group === 'tables' ? schemaStore.tables
      : group === 'views' ? schemaStore.views
      : group === 'sequences' ? schemaStore.sequences
      : schemaStore.triggers;
  }

  /** The names of a group that pass the filter. */
  function namesIn(group: SchemaGroup): string[] {
    const out: string[] = [];
    for (const o of listOf(group)) {
      if (!needle || o.name.toLowerCase().includes(needle)) out.push(o.name);
    }
    return out;
  }

  /** The tab kind an object of this group opens as. */
  function objectKindOf(group: SchemaGroup) {
    return group === 'tables' ? 'table' as const
      : group === 'views' ? 'view' as const
      : group === 'sequences' ? 'sequence' as const
      : 'trigger' as const;
  }

  /**
   * What the object is, in its own terms — the trailing metadata of a row.
   *
   * Deliberately unabbreviated: only the hovered row shows it, so it can afford
   * to be words rather than a code the user has to decipher.
   */
  function detailFor(group: SchemaGroup, name: string): string | null {
    if (group === 'tables' || group === 'views') {
      const rel = schemaStore.relation(name);
      if (!rel) return null;
      const parts = [`${rel.columns.length} columns`];
      if (rel.foreignKeys?.length) parts.push(`${rel.foreignKeys.length} FK`);
      if (rel.estimatedRows != null) parts.push(`~${rel.estimatedRows.toLocaleString()} rows`);
      return parts.join(' · ');
    }
    if (group === 'sequences') {
      const seq = schemaStore.sequence(name);
      return seq ? `last ${seq.lastValue.toLocaleString()} · step ${seq.incrementBy}` : null;
    }
    const trg = schemaStore.trigger(name);
    // What fires it is the group it is under — repeating it here is what left
    // `AFTER INSERT/UPDATE/DELETE on astra` sitting on top of the name, so the
    // names read `art88…`, `astra…` and the list stopped being usable.
    return trg ? `on ${trg.table}${trg.enabled ? '' : ' · disabled'}` : null;
  }

  /**
   * Triggers, split by what fires them.
   *
   * A schema's triggers are dozens of names that differ by two characters and are
   * told apart by their timing and their events — which is exactly the wrong thing
   * to carry on each row, because it is long, it is the same on every row of a run,
   * and it wins the fight for the width against the one thing that is unique. As a
   * heading it costs one line per kind, says the count, and gives the whole row
   * back to the name.
   */
  interface TriggerBucket { key: string; label: string; names: string[] }

  function triggerBuckets(names: string[]): TriggerBucket[] {
    const map = new Map<string, string[]>();
    for (const name of names) {
      const trg = schemaStore.trigger(name);
      const label = trg ? `${trg.timing} ${trg.events.join('/')}` : 'unknown';
      const list = map.get(label) ?? [];
      list.push(name);
      map.set(label, list);
    }
    return [...map.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([label, list]) => ({ key: `triggers:${label}`, label, names: list }));
  }

  /** Unlike the four top groups, a bucket is **open** by default: you already
   *  asked for the triggers by opening the group above it. */
  function isBucketOpen(key: string, hits: number): boolean {
    if (filtering) return hits > 0;
    return expanded[key] ?? true;
  }

  /** A filter opens whatever it found; otherwise the stored (default closed) state. */
  function isOpen(group: SchemaGroup, hits: number): boolean {
    if (filtering) return hits > 0;
    return expanded[group] ?? false;
  }

  function toggle(group: SchemaGroup, open: boolean) {
    expanded = { ...expanded, [group]: !open };
  }
</script>

<!-- One object, one line. Shared by the flat groups and by the trigger buckets so
     that a row means the same thing at either depth — the indent is the only
     difference, and passing it is cheaper than a second copy of the row. -->
{#snippet objectRow(group: SchemaGroup, name: string, indent: number)}
  <SidebarItem
    {indent}
    badgesOnHover
    selected={picusTabsStore.active?.table === name}
    onclick={() => picusTabsStore.openObject(name, objectKindOf(group), connection.id)}
  >
    <span class="cst-object">
      {connection.dialect === 'postgres' ? name.toLowerCase() : name}
    </span>
    {#snippet badges()}
      {@const detail = detailFor(group, name)}
      {#if detail}<span class="cst-detail">{detail}</span>{/if}
    {/snippet}
  </SidebarItem>
{/snippet}

<div class="cst-meta" style:--conn-color={connectionColorVar(connection)}>
  {connection.schema}
  {#if connection.dbVersion}· database version {connection.dbVersion}{/if}
  {#if isLoaded && schemaStore.loadedAt}
    <span class="cst-stamp">cached {schemaStore.loadedAt}</span>
  {/if}
</div>

{#if schemaStore.loading}
  <p class="cst-note"><Spinner size={10} /> reading schema…</p>
{:else if schemaStore.error}
  <!-- A failed read leaves no catalogue at all, so it is the answer for whichever
       connection is being looked at. -->
  <p class="cst-note cst-bad">{schemaStore.error}</p>
{:else if !isLoaded}
  <!-- One catalogue is held at a time, for the connection in use. Showing another
       connection's tables under this name is the kind of quiet wrongness that gets
       a DELETE written against the wrong database. -->
  <p class="cst-note">
    {connection.state === 'disconnected'
      ? 'Not connected — open the session to read its schema.'
      : 'Select this connection to read its schema.'}
  </p>
{:else}
  {#each GROUPS as group (group)}
    {@const names = namesIn(group)}
    {@const total = listOf(group).length}
    {@const open = isOpen(group, names.length)}
    {@const Icon = GROUP_ICONS[group]}
    <SidebarItem indent={22} onclick={() => toggle(group, open)}>
      {#snippet icon()}
        <span class="cst-twist" class:cst-open={open}><ChevronRight size={12} /></span>
      {/snippet}
      <Icon size={12} class="cst-group-icon" />
      <span class="cst-group">{SCHEMA_GROUP_LABELS[group]}</span>
      {#snippet badges()}
        <Badge variant="count" label={filtering ? `${names.length}/${total}` : String(total)} />
      {/snippet}
    </SidebarItem>

    {#if open}
      {@const shown = names.slice(0, MAX_ROWS)}
      {#if group === 'triggers'}
        <!-- Bucketed by what fires them; the ceiling is applied to the group as a
             whole first, so the buckets divide what is drawn rather than each
             getting a limit of its own. -->
        {#each triggerBuckets(shown) as bucket (bucket.key)}
          {@const bucketOpen = isBucketOpen(bucket.key, bucket.names.length)}
          <SidebarItem
            indent={40}
            onclick={() => (expanded = { ...expanded, [bucket.key]: !bucketOpen })}
          >
            {#snippet icon()}
              <span class="cst-twist" class:cst-open={bucketOpen}><ChevronRight size={11} /></span>
            {/snippet}
            <span class="cst-bucket">{bucket.label}</span>
            {#snippet badges()}
              <Badge variant="count" label={String(bucket.names.length)} />
            {/snippet}
          </SidebarItem>
          {#if bucketOpen}
            {#each bucket.names as name (name)}
              {@render objectRow(group, name, 58)}
            {/each}
          {/if}
        {/each}
      {:else}
        {#each shown as name (name)}
          {@render objectRow(group, name, 40)}
        {/each}
      {/if}

      {#if names.length > MAX_ROWS}
        <p class="cst-more">
          <span>{(names.length - MAX_ROWS).toLocaleString()} more not shown.</span>
          <button type="button" class="cst-narrow" onclick={() => onNarrow?.()}>
            Narrow the filter
          </button>
        </p>
      {:else if !names.length}
        <p class="cst-none">{filtering ? 'Nothing here matches the filter.' : 'Nothing in this group.'}</p>
      {/if}
    {/if}
  {/each}
{/if}

<style>
  .cst-twist {
    display: inline-flex;
    color: var(--text-disabled);
    transition: transform var(--transition-fast);
  }
  .cst-twist.cst-open { transform: rotate(90deg); }

  .cst-group {
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-secondary);
  }

  /* What fires a run of triggers, as a heading rather than on every row. Not
     upper-cased: `AFTER INSERT/UPDATE/DELETE` already is, and it is a fact from
     the catalogue rather than a section label of ours. */
  .cst-bucket {
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* One line, code face, the name and nothing else at rest. */
  .cst-object {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Trailing metadata — muted, capped, and only on the row under the pointer
     (SidebarItem's `badgesOnHover`), so it never competes with the names. */
  .cst-detail {
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }

  .cst-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 3px 12px 5px 34px;
    margin-left: 10px;
    border-left: 2px solid var(--conn-color);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }
  .cst-stamp { color: var(--text-disabled); }

  .cst-note {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 12px 6px 34px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .cst-bad { color: var(--error); }

  .cst-none {
    padding: 4px 12px 4px 46px;
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    font-style: italic;
  }

  .cst-more {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 4px 12px 6px 46px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .cst-narrow {
    padding: 0;
    background: none;
    border: none;
    color: var(--accent);
    font-family: inherit;
    font-size: inherit;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .cst-narrow:hover { color: var(--accent-hover); }

  :global(.cst-group-icon) { color: var(--text-muted); flex-shrink: 0; }
</style>
