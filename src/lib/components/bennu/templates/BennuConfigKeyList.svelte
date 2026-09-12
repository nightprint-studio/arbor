<script lang="ts">
  /**
   * The keys under a prefix, each with a box: what the configuration class gets.
   *
   * A group's box stands for every key below it — ticked when all of them are, a bar when some are — so
   * a whole group goes in or out with one click and a single key without opening anything.
   *
   * A group also says what it becomes: a type with a field per key, or a `Map<String, …>` of one type —
   * the second by default when its entries look like names of one thing, `postgres1` and `postgres2`.
   */
  import { Braces } from 'lucide-svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import type { ConfigKeyNode } from '$lib/ipc/bennu/templates';

  let {
    nodes,
    chosen,
    onToggle,
    maps,
    onMap,
  }: {
    nodes: ConfigKeyNode[];
    /** The chosen keys — single keys and lists, never a group, which is what is below it. */
    chosen: ReadonlySet<string>;
    onToggle: (keys: string[], on: boolean) => void;
    /** The shape chosen for a group, by key — absent where the default stands. */
    maps: ReadonlyMap<string, boolean>;
    onMap: (key: string, asMap: boolean) => void;
  } = $props();

  const leaves = $derived(nodes.filter((n) => !n.group).map((n) => n.key));
  const chosenCount = $derived(leaves.filter((k) => chosen.has(k)).length);

  function leavesOf(node: ConfigKeyNode): string[] {
    if (!node.group) return [node.key];
    const base = `${node.key}.`;
    return leaves.filter((k) => k.startsWith(base));
  }
</script>

<div class="kl">
  <div class="kl-head">
    <Checkbox
      checked={leaves.length > 0 && chosenCount === leaves.length}
      indeterminate={chosenCount > 0}
      label={`${chosenCount} of ${leaves.length} keys`}
      onchange={(on) => onToggle(leaves, on)}
    />
  </div>
  <ul class="kl-list">
    {#each nodes as node (node.key)}
      {@const below = leavesOf(node)}
      <li class="kl-row" style:padding-left={`${10 + node.depth * 14}px`}>
        <Checkbox
          checked={below.length > 0 && below.every((k) => chosen.has(k))}
          indeterminate={below.some((k) => chosen.has(k))}
          ariaLabel={node.key}
          onchange={(on) => onToggle(below, on)}
        />
        <span class="kl-name" class:group={node.group}>{node.name}</span>
        {#if node.sample}<span class="kl-sample">{node.sample}</span>{/if}
        {#if node.group}
          {@const asMap = maps.get(node.key) ?? node.map}
          <span class="kl-shape">
            <IconButton
              tooltip={asMap ? 'A Map<String, …> of one type — click for a type with a field per key' : 'A type with a field per key — click for a Map<String, …>'}
              size={20}
              active={asMap}
              onclick={() => onMap(node.key, !asMap)}
            >
              <Braces size={11} />
            </IconButton>
          </span>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .kl { display: flex; flex-direction: column; min-height: 0; }
  .kl-head { padding: 8px 10px; border-bottom: 1px solid var(--border-subtle); font-size: var(--font-size-xs); color: var(--text-secondary); }
  .kl-list { list-style: none; margin: 0; padding: 4px 0; overflow: auto; }
  .kl-row { display: flex; align-items: center; gap: 6px; padding-top: 2px; padding-bottom: 2px; padding-right: 10px; min-width: 0; }
  .kl-name { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); white-space: nowrap; }
  .kl-name.group { color: var(--text-secondary); }
  .kl-shape { margin-left: auto; flex-shrink: 0; }
  .kl-sample { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
</style>
