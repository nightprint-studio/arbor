<script lang="ts">
  /**
   * Dependencies — what the object in front of you needs, what needs it, and in
   * what order the whole lot would have to be created.
   *
   * ## It starts from one object, always
   *
   * The graph is the schema's; the *panel* is about one object in it. A view of the
   * whole graph is a picture nobody can read on a real schema, and — more to the
   * point — nobody asks "show me every relationship in the database". They ask
   * "what breaks if I drop this", which is the same graph walked from one place.
   *
   * The root follows the tab you are on, until you pick something else; picking
   * sticks until the tab changes under it. That is the arrangement that lets the
   * panel be useful without being in the way — it is right by default and it does
   * not argue when you disagree.
   *
   * ## Creation order is the point, not a bonus
   *
   * The trees answer "why"; the ordered list answers "then what do I do". A
   * migration, or a repository installed in one transaction, has to emit objects in
   * an order that works, and that order is this graph sorted. It is a second view
   * of the same walk rather than a second panel, because it is the same question
   * asked with the answer already in hand.
   */
  import { untrack } from 'svelte';
  import { RefreshCw, TriangleAlert } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import DependsTree from './DependsTree.svelte';
  import { iconForKind, isOpenable } from './depends-icons';
  import { dependsStore } from '$lib/stores/picus/depends.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';

  const graph = $derived(dependsStore.graph);

  /** The object the active tab is showing, when it is showing one. */
  const activeTab = $derived(picusTabsStore.active);
  const tabObject = $derived(activeTab?.kind === 'table' ? (activeTab.table ?? '') : '');

  /** A root the user chose by hand. Cleared when the tab moves to another object. */
  let picked = $state('');
  /** The tab object the current pick was made against — plain, not reactive: it is
   *  a memory of what the effect below has already reacted to, not state anything
   *  renders from. */
  let followed = '';

  $effect(() => {
    const object = tabObject;
    if (!object || object === followed) return;
    followed = object;
    picked = '';
  });

  // Every candidate goes through the graph's own spelling: a tab that names an
  // object differently from the catalogue would otherwise root the panel on a name
  // no edge mentions, and read as an object with no dependencies.
  const root = $derived(
    dependsStore.matchName(picked)
      || dependsStore.matchName(tabObject)
      || graph?.nodes[0]?.name
      || '',
  );

  let mode = $state<'graph' | 'order'>('graph');
  const modes: TabItem[] = [
    { id: 'graph', label: 'Graph' },
    { id: 'order', label: 'Creation order' },
  ];

  const order = $derived(mode === 'order' && root ? dependsStore.creationOrder(root) : null);

  const options = $derived(
    (graph?.nodes ?? []).map((n) => ({ value: n.name, label: `${n.name} · ${n.kind}` })),
  );

  // Read the graph when the panel is on screen and the connection has one. The
  // store drops a second ask for a connection already being read, so an effect
  // re-firing costs nothing; `untrack` keeps the graph it writes from being a
  // dependency of the effect that asked for it.
  $effect(() => {
    void connectionsStore.activeId;
    untrack(() => {
      void dependsStore.loadCapabilities();
      void dependsStore.load();
    });
  });

  function openObject(name: string, kind: string) {
    if (!isOpenable(kind)) return;
    picusTabsStore.openObject(name, kind, connectionsStore.activeId);
  }
</script>

