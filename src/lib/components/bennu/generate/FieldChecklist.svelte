<script lang="ts" module>
  export interface ChecklistItem {
    /** The value the selection holds. */
    name: string;
    /** What the row reads, when not the name. */
    label?: string;
    /** Shown muted at the right: a type, a path. */
    detail?: string;
    /** A small leading tag: an HTTP method, `id`. */
    badge?: string;
    group?: string;
  }
</script>

<script lang="ts">
  /**
   * A checklist of named things with a detail beside each — the fields of a class, the endpoints of a
   * controller. The selection is a list of names so it round-trips through a generator's options as it is;
   * a group header selects or clears its whole group.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';

  let {
    items,
    selected = $bindable<string[]>([]),
    empty = 'Nothing to choose from.',
    ariaLabel,
    maxHeight = 220,
  }: {
    items: ChecklistItem[];
    selected?: string[];
    empty?: string;
    ariaLabel: string;
    maxHeight?: number;
  } = $props();

  const chosen = $derived(new Set(selected));

  const groups = $derived.by(() => {
    const out: { name: string; items: ChecklistItem[] }[] = [];
    for (const item of items) {
      const name = item.group ?? '';
      let group = out.find((g) => g.name === name);
      if (!group) out.push((group = { name, items: [] }));
      group.items.push(item);
    }
    return out;
  });

  function toggle(name: string, on: boolean) {
    selected = on ? [...selected, name] : selected.filter((n) => n !== name);
  }

  function toggleGroup(group: { items: ChecklistItem[] }, on: boolean) {
    const names = new Set(group.items.map((i) => i.name));
    selected = on ? [...selected.filter((n) => !names.has(n)), ...names] : selected.filter((n) => !names.has(n));
  }

  const METHOD_TONE: Record<string, 'success' | 'info' | 'warning' | 'error' | 'neutral'> = {
    GET: 'success', POST: 'info', PUT: 'warning', PATCH: 'warning', DELETE: 'error',
  };
</script>

<div class="fcl" role="group" aria-label={ariaLabel} style:max-height={`${maxHeight}px`}>
  {#if items.length === 0}
    <EmptyState compact message={empty} />
  {/if}
  {#each groups as group (group.name)}
    {#if group.name}
      <div class="fcl-group">
        <Checkbox
          checked={group.items.every((i) => chosen.has(i.name))}
          indeterminate={group.items.some((i) => chosen.has(i.name)) && !group.items.every((i) => chosen.has(i.name))}
          label={group.name}
          labelSize="xs"
          onchange={(on) => toggleGroup(group, on)}
        />
      </div>
    {/if}
    {#each group.items as item (item.name)}
      <div class="fcl-row" class:indented={!!group.name}>
        <Checkbox checked={chosen.has(item.name)} ariaLabel={item.name} onchange={(on) => toggle(item.name, on)} />
        {#if item.badge}
          <Badge variant="tone" tone={METHOD_TONE[item.badge] ?? 'neutral'} size="sm" label={item.badge} />
        {/if}
        <span class="fcl-name">{item.label ?? item.name}</span>
        {#if item.detail}<span class="fcl-detail">{item.detail}</span>{/if}
      </div>
    {/each}
  {/each}
</div>

<style>
  .fcl {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 3px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow-x: hidden;
    overflow-y: auto;
    min-height: 0;
  }
  .fcl-group { padding: 4px 6px 2px; }
  .fcl-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 7px;
    border-radius: var(--radius-sm);
    min-width: 0;
  }
  .fcl-row.indented { padding-left: 24px; }
  .fcl-row:hover { background: var(--bg-hover); }
  .fcl-name {
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fcl-detail {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    flex: 0 1 auto;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
