<script lang="ts">
  /**
   * One direction of the dependency graph, as a tree grown from one object.
   *
   * ## Why a tree and not a graph drawing
   *
   * A schema has hundreds of objects and thousands of edges. Drawn as nodes on a
   * canvas that is a cloud — unreadable at any zoom, and it would need a layout
   * library we are not adding. Grown from *the object you are looking at*, the same
   * data is a list you can read a line at a time: this needs that, because of this
   * constraint.
   *
   * ## Cycles end, they do not recur
   *
   * The tree is unrolled from a graph, so a cycle would unroll forever. Each branch
   * carries the names on the path above it, and a name that appears twice is drawn
   * once and marked — the cycle is *stated*, which is what somebody ordering an
   * install actually needs to know, rather than silently pruned or expanded until
   * the tab dies.
   *
   * ## Children are built on expansion, not up front
   *
   * The row list is derived from the expansion set, so an unexpanded branch has no
   * children built at all. That is what keeps the whole thing linear in what is on
   * screen instead of exponential in the graph's density.
   */
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { ExternalLink } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { DependencyEdge } from '$lib/ipc/picus/depends';
  import { dependsStore, edgeReason, edgeTone } from '$lib/stores/picus/depends.svelte';
  import { iconForKind, isOpenable } from './depends-icons';

  interface Props {
    /** The object at the top. Empty renders nothing. */
    root: string;
    /** `needs` walks what the root depends on; `usedBy` walks what depends on it. */
    direction: 'needs' | 'usedBy';
    /** Re-root the panel here — Enter, or a double click. */
    onFollow: (name: string) => void;
    /** Open the object's own tab. Only offered for kinds that have one. */
    onOpen: (name: string, kind: string) => void;
  }

  let { root, direction, onFollow, onOpen }: Props = $props();

  /** One position in the unrolled tree. `id` is the path, which is what makes two
   *  appearances of the same object two independent rows. */
  interface DepRow {
    id: string;
    name: string;
    kind: string;
    /** The edge that led here — the "why". Null on the root. */
    edge: DependencyEdge | null;
    /** This name is already on the path above: the branch stops here. */
    cycle: boolean;
    hasKids: boolean;
    children: DepRow[];
  }

  const ROOT_ID = '/';

  let expanded = $state<Set<string>>(new Set([ROOT_ID]));

  // A new root (or a flipped direction) invalidates every path id, so the old
  // expansion would be a set of ids that no longer exist — carried around, matching
  // nothing, and occasionally matching the wrong thing.
  $effect(() => {
    void root;
    void direction;
    expanded = new Set([ROOT_ID]);
  });

  function edgesOf(name: string): DependencyEdge[] {
    return direction === 'needs' ? dependsStore.dependsOn(name) : dependsStore.usedBy(name);
  }

  /** The object at the other end of an edge, in this direction. */
  function far(edge: DependencyEdge): string {
    return direction === 'needs' ? edge.to : edge.from;
  }

  function branch(
    name: string,
    edge: DependencyEdge | null,
    id: string,
    ancestors: string[],
  ): DepRow {
    const cycle = ancestors.includes(name);
    const kids = cycle ? [] : edgesOf(name);
    const row: DepRow = {
      id,
      name,
      kind: dependsStore.resolve(name).kind,
      edge,
      cycle,
      hasKids: kids.length > 0,
      children: [],
    };
    if (row.hasKids && expanded.has(id)) {
      const path = [...ancestors, name];
      row.children = kids.map((e, i) => branch(far(e), e, `${id}${i}:${far(e)}/`, path));
    }
    return row;
  }

  const nodes = $derived<DepRow[]>(root ? [branch(root, null, ROOT_ID, [])] : []);

  /**
   * Whether the root has anything at all in this direction.
   *
   * Asked here rather than left to the Tree's own empty state: the tree always has
   * one row — the root — so it is never empty in the widget's terms, and "this
   * object depends on nothing" would render as a lone row with a chevron that does
   * not open. That is a fact worth a sentence.
   */
  const barren = $derived(!!root && edgesOf(root).length === 0);

  function setExpanded(id: string, next: boolean) {
    const s = new Set(expanded);
    if (next) s.add(id);
    else s.delete(id);
    expanded = s;
  }

  function rowTitle(node: DepRow): string {
    if (node.cycle) return `${node.name} — already above in this branch (a cycle)`;
    if (!node.edge) return node.name;
    return direction === 'needs'
      ? `${node.name} — needed because of the ${edgeReason(node.edge)}`
      : `${node.name} — needs this because of the ${edgeReason(node.edge)}`;
  }
</script>

{#if !root}
  <div class="dt-empty">No object selected.</div>
{:else if barren}
  <div class="dt-empty">
    {direction === 'needs' ? `${root} depends on nothing.` : `Nothing depends on ${root}.`}
  </div>
{:else}
  <Tree
    {nodes}
    getId={(n) => n.id}
    getChildren={(n) => n.children}
    hasChildren={(n) => n.hasKids}
    expandedIds={expanded}
    onExpandToggle={(id, next) => setExpanded(id, next)}
    onActivate={(n) => onFollow(n.name)}
    onRowKeydown={(n, e) => {
      // Enter follows rather than toggles: Space and the arrows already expand,
      // and "go to this object" is the verb this panel exists for.
      if (e.key === 'Enter') {
        e.preventDefault();
        onFollow(n.name);
      }
    }}
    {rowTitle}
    rowHeight={22}
    guides
    ariaLabel={direction === 'needs' ? 'Depends on' : 'Used by'}
  >
    {#snippet row({ node })}
      {@const Icon = iconForKind(node.kind)}
      <span class="tree-icon"><Icon size={12} /></span>
      <span class="tree-label">{node.name}</span>
      {#if node.edge}
        <Badge variant="tone" tone={edgeTone(node.edge.kind)} size="sm">
          {edgeReason(node.edge)}
        </Badge>
      {/if}
      {#if node.cycle}
        <Badge variant="tone" tone="warning" size="sm">cycle</Badge>
      {/if}
      {#if isOpenable(node.kind)}
        <span class="tree-actions">
          <button
            class="tree-row-action accent"
            use:tooltip={'Open this object'}
            aria-label="Open {node.name}"
            onclick={(e) => {
              e.stopPropagation();
              onOpen(node.name, node.kind);
            }}
          >
            <ExternalLink size={11} />
          </button>
        </span>
      {/if}
    {/snippet}
  </Tree>
{/if}

<style>
  .dt-empty {
    padding: 14px 12px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-style: italic;
  }
</style>