<div class="dp">
  <BottomPanelHeader
    title="Dependencies"
    count={graph?.nodes.length ?? null}
    onClose={() => picusUiStore.closeBottom()}
  >
    <Tabs
      items={modes}
      value={mode}
      size="sm"
      variant="underline"
      onSelect={(id) => (mode = id as 'graph' | 'order')}
      ariaLabel="Dependency view"
    />
    <div class="dp-root">
      <Select
        value={root}
        options={options}
        searchable
        searchPlaceholder="Find an object…"
        placeholder="No object"
        emptyMessage="The graph has not been read yet."
        maxHeight={320}
        onchange={(v) => (picked = v)}
      />
    </div>
    {#snippet actions()}
      <Button
        variant="icon"
        size="xs"
        tooltip="Re-read the dependency graph"
        ariaLabel="Re-read the dependency graph"
        disabled={dependsStore.loading || !connectionsStore.activeId}
        onclick={() => void dependsStore.load(true)}
      >
        {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
      </Button>
    {/snippet}
  </BottomPanelHeader>

  <div class="dp-body">
    {#if dependsStore.error}
      <StateBlock tone="error" label={dependsStore.error} />
    {:else if dependsStore.loading && !dependsStore.loaded}
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Walking the catalogue…</span>
      </StateBlock>
    {:else if !dependsStore.loaded}
      <StateBlock
        tone="info"
        label="Connect to read what this schema's objects depend on."
      />
    {:else if !root}
      <StateBlock tone="info" label="This schema has no objects to walk." />
    {:else if mode === 'graph'}
      <div class="dp-cols">
        <section class="dp-col">
          <h4 class="dp-col-title">
            Depends on
            <span class="dp-col-hint">{root} needs these first</span>
          </h4>
          <div class="dp-scroll">
            <DependsTree
              {root}
              direction="needs"
              onFollow={(name) => (picked = name)}
              onOpen={openObject}
            />
          </div>
        </section>
        <section class="dp-col">
          <h4 class="dp-col-title">
            Used by
            <span class="dp-col-hint">these need {root}</span>
          </h4>
          <div class="dp-scroll">
            <DependsTree
              {root}
              direction="usedBy"
              onFollow={(name) => (picked = name)}
              onOpen={openObject}
            />
          </div>
        </section>
      </div>
    {:else if order}
      <div class="dp-scroll dp-order-body">
        <p class="dp-lead">
          Everything <strong>{root}</strong> needs, in an order that would create it —
          {order.order.length} object(s).
        </p>
        <ol class="dp-order">
          {#each order.order as node, i (node.name)}
            {@const Icon = iconForKind(node.kind)}
            <li>
              <button
                type="button"
                class="dp-order-row"
                onclick={() => (picked = node.name)}
                ondblclick={() => openObject(node.name, node.kind)}
              >
                <span class="dp-num">{i + 1}</span>
                <Icon size={12} />
                <span class="dp-order-name">{node.name}</span>
                <Badge variant="tone" tone="neutral" size="sm">{node.kind}</Badge>
              </button>
            </li>
          {/each}
        </ol>

        {#if order.cyclic.length}
          <!-- Not an error and not hidden: two tables that reference each other is a
               real design, and the honest answer is "these cannot be created in one
               pass" rather than an order that pretends otherwise. -->
          <Alert variant="warning" compact title="These sit on a cycle">
            {order.cyclic.map((n) => n.name).join(', ')} — each needs another of them, so
            they cannot all be created in one pass. One of the constraints has to be
            added after the objects exist.
          </Alert>
        {/if}
      </div>
    {/if}

    {#if dependsStore.unresolved.length}
      <div class="dp-unresolved">
        <Collapsible chevron>
          {#snippet header()}
            <span class="dp-unresolved-head">
              <TriangleAlert size={12} />
              {dependsStore.unresolved.length} object(s) the catalogue could not be read for
            </span>
          {/snippet}
          <ul class="dp-unresolved-list">
            {#each dependsStore.unresolved as line (line)}
              <li>{line}</li>
            {/each}
          </ul>
        </Collapsible>
      </div>
    {/if}
  </div>
</div>

<style>
  .dp { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .dp-body { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  .dp-root { width: 220px; }

  /* Two columns while there is room, stacked when there is not — the panel is a
     dock and its height is the user's to give away. */
  .dp-cols { flex: 1; min-height: 0; display: flex; gap: 1px; background: var(--border-subtle); }
  .dp-col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
  }
  @media (max-width: 720px) {
    .dp-cols { flex-direction: column; }
  }

  .dp-col-title {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0;
    padding: 5px 12px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }
  .dp-col-hint {
    font-size: var(--font-size-2xs);
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The Tree widget does not own a scroll container — it climbs to the nearest
     scrollable ancestor. This is it. */
  .dp-scroll { flex: 1; min-height: 0; overflow: auto; }

  .dp-order-body { padding: 10px 12px 14px; }
  .dp-lead { margin: 0 0 8px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .dp-order { list-style: none; margin: 0 0 10px; padding: 0; }

  .dp-order-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 3px 6px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    text-align: left;
    cursor: pointer;
  }
  .dp-order-row:hover { background: var(--bg-hover); }
  .dp-order-row:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }

  .dp-num {
    min-width: 26px;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    text-align: right;
  }
  .dp-order-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }

  .dp-unresolved {
    flex-shrink: 0;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    padding: 0 12px;
    max-height: 40%;
    overflow: auto;
  }
  .dp-unresolved-head {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 0;
    font-size: var(--font-size-xs);
    color: var(--warning);
  }
  .dp-unresolved-list {
    margin: 0 0 8px;
    padding: 0 0 0 20px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .dp-unresolved-list li { padding: 1px 0; }
</style>
