<script lang="ts">
  /**
   * The syntax tree of the document in front of the user.
   *
   * It answers one question, and the whole panel is arranged around it: **why did
   * the parser read it that way?** Which is why the anonymous nodes — the commas,
   * the keywords — are shown by default. They are noisy, and they are very often
   * the answer; the toggle is there for the reading where they are not.
   *
   * A click selects the node's bytes in the editor. Moving the caret opens the
   * tree down to the node holding it. Both directions matter: one is "show me what
   * this is", the other is "show me where that is".
   */
  import { Braces, TriangleAlert, Filter as FilterIcon } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import { astStore } from '$lib/stores/picus/ast.svelte';
  import type { SyntaxNode } from '$lib/ipc/picus/ast';

  let filter = $state('');

  const tree = $derived(astStore.tree);
  /** The root is drawn as a row too — it is a node, and its range is the file. */
  const nodes = $derived<SyntaxNode[]>(tree ? [tree.root] : []);

  /** Identity: the range plus the kind. Unique inside one tree, stable across
   *  re-parses that did not touch this node — so expansion survives typing. */
  function idOf(node: SyntaxNode): string {
    return `${node.range.start}:${node.range.end}:${node.kind}`;
  }

  const expanded = $derived(new Set(astStore.revealed.flatMap((r) => {
    // The store keeps `start:end`; the tree keys include the kind, so a revealed
    // range opens whichever node holds it.
    return nodes.length ? idsMatching(nodes[0], r) : [];
  })));

  function idsMatching(node: SyntaxNode, range: string): string[] {
    const own = `${node.range.start}:${node.range.end}` === range ? [idOf(node)] : [];
    return own.concat((node.children ?? []).flatMap((c) => idsMatching(c, range)));
  }

  /** Locally toggled nodes, on top of whatever the caret revealed. */
  let opened = $state(new Set<string>());
  const expandedIds = $derived(new Set([...expanded, ...opened]));

  function toggle(id: string, next: boolean) {
    const copy = new Set(opened);
    if (next) copy.add(id);
    else copy.delete(id);
    opened = copy;
  }

  /** Matching on the kind AND the text: "the identifier called ETICHETTA" is the
   *  search somebody actually runs, and neither half alone finds it. */
  function match(node: SyntaxNode, q: string): boolean {
    const needle = q.toLowerCase();
    return (
      node.kind.toLowerCase().includes(needle) ||
      (node.field ?? '').toLowerCase().includes(needle) ||
      (node.text ?? '').toLowerCase().includes(needle)
    );
  }
</script>

<PanelShell title="Syntax tree">
  {#snippet icon()}<Braces size={13} />{/snippet}

  {#snippet actions()}
    {#if astStore.loading}<Spinner size={11} />{/if}
    <Button
      variant="icon"
      size="xs"
      ariaLabel={astStore.namedOnly ? 'Show every node' : 'Hide punctuation and keywords'}
      tooltip={astStore.namedOnly
        ? 'Showing only the grammar’s own concepts — click for every node, punctuation included'
        : 'Showing every node. The commas and the keywords are often the reason a file reads oddly — click to hide them anyway'}
      onclick={() => astStore.setNamedOnly(!astStore.namedOnly)}
    >
      {#snippet iconStart()}<FilterIcon size={13} />{/snippet}
    </Button>
  {/snippet}

  {#if astStore.error}
    <div class="ap-pad">
      <Alert variant="error" compact title="The tree could not be built" text={astStore.error} />
    </div>
  {:else if !tree}
    <StateBlock
      tone="info"
      label={astStore.loading
        ? 'Reading the document…'
        : 'Open a script or a query and its syntax tree appears here.'}
    />
  {:else}
    <div class="ap-head">
      <SearchBar
        query={filter}
        showRegex={false}
        showCounter={false}
        placeholder="Filter by kind, field or text"
        oninput={(v: string) => (filter = v)}
      />
      <div class="ap-stats">
        <Badge variant="tone" tone="neutral" size="sm" label={`${tree.nodeCount} nodes`} />
        {#if tree.hasErrors}
          <Badge variant="tone" tone="warning" size="sm" label="parse errors" />
        {/if}
        {#if tree.truncated}
          <!-- Said, never implied: a tree that stopped early and a file that simply
               ends here look identical otherwise. -->
          <Badge variant="tone" tone="info" size="sm" label="truncated" />
        {/if}
      </div>
    </div>

    <div class="ap-tree">
      <Tree
        {nodes}
        getId={idOf}
        getChildren={(n) => n.children}
        hasChildren={(n) => !!(n.children?.length || n.elided)}
        expandedIds={expandedIds}
        onExpandToggle={(id, next) => toggle(id, next)}
        selectedId={astStore.selectedKey}
        {filter}
        {match}
        guides
        rowHeight={20}
        onSelect={(node) => astStore.select(node)}
      >
        {#snippet row({ node })}
          <span class="ap-row" class:ap-error={node.error || node.missing}>
            {#if node.field}
              <!-- The single most useful column: it is the difference between "an
                   identifier" and "the table being written to". -->
              <span class="ap-field">{node.field}:</span>
            {/if}
            <span class="ap-kind" class:ap-anon={!node.named}>{node.kind}</span>
            {#if node.missing}
              <Badge variant="tone" tone="warning" size="sm" label="invented" />
            {:else if node.error}
              <TriangleAlert size={11} class="ap-warn" />
            {/if}
            {#if node.injected}
              <!-- Its children came from a second parse: the SQL grammar hands a
                   `$$ … $$` routine body back as one token, and that is where an
                   update script does its work. Worth marking — it is the
                   difference between "the grammar reads this" and "we read this
                   separately because the grammar would not". -->
              <Badge variant="tone" tone="info" size="sm" label="body" />
            {/if}
            {#if node.text}<span class="ap-text">{node.text}</span>{/if}
            {#if node.elided}<span class="ap-more">…</span>{/if}
          </span>
        {/snippet}
      </Tree>
    </div>
  {/if}
</PanelShell>

<style>
  .ap-pad { padding: 8px; }

  .ap-head {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .ap-stats { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }

  .ap-tree { overflow: auto; flex: 1; min-height: 0; }

  .ap-row {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    font-size: 11px;
  }
  .ap-row.ap-error { color: var(--error); }

  .ap-field {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--accent);
    flex-shrink: 0;
  }
  .ap-kind {
    font-family: var(--font-code);
    color: var(--text-primary);
    flex-shrink: 0;
  }
  /* Anonymous nodes are the punctuation and the keywords: present, and visibly
     secondary to the grammar's own concepts. */
  .ap-kind.ap-anon { color: var(--text-muted); font-style: italic; }

  .ap-text {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0.8;
  }
  .ap-more { color: var(--text-muted); }
</style>
